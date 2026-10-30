// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing commands of worksheets — the spreadsheet担当's file.
//!
//! A worksheet is a [`crate::Sheet`] held by a slide's `ShapeKind::Sheet`
//! (behind an `Arc`: changed with `Arc::make_mut`). Each command applied
//! returns the command that undoes it, as [`crate::Command`]s do; see
//! `command.rs`.
//!
//! The commands carry final values (a value or formula for a cell, a
//! format change), never UI state, so the UI (or a server, or a test)
//! builds them and the model does the rest: after cells change, the
//! formulas that depend on them are calculated again (`sheet_calc.rs`).
//! Undo does not calculate: it puts back the very cells (values, texts and
//! formats) that were there, as a [`SheetCommand::Restore`].

use std::sync::Arc;

use crate::command::{Command, EditError};
use crate::drawing::Rgb;
use crate::geometry::{Emu, EmuRect, EmuSize};
pub use ibis_calc::formula::Axis;

use crate::model::{Presentation, Shape, ShapeId, ShapeKind, Slide, SlideId};
use crate::sheet::{Border, CellUnderline, CellAlign, CellKind, CellRange, CellStyle, CellValue, CellVerticalAlign, CustomCellStyle, CalcMode, Sheet, SheetCell, Sparkline, SparklineGroup, SubtotalParams, Workbook, cell_name};
use crate::sheet_calc::BookView;
use crate::sheet_ops::{StructureEdit, apply_structure};

/// What is put in a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellInput {
    /// A constant value (a number, text, a logical value or an error).
    Value(CellValue),
    /// A formula, without the leading `=`, as the user sees it (no
    /// `_xlfn.` prefixes).
    Formula(String),
    /// Nothing: the value and formula are removed, the format stays.
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEdit {
    pub row: u32,
    pub column: u32,
    pub input: CellInput,
}

/// A cell pasted: its content and, if given, its format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PutCell {
    pub row: u32,
    pub column: u32,
    pub input: CellInput,
    pub style: Option<CellStyle>,
}

/// A cell as it is to be (`None`: no cell there).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellSlot {
    pub row: u32,
    pub column: u32,
    pub cell: Option<SheetCell>,
}

/// Cells (and the list of styles) of one sheet put back as they were.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetPatch {
    pub slide: SlideId,
    pub cells: Vec<CellSlot>,
    pub styles: Option<Vec<CellStyle>>,
    /// The pictures IMAGE asked for in cells (`Sheet::web_images`) as they were, so that an undo gives back the picture
    /// with the formula.
    pub images: Vec<((u32, u32), Option<ibis_calc::eval::ImageSpec>)>,
}

/// An edge of a cell, for borders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    DiagonalUp,
    DiagonalDown,
}

/// A change to cells' format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleChange {
    Font(Option<String>),
    /// Hundredths of a point.
    FontSize(u32),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    UnderlineKind(CellUnderline),
    Strike(bool),
    Color(Rgb),
    /// A theme colour for the text (the colour it is in this workbook's theme, and which).
    ColorTheme(Rgb, crate::sheet::ThemeColor),
    Fill(Option<Rgb>),
    /// A theme colour for the fill.
    FillTheme(Rgb, crate::sheet::ThemeColor),
    Border(Edge, Option<Border>),
    Align(CellAlign),
    Vertical(CellVerticalAlign),
    Wrap(bool),
    Indent(u32),
    Shrink(bool),
    NumberFormat(String),
    /// ロック (`false`: can be changed while the sheet is protected).
    Locked(bool),
    /// 表示しない (the formula hidden while the sheet is protected).
    FormulaHidden(bool),
    /// 文字の方向: 0–180 degrees (90 up, 180 down: Excel's −90), 255 縦書き.
    Rotation(u8),
    /// 上付き・下付き.
    VertAlign(crate::flow::VerticalPosition),
    /// 塗りつぶしのパターン (preset, pattern colour, background), or none.
    Pattern(Option<(String, Rgb, Rgb)>),
    /// パターンの前景色（パターンの色）の変更
    PatternColor(Rgb),
    /// 塗りつぶしのグラデーション (gradientFill), or none.
    Gradient(Option<crate::sheet::CellGradient>),
}

impl StyleChange {
    pub fn apply(&self, s: &mut CellStyle) {
        match self {
            Self::Font(f) => s.font = f.clone(),
            Self::FontSize(v) => s.font_size = *v,
            Self::Bold(v) => s.bold = *v,
            Self::Italic(v) => s.italic = *v,
            Self::Underline(v) => s.underline = if *v { CellUnderline::Single } else { CellUnderline::None },
            Self::UnderlineKind(v) => s.underline = *v,
            Self::Strike(v) => s.strike = *v,
            Self::Color(c) => {
                s.color = *c;
                s.color_theme = None;
            }
            Self::ColorTheme(c, t) => {
                s.color = *c;
                s.color_theme = Some(*t);
            }
            Self::FillTheme(c, t) => {
                s.fill = Some(*c);
                s.fill_theme = Some(*t);
                s.pattern = None;
                s.gradient = None;
            }
            Self::Fill(f) => {
                s.fill = *f;
                s.fill_theme = None;
                // A colour chosen: no pattern or gradient over it.
                s.pattern = None;
                s.gradient = None;
            }
            Self::Border(edge, b) => match edge {
                Edge::Left => s.left = *b,
                Edge::Right => s.right = *b,
                Edge::Top => s.top = *b,
                Edge::Bottom => s.bottom = *b,
                Edge::DiagonalUp => s.diagonal_up = *b,
                Edge::DiagonalDown => s.diagonal_down = *b,
            },
            Self::Align(a) => s.align = *a,
            Self::Vertical(v) => {
                // As saving writes it: a changed position names itself unless
                // it is the default (bottom).
                if s.vertical != *v {
                    s.vertical_set = *v != CellVerticalAlign::Bottom;
                }
                s.vertical = *v
            }
            Self::Wrap(v) => s.wrap = *v,
            Self::Indent(v) => s.indent = *v,
            Self::Shrink(v) => s.shrink = *v,
            Self::NumberFormat(code) => s.number_format = code.clone(),
            Self::Rotation(r) => s.rotation = *r,
            Self::VertAlign(v) => s.vert_align = *v,
            Self::Pattern(p) => {
                s.pattern = p.clone();
                s.gradient = None;
                // A pattern's background is the cell's fill.
                if let Some((_, _, bg)) = p {
                    s.fill = Some(*bg);
                    s.fill_theme = None;
                }
            }
            Self::PatternColor(c) => {
                let bg = s.pattern.as_ref().map(|p| p.2).or(s.fill).unwrap_or(Rgb(0xFF_FFFF));
                let preset = s.pattern.as_ref().map(|p| p.0.clone()).unwrap_or_else(|| "pct50".to_string());
                s.pattern = Some((preset, *c, bg));
                s.fill = Some(bg);
                s.fill_theme = None;
                s.gradient = None;
            }
            Self::Gradient(g) => {
                s.gradient = g.clone();
                if g.is_some() {
                    s.fill = None;
                    s.fill_theme = None;
                    s.pattern = None;
                }
            }
            Self::Locked(v) => s.unlocked = !*v,
            Self::FormulaHidden(v) => s.formula_hidden = *v,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetCommand {
    /// Put values or formulas in cells (their formats stay), then calculate
    /// again what depends on them.
    SetCells { slide: SlideId, edits: Vec<CellEdit> },
    /// The text of a cell (a text constant) in parts of their own fonts, or
    /// (no runs) in the cell's font alone.
    /// A data table made (or its inputs changed) and its results filled in.
    SetDataTable { slide: SlideId, table: crate::sheet::DataTable },
    /// A data table turned into the values it holds (the cells stay).
    ClearDataTable { slide: SlideId, range: CellRange },
    SetRuns { slide: SlideId, row: u32, column: u32, runs: Vec<crate::sheet::CellRun> },
    /// Select workbook calculation compatibility and recalculate all sheets.
    SetCompatibilityVersion { version: crate::sheet::CalculationCompatibilityVersion },
    /// 計算方法の設定 (`calcMode`): 自動 (Auto)、データテーブル以外自動 (AutoNoTable)、手動 (Manual).
    SetCalcMode { mode: CalcMode },
    /// ブックの保存前に再計算 (`calcOnSave`): true または false.
    SetCalcOnSave { calc_on_save: bool },
    /// 計算の表示桁数 (`fullPrecision`): true (完全精度) または false (表示桁数で計算).
    SetFullPrecision { full_precision: bool },
    /// 反復計算の設定 (`calcPr/@iterate`, `@maxIterations`, `@maxChange`):
    /// iterate: true/false, max_iterations: 最大反復回数 (既定 100), max_change: 変化の最大値 (既定 0.001)
    SetIterativeCalculation { iterate: bool, max_iterations: u32, max_change: crate::sheet::SheetFloat },
    /// 再計算実行 (Excel's F9): すべてのシートの数式とデータテーブルを再計算.
    CalculateNow,
    /// シート再計算 (Excel's Shift+F9): 指定シートの数式とデータテーブルを再計算.
    CalculateSheet { slide: SlideId },
    /// Change the format of every cell of the ranges (cells are made where
    /// there are none).
    SetFormat { slide: SlideId, ranges: Vec<CellRange>, change: StyleChange },
    /// Put cells back as they were (the inverse of the others).
    Restore(Vec<SheetPatch>),
    /// Put cells with their formats (pasting): each cell's content, and
    /// its format if given.
    PutCells { slide: SlideId, cells: Vec<PutCell> },
    /// Take the formats off the cells of the ranges (the values stay).
    ClearFormats { slide: SlideId, ranges: Vec<CellRange> },
    /// Insert `count` rows or columns before `at` (0-based).
    Insert { slide: SlideId, axis: Axis, at: u32, count: u32 },
    /// Delete `count` rows or columns from `at`.
    Delete { slide: SlideId, axis: Axis, at: u32, count: u32 },
    /// Set the width of columns or the height of rows `first..=last`
    /// (0 hides them).
    SetSize { slide: SlideId, axis: Axis, first: u32, last: u32, size: Emu },
    /// Define a name (`formula` without `=`, e.g. `Sheet1!$A$1:$B$3`) for
    /// the workbook or one sheet (`sheet`: its index), change what it
    /// stands for, or (`None`) delete it; formulas are calculated again.
    SetName { name: String, sheet: Option<usize>, formula: Option<String> },
    /// セルの挿入・削除: cells inserted at `range` (`insert`) pushing the
    /// cells below (`down`) or to the right along, or the range's cells
    /// deleted, those below or to the right moving in; formulas referring
    /// to the cells that move follow (the cells deleted become `#REF!`).
    ShiftCells { slide: SlideId, range: CellRange, down: bool, insert: bool },
    /// グループ化 (`deeper`) or グループ解除 of rows or columns `first`–`last`:
    /// their outline level one more (up to 7) or one less.
    Group { slide: SlideId, axis: Axis, first: u32, last: u32, deeper: bool },
    /// A hyperlink on `range` (the links there replaced), or (`None`) the
    /// links there taken off.
    SetHyperlink { slide: SlideId, range: CellRange, link: Option<crate::sheet::Hyperlink> },
    /// A comment on a cell: set, updated, or (`None`) deleted.
    SetComment { slide: SlideId, row: u32, column: u32, comment: Option<crate::sheet::SheetComment> },
    /// A threaded comment on a cell: set, updated, or (`None`) deleted.
    SetThreadedComment { slide: SlideId, row: u32, column: u32, thread: Option<crate::sheet::ThreadedComment> },
    /// Data validation over `ranges`: the rules wholly inside them go, and
    /// `rule` (if any) is set over them.
    SetValidation { slide: SlideId, ranges: Vec<CellRange>, rule: Option<crate::sheet::Validation> },
    /// シートの保護: the sheet protected (with its password's hash and what
    /// may be done) or not.
    SetProtection { slide: SlideId, protection: Option<crate::sheet::SheetProtection> },
    /// 範囲の編集の許可: the protected ranges allowing edits while the sheet is protected.
    SetProtectedRanges { slide: SlideId, ranges: Vec<crate::sheet::ProtectedRange> },
    /// ブックの保護: the workbook protected (with its password's hash and what
    /// is locked: structure, windows) or not.
    SetWorkbookProtection { protection: Option<crate::sheet::WorkbookProtection> },
    /// ウィンドウの非表示・再表示: the workbook's window hidden or shown.
    SetWindowHidden { hidden: bool },
    /// 並べ替え: the rows of `range` (its first row a heading kept in place
    /// if `header`) sorted by `keys` (column, ascending), first key first;
    /// formulas moving with a row move their references with it.
    Sort { slide: SlideId, range: CellRange, keys: Vec<(u32, bool)>, header: bool },
    /// 重複の削除: the rows of `range` (after its first row if `header`)
    /// whose values in `columns` repeat an earlier row's go; the rows left
    /// move up in their order and the range's last rows are left empty.
    /// Only the range's own columns move (Excel's データ > 重複の削除).
    RemoveDuplicates { slide: SlideId, range: CellRange, columns: Vec<u32>, header: bool },
    /// テーブル: `table` put at `index` of the sheet's tables (past the end:
    /// a new one), or the one there taken off (`None`: 範囲に変換, its
    /// formats staying); `cells` put with it (headings, the formats of its
    /// style, a calculated column's formulas).
    SetTable { slide: SlideId, index: usize, table: Option<crate::sheet::SheetTable>, cells: Vec<PutCell> },
    /// A table's filter: its ▼ and what they keep (`None`: none).
    SetTableFilter { slide: SlideId, index: usize, filter: Option<crate::sheet::AutoFilter> },
    /// 並べ替え as [`SheetCommand::Sort`], with keys by value or by colour,
    /// and optional case sensitivity (`match_case`).
    SortBy { slide: SlideId, range: CellRange, keys: Vec<(u32, SortBy)>, header: bool, match_case: bool },
    /// A conditional format added (after the others, its rules last in priority).
    AddConditional { slide: SlideId, format: crate::sheet::ConditionalFormat },
    /// Replace a conditional format, keeping its place in the rule order.
    UpdateConditional { slide: SlideId, index: usize, format: crate::sheet::ConditionalFormat },
    /// Duplicate a conditional formatting rule, placing it adjacent in evaluation order.
    DuplicateConditionalRule { slide: SlideId, index: usize, rule: usize },
    /// Delete a single conditional formatting rule by format index and rule index.
    DeleteConditionalRule { slide: SlideId, index: usize, rule: usize },
    /// Move one rule ahead of or behind the adjacent rule in priority order.
    MoveConditionalRule { slide: SlideId, index: usize, rule: usize, up: bool },
    /// Set stopIfTrue without changing a rule's type, formula or style.
    SetConditionalStop { slide: SlideId, index: usize, rule: usize, stop: bool },
    /// The conditional formats whose ranges lie within `ranges` taken off.
    ClearConditional { slide: SlideId, ranges: Vec<CellRange> },
    /// オートフィルター: set (rows not kept are hidden, the others shown)
    /// or (`None`) taken off (its hidden rows shown again).
    SetAutoFilter { slide: SlideId, filter: Option<crate::sheet::AutoFilter> },
    /// 詳細設定フィルター: filter rows of `list_range` according to `criteria_range`,
    /// either in-place (`copy_to: None`) or copying matching rows to `copy_to`.
    AdvancedFilter { slide: SlideId, list_range: CellRange, criteria_range: CellRange, copy_to: Option<CellRange>, unique: bool },
    /// 統合: consolidate data from `consolidate.data_refs` into destination at `(row, column)`.
    Consolidate { slide: SlideId, destination: (u32, u32), consolidate: crate::sheet::DataConsolidate },
    /// A What-If scenario added to the worksheet.
    AddScenario { slide: SlideId, scenario: crate::sheet::SheetScenario },
    /// A What-If scenario updated on the worksheet.
    EditScenario { slide: SlideId, index: usize, scenario: crate::sheet::SheetScenario },
    /// A What-If scenario deleted from the worksheet.
    DeleteScenario { slide: SlideId, index: usize },
    /// A What-If scenario shown/applied on the worksheet.
    ShowScenario { slide: SlideId, index: usize },
    /// A What-If scenario summary report sheet generated.
    CreateScenarioSummary { slide: SlideId, result_cells: Vec<String> },
    /// A new sheet (after the sheet of `slide`) with the given cells, for a
    /// report: its name made different from the others' (`name`, `name 2`, …).
    CreateReportSheet { slide: SlideId, name: String, cells: Vec<ReportCell>, widths: Vec<(u32, u32)> },
    /// 閉じて読み込む of the クエリ エディター: the query's result written to its sheet (made after the
    /// sheet of `slide`, or rewritten if the query has loaded one) and the query kept in the workbook.
    LoadQuery { slide: SlideId, query: crate::query::Query, table: crate::query::QTable },
    /// The workbook's queries and their parameters replaced (a query edited or deleted).
    SetQueries { queries: Vec<crate::query::Query>, parameters: Vec<crate::query::Parameter> },
    /// The Solver's model kept in the sheet's hidden names (`solver_*`): the
    /// listed ones replace those the sheet has.
    SetSolverNames { slide: SlideId, names: Vec<(String, String)> },
    /// Merge all scenarios from `from_slide` into worksheet `slide`.
    MergeScenarios { slide: SlideId, from_slide: SlideId },
    /// ユーザー設定のビューの追加: A custom view added to the workbook.
    AddCustomView {
        view: crate::sheet::CustomWorkbookView,
        sheet_views: Vec<(SlideId, crate::sheet::CustomSheetView)>,
    },
    /// ユーザー設定のビューの削除: A custom view deleted from the workbook.
    DeleteCustomView { name: String },
    /// ユーザー設定のビューの表示: A custom view applied / shown.
    ShowCustomView { name: String },
    /// Show details (drill-down / 詳細データの展開) for a pivot table cell:
    /// generates a new worksheet with the source records that contribute to cell at (`column`, `row`).
    ShowPivotDetails { slide: SlideId, column: u32, row: u32 },
    /// データ > ブックのリンク > 値の更新: the cached cells of the external
    /// workbook `index` set to `sheets` (read from its file), and the formulas
    /// that read them calculated again.
    RefreshExternalBook { index: usize, sheets: Vec<crate::sheet::ExternalSheet> },
    /// A formula typed with a workbook the file does not link yet (`[Budget.xlsx]Sheet1!A1`): the link added
    /// (its cached cells as read from the file, when it was found), numbered after the others.
    AddExternalBook { book: crate::sheet::ExternalBook },
    /// データ > ブックのリンク > リンク元の変更: the source of external workbook `index`
    /// changed to `new_name` and `new_path`, formulas referencing it rewritten, and cached cells set to `sheets`.
    ChangeExternalBookSource { index: usize, new_name: String, new_path: String, sheets: Vec<crate::sheet::ExternalSheet> },
    /// データ > ブックのリンク > リンクの解除: the formulas that refer to the
    /// external workbook `index` become their values (a defined name, a
    /// conditional format and a validation get `#REF!`), and the link goes
    /// when the file is saved.
    BreakExternalLink { index: usize },
    /// エラー チェック > エラーを無視: errors of `kind` in `range` ignored.
    IgnoreErrors { slide: SlideId, range: CellRange, kind: crate::error_check::ErrorKind },
    /// スライサーの設定: the slicer shape `shape` of `slide` made `slicer` (its items are made again from its source).
    EditSlicer { slide: SlideId, shape: crate::model::ShapeId, slicer: crate::slicer::Slicer },
    /// A timeline's selection, level or scroll changed: the timeline shape `shape` of `slide` made `timeline`.
    EditTimeline { slide: SlideId, shape: crate::model::ShapeId, timeline: crate::timeline::Timeline },
    /// A form control's state changed: the control shape `shape` of `slide` made `control`.
    EditControl { slide: SlideId, shape: crate::model::ShapeId, control: crate::form_control::FormControl },
    /// 無視したエラーをリセット: the errors ignored on the sheet marked again.
    ResetIgnoredErrors { slide: SlideId },
    /// The sheet tab's colour (`None`: none).
    SetTabColor { slide: SlideId, color: Option<Rgb> },
    /// 目盛線 (the grid lines between cells) shown or not.
    SetGridLines { slide: SlideId, shown: bool },
    /// 見出し (row numbers and column letters) shown or not.
    SetHeadings { slide: SlideId, shown: bool },
    /// The sheet's zoom scale in percent (`sheetView/@zoomScale`).
    SetZoom { slide: SlideId, zoom: Option<u16> },
    /// 改ページ プレビュー (page break preview) shown or not.
    SetPageBreakPreview { slide: SlideId, shown: bool },
    /// 表示 > ページ レイアウト (turns page break preview off).
    SetPageLayout { slide: SlideId, shown: bool },
    /// ルーラー (the rulers in page layout view) shown or not.
    SetShowRuler { slide: SlideId, shown: bool },
    /// 数式の表示 (`sheetView/@showFormulas`).
    SetShowFormulas { slide: SlideId, shown: bool },
    /// ゼロ値の表示 (`sheetView/@showZeros`).
    SetShowZeros { slide: SlideId, shown: bool },
    /// インクの非表示.
    SetHideInk { slide: SlideId, hidden: bool },
    /// Freeze the columns left of and the rows above (columns, rows), or
    /// unfreeze (`None`): ウィンドウ枠の固定.
    Freeze { slide: SlideId, frozen: Option<(u32, u32)> },
    /// Split the window into panes at (columns, rows), or unsplit (`None`):
    /// ウィンドウの分割 (`pane/@state="split"`).
    Split { slide: SlideId, split: Option<(u32, u32)> },
    /// The sheet's print settings, and (`Some`) its print area (`None` in
    /// it: none), kept as the sheet's `_xlnm.Print_Area` name too.
    SetPrint { slide: SlideId, print: crate::sheet::SheetPrint, area: Option<Option<CellRange>> },
    /// Move a worksheet to position `to` of the workbook.
    MoveSheet { slide: SlideId, to: usize },
    /// A copy of a worksheet right after it, named `name`, on a new slide
    /// (cells, formats, sizes, merged ranges; not its drawings).
    CopySheet { slide: SlideId, name: String, new_slide: SlideId, shape: ShapeId },
    /// Hide a worksheet (at least one stays shown), or show a hidden one
    /// again on a new slide with the ids given.
    HideSheet { slide: SlideId, sheet: usize, hidden: bool, shape: ShapeId },
    /// Move a range (切り取りと貼り付け, or a drag of the range's edge) so its
    /// top-left cell is at (`row`, `column`): values, formulas and formats
    /// go; formulas anywhere that refer to cells inside the range follow
    /// them (Excel's rule: a reference wholly inside the range moves).
    MoveCells { slide: SlideId, from: CellRange, row: u32, column: u32 },
    /// Hide rows or columns (`hidden`), or show the hidden ones among them
    /// again at the size they had.
    Hide { slide: SlideId, axis: Axis, first: u32, last: u32, hidden: bool },
    /// Merge a range: the cells but the top-left one are emptied, and with
    /// `center` its text is centred.
    Merge { slide: SlideId, range: CellRange, center: bool },
    /// Undo the merged ranges within `range`.
    Unmerge { slide: SlideId, range: CellRange },
    /// A new, empty worksheet at `index` of the workbook, on a new slide
    /// with the ids given.
    AddSheet { index: usize, name: String, slide: SlideId, shape: ShapeId },
    /// Delete the worksheet on `slide` (not the last one shown).
    DeleteSheet { slide: SlideId },
    /// Rename the worksheet on `slide`.
    RenameSheet { slide: SlideId, name: String },
    /// Put the workbook's sheets back as they were (the inverse of
    /// structural changes, which reach every sheet).
    RestoreAll { slides: Vec<Slide>, workbook: Option<Arc<Workbook>> },
    /// Sizes of columns or rows from `first` put back (the undo of a size change: not a copy of the whole book), with their entries in the hidden ones (the size they come back at, if any).
    RestoreSizes { slide: SlideId, axis: Axis, first: u32, sizes: Vec<Emu>, hidden: Vec<Option<Emu>>, custom: Vec<bool> },
    /// A column width the sheet chose by itself (a number entered that is wider than the column; "fit to selection"):
    /// the columns are not marked as fixed by the user, so they widen again and their width is not saved.
    SetColumnWidthAuto { slide: SlideId, first: u32, last: u32, size: Emu },
    /// The sheet as built elsewhere, in place of the slide's (a pivot table
    /// built again from its source, in its style: ピボットテーブルの更新).
    ReplaceSheet { slide: SlideId, sheet: Box<Sheet> },
    /// A user-defined custom cell style added or updated in the workbook.
    AddCustomCellStyle { style: CustomCellStyle },
    /// A user-defined custom cell style removed from the workbook.
    DeleteCustomCellStyle { name: String },
    /// A new sparkline group added to the sheet.
    AddSparklineGroup { slide: SlideId, group: SparklineGroup },
    /// A sparkline group updated on the sheet.
    EditSparklineGroup { slide: SlideId, index: usize, group: SparklineGroup },
    /// A sparkline group removed from the sheet.
    DeleteSparklineGroup { slide: SlideId, index: usize },
    /// Sparklines cleared from ranges (specific cells or whole groups).
    ClearSparklines { slide: SlideId, ranges: Vec<CellRange>, whole_group: bool },
    /// Subtotals inserted into a sheet (データ > 小計).
    ApplySubtotal { slide: SlideId, params: SubtotalParams },
    /// Subtotals removed from a sheet (すべて削除).
    RemoveSubtotal { slide: SlideId, range: CellRange },
    /// 文書のプロパティ（タイトル、作成者、作成日時等）の変更。
    SetDocProperties(crate::flow::DocProperties),
    /// グラフの移動 (Move Chart): 既存のシートまたは新しいシートへグラフを移動.
    MoveChart {
        from_slide: SlideId,
        shape: ShapeId,
        target: ChartMoveTarget,
    },
    /// ふりがなの設定（セル単位）: 指定セルのふりがな文字列を設定または解除 (`None`).
    SetCellPhonetic { slide: SlideId, row: u32, column: u32, phonetic: Option<String> },
    /// ふりがなの表示/非表示トグル: シート全体のふりがな表示状態 (`show_phonetic`) を切り替え.
    SetShowPhonetic { slide: SlideId, show: bool },
    /// ふりがなの設定 (書式/配置/文字種): シートの `<phoneticPr>` を設定または解除 (`None`).
    SetPhoneticProperties { slide: SlideId, properties: Option<crate::sheet::PhoneticProperties> },
    /// 浮動画像をセルに配置 (Place in Cell): セル内に画像を配置し MoveAndSize 設定.
    PlacePictureInCell { slide: SlideId, shape: ShapeId, row: u32, column: u32 },
    /// セル内の画像をセル上に配置 (Place over Cells): セル内配置を解除して浮動画像に変換.
    PlacePictureOverCells { slide: SlideId, shape: ShapeId, row: u32, column: u32 },
}

/// グラフの移動先 (Move Chart Destination).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartMoveTarget {
    /// 既存のワークシート
    ExistingSheet { slide: SlideId },
    /// 新しいシート（シート名、スライドID、シート形状ID）
    NewSheet { name: String, slide: SlideId, sheet_shape: ShapeId },
}

/// Subtract an inclusive rectangle as at most four non-overlapping retained rectangles.
fn validation_range_difference(range:CellRange,cut:CellRange)->Vec<CellRange>{
 let top=range.first_row.max(cut.first_row);let bottom=range.last_row.min(cut.last_row);
 let left=range.first_column.max(cut.first_column);let right=range.last_column.min(cut.last_column);
 if top>bottom||left>right{return vec![range];}
 let mut out=Vec::new();
 if range.first_row<top{out.push(CellRange{last_row:top-1,..range});}
 if bottom<range.last_row{out.push(CellRange{first_row:bottom+1,..range});}
 if range.first_column<left{out.push(CellRange{first_row:top,last_row:bottom,last_column:left-1,..range});}
 if right<range.last_column{out.push(CellRange{first_row:top,last_row:bottom,first_column:right+1,..range});}
 out
}

pub(crate) fn apply(presentation: &mut Presentation, command: SheetCommand) -> Result<Command, EditError> {
    let view_only = matches!(
        command,
        SheetCommand::SetZoom { .. }
            | SheetCommand::SetGridLines { .. }
            | SheetCommand::SetHeadings { .. }
            | SheetCommand::SetPageBreakPreview { .. }
            | SheetCommand::SetPageLayout { .. }
            | SheetCommand::SetShowRuler { .. }
            | SheetCommand::SetHideInk { .. }
            | SheetCommand::Freeze { .. }
            | SheetCommand::Split { .. }
            | SheetCommand::SetWindowHidden { .. }
            | SheetCommand::SetDocProperties(_)
    );
    let result = apply_command(presentation, command);
    // The slicers, controls and charts show what the cells and filters hold after the edit (or its undo).
    if result.is_ok() && !view_only {
        crate::slicer::refresh_slicers(presentation);
        crate::form_control::refresh_controls(presentation);
        crate::chart_refresh::refresh_charts(presentation);
    }
    if result.is_ok() {
        sync_sheet_book_context(presentation);
    }
    result
}

fn apply_command(presentation: &mut Presentation, command: SheetCommand) -> Result<Command, EditError> {
    match command {
        SheetCommand::SetCells { slide, edits } => set_cells(presentation, slide, edits),
        SheetCommand::SetDataTable { slide, table } => {
            let inverse = snapshot(presentation);
            {
                let sheet = sheet_mut(presentation, slide)?;
                check_bounds(sheet, table.range.last_row, table.range.last_column)?;
                if table.range.first_row == 0 || table.range.first_column == 0 || (table.row_input.is_none() && table.column_input.is_none()) {
                    return Err(EditError::IndexOutOfRange);
                }
                // Not over another table, nor with an input cell inside its results.
                let inside = |r: &CellRange, (row, column): (u32, u32)| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column);
                if table.row_input.into_iter().chain(table.column_input).any(|c| inside(&table.range, c)) {
                    return Err(EditError::IndexOutOfRange);
                }
                sheet.data_tables.retain(|t| t.range != table.range);
                let overlaps = |a: &CellRange, b: &CellRange| a.first_row <= b.last_row && a.last_row >= b.first_row && a.first_column <= b.last_column && a.last_column >= b.first_column;
                if sheet.data_tables.iter().any(|t| overlaps(&t.range, &table.range)) {
                    return Err(EditError::IndexOutOfRange);
                }
                sheet.data_tables.push(table);
            }
            refresh_data_tables(presentation, slide, &mut Undo::default())?;
            Ok(inverse)
        }
        SheetCommand::ClearDataTable { slide, range } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.data_tables.retain(|t| t.range != range);
            Ok(inverse)
        }
        SheetCommand::SetRuns { slide, row, column, runs } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let cell = sheet.cell(row, column).cloned().ok_or(EditError::IndexOutOfRange)?;
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            if cell.formula.is_some() || (!runs.is_empty() && cell.value != CellValue::Text(text)) {
                return Err(EditError::IndexOutOfRange);
            }
            sheet.put_cell(row, column, Some(SheetCell { runs: (!runs.is_empty()).then(|| std::sync::Arc::new(runs)), ..cell }));
            Ok(inverse)
        }
        SheetCommand::SetFormat { slide, ranges, change } => set_format(presentation, slide, ranges, change),
        SheetCommand::Restore(patches) => restore(presentation, patches),
        SheetCommand::PutCells { slide, cells } => put_cells(presentation, slide, cells),
        SheetCommand::ClearFormats { slide, ranges } => clear_formats(presentation, slide, ranges),
        SheetCommand::Insert { slide, axis, at, count } => structure(presentation, slide, true, axis, at, count),
        SheetCommand::Delete { slide, axis, at, count } => structure(presentation, slide, false, axis, at, count),
        SheetCommand::RestoreAll { slides, workbook } => restore_all(presentation, slides, workbook),
        SheetCommand::RestoreSizes { slide, axis, first, sizes, hidden, custom } => restore_sizes(presentation, slide, axis, first, sizes, hidden, custom),
        SheetCommand::SetColumnWidthAuto { slide, first, last, size } => set_size_marked(presentation, slide, Axis::Columns, first, last, size, false),
        SheetCommand::SetSize { slide, axis, first, last, size } => set_size_marked(presentation, slide, axis, first, last, size, true),
        SheetCommand::Hide { slide, axis, first, last, hidden } => hide(presentation, slide, axis, first, last, hidden),
        SheetCommand::MoveCells { slide, from, row, column } => move_cells(presentation, slide, from, row, column),
        SheetCommand::HideSheet { slide, sheet, hidden, shape } => hide_sheet(presentation, slide, sheet, hidden, shape),
        SheetCommand::SetPrint { slide, print, area } => {
            let inverse = snapshot(presentation);
            let index = workbook_index(presentation, slide)?;
            let name = presentation.workbook.as_ref().map(|w| w.sheets[index].name.clone()).unwrap_or_default();
            let sheet = sheet_mut(presentation, slide)?;
            sheet.print = print;
            if let Some(area) = area {
                sheet.print_area = area;
                let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
                wb.names.retain(|n| !(n.name.eq_ignore_ascii_case("_xlnm.Print_Area") && n.sheet == Some(index)));
                if let Some(r) = area {
                    let a = |c: u32, row: u32| format!("${}${}", crate::sheet::cell_name(c, 0).trim_end_matches('1'), row + 1);
                    let formula = format!("{}!{}:{}", ibis_calc::formula::quote_sheet(&name), a(r.first_column, r.first_row), a(r.last_column, r.last_row));
                    wb.names.push(crate::sheet::DefinedName { name: "_xlnm.Print_Area".into(), sheet: Some(index), formula, hidden: false });
                }
            }
            Ok(inverse)
        }
        SheetCommand::MoveSheet { slide, to } => {
            check_workbook_structure_not_locked(presentation)?;
            let sheet = workbook_index(presentation, slide)?;
            structural(presentation, StructureEdit::MoveSheet { sheet, to })
        }
        SheetCommand::CopySheet { slide, name, new_slide, shape } => {
            check_workbook_structure_not_locked(presentation)?;
            let sheet = workbook_index(presentation, slide)?;
            if !crate::sheet_ops::valid_sheet_name(presentation, &name, None) || presentation.slide(new_slide).is_some() {
                return Err(EditError::IndexOutOfRange);
            }
            let original = presentation.slide(slide).and_then(crate::sheet_calc::sheet_of_slide).ok_or(EditError::IndexOutOfRange)?;
            let content = Arc::new(Sheet { frozen: original.frozen, ..(**original).clone() });
            structural(presentation, StructureEdit::AddSheet { sheet: sheet + 1, name, slide: new_slide, shape, content })
        }
        SheetCommand::SetName { name, sheet, formula } => set_name(presentation, name, sheet, formula),
        SheetCommand::SetCompatibilityVersion { version } => {
            let inverse = snapshot(presentation);
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb.compatibility_version = Some(version.number());
            invalidate_calc_index(presentation);
            let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
            recalculate_cells(presentation, &all)?;
            Ok(inverse)
        }
        SheetCommand::SetCalcMode { mode } => {
            let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
            let old_mode = wb.calc_mode;
            if old_mode == mode {
                return Ok(Command::Group(Vec::new()));
            }
            let inverse = snapshot(presentation);
            let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb_mut.calc_mode = mode;
            if mode == CalcMode::Auto {
                let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
                recalculate_cells_force(presentation, &all)?;
                let slides: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
                for s in slides {
                    let mut undo = Undo::default();
                    refresh_data_tables(presentation, s, &mut undo)?;
                }
            } else if mode == CalcMode::AutoNoTable && old_mode == CalcMode::Manual {
                let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
                recalculate_cells_force(presentation, &all)?;
            }
            Ok(inverse)
        }
        SheetCommand::SetCalcOnSave { calc_on_save } => {
            let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
            let old = wb.calc_on_save;
            if old == calc_on_save {
                return Ok(Command::Group(Vec::new()));
            }
            let inverse = snapshot(presentation);
            let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb_mut.calc_on_save = calc_on_save;
            Ok(inverse)
        }
        SheetCommand::SetFullPrecision { full_precision } => {
            let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
            let old = wb.full_precision;
            if old == full_precision {
                return Ok(Command::Group(Vec::new()));
            }
            let inverse = snapshot(presentation);
            let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb_mut.full_precision = full_precision;
            if !full_precision {
                let date1904 = wb_mut.date1904;
                let slides: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
                for s in slides {
                    if let Ok(sheet) = sheet_mut(presentation, s) {
                        for cell in sheet.cells.iter_mut() {
                            if let CellValue::Number(n) = cell.value {
                                if let Some(style) = sheet.styles.get(cell.style as usize) {
                                    let rounded = ibis_calc::numfmt::round_to_display_precision(n, &style.number_format, date1904);
                                    cell.value = CellValue::Number(rounded);
                                    cell.text = display_text(&cell.value, style, date1904);
                                }
                            }
                        }
                    }
                }
                let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
                recalculate_cells_force(presentation, &all)?;
            }
            Ok(inverse)
        }
        SheetCommand::SetIterativeCalculation { iterate, max_iterations, max_change } => {
            let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
            let old_iterate = wb.iterate;
            let old_max_iter = wb.max_iterations;
            let old_max_change = wb.max_change;
            if old_iterate == iterate && old_max_iter == max_iterations && old_max_change == max_change {
                return Ok(Command::Group(Vec::new()));
            }
            let inverse = snapshot(presentation);
            let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb_mut.iterate = iterate;
            wb_mut.max_iterations = max_iterations;
            wb_mut.max_change = max_change;
            let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
            recalculate_cells_force(presentation, &all)?;
            Ok(inverse)
        }
        SheetCommand::CalculateNow => {
            let inverse = snapshot(presentation);
            let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
            recalculate_cells_force(presentation, &all)?;
            let slides: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
            for s in slides {
                let mut undo = Undo::default();
                refresh_data_tables(presentation, s, &mut undo)?;
            }
            Ok(inverse)
        }
        SheetCommand::CalculateSheet { slide } => {
            let inverse = snapshot(presentation);
            let view = crate::sheet_calc::BookView::new(presentation);
            let Some(sheet_index) = view.index_of_slide(slide) else { return Ok(Command::Group(Vec::new())) };
            let formula_cells: Vec<_> = view.formula_cells().into_iter().filter(|c| c.0 == sheet_index).collect();
            recalculate_cells_force(presentation, &formula_cells)?;
            let mut undo = Undo::default();
            refresh_data_tables(presentation, slide, &mut undo)?;
            Ok(inverse)
        }
        SheetCommand::ShiftCells { slide, range, down, insert } => shift_cells(presentation, slide, range, down, insert),
        SheetCommand::Group { slide, axis, first, last, deeper } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let levels = match axis {
                Axis::Rows => &mut sheet.outline_rows,
                Axis::Columns => &mut sheet.outline_columns,
            };
            let mut changed = false;
            for i in first..=last {
                let level = levels.get(&i).copied().unwrap_or(0);
                let new = if deeper { (level + 1).min(7) } else { level.saturating_sub(1) };
                if new != level {
                    changed = true;
                    if new == 0 {
                        levels.remove(&i);
                    } else {
                        levels.insert(i, new);
                    }
                }
            }
            if !changed {
                return Err(EditError::IndexOutOfRange);
            }
            Ok(inverse)
        }
        SheetCommand::SetHyperlink { slide, range, link } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let meets = |r: &CellRange| r.first_row <= range.last_row && r.last_row >= range.first_row && r.first_column <= range.last_column && r.last_column >= range.first_column;
            sheet.hyperlinks.retain(|h| !meets(&h.range));
            if let Some(mut link) = link {
                link.range = range;
                sheet.hyperlinks.push(link);
            }
            Ok(inverse)
        }
        SheetCommand::SetComment { slide, row, column, comment } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.comments.retain(|c| !(c.row == row && c.column == column));
            if let Some(mut c) = comment {
                c.row = row;
                c.column = column;
                sheet.comments.push(c);
            }
            Ok(inverse)
        }
        SheetCommand::SetThreadedComment { slide, row, column, thread } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.threads.retain(|t| !(t.row == row && t.column == column));
            if let Some(mut t) = thread {
                t.row = row;
                t.column = column;
                sheet.threads.push(t);
            }
            Ok(inverse)
        }
        SheetCommand::SetValidation { slide, ranges, rule } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            // Replace only the addressed cells, including a partial overlap of an existing rule.
            // Relative formula references are anchored at the first cell of the retained areas.
            for v in sheet.validations.iter_mut() {
                let original=v.ranges.first().copied();
                for target in &ranges {v.ranges=v.ranges.iter().flat_map(|r|validation_range_difference(*r,*target)).collect();}
                if let (Some(old),Some(new))=(original,v.ranges.first()) {
                    let delta=(new.first_row as i64-old.first_row as i64,new.first_column as i64-old.first_column as i64);
                    if delta!=(0,0) {for formula in [&mut v.formula1,&mut v.formula2] {if let Some(f)=formula { *f=ibis_calc::formula::shift(f,delta.0,delta.1); }}}
                }
            }
            sheet.validations.retain(|v| !v.ranges.is_empty());
            if let Some(mut rule) = rule {
                rule.ranges = ranges;
                sheet.validations.push(rule);
            }
            Ok(inverse)
        }
        SheetCommand::Sort { slide, range, keys, header } => sort_rows(presentation, slide, range, keys.into_iter().map(|(c, a)| (c, SortBy::Value(a))).collect(), header, false),
        SheetCommand::SortBy { slide, range, keys, header, match_case } => sort_rows(presentation, slide, range, keys, header, match_case),
        SheetCommand::RemoveDuplicates { slide, range, columns, header } => remove_duplicates(presentation, slide, range, columns, header),
        SheetCommand::SetTable { slide, index, table, cells } => set_table(presentation, slide, index, table, cells),
        SheetCommand::ReplaceSheet { slide, sheet } => replace_sheet(presentation, slide, *sheet),
        SheetCommand::SetTableFilter { slide, index, filter } => set_table_filter(presentation, slide, index, filter),
        SheetCommand::SetProtection { slide, protection } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.protection = protection;
            Ok(inverse)
        }
        SheetCommand::SetProtectedRanges { slide, ranges } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.protected_ranges = ranges;
            Ok(inverse)
        }
        SheetCommand::SetWorkbookProtection { protection } => {
            let inverse = snapshot(presentation);
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb.protection = protection;
            Ok(inverse)
        }
        SheetCommand::SetWindowHidden { hidden } => apply_set_window_hidden(presentation, hidden),
        SheetCommand::SetAutoFilter { slide, filter } => set_auto_filter(presentation, slide, filter),
        SheetCommand::AdvancedFilter { slide, list_range, criteria_range, copy_to, unique } => {
            advanced_filter(presentation, slide, list_range, criteria_range, copy_to, unique)
        }
        SheetCommand::Consolidate { slide, destination, consolidate } => {
            apply_consolidate(presentation, slide, destination, consolidate)
        }
        SheetCommand::AddScenario { slide, scenario } => apply_add_scenario(presentation, slide, scenario),
        SheetCommand::EditScenario { slide, index, scenario } => apply_edit_scenario(presentation, slide, index, scenario),
        SheetCommand::DeleteScenario { slide, index } => apply_delete_scenario(presentation, slide, index),
        SheetCommand::ShowScenario { slide, index } => apply_show_scenario(presentation, slide, index),
        SheetCommand::CreateScenarioSummary { slide, result_cells } => apply_create_scenario_summary(presentation, slide, result_cells),
        SheetCommand::CreateReportSheet { slide, name, cells, widths } => apply_create_report_sheet(presentation, slide, name, cells, widths),
        SheetCommand::LoadQuery { slide, query, table } => apply_load_query(presentation, slide, query, table),
        SheetCommand::SetQueries { queries, parameters } => {
            let inverse = snapshot(presentation);
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb.queries = queries;
            wb.query_parameters = parameters;
            Ok(inverse)
        }
        SheetCommand::SetSolverNames { slide, names } => apply_set_solver_names(presentation, slide, names),
        SheetCommand::MergeScenarios { slide, from_slide } => apply_merge_scenarios(presentation, slide, from_slide),
        SheetCommand::AddCustomView { view, sheet_views } => apply_add_custom_view(presentation, view, sheet_views),
        SheetCommand::DeleteCustomView { name } => apply_delete_custom_view(presentation, &name),
        SheetCommand::ShowCustomView { name } => apply_show_custom_view(presentation, &name),
        SheetCommand::ShowPivotDetails { slide, column, row } => apply_show_pivot_details(presentation, slide, column, row),
        SheetCommand::IgnoreErrors { slide, range, kind } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.ignore_errors(range, kind);
            Ok(inverse)
        }
        SheetCommand::EditSlicer { slide, shape, slicer } => {
            let inverse = snapshot(presentation);
            let target = presentation.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|sh| sh.id == shape)).ok_or(EditError::IndexOutOfRange)?;
            if !matches!(target.kind, crate::model::ShapeKind::Slicer(_)) {
                return Err(EditError::IndexOutOfRange);
            }
            target.kind = crate::model::ShapeKind::Slicer(Arc::new(slicer));
            Ok(inverse)
        }
        SheetCommand::EditTimeline { slide, shape, timeline } => {
            let inverse = snapshot(presentation);
            let target = presentation.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|sh| sh.id == shape)).ok_or(EditError::IndexOutOfRange)?;
            if !matches!(target.kind, crate::model::ShapeKind::Timeline(_)) {
                return Err(EditError::IndexOutOfRange);
            }
            target.kind = crate::model::ShapeKind::Timeline(Arc::new(timeline));
            Ok(inverse)
        }
        SheetCommand::EditControl { slide, shape, control } => {
            let inverse = snapshot(presentation);
            let target = presentation.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|sh| sh.id == shape)).ok_or(EditError::IndexOutOfRange)?;
            if !matches!(target.kind, crate::model::ShapeKind::Control(_)) {
                return Err(EditError::IndexOutOfRange);
            }
            target.kind = crate::model::ShapeKind::Control(Arc::new(control));
            Ok(inverse)
        }
        SheetCommand::ResetIgnoredErrors { slide } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.ignored_errors.clear();
            Ok(inverse)
        }
        SheetCommand::RefreshExternalBook { index, sheets } => refresh_external_book(presentation, index, sheets),
        SheetCommand::AddExternalBook { book } => add_external_book(presentation, book),
        SheetCommand::ChangeExternalBookSource { index, new_name, new_path, sheets } => change_external_book_source(presentation, index, new_name, new_path, sheets),
        SheetCommand::BreakExternalLink { index } => break_external_link(presentation, index),
        SheetCommand::AddSparklineGroup { slide, group } => apply_add_sparkline_group(presentation, slide, group),
        SheetCommand::EditSparklineGroup { slide, index, group } => apply_edit_sparkline_group(presentation, slide, index, group),
        SheetCommand::DeleteSparklineGroup { slide, index } => apply_delete_sparkline_group(presentation, slide, index),
        SheetCommand::ClearSparklines { slide, ranges, whole_group } => apply_clear_sparklines(presentation, slide, ranges, whole_group),
        SheetCommand::ApplySubtotal { slide, params } => apply_subtotal(presentation, slide, params),
        SheetCommand::RemoveSubtotal { slide, range } => apply_remove_subtotal(presentation, slide, range),
        SheetCommand::AddConditional { slide, mut format } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            if format.ranges.is_empty() || format.rules.is_empty() || format.ranges.iter().any(|r| r.first_row > r.last_row || r.first_column > r.last_column || r.last_row >= 1_048_576 || r.last_column >= 16_384) {
                return Err(EditError::IndexOutOfRange);
            }
            let next = sheet.conditional.iter().flat_map(|c| c.rules.iter().map(|r| r.priority)).max().unwrap_or(0).checked_add(1).ok_or(EditError::IndexOutOfRange)?;
            for (i, r) in format.rules.iter_mut().enumerate() {
                r.priority = next.checked_add(i as u32).ok_or(EditError::IndexOutOfRange)?;
            }
            format.source_index = None;
            sheet.conditional.push(format);
            Ok(inverse)
        }
        SheetCommand::UpdateConditional { slide, index, mut format } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let old = sheet.conditional.get(index).ok_or(EditError::IndexOutOfRange)?;
            if format.ranges.is_empty() || format.rules.len() != old.rules.len() || format.ranges.iter().any(|r| r.first_row > r.last_row || r.first_column > r.last_column || r.last_row >= 1_048_576 || r.last_column >= 16_384) {
                return Err(EditError::IndexOutOfRange);
            }
            for (r, previous) in format.rules.iter_mut().zip(&old.rules) {
                r.priority = previous.priority;
            }
            format.source_index = old.source_index;
            sheet.conditional[index] = format;
            Ok(inverse)
        }
        SheetCommand::DuplicateConditionalRule { slide, index, rule } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let (ranges, mut cloned_rule) = {
                let old = sheet.conditional.get(index).ok_or(EditError::IndexOutOfRange)?;
                let old_rule = old.rules.get(rule).ok_or(EditError::IndexOutOfRange)?;
                (old.ranges.clone(), old_rule.clone())
            };
            let source_priority = cloned_rule.priority;
            let new_priority = source_priority.checked_add(1).ok_or(EditError::IndexOutOfRange)?;
            for f in &mut sheet.conditional {
                for r in &mut f.rules {
                    if r.priority >= new_priority {
                        r.priority = r.priority.checked_add(1).ok_or(EditError::IndexOutOfRange)?;
                    }
                }
            }
            cloned_rule.priority = new_priority;
            let duplicate = crate::sheet::ConditionalFormat {
                source_index: None,
                ranges,
                rules: vec![cloned_rule],
            };
            sheet.conditional.push(duplicate);
            Ok(inverse)
        }
        SheetCommand::DeleteConditionalRule { slide, index, rule } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let (deleted_priority, single_rule) = {
                let old = sheet.conditional.get(index).ok_or(EditError::IndexOutOfRange)?;
                let r = old.rules.get(rule).ok_or(EditError::IndexOutOfRange)?;
                (r.priority, old.rules.len() == 1)
            };
            if single_rule {
                sheet.conditional.remove(index);
            } else {
                sheet.conditional[index].rules.remove(rule);
                sheet.conditional[index].source_index = None;
            }
            for f in &mut sheet.conditional {
                for r in &mut f.rules {
                    if r.priority > deleted_priority {
                        r.priority = r.priority.saturating_sub(1);
                    }
                }
            }
            Ok(inverse)
        }
        SheetCommand::MoveConditionalRule { slide, index, rule, up } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let mut order: Vec<_> = sheet.conditional.iter().enumerate()
                .flat_map(|(i, f)| f.rules.iter().enumerate().map(move |(j, r)| (r.priority, i, j))).collect();
            order.sort_by_key(|&(priority, _, _)| priority);
            let at = order.iter().position(|&(_, i, j)| i == index && j == rule).ok_or(EditError::IndexOutOfRange)?;
            let to = if up { at.checked_sub(1) } else { at.checked_add(1).filter(|&n| n < order.len()) }.ok_or(EditError::IndexOutOfRange)?;
            if order.len() > u32::MAX as usize { return Err(EditError::IndexOutOfRange); }
            order.swap(at, to);
            for (priority, (_, i, j)) in order.into_iter().enumerate() {
                sheet.conditional[i].rules[j].priority = priority as u32 + 1;
            }
            Ok(inverse)
        }
        SheetCommand::SetConditionalStop { slide, index, rule, stop } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let rule = sheet.conditional.get_mut(index).and_then(|f| f.rules.get_mut(rule)).ok_or(EditError::IndexOutOfRange)?;
            rule.stop = stop;
            Ok(inverse)
        }
        SheetCommand::ClearConditional { slide, ranges } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            let within = |r: &CellRange| ranges.iter().any(|t| r.first_row >= t.first_row && r.last_row <= t.last_row && r.first_column >= t.first_column && r.last_column <= t.last_column);
            let before = sheet.conditional.len();
            sheet.conditional.retain(|c| !c.ranges.iter().all(within));
            if sheet.conditional.len() == before {
                return Err(EditError::IndexOutOfRange);
            }
            Ok(inverse)
        }
        SheetCommand::SetTabColor { slide, color } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.tab_color = color;
            Ok(inverse)
        }
        SheetCommand::SetGridLines { slide, shown } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.grid_lines = shown;
            Ok(inverse)
        }
        SheetCommand::SetHeadings { slide, shown } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.headings = shown;
            Ok(inverse)
        }
        SheetCommand::SetZoom { slide, zoom } => {
            let sheet = sheet_zoom_mut(presentation, slide)?;
            let old = sheet.zoom;
            sheet.zoom = zoom;
            Ok(Command::Sheet(SheetCommand::SetZoom { slide, zoom: old }))
        }
        SheetCommand::SetPageBreakPreview { slide, shown } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.page_break_preview = shown;
            if shown {
                sheet.page_layout = false;
            }
            Ok(inverse)
        }
        SheetCommand::SetPageLayout { slide, shown } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.page_layout = shown;
            if shown {
                sheet.page_break_preview = false;
            }
            Ok(inverse)
        }
        SheetCommand::SetShowRuler { slide, shown } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.show_ruler = shown;
            Ok(inverse)
        }
        SheetCommand::SetShowFormulas { slide, shown } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.show_formulas = shown;
            Ok(inverse)
        }
        SheetCommand::SetShowZeros { slide, shown } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.show_zeros = shown;
            Ok(inverse)
        }
        SheetCommand::SetHideInk { slide, hidden } => {
            let inverse = snapshot(presentation);
            sheet_mut(presentation, slide)?.hide_ink = hidden;
            Ok(inverse)
        }
        SheetCommand::Freeze { slide, frozen } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.frozen = frozen.filter(|&(c, r)| c > 0 || r > 0);
            if sheet.frozen.is_some() {
                sheet.split = None;
            }
            Ok(inverse)
        }
        SheetCommand::Split { slide, split } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.split = split.filter(|&(c, r)| c > 0 || r > 0);
            if sheet.split.is_some() {
                sheet.frozen = None;
            }
            Ok(inverse)
        }
        SheetCommand::AddSheet { index, name, slide, shape } => add_sheet(presentation, index, name, slide, shape),
        SheetCommand::DeleteSheet { slide } => delete_sheet(presentation, slide),
        SheetCommand::RenameSheet { slide, name } => rename_sheet(presentation, slide, name),
        SheetCommand::Merge { slide, range, center } => merge(presentation, slide, range, center),
        SheetCommand::Unmerge { slide, range } => unmerge(presentation, slide, range),
        SheetCommand::AddCustomCellStyle { style } => {
            let inverse = snapshot(presentation);
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            if let Some(pos) = wb.custom_cell_styles.iter().position(|s| s.name.eq_ignore_ascii_case(&style.name)) {
                wb.custom_cell_styles[pos] = style;
            } else {
                wb.custom_cell_styles.push(style);
            }
            Ok(inverse)
        }
        SheetCommand::DeleteCustomCellStyle { name } => {
            let inverse = snapshot(presentation);
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb.custom_cell_styles.retain(|s| !s.name.eq_ignore_ascii_case(&name));
            Ok(inverse)
        }
        SheetCommand::SetDocProperties(props) => {
            let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
            let old = wb.properties.clone();
            if old == props {
                return Ok(Command::Group(Vec::new()));
            }
            let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            wb_mut.properties = props;
            Ok(Command::Sheet(SheetCommand::SetDocProperties(old)))
        }
        SheetCommand::MoveChart { from_slide, shape, target } => move_chart(presentation, from_slide, shape, target),
        SheetCommand::SetCellPhonetic { slide, row, column, phonetic } => {
            let inverse = snapshot(presentation);
            let sheet_index = BookView::new(presentation).index_of_slide(slide);
            let sheet = sheet_mut(presentation, slide)?;
            check_bounds(sheet, row, column)?;
            let cell = sheet.cell(row, column).cloned().ok_or(EditError::IndexOutOfRange)?;
            sheet.put_cell(row, column, Some(SheetCell {
                phonetic: phonetic.map(Into::into),
                ..cell
            }));
            if let Some(idx) = sheet_index {
                recalculate_cells(presentation, &[(idx, row, column)])?;
            }
            Ok(inverse)
        }
        SheetCommand::SetShowPhonetic { slide, show } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.show_phonetic = show;
            Ok(inverse)
        }
        SheetCommand::SetPhoneticProperties { slide, properties } => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, slide)?;
            sheet.phonetic_pr = properties;
            Ok(inverse)
        }
        SheetCommand::PlacePictureInCell { slide, shape, row, column } => {
            let inverse = snapshot(presentation);
            let (xs, ys) = {
                let sheet = sheet_mut(presentation, slide)?;
                check_bounds(sheet, row, column)?;
                if !sheet.cell_images.iter().any(|i| i.row == row && i.column == column) {
                    sheet.cell_images.push(crate::sheet::CellImage { row, column, index: 0 });
                }
                (sheet.column_offsets(), sheet.row_offsets())
            };
            if let Some(slide_obj) = presentation.slide_mut(slide) {
                slide_obj.shape_placements.insert(shape, crate::model::ObjectPlacement::MoveAndSize);
                if let Some(sh) = slide_obj.shapes.iter_mut().find(|s| s.id == shape) {
                    let col_idx = column as usize;
                    let row_idx = row as usize;
                    if col_idx + 1 < xs.len() && row_idx + 1 < ys.len() {
                        let (x0, y0) = (xs[col_idx], ys[row_idx]);
                        let (x1, y1) = (xs[col_idx + 1], ys[row_idx + 1]);
                        let (cw, ch) = ((x1 - x0).max(1), (y1 - y0).max(1));
                        let (w, h) = (sh.frame.width.max(1), sh.frame.height.max(1));
                        let scale = (cw as f64 / w as f64).min(ch as f64 / h as f64);
                        let (fw, fh) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
                        sh.frame = EmuRect::new(x0 + (cw - fw) / 2, y0 + (ch - fh) / 2, fw.max(1), fh.max(1));
                    }
                }
            }
            Ok(inverse)
        }
        SheetCommand::PlacePictureOverCells { slide, shape, row, column } => {
            let inverse = snapshot(presentation);
            {
                let sheet = sheet_mut(presentation, slide)?;
                sheet.cell_images.retain(|i| !(i.row == row && i.column == column));
            }
            if let Some(slide_obj) = presentation.slide_mut(slide) {
                slide_obj.shape_placements.insert(shape, crate::model::ObjectPlacement::MoveOnly);
            }
            Ok(inverse)
        }
    }
}

/// The worksheet on `slide`, to change.
pub fn sheet_mut(presentation: &mut Presentation, slide: SlideId) -> Result<&mut Sheet, EditError> {
    let s = presentation.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
    let Some(shape) = s.shapes.iter_mut().find(|sh| matches!(sh.kind, ShapeKind::Sheet(_))) else {
        return Err(EditError::IndexOutOfRange);
    };
    let ShapeKind::Sheet(sheet) = &mut shape.kind else { unreachable!() };
    let sheet = Arc::make_mut(sheet);
    sheet.cells.bump_content_id();
    Ok(sheet)
}

/// The worksheet on `slide`, to change only its zoom (leaving `cells.content_id()` unchanged).
fn sheet_zoom_mut(presentation: &mut Presentation, slide: SlideId) -> Result<&mut Sheet, EditError> {
    let s = presentation.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
    let Some(shape) = s.shapes.iter_mut().find(|sh| matches!(sh.kind, ShapeKind::Sheet(_))) else {
        return Err(EditError::IndexOutOfRange);
    };
    let ShapeKind::Sheet(sheet) = &mut shape.kind else { unreachable!() };
    Ok(Arc::make_mut(sheet))
}

/// The worksheet on `slide`, to read.
pub(crate) fn sheet_of(presentation: &Presentation, slide: SlideId) -> Result<&Sheet, EditError> {
    let s = presentation.slide(slide).ok_or(EditError::NoSuchSlide(slide))?;
    let Some(shape) = s.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Sheet(_))) else {
        return Err(EditError::IndexOutOfRange);
    };
    let ShapeKind::Sheet(sheet) = &shape.kind else { unreachable!() };
    Ok(sheet.as_ref())
}

/// The text a value shows in a cell of `style` (`#` for a value the
/// format cannot show, which Excel repeats across the cell).
pub fn display_text(value: &CellValue, style: &CellStyle, date1904: bool) -> String {
    let f = ibis_calc::numfmt::format(value, &style.number_format, date1904);
    if f.overflow { "########".into() } else { f.text }
}

fn date1904(presentation: &Presentation) -> bool {
    presentation.workbook.as_ref().is_some_and(|w| w.date1904)
}

fn calc_mode(presentation: &Presentation) -> CalcMode {
    presentation.workbook.as_ref().map(|w| w.calc_mode).unwrap_or_default()
}

fn invalidate_calc_index(presentation: &Presentation) {
    if let Some(w) = presentation.workbook.as_ref() {
        w.calc_index.invalidate();
    }
}

impl Sheet {
    /// `f` applied to every formula of the sheet that is not a cell's: those
    /// of its conditional formats and validations.
    pub fn map_rule_formulas(&mut self, f: &dyn Fn(&str) -> String) {
        for cf in self.conditional.iter_mut() {
            for rule in cf.rules.iter_mut() {
                match &mut rule.kind {
                    crate::sheet::CfKind::CellIs { formulas, .. } => formulas.iter_mut().for_each(|x| *x = f(x)),
                    crate::sheet::CfKind::Expression(x) => *x = f(x),
                    crate::sheet::CfKind::TimePeriod { formula, .. } => *formula = f(formula),
                    _ => {}
                }
            }
        }
        for v in self.validations.iter_mut() {
            for x in [&mut v.formula1, &mut v.formula2].into_iter().flatten() {
                *x = f(x);
            }
        }
    }

    /// Put `cell` at (row, column), or take the cell there away.
    pub fn put_cell(&mut self, row: u32, column: u32, cell: Option<SheetCell>) {
        match (self.cells.binary_search_by_key(&(row, column), |c| (c.row, c.column)), cell) {
            (Ok(i), Some(c)) => self.cells[i] = c,
            (Ok(i), None) => {
                self.cells.remove(i);
            }
            (Err(i), Some(c)) => self.cells.insert(i, c),
            (Err(_), None) => {}
        }
    }

    /// The index in [`Sheet::styles`] of `style`, added if new.
    pub fn style_index(&mut self, style: CellStyle) -> u32 {
        match self.styles.iter().position(|s| *s == style) {
            Some(i) => i as u32,
            None => {
                self.styles.push(style);
                (self.styles.len() - 1) as u32
            }
        }
    }
}

/// The format a new cell gets: the workbook's default (`cellXfs` 0).
fn default_style(presentation: &Presentation) -> CellStyle {
    presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default()
}

/// Remembers each cell's state (and the list of styles) before a command
/// first changes it.
#[derive(Default)]
struct Undo {
    patches: Vec<SheetPatch>,
    /// The cells in `patches` (by sheet), so that a cell is remembered once without looking through them all (a column of a million cells).
    seen: std::collections::HashSet<(SlideId, u32, u32)>,
}

impl Undo {
    fn patch(&mut self, slide: SlideId) -> &mut SheetPatch {
        match self.patches.iter().position(|p| p.slide == slide) {
            Some(i) => &mut self.patches[i],
            None => {
                self.patches.push(SheetPatch { slide, cells: Vec::new(), styles: None, images: Vec::new() });
                self.patches.last_mut().unwrap()
            }
        }
    }

    /// Remember the picture of a cell before it changes.
    fn remember_image(&mut self, slide: SlideId, sheet: &Sheet, at: (u32, u32)) {
        let patch = self.patch(slide);
        if !patch.images.iter().any(|(k, _)| *k == at) {
            patch.images.push((at, sheet.web_images.get(&at).cloned()));
        }
    }

    fn remember(&mut self, slide: SlideId, sheet: &Sheet, row: u32, column: u32) {
        if self.seen.insert((slide, row, column)) {
            self.patch(slide).cells.push(CellSlot { row, column, cell: sheet.cell(row, column).cloned() });
        }
    }

    /// Keep the styles to put back, if the command added any.
    fn styles_if_grown(&mut self, slide: SlideId, before: Vec<CellStyle>, sheet: &Sheet) {
        if sheet.styles.len() != before.len() {
            self.patch(slide).styles = Some(before);
        }
    }

    fn command(self) -> Command {
        Command::Sheet(SheetCommand::Restore(self.patches))
    }
}

fn check_bounds(sheet: &Sheet, row: u32, column: u32) -> Result<(), EditError> {
    let _ = sheet;
    if row >= 1_048_576 || column >= 16_384 {
        return Err(EditError::IndexOutOfRange);
    }
    Ok(())
}

fn set_cells(presentation: &mut Presentation, slide: SlideId, edits: Vec<CellEdit>) -> Result<Command, EditError> {
    let default = default_style(presentation);
    let d1904 = date1904(presentation);
    let full_precision = presentation.workbook.as_deref().map(|w| w.full_precision).unwrap_or(true);
    let mut undo = Undo::default();
    // With dynamic arrays about, cells beyond the edited ones change (the
    // spilled values): undone from a picture of the whole book.
    let spilling = any_spills(presentation) || edits.iter().any(|e| matches!(e.input, CellInput::Formula(_)));
    let before = spilling.then(|| snapshot(presentation));
    let mut spill_changed: Vec<(u32, u32)> = Vec::new();
    let mut formula_changed = false;
    let mut resized_grid = false;
    {
        let sheet = sheet_mut(presentation, slide)?;
        for e in &edits {
            check_bounds(sheet, e.row, e.column)?;
            // A data table is changed as a whole (Excel: 「テーブルの一部を変更することはできません」).
            if sheet.data_tables.iter().any(|t| (t.range.first_row..=t.range.last_row).contains(&e.row) && (t.range.first_column..=t.range.last_column).contains(&e.column)) {
                return Err(EditError::DataTablePart);
            }
        }
        for e in &edits {
            if e.row as usize >= sheet.rows.len() {
                let def_h = if sheet.default_row > 0 { sheet.default_row } else { 20 * crate::EMU_PER_POINT };
                sheet.rows.resize(e.row as usize + 1, def_h);
                resized_grid = true;
            }
            if e.column as usize >= sheet.columns.len() {
                let def_w = if sheet.default_column > 0 { sheet.default_column } else { 54 * crate::EMU_PER_POINT };
                sheet.columns.resize(e.column as usize + 1, def_w);
                resized_grid = true;
            }
        }
        let styles_before = sheet.styles.clone();
        for e in &edits {
            undo.remember(slide, sheet, e.row, e.column);
            let existing = sheet.cell(e.row, e.column).cloned();
            if existing.as_ref().is_some_and(|c| c.formula.is_some()) || matches!(e.input, CellInput::Formula(_)) {
                formula_changed = true;
            }
            let style = match &existing {
                Some(c) => c.style,
                None => sheet.style_index(default.clone()),
            };
            let cell_style = sheet.styles[style as usize].clone();
            let blank = |value: CellValue, formula: Option<String>| SheetCell { runs: None, phonetic: None,
                row: e.row,
                column: e.column,
                text: display_text(&value, &cell_style, d1904),
                kind: CellKind::of(&value),
                value,
                formula,
                style,
            };
            let new = match &e.input {
                CellInput::Value(v) => {
                    let val = match v {
                        CellValue::Number(n) if !full_precision => {
                            let rounded = ibis_calc::numfmt::round_to_display_precision(*n, &cell_style.number_format, d1904);
                            CellValue::Number(rounded)
                        }
                        _ => v.clone(),
                    };
                    Some(blank(val, None))
                }
                // The value is calculated below.
                CellInput::Formula(f) => Some(blank(CellValue::Empty, Some(f.clone()))),
                // A cell left with the default format goes away.
                CellInput::Clear => (existing.is_some() && cell_style != default).then(|| blank(CellValue::Empty, None)),
            };
            sheet.put_cell(e.row, e.column, new);
            // A formula typed is a dynamic array formula (as in Excel 365);
            // a value typed over one takes back what it spilled.
            match &e.input {
                CellInput::Formula(_) => {
                    sheet.spills.entry((e.row, e.column)).or_insert(CellRange { first_row: e.row, first_column: e.column, last_row: e.row, last_column: e.column });
                }
                _ => {
                    if let Some(old) = sheet.spills.remove(&(e.row, e.column)) {
                        spill_changed.extend(clear_spilled(sheet, (e.row, e.column), old, None, d1904));
                    }
                }
            }
        }
        undo.styles_if_grown(slide, styles_before, sheet);
    }
    let mut changed: Vec<(u32, u32)> = edits.iter().map(|e| (e.row, e.column)).collect();
    changed.extend(spill_changed);
    // A dynamic array whose area holds an edited cell is calculated again
    // (to spill, or to show #SPILL!). Something typed over its spilled
    // values blocks it: they go (but what was typed), and it is marked
    // blocked, so the calculation sees what was typed as in the way.
    if let Ok(sheet) = sheet_mut(presentation, slide) {
        let inside = |r: &CellRange, row: u32, column: u32| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column);
        let anchors: Vec<((u32, u32), CellRange)> = sheet
            .spills
            .iter()
            .filter(|(a, r)| {
                let blocked_1x1 = r.first_row == r.last_row
                    && r.first_column == r.last_column
                    && sheet.cell(a.0, a.1).is_some_and(|c| c.value == CellValue::Error(crate::sheet::CellError::Spill));
                blocked_1x1 || changed.iter().any(|&(row, column)| (row, column) != **a && inside(r, row, column))
            })
            .map(|(a, r)| (*a, *r))
            .collect();
        let typed: Vec<(u32, u32)> = edits.iter().filter(|e| !matches!(e.input, CellInput::Clear)).map(|e| (e.row, e.column)).collect();
        for (a, r) in anchors {
            if typed.iter().any(|&(row, column)| (row, column) != a && inside(&r, row, column)) {
                let keep: Vec<(u32, u32)> = typed.clone();
                for row in r.first_row..=r.last_row {
                    for column in r.first_column..=r.last_column {
                        if (row, column) == a || keep.contains(&(row, column)) {
                            continue;
                        }
                        let one = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
                        changed.extend(clear_spilled(sheet, a, one, None, d1904));
                    }
                }
                if let Some(cell) = sheet.cell(a.0, a.1).cloned() {
                    let v = CellValue::Error(crate::sheet::CellError::Spill);
                    sheet.put_cell(a.0, a.1, Some(SheetCell { value: v, ..cell }));
                }
            }
            changed.push(a);
        }
    }
    if formula_changed {
        invalidate_calc_index(presentation);
    }
    recalculate(presentation, slide, &changed, &mut undo)?;
    if resized_grid {
        fit_frame(presentation, slide);
    }
    match before {
        Some(b) => Ok(b),
        None => Ok(undo.command()),
    }
}

/// Whether any sheet of the book has a dynamic array formula.
fn any_spills(presentation: &Presentation) -> bool {
    presentation.slides.iter().filter_map(crate::sheet_calc::sheet_of_slide).any(|s| !s.spills.is_empty())
}

/// Take back the values a dynamic array at `anchor` spilled into `old`
/// (unless it was blocked, when they were never its), except the cells
/// `keep` it will fill again; the cells emptied.
fn clear_spilled(sheet: &mut Sheet, anchor: (u32, u32), old: CellRange, keep: Option<CellRange>, d1904: bool) -> Vec<(u32, u32)> {
    let blocked = sheet.cell(anchor.0, anchor.1).is_some_and(|c| c.value == CellValue::Error(crate::sheet::CellError::Spill));
    let _ = d1904;
    if blocked {
        return Vec::new();
    }
    let mut out = Vec::new();
    // A spill of a million cells: taking them away one by one shifts the rest of the (sorted) cells each time,
    // so the cells to remove are collected and taken out in one pass (#44).
    let mut removed = std::collections::HashSet::new();
    for row in old.first_row..=old.last_row {
        for column in old.first_column..=old.last_column {
            if (row, column) == anchor || keep.is_some_and(|k| (k.first_row..=k.last_row).contains(&row) && (k.first_column..=k.last_column).contains(&column)) {
                continue;
            }
            let Some(cell) = sheet.cell(row, column).cloned() else { continue };
            if cell.formula.is_some() {
                continue;
            }
            if cell.style != 0 {
                // The cell stays where it is, so this does not move the others.
                let emptied = SheetCell { value: CellValue::Empty, text: String::new(), kind: CellKind::Empty, phonetic: None, runs: None, ..cell };
                sheet.put_cell(row, column, Some(emptied));
            } else {
                removed.insert((row, column));
            }
            out.push((row, column));
        }
    }
    if !removed.is_empty() {
        sheet.cells.retain(|c| !removed.contains(&(c.row, c.column)));
    }
    out
}

/// A dynamic array formula's new result at `anchor`: its values spilled
/// into the cells beside it (from `spill`, or the one `value`), unless a
/// cell there holds something else (or the sheet ends), when the formula
/// shows `#SPILL!`. Returns the value the anchor shows and the other cells
/// whose values changed.
fn spill(sheet: &mut Sheet, anchor: (u32, u32), value: CellValue, spill: Option<&(usize, usize, Vec<CellValue>)>, d1904: bool) -> (CellValue, Vec<(u32, u32)>) {
    let Some(old) = sheet.spills.get(&anchor).copied() else { return (value, Vec::new()) };
    let was_blocked = sheet.cell(anchor.0, anchor.1).is_some_and(|c| c.value == CellValue::Error(crate::sheet::CellError::Spill));
    let (rows, columns, values) = match spill {
        Some((r, c, v)) => (*r as u32, *c as u32, v.clone()),
        None => (1, 1, vec![value.clone()]),
    };
    let range = CellRange { first_row: anchor.0, first_column: anchor.1, last_row: anchor.0 + rows - 1, last_column: anchor.1 + columns - 1 };
    let in_old = |row: u32, column: u32| !was_blocked && (old.first_row..=old.last_row).contains(&row) && (old.first_column..=old.last_column).contains(&column);
    let off_sheet = range.last_row >= 1_048_576 || range.last_column >= 16_384;
    let blocked = off_sheet
        || (range.first_row..=range.last_row).any(|row| {
            (range.first_column..=range.last_column).any(|column| {
                (row, column) != anchor && !in_old(row, column) && sheet.cell(row, column).is_some_and(|c| c.formula.is_some() || !c.value.is_empty())
            })
        });
    let mut changed = clear_spilled(sheet, anchor, old, (!blocked).then_some(range), d1904);
    if blocked {
        sheet.spills.insert(anchor, range);
        return (CellValue::Error(crate::sheet::CellError::Spill), changed);
    }
    if range.last_row as usize >= sheet.rows.len() {
        let def_h = if sheet.default_row > 0 { sheet.default_row } else { 20 * crate::EMU_PER_POINT };
        sheet.rows.resize(range.last_row as usize + 1, def_h);
    }
    if range.last_column as usize >= sheet.columns.len() {
        let def_w = if sheet.default_column > 0 { sheet.default_column } else { 54 * crate::EMU_PER_POINT };
        sheet.columns.resize(range.last_column as usize + 1, def_w);
    }
    for (i, v) in values.iter().enumerate() {
        let (row, column) = (anchor.0 + i as u32 / columns, anchor.1 + i as u32 % columns);
        if (row, column) == anchor {
            continue;
        }
        let existing = sheet.cell(row, column).cloned();
        if existing.as_ref().is_some_and(|c| c.value == *v) {
            continue;
        }
        let style = existing.as_ref().map_or(0, |c| c.style);
        let text = display_text(v, &sheet.styles[style as usize], d1904);
        sheet.put_cell(row, column, Some(SheetCell { runs: None, phonetic: None, row, column, value: v.clone(), text, kind: CellKind::of(v), formula: None, style }));
        changed.push((row, column));
    }
    sheet.spills.insert(anchor, range);
    (values[0].clone(), changed)
}

/// Keep the picture IMAGE asked for in the cell (or none), changing the sheet only if it differs.
fn set_web_image(presentation: &mut Presentation, slide: SlideId, at: (u32, u32), image: Option<&ibis_calc::eval::ImageSpec>) -> Result<(), EditError> {
    let same = presentation.slide(slide).and_then(crate::sheet_calc::sheet_of_slide).is_none_or(|s| s.web_images.get(&at) == image);
    if same {
        return Ok(());
    }
    let sheet = sheet_mut(presentation, slide)?;
    match image {
        Some(i) => sheet.web_images.insert(at, i.clone()),
        None => sheet.web_images.remove(&at),
    };
    Ok(())
}

/// Calculate again what depends on the `changed` cells of `slide`'s sheet,
/// and the changed cells' own formulas, remembering what changes in `undo`.
fn recalculate(presentation: &mut Presentation, slide: SlideId, changed: &[(u32, u32)], undo: &mut Undo) -> Result<(), EditError> {
    let mode = calc_mode(presentation);
    let d1904 = date1904(presentation);
    let (results, slides) = {
        let view = crate::sheet_calc::BookView::new(presentation);
        let Some(index) = view.index_of_slide(slide) else { return Ok(()) };
        let cells: Vec<_> = changed.iter().map(|&(r, c)| (index, r, c)).collect();
        let results = view.recalculate(&cells);
        let count = results.iter().map(|r| r.cell.0 + 1).max().unwrap_or(0);
        let slides: Vec<Option<SlideId>> = (0..count).map(|i| view.slide_of(i)).collect();
        (results, slides)
    };
    let mut follow: Vec<(usize, u32, u32)> = Vec::new();
    for r in results {
        let (sheet_index, row, column) = r.cell;
        let Some(Some(target)) = slides.get(sheet_index).copied() else { continue };
        let edited = target == slide && changed.contains(&(row, column));
        // In manual calculation mode, do not update dependent formula cells.
        if mode == CalcMode::Manual && !edited {
            continue;
        }
        // The picture IMAGE asks for (kept apart from the value, which stays as it was).
        if presentation.slide(target).and_then(crate::sheet_calc::sheet_of_slide).is_some_and(|s| s.web_images.get(&(row, column)) != r.image.as_ref()) {
            let sheet = sheet_mut(presentation, target)?;
            undo.remember_image(target, sheet, (row, column));
            set_web_image(presentation, target, (row, column), r.image.as_ref())?;
        }
        // A formula the engine cannot calculate keeps the value Excel gave
        // it; a formula just typed has no such value.
        if r.unsupported && !edited {
            continue;
        }
        let sheet = sheet_mut(presentation, target)?;
        let Some(cell) = sheet.cell(row, column).cloned() else { continue };
        let (value, spilled) = spill(sheet, (row, column), r.value.clone(), r.spill.as_ref(), d1904);
        follow.extend(spilled.into_iter().map(|(a, b)| (sheet_index, a, b)));
        if cell.value == value && !edited {
            continue;
        }
        undo.remember(target, sheet, row, column);
        let text = display_text(&value, &sheet.styles[cell.style as usize], d1904);
        sheet.put_cell(row, column, Some(SheetCell { kind: CellKind::of(&value), text, value, ..cell }));
    }
    if mode == CalcMode::Auto {
        refresh_data_tables(presentation, slide, undo)?;
    }
    // What the spilled cells feed.
    if mode != CalcMode::Manual {
        recalculate_cells(presentation, &follow)?;
    }
    Ok(())
}

/// The data tables of `slide`'s sheet calculated again after an edit.
fn refresh_data_tables(presentation: &mut Presentation, slide: SlideId, undo: &mut Undo) -> Result<(), EditError> {
    let d1904 = date1904(presentation);
    let default = default_style(presentation);
    let tables = match presentation.slide(slide).and_then(crate::sheet_calc::sheet_of_slide) {
        Some(s) if !s.data_tables.is_empty() => s.data_tables.clone(),
        _ => return Ok(()),
    };
    for t in &tables {
        let values = crate::data_table::values(presentation, slide, t);
        let sheet = sheet_mut(presentation, slide)?;
        for (row, column, value) in values {
            let cell = sheet.cell(row, column).cloned();
            if cell.as_ref().is_some_and(|c| c.value == value) {
                continue;
            }
            undo.remember(slide, sheet, row, column);
            let style = match &cell {
                Some(c) => c.style,
                None => sheet.style_index(default.clone()),
            };
            let text = display_text(&value, &sheet.styles[style as usize], d1904);
            sheet.put_cell(row, column, Some(SheetCell { runs: None, phonetic: None, row, column, value: value.clone(), text, kind: CellKind::of(&value), formula: None, style }));
        }
    }
    Ok(())
}

fn set_format(presentation: &mut Presentation, slide: SlideId, ranges: Vec<CellRange>, change: StyleChange) -> Result<Command, EditError> {
    let default = default_style(presentation);
    let d1904 = date1904(presentation);
    let full_precision = presentation.workbook.as_deref().map(|w| w.full_precision).unwrap_or(true);
    let mut undo = Undo::default();
    let sheet = sheet_mut(presentation, slide)?;
    let mut resized_grid = false;
    for range in &ranges {
        check_bounds(sheet, range.last_row, range.last_column)?;
        if range.last_row as usize >= sheet.rows.len() {
            let def_h = if sheet.default_row > 0 { sheet.default_row } else { 20 * crate::EMU_PER_POINT };
            sheet.rows.resize(range.last_row as usize + 1, def_h);
            resized_grid = true;
        }
        if range.last_column as usize >= sheet.columns.len() {
            let def_w = if sheet.default_column > 0 { sheet.default_column } else { 54 * crate::EMU_PER_POINT };
            sheet.columns.resize(range.last_column as usize + 1, def_w);
            resized_grid = true;
        }
    }
    let styles_before = sheet.styles.clone();
    // The style each style of a cell comes to (None: no cell), worked out once: a column is a million cells of a few styles.
    let mut changed_style: std::collections::HashMap<Option<u32>, u32> = std::collections::HashMap::new();
    // The cells that were there and take another style (by place in `cells`, and the style), and the cells that were not there (put in after the loop: inserting each shifts every cell after it).
    let mut changes: Vec<(usize, u32)> = Vec::new();
    let mut added: Vec<SheetCell> = Vec::new();
    for range in &ranges {
        for row in range.first_row..=range.last_row {
            for column in range.first_column..=range.last_column {
                let at = sheet.cells.binary_search_by_key(&(row, column), |c| (c.row, c.column));
                let old = at.ok().map(|i| sheet.cells[i].style);
                let index = match changed_style.get(&old) {
                    Some(&index) => index,
                    None => {
                        let mut style = match old {
                            Some(s) => sheet.styles[s as usize].clone(),
                            None => default.clone(),
                        };
                        change.apply(&mut style);
                        let index = sheet.style_index(style);
                        changed_style.insert(old, index);
                        index
                    }
                };
                if old == Some(index) {
                    continue;
                }
                // A cell in two of the ranges: the first time did it (within one range no cell comes twice).
                if ranges.len() > 1 && !undo.seen.insert((slide, row, column)) {
                    continue;
                }
                undo.patch(slide).cells.push(CellSlot { row, column, cell: at.ok().map(|i| sheet.cells[i].clone()) });
                match at {
                    Ok(i) => changes.push((i, index)),
                    Err(_) => added.push(SheetCell { runs: None, phonetic: None, row, column, value: CellValue::Empty, text: String::new(), kind: CellKind::Empty, formula: None, style: index }),
                }
            }
        }
    }
    // What the cells show, by their new formats (the most of the time of a column: on several threads when there are many).
    let shown: Vec<(Option<CellValue>, String)> = par_map(&changes, |&(i, index)| {
        let c = &sheet.cells[i];
        let style = &sheet.styles[index as usize];
        let rounded = match c.value {
            CellValue::Number(n) if !full_precision => Some(CellValue::Number(ibis_calc::numfmt::round_to_display_precision(n, &style.number_format, d1904))),
            _ => None,
        };
        let text = display_text(rounded.as_ref().unwrap_or(&c.value), style, d1904);
        (rounded, text)
    });
    for (&(i, index), (rounded, text)) in changes.iter().zip(shown) {
        let c = &mut sheet.cells[i];
        if let Some(value) = rounded {
            c.value = value;
        }
        c.text = text;
        c.style = index;
    }
    if added.len() < 32 {
        for cell in added {
            let (row, column) = (cell.row, cell.column);
            sheet.put_cell(row, column, Some(cell));
        }
    } else {
        sheet.cells.extend(added);
        sheet.cells.sort_by_key(|c| (c.row, c.column));
    }
    undo.styles_if_grown(slide, styles_before, sheet);
    if resized_grid {
        fit_frame(presentation, slide);
    }
    Ok(undo.command())
}

/// `f` of each of `items`, in order: on several threads when there are many (a column of a million cells), one on the web.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(target_family = "wasm")]
    return items.iter().map(f).collect();
    #[cfg(not(target_family = "wasm"))]
    {
        const PER_THREAD: usize = 20_000;
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1)).min(items.len() / PER_THREAD).max(1);
        if threads <= 1 {
            return items.iter().map(f).collect();
        }
        let size = items.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let workers: Vec<_> = items.chunks(size).map(|chunk| scope.spawn(|| chunk.iter().map(&f).collect::<Vec<R>>())).collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
        })
    }
}

fn restore(presentation: &mut Presentation, patches: Vec<SheetPatch>) -> Result<Command, EditError> {
    // Check first, so that a failure changes nothing.
    for p in &patches {
        sheet_mut(presentation, p.slide)?;
    }
    let mut inverse = Vec::with_capacity(patches.len());
    for p in patches.into_iter().rev() {
        let sheet = sheet_mut(presentation, p.slide)?;
        let mut cells = Vec::with_capacity(p.cells.len());
        for slot in p.cells.into_iter().rev() {
            cells.push(CellSlot { row: slot.row, column: slot.column, cell: sheet.cell(slot.row, slot.column).cloned() });
            sheet.put_cell(slot.row, slot.column, slot.cell);
        }
        let styles = p.styles.map(|s| std::mem::replace(&mut sheet.styles, s));
        let mut images = Vec::with_capacity(p.images.len());
        for (at, image) in p.images {
            images.push((at, sheet.web_images.get(&at).cloned()));
            match image {
                Some(i) => sheet.web_images.insert(at, i),
                None => sheet.web_images.remove(&at),
            };
        }
        cells.reverse();
        inverse.push(SheetPatch { slide: p.slide, cells, styles, images });
    }
    inverse.reverse();
    Ok(Command::Sheet(SheetCommand::Restore(inverse)))
}

/// The workbook's slides (its sheets, cheap to copy: the cells are
/// shared) and the workbook, to put back.
fn snapshot(presentation: &Presentation) -> Command {
    Command::Sheet(SheetCommand::RestoreAll { slides: presentation.slides.clone(), workbook: presentation.workbook.clone() })
}

fn restore_all(presentation: &mut Presentation, slides: Vec<Slide>, workbook: Option<Arc<Workbook>>) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    presentation.slides = slides;
    presentation.workbook = workbook;
    invalidate_calc_index(presentation);
    Ok(inverse)
}

fn workbook_index(presentation: &Presentation, slide: SlideId) -> Result<usize, EditError> {
    presentation.workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide))).ok_or(EditError::NoSuchSlide(slide))
}

fn check_workbook_structure_not_locked(presentation: &Presentation) -> Result<(), EditError> {
    if presentation.workbook.as_ref().is_some_and(|w| w.protection.as_ref().is_some_and(|p| p.lock_structure)) {
        return Err(EditError::WorkbookProtected);
    }
    Ok(())
}

fn check_workbook_windows_not_locked(presentation: &Presentation) -> Result<(), EditError> {
    if presentation.is_workbook_windows_locked() {
        return Err(EditError::WorkbookProtected);
    }
    Ok(())
}

fn apply_set_window_hidden(presentation: &mut Presentation, hidden: bool) -> Result<Command, EditError> {
    check_workbook_windows_not_locked(presentation)?;
    let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
    if wb.window_hidden == hidden {
        return Ok(Command::Group(Vec::new()));
    }
    let inverse = snapshot(presentation);
    let wb_mut = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    wb_mut.window_hidden = hidden;
    Ok(inverse)
}

fn structural(presentation: &mut Presentation, edit: StructureEdit) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let changed = apply_structure(presentation, &edit);
    Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?).structure.push(edit);
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &changed)?;
    Ok(inverse)
}

/// An empty sheet the size of the workbook's first sheet's grid.
fn empty_sheet(presentation: &Presentation) -> Sheet {
    let first = presentation.slides.iter().find_map(|s| crate::sheet_calc::sheet_of_slide(s));
    let (row, column) = first.map_or((EMPTY_ROW, EMPTY_COLUMN), |s| (if s.default_row > 0 { s.default_row } else { EMPTY_ROW }, if s.default_column > 0 { s.default_column } else { EMPTY_COLUMN }));
    let (rows, columns) = first.map_or((100, 26), |s| (s.rows.len().max(100), s.columns.len().max(26)));
    Sheet { columns: vec![column; columns], rows: vec![row; rows], grid_lines: true, headings: true, show_ruler: true, default_row: row, default_column: column, ..Sheet::default() }
}

/// Excel's defaults for a new sheet: 18 pt rows, 64 pt columns (MS P Gothic 11 pt).
const EMPTY_ROW: Emu = 18 * crate::geometry::EMU_PER_POINT;
const EMPTY_COLUMN: Emu = 64 * crate::geometry::EMU_PER_POINT;

fn add_sheet(presentation: &mut Presentation, index: usize, name: String, slide: SlideId, shape: ShapeId) -> Result<Command, EditError> {
    check_workbook_structure_not_locked(presentation)?;
    let count = presentation.workbook.as_ref().ok_or(EditError::IndexOutOfRange)?.sheets.len();
    if index > count || !crate::sheet_ops::valid_sheet_name(presentation, &name, None) {
        return Err(EditError::IndexOutOfRange);
    }
    if presentation.slide(slide).is_some() {
        return Err(EditError::DuplicateId);
    }
    let content = Arc::new(empty_sheet(presentation));
    structural(presentation, StructureEdit::AddSheet { sheet: index, name, slide, shape, content })
}

fn delete_sheet(presentation: &mut Presentation, slide: SlideId) -> Result<Command, EditError> {
    check_workbook_structure_not_locked(presentation)?;
    let index = workbook_index(presentation, slide)?;
    let shown = presentation.workbook.as_ref().unwrap().sheets.iter().filter(|s| s.slide.is_some()).count();
    if shown <= 1 {
        return Err(EditError::LastSlide);
    }
    structural(presentation, StructureEdit::DeleteSheet { sheet: index })
}

fn move_chart(
    presentation: &mut Presentation,
    from_slide: SlideId,
    shape_id: ShapeId,
    target: ChartMoveTarget,
) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);

    // 1. Verify shape exists on from_slide and is a chart
    let from = presentation.slide(from_slide).ok_or(EditError::NoSuchSlide(from_slide))?;
    let shape_idx = from.shapes.iter().position(|s| s.id == shape_id).ok_or(EditError::NoSuchShape(shape_id))?;
    if !matches!(from.shapes[shape_idx].kind, ShapeKind::Chart(_)) {
        return Err(EditError::NotApplicable(shape_id));
    }

    match target {
        ChartMoveTarget::ExistingSheet { slide: to_slide } => {
            if to_slide == from_slide {
                return Err(EditError::IndexOutOfRange);
            }
            if presentation.slide(to_slide).is_none() {
                return Err(EditError::NoSuchSlide(to_slide));
            }

            // Extract shape and its metadata from from_slide
            let from_mut = presentation.slide_mut(from_slide).unwrap();
            let shape = from_mut.shapes.remove(shape_idx);
            let name = from_mut.shape_names.remove(&shape_id);
            let alt = from_mut.shape_alt_texts.remove(&shape_id);
            let placement = from_mut.shape_placements.remove(&shape_id);
            let print = from_mut.shape_prints.remove(&shape_id);
            let aspect = from_mut.aspect_locked.remove(&shape_id);
            let hidden = from_mut.hidden_shapes.remove(&shape_id);
            let attached = from_mut.attached.remove(&shape_id);

            // Also move any attached shapes (e.g. shapes drawn over chart)
            let mut attached_shapes = Vec::new();
            if let Some(att_ids) = &attached {
                for att_id in att_ids {
                    if let Some(pos) = from_mut.shapes.iter().position(|s| s.id == *att_id) {
                        attached_shapes.push(from_mut.shapes.remove(pos));
                    }
                }
            }

            // Insert into to_slide
            let to_mut = presentation.slide_mut(to_slide).unwrap();
            to_mut.shapes.push(shape);
            for att in attached_shapes {
                to_mut.shapes.push(att);
            }
            if let Some(n) = name { to_mut.shape_names.insert(shape_id, n); }
            if let Some(a) = alt { to_mut.shape_alt_texts.insert(shape_id, a); }
            if let Some(p) = placement { to_mut.shape_placements.insert(shape_id, p); }
            if let Some(pr) = print { to_mut.shape_prints.insert(shape_id, pr); }
            if aspect { to_mut.aspect_locked.insert(shape_id); }
            if hidden { to_mut.hidden_shapes.insert(shape_id); }
            if let Some(att) = attached { to_mut.attached.insert(shape_id, att); }
        }
        ChartMoveTarget::NewSheet { name, slide: new_slide, sheet_shape } => {
            check_workbook_structure_not_locked(presentation)?;
            if !crate::sheet_ops::valid_sheet_name(presentation, &name, None) {
                return Err(EditError::IndexOutOfRange);
            }
            if presentation.slide(new_slide).is_some() {
                return Err(EditError::DuplicateId);
            }

            // Extract shape and its metadata from from_slide
            let from_mut = presentation.slide_mut(from_slide).unwrap();
            let shape = from_mut.shapes.remove(shape_idx);
            let shape_name = from_mut.shape_names.remove(&shape_id);
            let alt = from_mut.shape_alt_texts.remove(&shape_id);
            let placement = from_mut.shape_placements.remove(&shape_id);
            let print = from_mut.shape_prints.remove(&shape_id);
            let aspect = from_mut.aspect_locked.remove(&shape_id);
            let hidden = from_mut.hidden_shapes.remove(&shape_id);
            let attached = from_mut.attached.remove(&shape_id);

            let mut attached_shapes = Vec::new();
            if let Some(att_ids) = &attached {
                for att_id in att_ids {
                    if let Some(pos) = from_mut.shapes.iter().position(|s| s.id == *att_id) {
                        attached_shapes.push(from_mut.shapes.remove(pos));
                    }
                }
            }

            // Create new slide
            let empty_content = Arc::new(empty_sheet(presentation));
            let grid_w = empty_content.column_offsets().last().copied().unwrap_or(0);
            let grid_h = empty_content.row_offsets().last().copied().unwrap_or(0);
            let mut new_slide_obj = Slide::new(new_slide);
            new_slide_obj.name = Some(name.clone());
            new_slide_obj.shapes.push(Shape {
                id: sheet_shape,
                frame: EmuRect::new(0, 0, grid_w, grid_h),
                rotation: 0,
                flip_h: false,
                flip_v: false,
                kind: ShapeKind::Sheet(empty_content.clone()),
                floating: None,
            });
            new_slide_obj.size = Some(EmuSize::new(grid_w.max(1), grid_h.max(1)));

            // Add chart shape to new slide
            new_slide_obj.shapes.push(shape);
            for att in attached_shapes {
                new_slide_obj.shapes.push(att);
            }
            if let Some(n) = shape_name { new_slide_obj.shape_names.insert(shape_id, n); }
            if let Some(a) = alt { new_slide_obj.shape_alt_texts.insert(shape_id, a); }
            if let Some(p) = placement { new_slide_obj.shape_placements.insert(shape_id, p); }
            if let Some(pr) = print { new_slide_obj.shape_prints.insert(shape_id, pr); }
            if aspect { new_slide_obj.aspect_locked.insert(shape_id); }
            if hidden { new_slide_obj.hidden_shapes.insert(shape_id); }
            if let Some(att) = attached { new_slide_obj.attached.insert(shape_id, att); }

            presentation.slides.push(new_slide_obj);

            // Update workbook
            let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
            let new_index = wb.sheets.len();
            wb.sheets.push(crate::sheet::WorkbookSheet {
                name: name.clone(),
                state: crate::sheet::SheetState::Visible,
                slide: Some(new_slide),
                hidden: None,
                other: false,
            });
            wb.structure.push(crate::sheet_ops::StructureEdit::AddSheet {
                sheet: new_index,
                name,
                slide: new_slide,
                shape: sheet_shape,
                content: empty_content,
            });
        }
    }

    invalidate_calc_index(presentation);
    Ok(inverse)
}

fn shift_cells(presentation: &mut Presentation, slide: SlideId, range: CellRange, down: bool, insert: bool) -> Result<Command, EditError> {
    use ibis_calc::formula::Axis as FAxis;
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    let name = presentation.workbook.as_ref().map(|w| w.sheets[index].name.clone()).unwrap_or_default();
    // The band that moves: the range's columns from its first row down, or
    // its rows from its first column right.
    let (count, at) = if down { (range.last_row - range.first_row + 1, range.first_row) } else { (range.last_column - range.first_column + 1, range.first_column) };
    let in_band = |row: u32, column: u32| if down { (range.first_column..=range.last_column).contains(&column) && row >= range.first_row } else { (range.first_row..=range.last_row).contains(&row) && column >= range.first_column };
    {
        let sheet = sheet_mut(presentation, slide)?;
        // A merged range across the band's edge cannot move.
        let crosses = sheet.merges.iter().any(|m| {
            let cells_in = (m.first_row..=m.last_row).flat_map(|r| (m.first_column..=m.last_column).map(move |c| (r, c))).filter(|&(r, c)| in_band(r, c)).count();
            let all = ((m.last_row - m.first_row + 1) * (m.last_column - m.first_column + 1)) as usize;
            cells_in != 0 && cells_in != all
        });
        if crosses {
            return Err(EditError::IndexOutOfRange);
        }
        let limit = if down { sheet.rows.len() as u32 } else { sheet.columns.len() as u32 };
        let moved: Vec<SheetCell> = sheet.cells.iter().filter(|c| in_band(c.row, c.column)).cloned().collect();
        for c in &moved {
            sheet.put_cell(c.row, c.column, None);
        }
        let n = count as i64;
        for c in moved {
            let pos = if down { c.row } else { c.column } as i64;
            let new = if insert {
                pos + n
            } else if pos < at as i64 + n {
                continue; // deleted
            } else {
                pos - n
            };
            if new < 0 || new >= limit as i64 {
                continue;
            }
            let (row, column) = if down { (new as u32, c.column) } else { (c.row, new as u32) };
            sheet.put_cell(row, column, Some(SheetCell { row, column, ..c }));
        }
        let shift = |m: &CellRange| -> Option<CellRange> {
            let all_in = (m.first_row..=m.last_row).all(|r| (m.first_column..=m.last_column).all(|c| in_band(r, c)));
            if !all_in {
                return Some(*m);
            }
            let axis = if down { Axis::Rows } else { Axis::Columns };
            crate::sheet_ops::move_range(*m, axis, at, if insert { n } else { -n })
        };
        sheet.merges = sheet.merges.iter().filter_map(shift).collect();
        for v in sheet.validations.iter_mut() {
            v.ranges = v.ranges.iter().filter_map(shift).collect();
        }
        sheet.validations.retain(|v| !v.ranges.is_empty());
        let shift_pos = |(r, c): (u32, u32)| -> Option<(u32, u32)> {
            if !in_band(r, c) {
                return Some((r, c));
            }
            let delta = if insert { n } else { -n };
            if down {
                Some((crate::sheet_ops::move_index(r, at, delta)?, c))
            } else {
                Some((r, crate::sheet_ops::move_index(c, at, delta)?))
            }
        };
        sheet.data_tables = std::mem::take(&mut sheet.data_tables)
            .into_iter()
            .filter_map(|mut t| {
                t.range = shift(&t.range)?;
                if let Some(p) = t.row_input {
                    t.row_input = Some(shift_pos(p)?);
                }
                if let Some(p) = t.column_input {
                    t.column_input = Some(shift_pos(p)?);
                }
                Some(t)
            })
            .collect();
    }
    // References within the band follow (only those wholly across its columns, or rows).
    let names: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
    let changed = crate::sheet_ops::rewrite_formulas(presentation, &|own, f| {
        let own = names.get(own).map(String::as_str).unwrap_or("");
        ibis_calc::formula::rewrite_references(f, |a| {
            let fits = if down {
                !a.whole_rows && a.start.column.min(a.end.column) >= range.first_column && a.start.column.max(a.end.column) <= range.last_column
            } else {
                !a.whole_columns && a.start.row.min(a.end.row) >= range.first_row && a.start.row.max(a.end.row) <= range.last_row
            };
            if !fits {
                return Some(a.clone());
            }
            let axis = if down { FAxis::Rows } else { FAxis::Columns };
            ibis_calc::formula::adjust_area(a, own, &name, axis, at, if insert { count as i64 } else { -(count as i64) })
        })
    });
    let mut all = changed;
    all.extend(crate::sheet_calc::BookView::new(presentation).formula_cells());
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    if calc_mode(presentation) == CalcMode::Auto {
        refresh_data_tables(presentation, slide, &mut Undo::default())?;
    }
    Ok(inverse)
}

/// The rows of `range` that 重複の削除 removes: those (after the first if
/// `header`) whose values in `columns` are those of an earlier row. Values
/// match as Excel compares them: text without regard to case, numbers by
/// value, an empty cell only an empty one.
pub fn duplicate_rows(sheet: &crate::sheet::Sheet, range: CellRange, columns: &[u32], header: bool) -> Vec<u32> {
    let first = range.first_row + u32::from(header);
    let key = |v: &CellValue| match v {
        CellValue::Empty => String::from("e"),
        CellValue::Number(n) => format!("n{n}"),
        CellValue::Text(t) => format!("t{}", t.to_lowercase()),
        CellValue::Bool(b) => format!("b{b}"),
        CellValue::Error(e) => format!("x{e:?}"),
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for row in first..=range.last_row {
        let k: Vec<String> = columns.iter().map(|&c| key(&sheet.cell(row, c).map(|x| x.value.clone()).unwrap_or_default())).collect();
        if !seen.insert(k) {
            out.push(row);
        }
    }
    out
}

fn remove_duplicates(presentation: &mut Presentation, slide: SlideId, range: CellRange, columns: Vec<u32>, header: bool) -> Result<Command, EditError> {
    if columns.is_empty() || columns.iter().any(|c| !(range.first_column..=range.last_column).contains(c)) || range.first_row + u32::from(header) > range.last_row {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    {
        let sheet = sheet_mut(presentation, slide)?;
        let gone = duplicate_rows(sheet, range, &columns, header);
        let first = range.first_row + u32::from(header);
        let kept: Vec<u32> = (first..=range.last_row).filter(|r| gone.binary_search(r).is_err()).collect();
        let inside = |c: &SheetCell| (first..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column);
        let moved: Vec<SheetCell> = sheet.cells.iter().filter(|c| inside(c)).cloned().collect();
        for c in &moved {
            sheet.put_cell(c.row, c.column, None);
        }
        for (i, &from) in kept.iter().enumerate() {
            let to = first + i as u32;
            for c in moved.iter().filter(|c| c.row == from) {
                let formula = c.formula.as_ref().map(|f| ibis_calc::formula::shift(f, to as i64 - from as i64, 0));
                sheet.put_cell(to, c.column, Some(SheetCell { row: to, formula, ..c.clone() }));
            }
        }
    }
    let mut changed: Vec<(usize, u32, u32)> = Vec::new();
    for row in range.first_row..=range.last_row {
        for column in range.first_column..=range.last_column {
            changed.push((index, row, column));
        }
    }
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &changed)?;
    Ok(inverse)
}

fn set_table(presentation: &mut Presentation, slide: SlideId, index: usize, table: Option<crate::sheet::SheetTable>, cells: Vec<PutCell>) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    // 範囲に変換: the formulas' references to it become cell references.
    if table.is_none() {
        let book_index = workbook_index(presentation, slide)?;
        let t = sheet_mut(presentation, slide)?.tables.get(index).cloned().ok_or(EditError::IndexOutOfRange)?;
        convert_structured(presentation, book_index, &t);
    }
    // A new name: the formulas and names that name the table follow.
    let renamed = match (&table, sheet_mut(presentation, slide)?.tables.get(index)) {
        (Some(t), Some(old)) if old.name != t.name => Some((old.name.clone(), t.name.clone())),
        _ => None,
    };
    if let Some((old, new)) = renamed {
        let rename = |f: &str| ibis_calc::formula::rewrite_structured(f, |table, spec| table.eq_ignore_ascii_case(&old).then(|| format!("{new}{spec}")));
        for s in presentation.slides.iter_mut() {
            for sh in s.shapes.iter_mut() {
                if let crate::model::ShapeKind::Sheet(sheet) = &mut sh.kind {
                    if !sheet.cells.iter().any(|c| c.formula.as_deref().is_some_and(|f| f.to_lowercase().contains(&old.to_lowercase()))) {
                        continue;
                    }
                    let sheet = Arc::make_mut(sheet);
                    for c in sheet.cells.iter_mut() {
                        if let Some(f) = &c.formula {
                            let g = rename(f);
                            if &g != f {
                                c.formula = Some(g);
                            }
                        }
                    }
                }
            }
        }
        if let Some(w) = presentation.workbook.as_mut() {
            let w = Arc::make_mut(w);
            for n in w.names.iter_mut() {
                n.formula = rename(&n.formula);
            }
        }
    }
    {
        let sheet = sheet_mut(presentation, slide)?;
        match table {
            Some(mut t) => {
                check_bounds(sheet, t.range.last_row, t.range.last_column)?;
                // The filter covers the table without its totals.
                if let Some(f) = t.filter.as_mut() {
                    f.range = CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range };
                }
                if index < sheet.tables.len() {
                    sheet.tables[index] = t;
                } else {
                    sheet.tables.push(t);
                }
            }
            None if index < sheet.tables.len() => {
                sheet.tables.remove(index);
            }
            None => return Err(EditError::IndexOutOfRange),
        }
    }
    if !cells.is_empty() {
        put_cells(presentation, slide, cells)?;
    }
    // Its filter over its rows as they are now.
    let d1904 = date1904(presentation);
    refilter(sheet_mut(presentation, slide)?, d1904);
    // Structured references to the table read its new range.
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

/// Replace the structured references to table `t` (on sheet `sheet` of
/// the book) in every formula of the book by cell references, as Excel
/// does when a table is converted to a range: a column or part of it
/// absolute (`$B$2:$B$5`), a row's cell with its column absolute (`$B2`).
fn convert_structured(presentation: &mut Presentation, sheet: usize, t: &crate::sheet::SheetTable) {
    let info = ibis_calc::eval::TableInfo {
        sheet,
        first_row: t.range.first_row,
        first_column: t.range.first_column,
        last_row: t.range.last_row,
        last_column: t.range.last_column,
        header_rows: t.header_rows,
        totals_rows: t.totals_rows,
        columns: t.columns.clone(),
    };
    let names: Vec<(Option<SlideId>, String)> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| (s.slide, s.name.clone())).collect()).unwrap_or_default();
    let table_sheet = names.get(sheet).map(|n| n.1.clone()).unwrap_or_default();
    for (i, (slide, _)) in names.iter().enumerate() {
        let Some(slide) = slide else { continue };
        let Ok(s) = sheet_mut(presentation, *slide) else { continue };
        let inside = |r: u32, c: u32| i == sheet && (t.range.first_row..=t.range.last_row).contains(&r) && (t.range.first_column..=t.range.last_column).contains(&c);
        for cell in s.cells.iter_mut() {
            let Some(f) = &cell.formula else { continue };
            let (row, column) = (cell.row, cell.column);
            let new = ibis_calc::formula::rewrite_structured(f, |table, spec| {
                let ours = if table.is_empty() { inside(row, column) } else { table.eq_ignore_ascii_case(&t.name) };
                if !ours {
                    return None;
                }
                let (r0, c0, r1, c1) = ibis_calc::eval::structured::area(&info, spec, row).ok()?;
                let col = |c: u32| crate::sheet::cell_name(c, 0).trim_end_matches('1').to_string();
                let prefix = if i == sheet { String::new() } else { format!("{}!", ibis_calc::formula::quote_sheet(&table_sheet)) };
                let this_row = r0 == r1 && (spec.contains('@') || spec.to_lowercase().contains("#this row"));
                Some(if this_row && c0 == c1 {
                    format!("{prefix}${}{}", col(c0), r0 + 1)
                } else if r0 == r1 && c0 == c1 {
                    format!("{prefix}${}${}", col(c0), r0 + 1)
                } else {
                    format!("{prefix}${}${}:${}${}", col(c0), r0 + 1, col(c1), r1 + 1)
                })
            });
            if &new != f {
                cell.formula = Some(new);
            }
        }
    }
}

/// What a sort key orders the rows by: the values (ascending or not), or a
/// colour, its rows on top (or at the bottom) and the others as they were,
/// or a user-defined custom list (e.g. days of week, priorities, or categories).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortBy {
    Value(bool),
    /// The cells' fill (`None`: no fill).
    CellColor { color: Option<Rgb>, top: bool },
    /// The text's colour (`None`: automatic, black).
    FontColor { color: Option<Rgb>, top: bool },
    /// Values ordered by a custom list (e.g. `["日", "月", "火", "水", "木", "金", "土"]`).
    /// `ascending: true` keeps the list's order, `ascending: false` reverses it.
    CustomList { list: Vec<String>, ascending: bool },
    /// An icon of a conditional formatting icon set (`sortBy="icon"`).
    Icon { icon_set: String, icon_id: u32, top: bool },
}

struct SheetBook<'a>(&'a Sheet);

impl ibis_calc::eval::Book for SheetBook<'_> {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _: usize) -> &str {
        ""
    }
    fn value(&self, _: usize, row: u32, column: u32) -> CellValue {
        self.0.cell(row, column).map(|c| c.value.clone()).unwrap_or_default()
    }
    fn formula(&self, _: usize, row: u32, column: u32) -> Option<&str> {
        self.0.cell(row, column)?.formula.as_deref()
    }
    fn for_each_cell(&self, _: usize, area: (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        let (r0, c0, r1, c1) = area;
        for c in &self.0.cells {
            if (r0..=r1).contains(&c.row) && (c0..=c1).contains(&c.column) && (!c.value.is_empty() || c.formula.is_some()) {
                f(c.row, c.column, &c.value);
            }
        }
    }
    fn extent(&self, _: usize) -> (u32, u32) {
        self.0.cells.iter().fold((0, 0), |(r, c), x| (r.max(x.row + 1), c.max(x.column + 1)))
    }
    fn defined_name(&self, _: &str, _: Option<usize>) -> Option<String> {
        None
    }
    fn pivot_data(
        &self,
        _: usize,
        row: u32,
        column: u32,
        data_field: &str,
        fields: &[(&str, &str)],
    ) -> Option<CellValue> {
        for pivot in &self.0.pivots {
            if pivot.contains_cell(row, column) {
                return pivot.get_pivot_data(self.0, data_field, fields);
            }
        }
        None
    }
}

fn cf_point(book: &SheetBook, p: &crate::sheet::CfValue, values: &[f64], anchor: (u32, u32)) -> Option<f64> {
    let given = || -> Option<f64> {
        let v = p.value.as_deref()?;
        v.parse::<f64>().ok().or_else(|| match ibis_calc::recalc::evaluate(book, (0, anchor.0, anchor.1), v.trim_start_matches('=')).0 {
            CellValue::Number(n) => Some(n),
            _ => None,
        })
    };
    match p.kind.as_str() {
        "num" | "formula" => given(),
        "min" | "autoMin" => values.first().copied(),
        "max" | "autoMax" => values.last().copied(),
        "percent" => {
            let (min, max) = (*values.first()?, *values.last()?);
            Some(min + (max - min) * given()? / 100.0)
        }
        "percentile" => {
            if values.is_empty() {
                return None;
            }
            let k = given()? / 100.0;
            let pos = k * (values.len().saturating_sub(1)) as f64;
            let (i, f) = (pos.floor() as usize, pos.fract());
            Some(values[i] + (values.get(i + 1).copied().unwrap_or(values[i]) - values[i]) * f)
        }
        _ => given(),
    }
}

/// Pre-compiled icon set rules for a column, avoiding recalculating thresholds per cell.
pub struct ColumnIconEvaluator {
    rules: Vec<CompiledIconRule>,
}

struct CompiledIconRule {
    ranges: Vec<CellRange>,
    thresholds: Vec<(f64, bool)>,
    count: usize,
    reverse: bool,
    custom_icons: Vec<Option<crate::sheet::CfIcon>>,
    default_set: String,
}

impl ColumnIconEvaluator {
    pub fn new(sheet: &Sheet, column: u32) -> Self {
        if sheet.conditional.is_empty() {
            return Self { rules: Vec::new() };
        }
        let mut cf_rules: Vec<(&crate::sheet::ConditionalFormat, &crate::sheet::CfRule)> = sheet
            .conditional
            .iter()
            .flat_map(|cf| cf.rules.iter().map(move |r| (cf, r)))
            .collect();
        cf_rules.sort_by_key(|(_, r)| r.priority);

        let book = SheetBook(sheet);
        let mut rules = Vec::new();

        for (cf, rule) in cf_rules {
            let in_col = cf.ranges.iter().any(|r| column >= r.first_column && column <= r.last_column);
            if !in_col {
                continue;
            }
            if let crate::sheet::CfKind::IconSet { set, values, reverse, show_value: _, custom: _, icons: custom_icons } = &rule.kind {
                let count = values.len();
                if count == 0 {
                    continue;
                }
                let anchor = cf.ranges.first().map(|r| (r.first_row, r.first_column)).unwrap_or((0, column));
                let mut numbers = Vec::new();
                let (rows_len, cols_len) = (sheet.rows.len() as u32, sheet.columns.len() as u32);
                for r in &cf.ranges {
                    for ro in r.first_row..=r.last_row.min(rows_len.saturating_sub(1)) {
                        for co in r.first_column..=r.last_column.min(cols_len.saturating_sub(1)) {
                            if let Some(c) = sheet.cell(ro, co) {
                                if let CellValue::Number(n) = c.value {
                                    numbers.push(n);
                                }
                            }
                        }
                    }
                }
                numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let thresholds: Vec<(f64, bool)> = values.iter().skip(1).filter_map(|p| {
                    Some((cf_point(&book, p, &numbers, anchor)?, p.gte))
                }).collect();

                rules.push(CompiledIconRule {
                    ranges: cf.ranges.clone(),
                    thresholds,
                    count,
                    reverse: *reverse,
                    custom_icons: custom_icons.clone(),
                    default_set: set.clone(),
                });
            }
        }

        Self { rules }
    }

    pub fn icon_of(&self, sheet: &Sheet, row: u32, column: u32) -> Option<(String, u32)> {
        let cell = sheet.cell(row, column)?;
        let CellValue::Number(v) = cell.value else {
            return None;
        };
        for rule in &self.rules {
            let in_range = rule.ranges.iter().any(|r| {
                row >= r.first_row && row <= r.last_row && column >= r.first_column && column <= r.last_column
            });
            if !in_range {
                continue;
            }
            let mut i = 0;
            for (t, gte) in &rule.thresholds {
                if if *gte { v >= *t } else { v > *t } {
                    i += 1;
                }
            }
            let i = i.min(rule.count.saturating_sub(1));
            let slot = if rule.reverse {
                rule.count.saturating_sub(1).saturating_sub(i)
            } else {
                i
            };
            if let Some(Some(ci)) = rule.custom_icons.get(slot) {
                if ci.is_no_icon() {
                    return None;
                }
                if ci.id >= 0 {
                    return Some((ci.set.clone(), ci.id as u32));
                }
            }
            return Some((rule.default_set.clone(), slot as u32));
        }
        None
    }
}

/// Resolve the conditional formatting icon (set name, icon ID) applied to a cell, if any.
pub fn cell_icon(sheet: &Sheet, row: u32, column: u32) -> Option<(String, u32)> {
    ColumnIconEvaluator::new(sheet, column).icon_of(sheet, row, column)
}

fn sort_rows(presentation: &mut Presentation, slide: SlideId, range: CellRange, keys: Vec<(u32, SortBy)>, header: bool, match_case: bool) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    let (row_map, moved_cell_images, xs, ys) = {
        let sheet = sheet_mut(presentation, slide)?;
        let first = range.first_row + u32::from(header);
        if first > range.last_row {
            return Err(EditError::IndexOutOfRange);
        }
        // Applied filter exclusions keep their row slots and values.
        let rows: Vec<u32> = (first..=range.last_row).filter(|r| !sheet.filtered_rows.contains(r)).collect();
        let icon_evaluators: Vec<Option<ColumnIconEvaluator>> = keys
            .iter()
            .map(|(c, by)| match by {
                SortBy::Icon { .. } => Some(ColumnIconEvaluator::new(sheet, *c)),
                _ => None,
            })
            .collect();
        // A colour key's value: 0 for the rows with the colour (1 at the bottom), else 1 (0).
        let key_of = |row: u32| -> Vec<(CellValue, Option<std::sync::Arc<str>>)> {
            keys.iter()
                .enumerate()
                .map(|(i, (c, by))| {
                    let cell = sheet.cell(row, *c);
                    let style = cell.and_then(|x| sheet.styles.get(x.style as usize));
                    let colour = |hit: bool, top: bool| CellValue::Number(if hit == top { 0.0 } else { 1.0 });
                    match by {
                        SortBy::Value(_) | SortBy::CustomList { .. } => (cell.map(|x| x.value.clone()).unwrap_or_default(), cell.and_then(|x| x.phonetic.clone())),
                        SortBy::CellColor { color, top } => (colour(style.and_then(|s| s.fill) == *color, *top), None),
                        SortBy::FontColor { color, top } => (colour(style.map_or(Rgb(0), |s| s.color) == color.unwrap_or(Rgb(0)), *top), None),
                        SortBy::Icon { icon_set, icon_id, top } => {
                            let hit = icon_evaluators.get(i).and_then(|e| e.as_ref()).and_then(|e| e.icon_of(sheet, row, *c)) == Some((icon_set.clone(), *icon_id));
                            (colour(hit, *top), None)
                        }
                    }
                })
                .collect()
        };
        // Excel's order: numbers, text (without regard to case), FALSE/TRUE,
        // errors; blanks last whichever way.
        let rank = |v: &CellValue| match v {
            CellValue::Number(_) => 0,
            CellValue::Text(_) => 1,
            CellValue::Bool(_) => 2,
            CellValue::Error(_) => 3,
            CellValue::Empty => 4,
        };
        let list_index = |v: &CellValue, list: &[String]| -> Option<usize> {
            match v {
                CellValue::Text(s) => {
                    list.iter().position(|item| {
                        if match_case {
                            &**s == item.as_str()
                        } else {
                            s.eq_ignore_ascii_case(item)
                                || s.chars().flat_map(|c| c.to_lowercase()).eq(item.chars().flat_map(|c| c.to_lowercase()))
                        }
                    })
                }
                CellValue::Number(n) => {
                    list.iter().position(|item| {
                        if let Ok(num) = item.trim().parse::<f64>() {
                            (num - n).abs() < f64::EPSILON
                        } else {
                            false
                        }
                    })
                }
                _ => None,
            }
        };
        let compare = |(a, pa): &(CellValue, Option<std::sync::Arc<str>>), (b, pb): &(CellValue, Option<std::sync::Arc<str>>), by: &SortBy| -> std::cmp::Ordering {
            use std::cmp::Ordering;
            if matches!(a, CellValue::Empty) || matches!(b, CellValue::Empty) {
                return rank(a).cmp(&rank(b));
            }
            match by {
                SortBy::CustomList { list, ascending } => {
                    let ia = list_index(a, list);
                    let ib = list_index(b, list);
                    match (ia, ib) {
                        (Some(x), Some(y)) => if *ascending { x.cmp(&y) } else { y.cmp(&x) },
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => {
                            let o = match (a, b) {
                                (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
                                (CellValue::Text(x), CellValue::Text(y)) => ibis_calc::text_order::compare_for_sort_with_case(x, pa.as_deref(), y, pb.as_deref(), match_case),
                                (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
                                _ => rank(a).cmp(&rank(b)),
                            };
                            if *ascending { o } else { o.reverse() }
                        }
                    }
                }
                SortBy::Value(ascending) => {
                    let o = match (a, b) {
                        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
                        (CellValue::Text(x), CellValue::Text(y)) => ibis_calc::text_order::compare_for_sort_with_case(x, pa.as_deref(), y, pb.as_deref(), match_case),
                        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
                        _ => rank(a).cmp(&rank(b)),
                    };
                    if *ascending { o } else { o.reverse() }
                }
                SortBy::CellColor { .. } | SortBy::FontColor { .. } | SortBy::Icon { .. } => {
                    match (a, b) {
                        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
                        _ => rank(a).cmp(&rank(b)),
                    }
                }
            }
        };
        let mut order: Vec<(u32, Vec<(CellValue, Option<std::sync::Arc<str>>)>)> = rows.iter().map(|&r| (r, key_of(r))).collect();
        order.sort_by(|a, b| {
            for (i, (_, by)) in keys.iter().enumerate() {
                let o = compare(&a.1[i], &b.1[i], by);
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
        // The cells of each row of the range, moved to their new rows.
        let inside = |c: &SheetCell| rows.binary_search(&c.row).is_ok() && (range.first_column..=range.last_column).contains(&c.column);
        // (Rebuilt in one pass: putting each cell in the sorted list took
        // minutes for tens of thousands of cells.)
        let (moved, mut kept): (Vec<SheetCell>, Vec<SheetCell>) = std::mem::take(&mut sheet.cells).into_vec().into_iter().partition(|c| inside(c));
        let mut by_row: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
        for (i, c) in moved.iter().enumerate() {
            by_row.entry(c.row).or_default().push(i);
        }
        for (to_index, (from, _)) in order.iter().enumerate() {
            let to = rows[to_index];
            for &i in by_row.get(from).map(Vec::as_slice).unwrap_or(&[]) {
                let c = &moved[i];
                let formula = c.formula.as_ref().map(|f| ibis_calc::formula::shift(f, to as i64 - *from as i64, 0));
                kept.push(SheetCell { row: to, formula, ..c.clone() });
            }
        }
        kept.sort_by_key(|c| (c.row, c.column));
        sheet.cells = kept.into();
        let row_map: std::collections::HashMap<u32, u32> = order.iter().enumerate().map(|(to_idx, (from, _))| (*from, rows[to_idx])).collect();
        sheet.cell_images = std::mem::take(&mut sheet.cell_images).into_iter().map(|mut img| {
            if (range.first_column..=range.last_column).contains(&img.column) {
                if let Some(&to) = row_map.get(&img.row) {
                    img.row = to;
                }
            }
            img
        }).collect();
        sheet.web_images = std::mem::take(&mut sheet.web_images).into_iter().map(|((r, c), spec)| {
            let new_r = if (range.first_column..=range.last_column).contains(&c) {
                row_map.get(&r).copied().unwrap_or(r)
            } else {
                r
            };
            ((new_r, c), spec)
        }).collect();
        sheet.comments = std::mem::take(&mut sheet.comments).into_iter().map(|mut c| {
            if (range.first_column..=range.last_column).contains(&c.column) {
                if let Some(&to) = row_map.get(&c.row) {
                    c.row = to;
                }
            }
            c
        }).collect();
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let moved_cell_images = sheet.cell_images.clone();
        Ok::<_, EditError>((row_map, moved_cell_images, xs, ys))
    }?;
    if let Some(slide_obj) = presentation.slide_mut(slide) {
        for shape in slide_obj.shapes.iter_mut() {
            if matches!(shape.kind, ShapeKind::Picture(_)) {
                for img in &moved_cell_images {
                    let (col, r) = (img.column as usize, img.row as usize);
                    if col + 1 < xs.len() && r + 1 < ys.len() {
                        let (x0, y0) = (xs[col], ys[r]);
                        let (x1, y1) = (xs[col + 1], ys[r + 1]);
                        if let Some(from) = row_map.iter().find_map(|(&from, &to)| if to == img.row && from != to { Some(from) } else { None }) {
                            let old_r = from as usize;
                            if old_r + 1 < ys.len() {
                                let old_y0 = ys[old_r];
                                let old_y1 = ys[old_r + 1];
                                if old_y0 <= shape.frame.y + 10_000 && shape.frame.bottom() <= old_y1 + 10_000 && xs[col] <= shape.frame.x + 10_000 && shape.frame.right() <= xs[col + 1] + 10_000 {
                                    let (cw, ch) = ((x1 - x0).max(1), (y1 - y0).max(1));
                                    let (w, h) = (shape.frame.width.max(1), shape.frame.height.max(1));
                                    let scale = (cw as f64 / w as f64).min(ch as f64 / h as f64);
                                    let (fw, fh) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
                                    shape.frame = EmuRect::new(x0 + (cw - fw) / 2, y0 + (ch - fh) / 2, fw.max(1), fh.max(1));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut changed: Vec<(usize, u32, u32)> = Vec::new();
    for row in range.first_row..=range.last_row {
        for column in range.first_column..=range.last_column {
            changed.push((index, row, column));
        }
    }
    recalculate_cells(presentation, &changed)?;
    Ok(inverse)
}

/// Whether `text` matches `pattern` with Excel's wildcards (`*` any run,
/// `?` one character, `~` the next one as is), case aside.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum T {
        Any,
        One,
        Char(char),
    }
    let mut p = Vec::new();
    let mut chars = pattern.chars().flat_map(char::to_lowercase);
    while let Some(c) = chars.next() {
        p.push(match c {
            '*' => T::Any,
            '?' => T::One,
            '~' => T::Char(chars.next().unwrap_or('~')),
            c => T::Char(c),
        });
    }
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Match with backtracking to the last `*`.
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        match p.get(i) {
            Some(T::Any) => {
                star = Some(i);
                mark = j;
                i += 1;
            }
            Some(T::One) => {
                i += 1;
                j += 1;
            }
            Some(T::Char(c)) if *c == t[j] => {
                i += 1;
                j += 1;
            }
            _ => match star {
                Some(s) => {
                    i = s + 1;
                    mark += 1;
                    j = mark;
                }
                None => return false,
            },
        }
    }
    p[i..].iter().all(|x| *x == T::Any)
}

/// Whether a cell keeps one comparison of a custom filter.
fn compare_keeps(cell: Option<&SheetCell>, op: &str, value: &str) -> bool {
    let text = cell.map_or("", |c| c.text.as_str());
    let number = match cell.map(|c| &c.value) {
        Some(CellValue::Number(n)) => Some(*n),
        _ => None,
    };
    let wanted = value.trim().parse::<f64>().ok();
    // A space stands for blank: `notEqual " "` is Excel's 空白以外 (XAF-00019).
    if value == " " && matches!(op, "equal" | "notEqual") {
        return text.is_empty() == (op == "equal");
    }
    let equal = || match (number, wanted) {
        (Some(n), Some(v)) => n == v,
        _ => wildcard_match(value, text),
    };
    let order = |f: fn(std::cmp::Ordering) -> bool| match (number, wanted) {
        (Some(n), Some(v)) => n.partial_cmp(&v).is_some_and(f),
        (None, None) if !text.is_empty() => f(text.to_lowercase().cmp(&value.to_lowercase())),
        _ => false,
    };
    match op {
        "notEqual" => !equal(),
        "greaterThan" => order(|o| o.is_gt()),
        "greaterThanOrEqual" => order(|o| o.is_ge()),
        "lessThan" => order(|o| o.is_lt()),
        "lessThanOrEqual" => order(|o| o.is_le()),
        _ => equal(),
    }
}

/// The serials (from, to; both kept) of a date period relative to `today`
/// (`dynamicFilter/@type`), or for `Q1`…`M12` the months kept in any year.
enum Period {
    Days(f64, f64),
    Months(u32, u32),
}

fn period(kind: &str, today: Option<i64>, date1904: bool) -> Option<Period> {
    use ibis_calc::eval::convert::{date_from_serial, days_in_month, serial_from_date};
    if let Some(q) = kind.strip_prefix('Q').and_then(|q| q.parse::<u32>().ok()).filter(|q| (1..=4).contains(q)) {
        return Some(Period::Months(q * 3 - 2, q * 3));
    }
    if let Some(m) = kind.strip_prefix('M').and_then(|m| m.parse::<u32>().ok()).filter(|m| (1..=12).contains(m)) {
        return Some(Period::Months(m, m));
    }
    let today = today?;
    let (y, m, _) = date_from_serial(today, date1904);
    let day = |y: i64, m: i64, d: i64| serial_from_date(y, m, d, date1904);
    // A month `k` months from this one (k may be negative): its first and last day.
    let month = |k: i64| {
        let n = y * 12 + (m as i64 - 1) + k;
        let (yy, mm) = (n.div_euclid(12), n.rem_euclid(12) + 1);
        (day(yy, mm, 1), day(yy, mm, days_in_month(yy, mm as u32) as i64))
    };
    let quarter = |k: i64| {
        let first = (m as i64 - 1) / 3 * 3 - (m as i64 - 1) + 3 * k;
        (month(first).0, month(first + 2).1)
    };
    // Weeks run Sunday to Saturday.
    let sunday = today - ibis_calc::eval::convert::weekday(if date1904 { today + 1462 } else { today }) as i64;
    let (from, to) = match kind {
        "today" => (today, today),
        "yesterday" => (today - 1, today - 1),
        "tomorrow" => (today + 1, today + 1),
        "thisWeek" => (sunday, sunday + 6),
        "lastWeek" => (sunday - 7, sunday - 1),
        "nextWeek" => (sunday + 7, sunday + 13),
        "thisMonth" => month(0),
        "lastMonth" => month(-1),
        "nextMonth" => month(1),
        "thisQuarter" => quarter(0),
        "lastQuarter" => quarter(-1),
        "nextQuarter" => quarter(1),
        "thisYear" => (day(y, 1, 1), day(y, 12, 31)),
        "lastYear" => (day(y - 1, 1, 1), day(y - 1, 12, 31)),
        "nextYear" => (day(y + 1, 1, 1), day(y + 1, 12, 31)),
        "yearToDate" => (day(y, 1, 1), today),
        _ => return None,
    };
    Some(Period::Days(from as f64, to as f64 + 1.0 - 1e-9))
}

/// Whether a date (a serial) falls in a `dateGroupItem`.
fn in_date_group(serial: f64, g: &crate::sheet::DateGroup, date1904: bool) -> bool {
    let (y, m, d) = ibis_calc::eval::convert::date_from_serial(serial.floor() as i64, date1904);
    let seconds = ((serial - serial.floor()) * 86_400.0).round() as u32;
    let parts = [(y as u32, g.year), (m, g.month), (d, g.day), (seconds / 3600, g.hour), (seconds / 60 % 60, g.minute), (seconds % 60, g.second)];
    let depth = match g.grouping.as_str() {
        "year" => 1,
        "month" => 2,
        "day" => 3,
        "hour" => 4,
        "minute" => 5,
        _ => 6,
    };
    parts[..depth].iter().all(|(a, b)| a == b)
}

/// What a column's filter is checked against, worked out once for the
/// column: its average, or the Top filter's threshold.
fn column_measure(sheet: &Sheet, f: &crate::sheet::ColumnFilter, body: std::ops::RangeInclusive<u32>, column: u32) -> f64 {
    use crate::sheet::ColumnFilter;
    let numbers = || -> Vec<f64> {
        body.clone().filter_map(|row| match sheet.cell(row, column).map(|cell| &cell.value) {
            Some(CellValue::Number(n)) => Some(*n),
            _ => None,
        }).collect()
    };
    match f {
        ColumnFilter::AboveAverage(_) => {
            let values = numbers();
            if values.is_empty() { 0.0 } else { values.iter().sum::<f64>() / values.len() as f64 }
        }
        ColumnFilter::Top { top, percent, value } => {
            let mut values = numbers();
            if values.is_empty() {
                return f64::NAN;
            }
            values.sort_by(|a, b| if *top { b.total_cmp(a) } else { a.total_cmp(b) });
            let n = value.trim().parse::<f64>().unwrap_or(10.0).max(0.0);
            let count = if *percent { (values.len() as f64 * n / 100.0).floor() as usize } else { n.floor() as usize };
            values[count.clamp(1, values.len()) - 1]
        }
        _ => 0.0,
    }
}

/// Whether a row's cell keeps a column filter.
fn filter_keeps(sheet: &Sheet, f: &crate::sheet::ColumnFilter, row: u32, column: u32, measure: f64, today: Option<i64>, date1904: bool) -> bool {
    use crate::sheet::ColumnFilter;
    let cell = sheet.cell(row, column);
    let text = cell.map_or("", |c| c.text.as_str());
    let number = match cell.map(|c| &c.value) {
        Some(CellValue::Number(n)) => Some(*n),
        _ => None,
    };
    let listed = |values: &[String]| values.iter().any(|v| v.eq_ignore_ascii_case(text) || v.to_lowercase() == text.to_lowercase());
    match f {
        ColumnFilter::Values(values) => listed(values),
        ColumnFilter::Dates { values, groups } => listed(values) || number.is_some_and(|n| groups.iter().any(|g| in_date_group(n, g, date1904))),
        ColumnFilter::AboveAverage(above) => number.is_some_and(|n| if *above { n > measure } else { n < measure }),
        ColumnFilter::Custom { and, conditions } => {
            let mut each = conditions.iter().map(|(op, value)| compare_keeps(cell, op, value));
            if *and { each.all(|k| k) } else { each.any(|k| k) }
        }
        ColumnFilter::Top { top, .. } => number.is_some_and(|n| if *top { n >= measure } else { n <= measure }),
        ColumnFilter::Period(kind) => match (number, period(kind, today, date1904)) {
            (Some(n), Some(Period::Days(from, to))) => n >= from && n <= to,
            (Some(n), Some(Period::Months(from, to))) => {
                let (_, m, _) = ibis_calc::eval::convert::date_from_serial(n.floor() as i64, date1904);
                (from..=to).contains(&m)
            }
            _ => false,
        },
        ColumnFilter::Color { cell: fill, color } => {
            let style = cell.and_then(|c| sheet.styles.get(c.style as usize));
            if *fill {
                style.and_then(|s| s.fill) == *color
            } else {
                let font = style.map_or(crate::drawing::Rgb(0), |s| s.color);
                font == color.unwrap_or(crate::drawing::Rgb(0))
            }
        }
        ColumnFilter::Icon { icon_set, icon_id } => {
            cell_icon(sheet, row, column) == Some((icon_set.clone(), *icon_id))
        }
    }
}

/// Rows excluded by the supported filter criteria, before they are hidden.
/// `date1904`: the workbook's dates count from 1904.
pub fn filtered_rows(sheet: &Sheet, f: &crate::sheet::AutoFilter, date1904: bool) -> std::collections::BTreeSet<u32> {
    filtered_rows_on(sheet, f, ibis_calc::eval::convert::today_serial(date1904), date1904)
}

/// [`filtered_rows`] with today (a serial) given, for the date periods.
pub fn filtered_rows_on(sheet: &Sheet, f: &crate::sheet::AutoFilter, today: Option<i64>, date1904: bool) -> std::collections::BTreeSet<u32> {
    let body = (f.range.first_row + 1)..=f.range.last_row;
    // A checked-value list is a membership query, not a fresh scan of every
    // selected string for each data row (quadratic on large distinct columns).
    let value_sets: Vec<Option<std::collections::HashSet<String>>> = f.columns.iter().map(|(_,criterion)|match criterion {
        crate::sheet::ColumnFilter::Values(values) => Some(values.iter().map(|v|v.to_lowercase()).collect()),
        _ => None,
    }).collect();
    let measures: Vec<f64> = f.columns.iter().map(|(column, criterion)| column_measure(sheet, criterion, body.clone(), *column)).collect();
    let icon_evaluators: Vec<Option<ColumnIconEvaluator>> = f.columns.iter().map(|(column, criterion)| match criterion {
        crate::sheet::ColumnFilter::Icon { .. } => Some(ColumnIconEvaluator::new(sheet, *column)),
        _ => None,
    }).collect();
    body.take_while(|&row| (row as usize) < sheet.rows.len()).filter(|&row| {
        !f.columns.iter().zip(&measures).zip(&icon_evaluators).zip(&value_sets).all(|((((column, criterion), measure), icon_eval),values)| {
            match criterion {
                crate::sheet::ColumnFilter::Values(_) => values.as_ref().unwrap().contains(&sheet.cell(row,*column).map_or("",|c|c.text.as_str()).to_lowercase()),
                crate::sheet::ColumnFilter::Icon { icon_set, icon_id } => {
                    icon_eval.as_ref().and_then(|e| e.icon_of(sheet, row, *column)) == Some((icon_set.clone(), *icon_id))
                }
                _ => filter_keeps(sheet, criterion, row, *column, *measure, today, date1904),
            }
        })
    }).collect()
}

fn set_auto_filter(presentation: &mut Presentation, slide: SlideId, filter: Option<crate::sheet::AutoFilter>) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let d1904 = date1904(presentation);
    {
        let sheet = sheet_mut(presentation, slide)?;
        sheet.auto_filter = filter;
        refilter(sheet, d1904);
    }
    fit_frame(presentation, slide);
    // SUBTOTAL and AGGREGATE leave hidden rows out: every formula again.
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

/// A table's filter (`None`: no ▼ on its headings, every row shown).
fn replace_sheet(presentation: &mut Presentation, slide: SlideId, sheet: Sheet) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    *sheet_mut(presentation, slide)? = sheet;
    fit_frame(presentation, slide);
    // Formulas reading its cells (a pivot table's values) read the new ones.
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

/// External workbook `book` linked: added after the others (one of that name is not added again).
fn add_external_book(presentation: &mut Presentation, book: crate::sheet::ExternalBook) -> Result<Command, EditError> {
    let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
    if book.name.is_empty() || wb.external_books.iter().any(|b| !b.broken && b.name.eq_ignore_ascii_case(&book.name)) {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    wb.external_books.push(book);
    invalidate_calc_index(presentation);
    Ok(inverse)
}

/// The cached cells of external workbook `index` replaced, and the formulas
/// reading them calculated again.
fn refresh_external_book(presentation: &mut Presentation, index: usize, sheets: Vec<crate::sheet::ExternalSheet>) -> Result<Command, EditError> {
    let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
    if wb.external_books.get(index).is_none_or(|b| b.broken) {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    wb.external_books[index].sheets = sheets;
    invalidate_calc_index(presentation);
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells_force(presentation, &all)?;
    Ok(inverse)
}

/// The source of external workbook `index` changed to `new_name` and `new_path`:
/// formulas reading the old workbook rewritten to `new_name`, cached cells replaced with `sheets`,
/// and recalculation triggered.
fn change_external_book_source(
    presentation: &mut Presentation,
    index: usize,
    new_name: String,
    new_path: String,
    sheets: Vec<crate::sheet::ExternalSheet>,
) -> Result<Command, EditError> {
    let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
    let old_name = match wb.external_books.get(index) {
        Some(b) if !b.broken && !b.name.is_empty() => b.name.clone(),
        _ => return Err(EditError::IndexOutOfRange),
    };
    let inverse = snapshot(presentation);
    if !old_name.eq_ignore_ascii_case(&new_name) {
        let slides: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
        for slide in slides {
            let Ok(sheet) = sheet_mut(presentation, slide) else { continue };
            for cell in sheet.cells.iter_mut() {
                if let Some(f) = cell.formula.as_deref().filter(|f| ibis_calc::formula::has_external_book(f)) {
                    let rewritten = ibis_calc::formula::rename_book(f, &old_name, &new_name, None);
                    if rewritten != f {
                        cell.formula = Some(rewritten);
                    }
                }
            }
            sheet.map_rule_formulas(&|f| ibis_calc::formula::rename_book(f, &old_name, &new_name, None));
        }
        let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
        for n in wb.names.iter_mut() {
            n.formula = ibis_calc::formula::rename_book(&n.formula, &old_name, &new_name, None);
        }
        for s in wb.sheets.iter_mut() {
            if let Some(h) = s.hidden.as_mut() {
                let h = Arc::make_mut(h);
                for cell in h.cells.iter_mut() {
                    if let Some(f) = cell.formula.as_deref().filter(|f| ibis_calc::formula::has_external_book(f)) {
                        let rewritten = ibis_calc::formula::rename_book(f, &old_name, &new_name, None);
                        if rewritten != f {
                            cell.formula = Some(rewritten);
                        }
                    }
                }
                h.map_rule_formulas(&|f| ibis_calc::formula::rename_book(f, &old_name, &new_name, None));
            }
        }
    }
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    wb.external_books[index].name = new_name;
    wb.external_books[index].path = new_path;
    wb.external_books[index].sheets = sheets;
    invalidate_calc_index(presentation);
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells_force(presentation, &all)?;
    Ok(inverse)
}

/// The formulas that read external workbook `index` made values, the link marked broken.
fn break_external_link(presentation: &mut Presentation, index: usize) -> Result<Command, EditError> {
    let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
    let name = match wb.external_books.get(index) {
        Some(b) if !b.broken && !b.name.is_empty() => b.name.clone(),
        _ => return Err(EditError::IndexOutOfRange),
    };
    let inverse = snapshot(presentation);
    let reads = |formula: &str| ibis_calc::formula::has_external_book(formula) && ibis_calc::formula::without_book(formula, &name) != formula;
    let slides: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
    for slide in slides {
        let Ok(sheet) = sheet_mut(presentation, slide) else { continue };
        for cell in sheet.cells.iter_mut() {
            if cell.formula.as_deref().is_some_and(reads) {
                cell.formula = None;
            }
        }
        sheet.map_rule_formulas(&|f| ibis_calc::formula::without_book(f, &name));
    }
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    for n in wb.names.iter_mut() {
        n.formula = ibis_calc::formula::without_book(&n.formula, &name);
    }
    for s in wb.sheets.iter_mut() {
        if let Some(h) = s.hidden.as_mut() {
            let h = Arc::make_mut(h);
            for cell in h.cells.iter_mut() {
                if cell.formula.as_deref().is_some_and(reads) {
                    cell.formula = None;
                }
            }
            h.map_rule_formulas(&|f| ibis_calc::formula::without_book(f, &name));
        }
    }
    wb.external_books[index].broken = true;
    invalidate_calc_index(presentation);
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells_force(presentation, &all)?;
    Ok(inverse)
}

fn set_table_filter(presentation: &mut Presentation, slide: SlideId, index: usize, filter: Option<crate::sheet::AutoFilter>) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let d1904 = date1904(presentation);
    {
        let sheet = sheet_mut(presentation, slide)?;
        let table = sheet.tables.get_mut(index).ok_or(EditError::IndexOutOfRange)?;
        table.filter = filter;
        refilter(sheet, d1904);
    }
    fit_frame(presentation, slide);
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

/// Hide the rows the sheet's filter and its tables' filters leave out,
/// the rows they hid before shown again first.
pub fn refilter(sheet: &mut Sheet, date1904: bool) {
    for row in sheet.filtered_rows.iter().copied() {
        if sheet.rows.get(row as usize) == Some(&0) {
            let size = sheet.hidden_rows.remove(&row).filter(|&s| s > 0).unwrap_or(if sheet.default_row > 0 { sheet.default_row } else { EMPTY_ROW });
            sheet.rows[row as usize] = size;
        }
    }
    let mut out = sheet.auto_filter.as_ref().map(|f| filtered_rows(sheet, f, date1904)).unwrap_or_default();
    for t in &sheet.tables {
        if let Some(f) = &t.filter {
            out.extend(filtered_rows(sheet, f, date1904));
        }
    }
    for row in out.iter().copied() {
        if sheet.rows.get(row as usize).is_some_and(|&r| r > 0) {
            sheet.hidden_rows.insert(row, sheet.rows[row as usize]);
            sheet.rows[row as usize] = 0;
        }
    }
    sheet.filtered_rows = out;
}

fn parse_range_spec(text: &str) -> Option<CellRange> {
    let mut parts = text.split(':');
    let a = ibis_calc::formula::parser::parse_cell(parts.next()?)?;
    let b = match parts.next() {
        Some(s) => ibis_calc::formula::parser::parse_cell(s)?,
        None => a.clone(),
    };
    if parts.next().is_some() || a.row > b.row || a.column > b.column {
        return None;
    }
    Some(CellRange { first_row: a.row, last_row: b.row, first_column: a.column, last_column: b.column })
}

fn advanced_filter(
    presentation: &mut Presentation,
    slide: SlideId,
    list_range: CellRange,
    criteria_range: CellRange,
    copy_to: Option<CellRange>,
    unique: bool,
) -> Result<Command, EditError> {
    if list_range.first_row > list_range.last_row
        || list_range.first_column > list_range.last_column
        || criteria_range.first_row > criteria_range.last_row
        || criteria_range.first_column > criteria_range.last_column
    {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    let sheet_name = presentation
        .workbook
        .as_ref()
        .and_then(|w| w.sheets.get(index))
        .map(|s| s.name.clone())
        .unwrap_or_default();

    let (matching_rows, list_headers) = {
        let sheet = sheet_mut(presentation, slide)?;
        check_bounds(sheet, list_range.last_row, list_range.last_column)?;
        check_bounds(sheet, criteria_range.last_row, criteria_range.last_column)?;

        let list_headers: Vec<(u32, String)> = (list_range.first_column..=list_range.last_column)
            .map(|col| (col, sheet.cell(list_range.first_row, col).map_or(String::new(), |c| c.text.clone())))
            .collect();

        let criteria_cols: Vec<(u32, Option<u32>)> = (criteria_range.first_column..=criteria_range.last_column)
            .map(|col| {
                let text = sheet.cell(criteria_range.first_row, col).map_or(String::new(), |c| c.text.trim().to_string());
                let list_col = list_headers.iter().find(|(_, h)| h.eq_ignore_ascii_case(&text)).map(|(c, _)| *c);
                (col, list_col)
            })
            .collect();

        let has_criteria_rows = criteria_range.last_row > criteria_range.first_row;
        let mut criteria_rules: Vec<Vec<(u32, String)>> = Vec::new();
        if has_criteria_rows {
            for cr in (criteria_range.first_row + 1)..=criteria_range.last_row {
                let mut row_conds = Vec::new();
                for &(crit_c, list_c) in &criteria_cols {
                    if let Some(lc) = list_c {
                        if let Some(cell) = sheet.cell(cr, crit_c) {
                            let t = cell.text.trim().to_string();
                            if !t.is_empty() {
                                row_conds.push((lc, t));
                            }
                        }
                    }
                }
                criteria_rules.push(row_conds);
            }
        }

        let cell_meets_cond = |cell: Option<&SheetCell>, cond: &str| -> bool {
            let cond = cond.trim();
            let unwrapped = if let Some(inner) = cond.strip_prefix("=\"").and_then(|s| s.strip_suffix('\"')) {
                inner
            } else if let Some(inner) = cond.strip_prefix('=').filter(|s| s.starts_with('"') && s.ends_with('"')) {
                &inner[1..inner.len() - 1]
            } else {
                cond
            };

            let (op, val) = if let Some(rest) = unwrapped.strip_prefix("<>") {
                ("notEqual", rest)
            } else if let Some(rest) = unwrapped.strip_prefix(">=") {
                ("greaterThanOrEqual", rest)
            } else if let Some(rest) = unwrapped.strip_prefix("<=") {
                ("lessThanOrEqual", rest)
            } else if let Some(rest) = unwrapped.strip_prefix('>') {
                ("greaterThan", rest)
            } else if let Some(rest) = unwrapped.strip_prefix('<') {
                ("lessThan", rest)
            } else if let Some(rest) = unwrapped.strip_prefix('=') {
                ("equal", rest)
            } else {
                ("equal", unwrapped)
            };

            if !unwrapped.starts_with(['<', '>', '=']) && !val.contains(['*', '?', '~']) {
                let text = cell.map_or("", |c| c.text.as_str());
                if val.trim().parse::<f64>().is_ok() {
                    compare_keeps(cell, op, val)
                } else {
                    text.to_lowercase().starts_with(&val.to_lowercase())
                }
            } else {
                compare_keeps(cell, op, val)
            }
        };

        let mut matching_rows = Vec::new();
        for r in (list_range.first_row + 1)..=list_range.last_row {
            let row_matches = if criteria_rules.is_empty() {
                true
            } else {
                criteria_rules.iter().any(|conds| {
                    if conds.is_empty() {
                        true
                    } else {
                        conds.iter().all(|(col, cond)| cell_meets_cond(sheet.cell(r, *col), cond))
                    }
                })
            };
            if row_matches {
                matching_rows.push(r);
            }
        }

        if unique {
            let mut seen = std::collections::HashSet::new();
            let mut unique_rows = Vec::new();
            for r in matching_rows {
                let key: Vec<String> = (list_range.first_column..=list_range.last_column)
                    .map(|c| sheet.cell(r, c).map_or(String::new(), |cell| format!("{:?}", cell.value)))
                    .collect();
                if seen.insert(key) {
                    unique_rows.push(r);
                }
            }
            matching_rows = unique_rows;
        }

        (matching_rows, list_headers)
    };

    match copy_to {
        None => {
            let sheet = sheet_mut(presentation, slide)?;
            let data_rows: Vec<u32> = ((list_range.first_row + 1)..=list_range.last_row).collect();
            let default_size = if sheet.default_row > 0 { sheet.default_row } else { EMPTY_ROW };
            for &r in &data_rows {
                if matching_rows.contains(&r) {
                    if sheet.rows.get(r as usize) == Some(&0) {
                        let size = sheet.hidden_rows.remove(&r).filter(|&s| s > 0).unwrap_or(default_size);
                        sheet.rows[r as usize] = size;
                    }
                    sheet.filtered_rows.remove(&r);
                } else {
                    if sheet.rows.get(r as usize).is_some_and(|&s| s > 0) {
                        sheet.hidden_rows.insert(r, sheet.rows[r as usize]);
                        sheet.rows[r as usize] = 0;
                    }
                    sheet.filtered_rows.insert(r);
                }
            }
            fit_frame(presentation, slide);
        }
        Some(dest) => {
            let sheet = sheet_mut(presentation, slide)?;
            let num_cols = list_range.last_column - list_range.first_column + 1;
            let dest_cols: Vec<(u32, u32)> = if dest.first_column == dest.last_column && dest.first_row == dest.last_row {
                (0..num_cols).map(|i| (list_range.first_column + i, dest.first_column + i)).collect()
            } else if dest.first_row == dest.last_row {
                (dest.first_column..=dest.last_column)
                    .filter_map(|dc| {
                        let text = sheet.cell(dest.first_row, dc).map_or(String::new(), |c| c.text.trim().to_string());
                        let sc = list_headers.iter().find(|(_, h)| h.eq_ignore_ascii_case(&text)).map(|(c, _)| *c)?;
                        Some((sc, dc))
                    })
                    .collect()
            } else {
                (0..num_cols).map(|i| (list_range.first_column + i, dest.first_column + i)).collect()
            };

            for &(sc, dc) in &dest_cols {
                if let Some(cell) = sheet.cell(list_range.first_row, sc).cloned() {
                    sheet.put_cell(dest.first_row, dc, Some(SheetCell { row: dest.first_row, column: dc, ..cell }));
                }
            }

            for (i, &from_row) in matching_rows.iter().enumerate() {
                let to_row = dest.first_row + 1 + i as u32;
                for &(sc, dc) in &dest_cols {
                    if let Some(cell) = sheet.cell(from_row, sc).cloned() {
                        let formula = cell.formula.as_ref().map(|f| ibis_calc::formula::shift(f, to_row as i64 - from_row as i64, dc as i64 - sc as i64));
                        sheet.put_cell(to_row, dc, Some(SheetCell { row: to_row, column: dc, formula, ..cell }));
                    }
                }
            }
            fit_frame(presentation, slide);
        }
    }

    let a = |c: u32, r: u32| format!("${}${}", crate::sheet::cell_name(c, 0).trim_end_matches('1'), r + 1);
    let to_formula = |sheet_name: &str, r: CellRange| {
        format!("{}!{}:{}", ibis_calc::formula::quote_sheet(sheet_name), a(r.first_column, r.first_row), a(r.last_column, r.last_row))
    };

    let filter_db = to_formula(&sheet_name, list_range);
    let criteria_f = to_formula(&sheet_name, criteria_range);

    if let Some(w) = presentation.workbook.as_mut() {
        let wb = Arc::make_mut(w);
        let names = &mut wb.names;
        names.retain(|n| !(n.name.eq_ignore_ascii_case("_FilterDatabase") && n.sheet == Some(index)));
        names.push(crate::sheet::DefinedName { name: "_FilterDatabase".into(), sheet: Some(index), formula: filter_db, hidden: true });
        names.retain(|n| !(n.name.eq_ignore_ascii_case("Criteria") && n.sheet == Some(index)));
        names.push(crate::sheet::DefinedName { name: "Criteria".into(), sheet: Some(index), formula: criteria_f, hidden: false });

        if let Some(dest) = copy_to {
            let num_extracted = matching_rows.len() as u32;
            let extract_range = CellRange {
                first_row: dest.first_row,
                last_row: dest.first_row + num_extracted,
                first_column: dest.first_column,
                last_column: dest.first_column + (list_range.last_column - list_range.first_column),
            };
            let extract_f = to_formula(&sheet_name, extract_range);
            names.retain(|n| !(n.name.eq_ignore_ascii_case("Extract") && n.sheet == Some(index)));
            names.push(crate::sheet::DefinedName { name: "Extract".into(), sheet: Some(index), formula: extract_f, hidden: false });
        }
    }

    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

fn apply_consolidate(
    presentation: &mut Presentation,
    slide: SlideId,
    destination: (u32, u32),
    consolidate: crate::sheet::DataConsolidate,
) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    let d1904 = date1904(presentation);

    {
        let sheet = sheet_mut(presentation, slide)?;
        sheet.data_consolidate = Some(consolidate.clone());
    }

    struct SourceData {
        cells: Vec<((u32, u32), CellValue)>,
        range: CellRange,
    }

    let mut sources: Vec<SourceData> = Vec::new();
    for ref_str in &consolidate.data_refs {
        let (sheet_opt, cell_part) = if let Some((s, rest)) = ref_str.split_once('!') {
            (Some(s.trim().trim_matches('\'')), rest.trim())
        } else {
            (None, ref_str.trim())
        };

        let target_slide = match sheet_opt {
            Some(sname) => {
                let s_idx = presentation.workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(sname)));
                match s_idx {
                    Some(i) => presentation.workbook.as_ref().and_then(|w| w.sheets.get(i)).and_then(|s| s.slide),
                    None => None,
                }
            }
            None => Some(slide),
        };

        let Some(tslide) = target_slide else { continue };
        let Some(sh) = presentation.slide(tslide).and_then(crate::sheet_calc::sheet_of_slide) else { continue };

        let clean = cell_part.replace('$', "");
        let Some(range) = parse_range_spec(&clean) else { continue };

        let mut cells = Vec::new();
        for r in range.first_row..=range.last_row {
            for c in range.first_column..=range.last_column {
                if let Some(cell) = sh.cell(r, c) {
                    if !cell.value.is_empty() {
                        cells.push(((r, c), cell.value.clone()));
                    }
                }
            }
        }
        sources.push(SourceData { cells, range });
    }

    if sources.is_empty() {
        return Ok(inverse);
    }

    let aggregate = |nums: &[f64], func: crate::sheet::ConsolidateFunction| -> f64 {
        use crate::sheet::ConsolidateFunction::*;
        if nums.is_empty() {
            return 0.0;
        }
        match func {
            Sum => nums.iter().sum(),
            Count | CountNums => nums.len() as f64,
            Average => nums.iter().sum::<f64>() / nums.len() as f64,
            Max => nums.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Min => nums.iter().copied().fold(f64::INFINITY, f64::min),
            Product => nums.iter().product(),
            StdDev => {
                if nums.len() < 2 {
                    0.0
                } else {
                    let m = nums.iter().sum::<f64>() / nums.len() as f64;
                    (nums.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (nums.len() - 1) as f64).sqrt()
                }
            }
            StdDevp => {
                let m = nums.iter().sum::<f64>() / nums.len() as f64;
                (nums.iter().map(|x| (x - m).powi(2)).sum::<f64>() / nums.len() as f64).sqrt()
            }
            Var => {
                if nums.len() < 2 {
                    0.0
                } else {
                    let m = nums.iter().sum::<f64>() / nums.len() as f64;
                    nums.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (nums.len() - 1) as f64
                }
            }
            Varp => {
                let m = nums.iter().sum::<f64>() / nums.len() as f64;
                nums.iter().map(|x| (x - m).powi(2)).sum::<f64>() / nums.len() as f64
            }
        }
    };

    let (dest_row, dest_col) = destination;
    let mut outputs: Vec<((u32, u32), CellValue)> = Vec::new();

    if consolidate.top_labels || consolidate.left_labels {
        let mut col_labels: Vec<String> = Vec::new();
        let mut row_labels: Vec<String> = Vec::new();

        for src in &sources {
            if consolidate.top_labels {
                let r0 = src.range.first_row;
                let c_start = if consolidate.left_labels { src.range.first_column + 1 } else { src.range.first_column };
                for c in c_start..=src.range.last_column {
                    if let Some((_, val)) = src.cells.iter().find(|&(pos, _)| *pos == (r0, c)) {
                        let s = match val {
                            CellValue::Text(t) => t.clone(),
                            CellValue::Number(n) => n.to_string(),
                            _ => String::new(),
                        };
                        if !s.is_empty() && !col_labels.contains(&s) {
                            col_labels.push(s);
                        }
                    }
                }
            }
            if consolidate.left_labels {
                let c0 = src.range.first_column;
                let r_start = if consolidate.top_labels { src.range.first_row + 1 } else { src.range.first_row };
                for r in r_start..=src.range.last_row {
                    if let Some((_, val)) = src.cells.iter().find(|&(pos, _)| *pos == (r, c0)) {
                        let s = match val {
                            CellValue::Text(t) => t.clone(),
                            CellValue::Number(n) => n.to_string(),
                            _ => String::new(),
                        };
                        if !s.is_empty() && !row_labels.contains(&s) {
                            row_labels.push(s);
                        }
                    }
                }
            }
        }

        if consolidate.top_labels && consolidate.left_labels {
            for (j, clabel) in col_labels.iter().enumerate() {
                outputs.push(((dest_row, dest_col + 1 + j as u32), CellValue::Text(clabel.clone())));
            }
            for (i, rlabel) in row_labels.iter().enumerate() {
                outputs.push(((dest_row + 1 + i as u32, dest_col), CellValue::Text(rlabel.clone())));
            }
            for (i, rlabel) in row_labels.iter().enumerate() {
                for (j, clabel) in col_labels.iter().enumerate() {
                    let mut nums = Vec::new();
                    for src in &sources {
                        let r0 = src.range.first_row;
                        let c0 = src.range.first_column;
                        let match_rows: Vec<u32> = src
                            .cells
                            .iter()
                            .filter_map(|((r, c), val)| {
                                if *c == c0 {
                                    match val {
                                        CellValue::Text(t) if t.eq_ignore_ascii_case(rlabel) => Some(*r),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();
                        let match_cols: Vec<u32> = src
                            .cells
                            .iter()
                            .filter_map(|((r, c), val)| {
                                if *r == r0 {
                                    match val {
                                        CellValue::Text(t) if t.eq_ignore_ascii_case(clabel) => Some(*c),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();

                        for &mr in &match_rows {
                            for &mc in &match_cols {
                                if let Some((_, CellValue::Number(n))) = src.cells.iter().find(|&(p, _)| *p == (mr, mc)) {
                                    nums.push(*n);
                                }
                            }
                        }
                    }
                    if !nums.is_empty() {
                        let val = aggregate(&nums, consolidate.function);
                        outputs.push(((dest_row + 1 + i as u32, dest_col + 1 + j as u32), CellValue::Number(val)));
                    }
                }
            }
        } else if consolidate.top_labels {
            for (j, clabel) in col_labels.iter().enumerate() {
                outputs.push(((dest_row, dest_col + j as u32), CellValue::Text(clabel.clone())));
            }
            let max_data_rows = sources.iter().map(|s| s.range.last_row - s.range.first_row).max().unwrap_or(0);
            for rel_r in 1..=max_data_rows {
                for (j, clabel) in col_labels.iter().enumerate() {
                    let mut nums = Vec::new();
                    for src in &sources {
                        let r0 = src.range.first_row;
                        let row = r0 + rel_r;
                        if row <= src.range.last_row {
                            let match_cols: Vec<u32> = src
                                .cells
                                .iter()
                                .filter_map(|((r, c), val)| {
                                    if *r == r0 {
                                        match val {
                                            CellValue::Text(t) if t.eq_ignore_ascii_case(clabel) => Some(*c),
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            for &mc in &match_cols {
                                if let Some((_, CellValue::Number(n))) = src.cells.iter().find(|&(p, _)| *p == (row, mc)) {
                                    nums.push(*n);
                                }
                            }
                        }
                    }
                    if !nums.is_empty() {
                        let val = aggregate(&nums, consolidate.function);
                        outputs.push(((dest_row + rel_r, dest_col + j as u32), CellValue::Number(val)));
                    }
                }
            }
        } else if consolidate.left_labels {
            for (i, rlabel) in row_labels.iter().enumerate() {
                outputs.push(((dest_row + i as u32, dest_col), CellValue::Text(rlabel.clone())));
            }
            let max_data_cols = sources.iter().map(|s| s.range.last_column - s.range.first_column).max().unwrap_or(0);
            for rel_c in 1..=max_data_cols {
                for (i, rlabel) in row_labels.iter().enumerate() {
                    let mut nums = Vec::new();
                    for src in &sources {
                        let c0 = src.range.first_column;
                        let col = c0 + rel_c;
                        if col <= src.range.last_column {
                            let match_rows: Vec<u32> = src
                                .cells
                                .iter()
                                .filter_map(|((r, c), val)| {
                                    if *c == c0 {
                                        match val {
                                            CellValue::Text(t) if t.eq_ignore_ascii_case(rlabel) => Some(*r),
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            for &mr in &match_rows {
                                if let Some((_, CellValue::Number(n))) = src.cells.iter().find(|&(p, _)| *p == (mr, col)) {
                                    nums.push(*n);
                                }
                            }
                        }
                    }
                    if !nums.is_empty() {
                        let val = aggregate(&nums, consolidate.function);
                        outputs.push(((dest_row + i as u32, dest_col + rel_c), CellValue::Number(val)));
                    }
                }
            }
        }
    } else {
        let max_r = sources.iter().map(|s| s.range.last_row - s.range.first_row + 1).max().unwrap_or(0);
        let max_c = sources.iter().map(|s| s.range.last_column - s.range.first_column + 1).max().unwrap_or(0);

        for r in 0..max_r {
            for c in 0..max_c {
                let mut nums = Vec::new();
                for src in &sources {
                    let target = (src.range.first_row + r, src.range.first_column + c);
                    if let Some((_, CellValue::Number(n))) = src.cells.iter().find(|&(p, _)| *p == target) {
                        nums.push(*n);
                    }
                }
                if !nums.is_empty() {
                    let val = aggregate(&nums, consolidate.function);
                    outputs.push(((dest_row + r, dest_col + c), CellValue::Number(val)));
                }
            }
        }
    }

    let default = default_style(presentation);
    {
        let sheet = sheet_mut(presentation, slide)?;
        let default_idx = sheet.style_index(default.clone());
        for &((r, c), ref val) in &outputs {
            check_bounds(sheet, r, c)?;
            let text = display_text(val, &default, d1904);
            let kind = CellKind::of(val);
            sheet.put_cell(
                r,
                c,
                Some(SheetCell {
                    runs: None,
                    phonetic: None,
                    row: r,
                    column: c,
                    value: val.clone(),
                    text,
                    kind,
                    formula: None,
                    style: default_idx,
                }),
            );
        }
        fit_frame(presentation, slide);
    }

    let mut changed = Vec::new();
    for &((r, c), _) in &outputs {
        changed.push((index, r, c));
    }
    recalculate_cells(presentation, &changed)?;
    Ok(inverse)
}

fn apply_add_scenario(presentation: &mut Presentation, slide: SlideId, scenario: crate::sheet::SheetScenario) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let scenarios = sheet.scenarios.get_or_insert_with(crate::sheet::SheetScenarios::default);
    if scenarios.sqref.is_none() || scenarios.sqref.as_deref() == Some("") {
        let refs: Vec<String> = scenario.input_cells.iter().map(|ic| ic.r.clone()).collect();
        scenarios.sqref = Some(refs.join(" "));
    }
    scenarios.scenarios.push(scenario);
    scenarios.current = Some((scenarios.scenarios.len() - 1) as u32);
    Ok(inverse)
}

fn apply_edit_scenario(presentation: &mut Presentation, slide: SlideId, index: usize, scenario: crate::sheet::SheetScenario) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let Some(scenarios) = sheet.scenarios.as_mut() else {
        return Err(EditError::IndexOutOfRange);
    };
    if index >= scenarios.scenarios.len() {
        return Err(EditError::IndexOutOfRange);
    }
    scenarios.scenarios[index] = scenario;
    scenarios.current = Some(index as u32);
    Ok(inverse)
}

fn apply_delete_scenario(presentation: &mut Presentation, slide: SlideId, index: usize) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let Some(scenarios) = sheet.scenarios.as_mut() else {
        return Err(EditError::IndexOutOfRange);
    };
    if index >= scenarios.scenarios.len() {
        return Err(EditError::IndexOutOfRange);
    }
    scenarios.scenarios.remove(index);
    if scenarios.scenarios.is_empty() {
        sheet.scenarios = None;
    } else {
        scenarios.current = Some(index.min(scenarios.scenarios.len() - 1) as u32);
    }
    Ok(inverse)
}

fn apply_show_scenario(presentation: &mut Presentation, slide: SlideId, index: usize) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet_idx = workbook_index(presentation, slide)?;
    let d1904 = date1904(presentation);
    let (sc, current_idx) = {
        let sheet = sheet_of(presentation, slide)?;
        let Some(scenarios) = sheet.scenarios.as_ref() else {
            return Err(EditError::IndexOutOfRange);
        };
        let sc = scenarios.scenarios.get(index).cloned().ok_or(EditError::IndexOutOfRange)?;
        (sc, index as u32)
    };
    {
        let sheet = sheet_mut(presentation, slide)?;
        if let Some(scenarios) = sheet.scenarios.as_mut() {
            scenarios.show = Some(current_idx);
            scenarios.current = Some(current_idx);
        }
    }
    let default = default_style(presentation);
    let mut changed = Vec::new();
    for ic in &sc.input_cells {
        let Some(cell_ref) = ibis_calc::formula::parser::parse_cell(&ic.r) else { continue };
        let col = cell_ref.column;
        let row = cell_ref.row;
        let val = if let Ok(n) = ic.val.parse::<f64>() {
            CellValue::Number(n)
        } else if ic.val.eq_ignore_ascii_case("true") {
            CellValue::Bool(true)
        } else if ic.val.eq_ignore_ascii_case("false") {
            CellValue::Bool(false)
        } else {
            CellValue::Text(ic.val.clone())
        };
        let sheet = sheet_mut(presentation, slide)?;
        let style_idx = sheet.cells.iter().find(|c| c.row == row && c.column == col).map(|c| c.style).unwrap_or_else(|| sheet.style_index(default.clone()));
        let st = sheet.styles.get(style_idx as usize).cloned().unwrap_or_else(|| default.clone());
        let text = display_text(&val, &st, d1904);
        let kind = CellKind::of(&val);
        sheet.put_cell(
            row,
            col,
            Some(SheetCell {
                runs: None,
                phonetic: None,
                row,
                column: col,
                value: val,
                text,
                kind,
                formula: None,
                style: style_idx,
            }),
        );
        changed.push((sheet_idx, row, col));
    }
    recalculate_cells(presentation, &changed)?;
    Ok(inverse)
}

fn apply_create_scenario_summary(presentation: &mut Presentation, slide: SlideId, result_cell_specs: Vec<String>) -> Result<Command, EditError> {
    let (scenarios, source_sheet_idx, source_sheet_name) = {
        let idx = workbook_index(presentation, slide)?;
        let sheet = sheet_of(presentation, slide)?;
        let name = presentation.workbook.as_ref().map(|w| w.sheets[idx].name.clone()).unwrap_or_default();
        let Some(sc) = sheet.scenarios.as_ref().cloned() else {
            return Err(EditError::IndexOutOfRange);
        };
        if sc.scenarios.is_empty() {
            return Err(EditError::IndexOutOfRange);
        }
        (sc, idx, name)
    };

    // 1. Collect changing cells (column, row, default_ref)
    let mut changing_cells: Vec<(u32, u32, String)> = Vec::new();
    for sc in &scenarios.scenarios {
        for ic in &sc.input_cells {
            if let Some(c) = ibis_calc::formula::parser::parse_cell(&ic.r) {
                if !changing_cells.iter().any(|(col, row, _)| *col == c.column && *row == c.row) {
                    let name = cell_name(c.column, c.row);
                    let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
                    let ref_name = format!("${}${}", &name[..split], &name[split..]);
                    changing_cells.push((c.column, c.row, ref_name));
                }
            }
        }
    }
    if changing_cells.is_empty() {
        return Err(EditError::IndexOutOfRange);
    }

    // 2. Collect result cells (column, row, default_ref)
    let mut result_cells: Vec<(u32, u32, String)> = Vec::new();
    for spec in &result_cell_specs {
        if spec.contains(':') {
            if let Some(r) = parse_range_spec(spec) {
                for row in r.first_row..=r.last_row {
                    for col in r.first_column..=r.last_column {
                        if !result_cells.iter().any(|(c, rw, _)| *c == col && *rw == row) {
                            let name = cell_name(col, row);
                            let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
                            let ref_name = format!("${}${}", &name[..split], &name[split..]);
                            result_cells.push((col, row, ref_name));
                        }
                    }
                }
            }
        } else if let Some(c) = ibis_calc::formula::parser::parse_cell(spec) {
            if !result_cells.iter().any(|(col, row, _)| *col == c.column && *row == c.row) {
                let name = cell_name(c.column, c.row);
                let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
                let ref_name = format!("${}${}", &name[..split], &name[split..]);
                result_cells.push((c.column, c.row, ref_name));
            }
        }
    }
    if result_cells.is_empty() {
        return Err(EditError::IndexOutOfRange);
    }

    // Defined names lookup
    let defined_names = presentation.workbook.as_ref().map(|w| w.names.clone()).unwrap_or_default();
    let name_for_cell = |col: u32, row: u32, default_ref: &str| -> String {
        let name = cell_name(col, row);
        let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
        let unquoted = format!("${}${}", &name[..split], &name[split..]);
        let quoted = format!("{}!{}", source_sheet_name, unquoted);
        for dn in &defined_names {
            if dn.formula == quoted || dn.formula == unquoted {
                return dn.name.clone();
            }
        }
        default_ref.to_string()
    };

    // 3. Read current values
    let current_changing: Vec<CellValue> = {
        let sheet = sheet_of(presentation, slide)?;
        changing_cells.iter().map(|&(col, row, _)| {
            sheet.cells.iter().find(|c| c.row == row && c.column == col).map(|c| c.value.clone()).unwrap_or(CellValue::Empty)
        }).collect()
    };
    let current_results: Vec<CellValue> = {
        let sheet = sheet_of(presentation, slide)?;
        result_cells.iter().map(|&(col, row, _)| {
            sheet.cells.iter().find(|c| c.row == row && c.column == col).map(|c| c.value.clone()).unwrap_or(CellValue::Empty)
        }).collect()
    };

    // 4. Evaluate each scenario on a cloned presentation
    let d1904 = date1904(presentation);
    let mut scenario_evaluations: Vec<(String, Vec<CellValue>, Vec<CellValue>)> = Vec::new();
    for sc in &scenarios.scenarios {
        let mut work = presentation.clone();
        let mut changed = Vec::new();
        for ic in &sc.input_cells {
            if let Some(c) = ibis_calc::formula::parser::parse_cell(&ic.r) {
                let col = c.column;
                let row = c.row;
                let val = if let Ok(n) = ic.val.parse::<f64>() {
                    CellValue::Number(n)
                } else if ic.val.eq_ignore_ascii_case("true") {
                    CellValue::Bool(true)
                } else if ic.val.eq_ignore_ascii_case("false") {
                    CellValue::Bool(false)
                } else {
                    CellValue::Text(ic.val.clone())
                };
                let default = default_style(&work);
                if let Ok(sheet) = sheet_mut(&mut work, slide) {
                    let style_idx = sheet.cells.iter().find(|cell| cell.row == row && cell.column == col).map(|cell| cell.style).unwrap_or_else(|| sheet.style_index(default.clone()));
                    let st = sheet.styles.get(style_idx as usize).cloned().unwrap_or(default);
                    let text = display_text(&val, &st, d1904);
                    let kind = CellKind::of(&val);
                    sheet.put_cell(
                        row,
                        col,
                        Some(SheetCell {
                            runs: None,
                            phonetic: None,
                            row,
                            column: col,
                            value: val,
                            text,
                            kind,
                            formula: None,
                            style: style_idx,
                        }),
                    );
                }
                changed.push((source_sheet_idx, row, col));
            }
        }
        let _ = recalculate_cells(&mut work, &changed);
        let sc_changing: Vec<CellValue> = changing_cells.iter().map(|&(col, row, _)| {
            sc.input_cells.iter().find_map(|ic| {
                if let Some(c) = ibis_calc::formula::parser::parse_cell(&ic.r) {
                    if c.column == col && c.row == row {
                        return if let Ok(n) = ic.val.parse::<f64>() {
                            Some(CellValue::Number(n))
                        } else {
                            Some(CellValue::Text(ic.val.clone()))
                        };
                    }
                }
                None
            }).unwrap_or(CellValue::Empty)
        }).collect();
        let sc_results: Vec<CellValue> = {
            let sheet = sheet_of(&work, slide)?;
            result_cells.iter().map(|&(col, row, _)| {
                sheet.cells.iter().find(|c| c.row == row && c.column == col).map(|c| c.value.clone()).unwrap_or(CellValue::Empty)
            }).collect()
        };
        scenario_evaluations.push((sc.name.clone(), sc_changing, sc_results));
    }

    // 5. Build summary sheet
    let mut sum_sheet = empty_sheet(presentation);
    let total_cols = 3 + scenario_evaluations.len() + 2;
    if sum_sheet.columns.len() < total_cols {
        sum_sheet.columns.resize(total_cols, EMPTY_COLUMN);
    }
    sum_sheet.columns[0] = 30 * crate::geometry::EMU_PER_POINT;
    sum_sheet.columns[1] = 140 * crate::geometry::EMU_PER_POINT;
    sum_sheet.columns[2] = 110 * crate::geometry::EMU_PER_POINT;
    for i in 0..scenario_evaluations.len() {
        sum_sheet.columns[3 + i] = 110 * crate::geometry::EMU_PER_POINT;
    }

    let default_st = default_style(presentation);
    let title_st = CellStyle { bold: true, font_size: 1400, ..default_st.clone() };
    let header_st = CellStyle { bold: true, fill: Some(Rgb(0xD9D9D9)), align: CellAlign::Center, ..default_st.clone() };
    let section_st = CellStyle { bold: true, ..default_st.clone() };
    let label_st = CellStyle { align: CellAlign::Left, ..default_st.clone() };
    let val_st = CellStyle { align: CellAlign::Right, ..default_st.clone() };
    let note_st = CellStyle { italic: true, font_size: 900, ..default_st.clone() };

    let s_idx_title = sum_sheet.style_index(title_st);
    let s_idx_header = sum_sheet.style_index(header_st);
    let s_idx_section = sum_sheet.style_index(section_st);
    let s_idx_label = sum_sheet.style_index(label_st);
    let s_idx_val = sum_sheet.style_index(val_st);
    let s_idx_note = sum_sheet.style_index(note_st);

    let add_c = |sheet: &mut Sheet, r: u32, c: u32, val: CellValue, style: u32| {
        let st = sheet.styles.get(style as usize).cloned().unwrap_or_else(|| default_st.clone());
        let text = display_text(&val, &st, d1904);
        let kind = CellKind::of(&val);
        sheet.put_cell(
            r,
            c,
            Some(SheetCell {
                runs: None,
                phonetic: None,
                row: r,
                column: c,
                value: val,
                text,
                kind,
                formula: None,
                style,
            }),
        );
    };

    // Title at Row 1, Col 1 (B2)
    add_c(&mut sum_sheet, 1, 1, CellValue::Text("シナリオの要約".into()), s_idx_title);

    // Table Header at Row 3 (B4..): Col 2: 現在の値, Col 3..: Scenario names
    add_c(&mut sum_sheet, 3, 2, CellValue::Text("現在の値".into()), s_idx_header);
    for (i, (name, _, _)) in scenario_evaluations.iter().enumerate() {
        add_c(&mut sum_sheet, 3, 3 + i as u32, CellValue::Text(name.clone()), s_idx_header);
    }

    // Section 1: 変化させるセル (Row 4, Col 1)
    add_c(&mut sum_sheet, 4, 1, CellValue::Text("変化させるセル:".into()), s_idx_section);
    let mut current_row = 5u32;
    for (ci, (col, row, default_ref)) in changing_cells.iter().enumerate() {
        let label = name_for_cell(*col, *row, default_ref);
        add_c(&mut sum_sheet, current_row, 1, CellValue::Text(label), s_idx_label);
        add_c(&mut sum_sheet, current_row, 2, current_changing[ci].clone(), s_idx_val);
        for (i, (_, sc_changing, _)) in scenario_evaluations.iter().enumerate() {
            add_c(&mut sum_sheet, current_row, 3 + i as u32, sc_changing[ci].clone(), s_idx_val);
        }
        current_row += 1;
    }

    // Section 2: 結果セル
    add_c(&mut sum_sheet, current_row, 1, CellValue::Text("結果セル:".into()), s_idx_section);
    current_row += 1;
    for (ri, (col, row, default_ref)) in result_cells.iter().enumerate() {
        let label = name_for_cell(*col, *row, default_ref);
        add_c(&mut sum_sheet, current_row, 1, CellValue::Text(label), s_idx_label);
        add_c(&mut sum_sheet, current_row, 2, current_results[ri].clone(), s_idx_val);
        for (i, (_, _, sc_results)) in scenario_evaluations.iter().enumerate() {
            add_c(&mut sum_sheet, current_row, 3 + i as u32, sc_results[ri].clone(), s_idx_val);
        }
        current_row += 1;
    }

    // Note row
    current_row += 1;
    add_c(&mut sum_sheet, current_row, 1, CellValue::Text("注: 各シナリオの入力値は、現在の値とは異なる場合があります。".into()), s_idx_note);

    let new_name = {
        let existing: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
        let base = "シナリオの要約";
        if !existing.iter().any(|s| s.eq_ignore_ascii_case(base)) {
            base.to_string()
        } else {
            let mut num = 2;
            loop {
                let candidate = format!("{base} {num}");
                if !existing.iter().any(|s| s.eq_ignore_ascii_case(&candidate)) {
                    break candidate;
                }
                num += 1;
            }
        }
    };

    let new_slide = presentation.new_slide_id();
    let shape_id = presentation.new_shape_id();
    structural(presentation, StructureEdit::AddSheet {
        sheet: source_sheet_idx,
        name: new_name,
        slide: new_slide,
        shape: shape_id,
        content: Arc::new(sum_sheet),
    })
}

/// A cell of a report sheet.
#[derive(Clone, Debug)]
pub struct ReportCell {
    pub row: u32,
    pub column: u32,
    pub value: CellValue,
    pub bold: bool,
}

impl PartialEq for ReportCell {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row && self.column == other.column && self.value == other.value && self.bold == other.bold
    }
}

impl Eq for ReportCell {}

fn apply_create_report_sheet(presentation: &mut Presentation, slide: SlideId, name: String, cells: Vec<ReportCell>, widths: Vec<(u32, u32)>) -> Result<Command, EditError> {
    let index = workbook_index(presentation, slide)?;
    let d1904 = date1904(presentation);
    let mut sheet = empty_sheet(presentation);
    let columns = cells.iter().map(|c| c.column + 1).chain(widths.iter().map(|w| w.0 + 1)).max().unwrap_or(1) as usize;
    if sheet.columns.len() < columns {
        sheet.columns.resize(columns, EMPTY_COLUMN);
    }
    for (column, points) in widths {
        sheet.columns[column as usize] = points as i64 * crate::geometry::EMU_PER_POINT;
    }
    let plain = default_style(presentation);
    let bold = CellStyle { bold: true, ..plain.clone() };
    let plain_index = sheet.style_index(plain.clone());
    let bold_index = sheet.style_index(bold.clone());
    for c in cells {
        let (style, st) = if c.bold { (bold_index, &bold) } else { (plain_index, &plain) };
        let text = display_text(&c.value, st, d1904);
        let kind = CellKind::of(&c.value);
        sheet.put_cell(c.row, c.column, Some(SheetCell { runs: None, phonetic: None, row: c.row, column: c.column, value: c.value, text, kind, formula: None, style }));
    }
    let existing: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
    let new_name = if !existing.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
        name
    } else {
        (2..).map(|n| format!("{name} {n}")).find(|c| !existing.iter().any(|s| s.eq_ignore_ascii_case(c))).unwrap_or(name)
    };
    let new_slide = presentation.new_slide_id();
    let shape_id = presentation.new_shape_id();
    structural(presentation, StructureEdit::AddSheet { sheet: index, name: new_name, slide: new_slide, shape: shape_id, content: Arc::new(sheet) })
}

/// The cells of a query's result as a sheet: the headings in bold, the dates and times in their formats,
/// the columns as wide as their texts.
fn query_sheet(presentation: &Presentation, table: &crate::query::QTable) -> Sheet {
    let d1904 = date1904(presentation);
    let mut sheet = empty_sheet(presentation);
    let plain = default_style(presentation);
    let bold = CellStyle { bold: true, ..plain.clone() };
    let header_index = sheet.style_index(bold.clone());
    let columns = table.columns.len().max(1);
    if sheet.columns.len() < columns {
        sheet.columns.resize(columns, EMPTY_COLUMN);
    }
    let mut widths = vec![0usize; columns];
    for (c, col) in table.columns.iter().enumerate() {
        widths[c] = col.name.chars().count() * 2;
        sheet.put_cell(0, c as u32, Some(SheetCell { runs: None, phonetic: None, row: 0, column: c as u32, value: CellValue::Text(col.name.clone()), text: col.name.clone(), kind: CellKind::Text, formula: None, style: header_index }));
    }
    let styles: Vec<(u32, CellStyle)> = table
        .columns
        .iter()
        .map(|col| {
            let st = match col.ty.number_format() {
                Some(f) => CellStyle { number_format: f.to_string(), ..plain.clone() },
                None => plain.clone(),
            };
            (sheet.style_index(st.clone()), st)
        })
        .collect();
    for (r, row) in table.rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            if matches!(value, CellValue::Empty) {
                continue;
            }
            let (style, st) = &styles[c];
            let text = display_text(value, st, d1904);
            widths[c] = widths[c].max(text.chars().map(|ch| if ch.is_ascii() { 1 } else { 2 }).sum());
            let row_index = r as u32 + 1;
            sheet.put_cell(row_index, c as u32, Some(SheetCell { runs: None, phonetic: None, row: row_index, column: c as u32, value: value.clone(), text, kind: CellKind::of(value), formula: None, style: *style }));
        }
    }
    for (c, w) in widths.into_iter().enumerate() {
        let points = ((w as i64 + 3) * 6).clamp(48, 300);
        sheet.columns[c] = points * crate::geometry::EMU_PER_POINT;
    }
    sheet
}

fn apply_load_query(presentation: &mut Presentation, slide: SlideId, mut query: crate::query::Query, table: crate::query::QTable) -> Result<Command, EditError> {
    let index = workbook_index(presentation, slide)?;
    let fresh = query_sheet(presentation, &table);
    // The sheet the query loaded before is written again; else a new one is made.
    let old = query.output.as_ref().and_then(|name| {
        let wb = presentation.workbook.as_ref()?;
        let i = wb.sheet_index(name)?;
        wb.sheets[i].slide
    });
    let inverse = match old {
        Some(old_slide) => {
            let inverse = snapshot(presentation);
            let sheet = sheet_mut(presentation, old_slide)?;
            sheet.cells = fresh.cells;
            sheet.styles = fresh.styles;
            sheet.columns = fresh.columns;
            sheet.rows = fresh.rows;
            invalidate_calc_index(presentation);
            inverse
        }
        None => {
            let existing: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
            let base = query.name.clone();
            let name = if !existing.iter().any(|s| s.eq_ignore_ascii_case(&base)) { base.clone() } else { (2..).map(|n| format!("{base} {n}")).find(|c| !existing.iter().any(|s| s.eq_ignore_ascii_case(c))).unwrap_or(base) };
            let new_slide = presentation.new_slide_id();
            let shape_id = presentation.new_shape_id();
            query.output = Some(name.clone());
            structural(presentation, StructureEdit::AddSheet { sheet: index, name, slide: new_slide, shape: shape_id, content: Arc::new(fresh) })?
        }
    };
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    match wb.queries.iter().position(|q| q.name == query.name) {
        Some(i) => wb.queries[i] = query,
        None => wb.queries.push(query),
    }
    Ok(inverse)
}

fn apply_set_solver_names(presentation: &mut Presentation, slide: SlideId, names: Vec<(String, String)>) -> Result<Command, EditError> {
    let index = workbook_index(presentation, slide)?;
    let inverse = snapshot(presentation);
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    let is_solver = |n: &str| n.to_ascii_lowercase().starts_with("solver_");
    // The numbered ones (lhs, rel, rhs) are all replaced; the others where given.
    let numbered = |n: &str| {
        let l = n.to_ascii_lowercase();
        ["solver_lhs", "solver_rel", "solver_rhs"].iter().any(|p| l.starts_with(p))
    };
    wb.names.retain(|n| !(n.sheet == Some(index) && is_solver(&n.name) && (numbered(&n.name) || names.iter().any(|(k, _)| k.eq_ignore_ascii_case(&n.name)))));
    for (name, formula) in names {
        wb.names.push(crate::sheet::DefinedName { name, sheet: Some(index), formula, hidden: true });
    }
    invalidate_calc_index(presentation);
    Ok(inverse)
}

fn apply_merge_scenarios(presentation: &mut Presentation, slide: SlideId, from_slide: SlideId) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let from_scenarios = {
        let sheet = sheet_of(presentation, from_slide)?;
        sheet.scenarios.as_ref().map(|s| s.scenarios.clone()).unwrap_or_default()
    };
    if from_scenarios.is_empty() {
        return Ok(inverse);
    }
    let sheet = sheet_mut(presentation, slide)?;
    let scenarios = sheet.scenarios.get_or_insert_with(crate::sheet::SheetScenarios::default);
    for sc in from_scenarios {
        if scenarios.sqref.is_none() || scenarios.sqref.as_deref() == Some("") {
            let refs: Vec<String> = sc.input_cells.iter().map(|ic| ic.r.clone()).collect();
            scenarios.sqref = Some(refs.join(" "));
        }
        scenarios.scenarios.push(sc);
    }
    scenarios.current = Some((scenarios.scenarios.len() - 1) as u32);
    Ok(inverse)
}

fn presentation_has_tables(presentation: &Presentation) -> bool {
    presentation.slides.iter().any(|s| {
        sheet_of(presentation, s.id).map_or(false, |sh| !sh.tables.is_empty())
    })
}

fn apply_add_custom_view(
    presentation: &mut Presentation,
    view: crate::sheet::CustomWorkbookView,
    sheet_views: Vec<(SlideId, crate::sheet::CustomSheetView)>,
) -> Result<Command, EditError> {
    if presentation_has_tables(presentation) {
        return Err(EditError::CustomViewsDisabledByTable);
    }
    let inverse = snapshot(presentation);
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    if let Some(pos) = wb.custom_views.iter().position(|v| v.name == view.name) {
        wb.custom_views[pos] = view.clone();
    } else {
        wb.custom_views.push(view.clone());
    }
    for (slide, sv) in sheet_views {
        if let Ok(sheet) = sheet_mut(presentation, slide) {
            if let Some(pos) = sheet.custom_views.iter().position(|s| s.name == sv.name || (!s.guid.is_empty() && s.guid == sv.guid)) {
                sheet.custom_views[pos] = sv;
            } else {
                sheet.custom_views.push(sv);
            }
        }
    }
    Ok(inverse)
}

fn apply_delete_custom_view(presentation: &mut Presentation, name: &str) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let guid = {
        let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
        let guid = wb.custom_views.iter().find(|v| v.name == name).map(|v| v.guid.clone());
        wb.custom_views.retain(|v| v.name != name);
        guid
    };
    let slide_ids: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
    for slide in slide_ids {
        if let Ok(sheet) = sheet_mut(presentation, slide) {
            sheet.custom_views.retain(|sv| {
                sv.name != name && guid.as_ref().map_or(true, |g| sv.guid != *g)
            });
        }
    }
    Ok(inverse)
}

fn apply_show_custom_view(presentation: &mut Presentation, name: &str) -> Result<Command, EditError> {
    if presentation_has_tables(presentation) {
        return Err(EditError::CustomViewsDisabledByTable);
    }
    let inverse = snapshot(presentation);
    let view = {
        let wb = presentation.workbook.as_deref().ok_or(EditError::IndexOutOfRange)?;
        wb.custom_views.iter().find(|v| v.name == name).cloned().ok_or(EditError::NotFound)?
    };
    let slide_ids: Vec<SlideId> = presentation.slides.iter().map(|s| s.id).collect();
    for slide in slide_ids {
        if let Ok(sheet) = sheet_mut(presentation, slide) {
            if let Some(csv) = sheet.custom_views.iter().find(|sv| sv.name == name || (!sv.guid.is_empty() && sv.guid == view.guid)).cloned() {
                sheet.zoom = csv.zoom;
                sheet.grid_lines = csv.grid_lines;
                sheet.headings = csv.headings;
                sheet.show_ruler = csv.show_ruler;
                sheet.page_break_preview = csv.page_break_preview;
                sheet.frozen = csv.frozen;
                sheet.split = csv.split;
                sheet.show_formulas = csv.show_formulas;
                sheet.show_zeros = csv.show_zeros;
                if view.include_print_settings {
                    if let Some(print) = &csv.print {
                        sheet.print = print.clone();
                    }
                }
                if view.include_hidden {
                    if let Some(hidden_rows) = &csv.hidden_rows {
                        sheet.hidden_rows = hidden_rows.clone();
                        for (r, h) in sheet.rows.iter_mut().enumerate() {
                            let r_u32 = r as u32;
                            if hidden_rows.contains_key(&r_u32) {
                                *h = 0;
                            } else if *h == 0 {
                                *h = if sheet.default_row > 0 { sheet.default_row } else { 15 * 36_000 };
                            }
                        }
                    }
                    if let Some(hidden_cols) = &csv.hidden_columns {
                        sheet.hidden_columns = hidden_cols.clone();
                        for (c, w) in sheet.columns.iter_mut().enumerate() {
                            let c_u32 = c as u32;
                            if hidden_cols.contains_key(&c_u32) {
                                *w = 0;
                            } else if *w == 0 {
                                *w = if sheet.default_column > 0 { sheet.default_column } else { 64 * 36_000 };
                            }
                        }
                    }
                    if let Some(auto_filter) = &csv.auto_filter {
                        sheet.auto_filter = auto_filter.clone();
                    }
                    if let Some(filtered_rows) = &csv.filtered_rows {
                        sheet.filtered_rows = filtered_rows.clone();
                    }
                }
            }
        }
    }
    Ok(inverse)
}

fn apply_show_pivot_details(presentation: &mut Presentation, slide: SlideId, column: u32, row: u32) -> Result<Command, EditError> {
    let (source_sheet_idx, pivot, data) = {
        let idx = workbook_index(presentation, slide)?;
        let sheet = sheet_of(presentation, slide)?;
        let pivot = sheet.pivots.iter().find(|p| {
            let loc = p.location;
            (loc.first_row..=loc.last_row).contains(&row) && (loc.first_column..=loc.last_column).contains(&column)
        }).cloned().ok_or(EditError::IndexOutOfRange)?;
        let data = match &pivot.source.sheet {
            Some(name) => {
                let src_slide = presentation.slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).ok_or(EditError::IndexOutOfRange)?;
                let src_sheet = sheet_of(presentation, src_slide.id)?;
                let range = pivot.source.range.ok_or(EditError::IndexOutOfRange)?;
                crate::pivot::source_data(&src_sheet, range)
            }
            None => {
                let range = pivot.source.range.ok_or(EditError::IndexOutOfRange)?;
                crate::pivot::source_data(&sheet, range)
            }
        };
        (idx, pivot, data)
    };

    let details = pivot.details_for_cell(&data, row, column).ok_or(EditError::IndexOutOfRange)?;
    if details.records.is_empty() {
        return Err(EditError::IndexOutOfRange);
    }

    let mut detail_sheet = empty_sheet(presentation);
    let total_cols = details.headings.len().max(1);
    if detail_sheet.columns.len() < total_cols {
        detail_sheet.columns.resize(total_cols, EMPTY_COLUMN);
    }
    for c in 0..total_cols {
        detail_sheet.columns[c] = 90 * crate::geometry::EMU_PER_POINT;
    }

    let default_st = default_style(presentation);
    let d1904 = date1904(presentation);
    let header_st = CellStyle {
        bold: true,
        fill: Some(Rgb(0xD9D9D9)),
        align: CellAlign::Center,
        ..default_st.clone()
    };
    let s_idx_header = detail_sheet.style_index(header_st);
    let s_idx_default = detail_sheet.style_index(default_st.clone());

    let put = |sheet: &mut Sheet, r: u32, c: u32, val: CellValue, style: u32| {
        let st = sheet.styles.get(style as usize).cloned().unwrap_or_else(|| default_st.clone());
        let text = display_text(&val, &st, d1904);
        let kind = CellKind::of(&val);
        sheet.put_cell(
            r,
            c,
            Some(SheetCell {
                runs: None,
                phonetic: None,
                row: r,
                column: c,
                value: val,
                text,
                kind,
                formula: None,
                style,
            }),
        );
    };

    // Header row at row 0
    for (c, heading) in details.headings.iter().enumerate() {
        put(&mut detail_sheet, 0, c as u32, CellValue::Text(heading.clone()), s_idx_header);
    }

    // Data rows at row 1..=details.records.len()
    for (r_idx, rec) in details.records.iter().enumerate() {
        let r = (r_idx + 1) as u32;
        for (c_idx, val) in rec.iter().enumerate() {
            put(&mut detail_sheet, r, c_idx as u32, val.clone(), s_idx_default);
        }
    }

    let new_name = {
        let existing: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
        let base = "展開";
        let mut num = 1;
        loop {
            let candidate = format!("{base}{num}");
            if !existing.iter().any(|s| s.eq_ignore_ascii_case(&candidate)) {
                break candidate;
            }
            num += 1;
        }
    };

    let new_slide = presentation.new_slide_id();
    let shape_id = presentation.new_shape_id();
    structural(presentation, StructureEdit::AddSheet {
        sheet: source_sheet_idx,
        name: new_name,
        slide: new_slide,
        shape: shape_id,
        content: Arc::new(detail_sheet),
    })
}

fn apply_add_sparkline_group(presentation: &mut Presentation, slide: SlideId, group: SparklineGroup) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let new_locs: Vec<(u32, u32)> = group.sparklines.iter().map(|s| (s.row, s.column)).collect();
    for g in &mut sheet.sparklines {
        g.sparklines.retain(|s| !new_locs.contains(&(s.row, s.column)));
    }
    sheet.sparklines.retain(|g| !g.sparklines.is_empty());
    sheet.sparklines.push(group);
    Ok(inverse)
}

fn apply_edit_sparkline_group(presentation: &mut Presentation, slide: SlideId, index: usize, group: SparklineGroup) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    if index >= sheet.sparklines.len() {
        return Err(EditError::IndexOutOfRange);
    }
    let new_locs: Vec<(u32, u32)> = group.sparklines.iter().map(|s| (s.row, s.column)).collect();
    for (i, g) in sheet.sparklines.iter_mut().enumerate() {
        if i != index {
            g.sparklines.retain(|s| !new_locs.contains(&(s.row, s.column)));
        }
    }
    sheet.sparklines[index] = group;
    sheet.sparklines.retain(|g| !g.sparklines.is_empty());
    Ok(inverse)
}

fn apply_delete_sparkline_group(presentation: &mut Presentation, slide: SlideId, index: usize) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    if index >= sheet.sparklines.len() {
        return Err(EditError::IndexOutOfRange);
    }
    sheet.sparklines.remove(index);
    Ok(inverse)
}

fn apply_clear_sparklines(presentation: &mut Presentation, slide: SlideId, ranges: Vec<CellRange>, whole_group: bool) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let inside = |row: u32, col: u32| {
        ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&col))
    };
    if whole_group {
        sheet.sparklines.retain(|g| !g.sparklines.iter().any(|s| inside(s.row, s.column)));
    } else {
        for g in &mut sheet.sparklines {
            g.sparklines.retain(|s| !inside(s.row, s.column));
        }
        sheet.sparklines.retain(|g| !g.sparklines.is_empty());
    }
    Ok(inverse)
}

fn is_subtotal_row(sheet: &Sheet, row: u32, first_col: u32, last_col: u32) -> bool {
    for c in first_col..=last_col {
        if let Some(cell) = sheet.cell(row, c) {
            if let Some(f) = &cell.formula {
                let trimmed = f.trim().trim_start_matches('=');
                if trimmed.starts_with("SUBTOTAL(") || trimmed.starts_with("AGGREGATE(") {
                    return true;
                }
            }
        }
    }
    false
}

fn apply_remove_subtotal(presentation: &mut Presentation, slide: SlideId, range: CellRange) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet_idx = workbook_index(presentation, slide)?;
    let sheet = sheet_mut(presentation, slide)?;

    let max_r = (range.last_row + 2).min(sheet.rows.len() as u32);
    let mut subtotal_rows = Vec::new();
    for r in (range.first_row + 1)..max_r {
        if is_subtotal_row(sheet, r, range.first_column, range.last_column) {
            subtotal_rows.push(r);
        }
    }
    if subtotal_rows.is_empty() {
        return Ok(inverse);
    }

    let removed_count = subtotal_rows.len() as u32;
    subtotal_rows.sort_unstable_by(|a, b| b.cmp(a));
    for r in subtotal_rows {
        let edit = StructureEdit::Delete { sheet: sheet_idx, axis: Axis::Rows, at: r, count: 1 };
        apply_structure(presentation, &edit);
        Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);
    }

    let sheet = sheet_mut(presentation, slide)?;
    let end_clear = (range.last_row + 2).saturating_sub(removed_count);
    for r in range.first_row..=end_clear {
        sheet.outline_rows.remove(&r);
    }

    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

fn apply_subtotal(presentation: &mut Presentation, slide: SlideId, params: SubtotalParams) -> Result<Command, EditError> {
    if params.range.last_row <= params.range.first_row || params.subtotal_columns.is_empty() {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let sheet_idx = workbook_index(presentation, slide)?;

    let mut range = params.range;

    // 1. If replace_existing: remove previous subtotals first
    if params.replace_existing {
        let sheet = sheet_mut(presentation, slide)?;
        let max_r = (range.last_row + 2).min(sheet.rows.len() as u32);
        let mut subtotal_rows = Vec::new();
        for r in (range.first_row + 1)..max_r {
            if is_subtotal_row(sheet, r, range.first_column, range.last_column) {
                subtotal_rows.push(r);
            }
        }
        if !subtotal_rows.is_empty() {
            let removed_count = subtotal_rows.len() as u32;
            subtotal_rows.sort_unstable_by(|a, b| b.cmp(a));
            for r in subtotal_rows {
                let edit = StructureEdit::Delete { sheet: sheet_idx, axis: Axis::Rows, at: r, count: 1 };
                apply_structure(presentation, &edit);
                Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);
            }
            range.last_row = range.last_row.saturating_sub(removed_count);
            let sheet = sheet_mut(presentation, slide)?;
            for r in range.first_row..=range.last_row {
                sheet.outline_rows.remove(&r);
            }
        }
    }

    let sheet = sheet_mut(presentation, slide)?;
    let first_data_row = range.first_row + 1;
    let last_data_row = range.last_row;
    if last_data_row < first_data_row {
        return Ok(inverse);
    }

    // 2. Identify contiguous groups by value in group_by_column
    struct Group {
        orig_start: u32,
        orig_end: u32,
        label: String,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut cur_start = first_data_row;
    let mut cur_val = sheet.cell(first_data_row, params.group_by_column)
        .map(|c| c.text.clone())
        .unwrap_or_default();

    for r in (first_data_row + 1)..=last_data_row {
        let val = sheet.cell(r, params.group_by_column)
            .map(|c| c.text.clone())
            .unwrap_or_default();
        if val != cur_val {
            groups.push(Group {
                orig_start: cur_start,
                orig_end: r - 1,
                label: cur_val,
            });
            cur_start = r;
            cur_val = val;
        }
    }
    groups.push(Group {
        orig_start: cur_start,
        orig_end: last_data_row,
        label: cur_val,
    });

    let num_groups = groups.len() as u32;
    if num_groups == 0 {
        return Ok(inverse);
    }

    // 3. Insert rows & populate cells
    if params.summary_below {
        // Insert Grand Total row at bottom
        let edit = StructureEdit::Insert { sheet: sheet_idx, axis: Axis::Rows, at: last_data_row + 1, count: 1 };
        apply_structure(presentation, &edit);
        Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);

        // Insert group subtotal rows from bottom to top
        for k in (0..groups.len()).rev() {
            let edit = StructureEdit::Insert { sheet: sheet_idx, axis: Axis::Rows, at: groups[k].orig_end + 1, count: 1 };
            apply_structure(presentation, &edit);
            Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);
        }

        let sheet = sheet_mut(presentation, slide)?;
        for (k, group) in groups.iter().enumerate() {
            let k = k as u32;
            let new_data_start = group.orig_start + k;
            let new_data_end = group.orig_end + k;
            let subtotal_row = new_data_end + 1;

            let label_text = if group.label.is_empty() {
                "集計".to_string()
            } else {
                format!("{} 集計", group.label)
            };
            let style = sheet.cell(new_data_end, params.group_by_column).map(|c| c.style).unwrap_or(0);
            let cell = SheetCell {
                row: subtotal_row,
                column: params.group_by_column,
                value: CellValue::Text(label_text.clone()),
                text: label_text,
                kind: CellKind::Text,
                formula: None,
                style,
                runs: None,
                phonetic: None,
            };
            sheet.put_cell(subtotal_row, params.group_by_column, Some(cell));

            for &col in &params.subtotal_columns {
                let col_name = crate::sheet::cell_name(col, 0);
                let col_letters = col_name.trim_end_matches('1');
                let r1 = new_data_start + 1;
                let r2 = new_data_end + 1;
                let formula = format!("SUBTOTAL({},{}{}:{}{})", params.function_code, col_letters, r1, col_letters, r2);
                let style = sheet.cell(new_data_end, col).map(|c| c.style).unwrap_or(0);
                let cell = SheetCell {
                    row: subtotal_row,
                    column: col,
                    value: CellValue::Empty,
                    text: String::new(),
                    kind: CellKind::Empty,
                    formula: Some(formula),
                    style,
                    runs: None,
                    phonetic: None,
                };
                sheet.put_cell(subtotal_row, col, Some(cell));
            }

            for r in new_data_start..=new_data_end {
                sheet.outline_rows.insert(r, 2);
            }
            sheet.outline_rows.insert(subtotal_row, 1);

            if params.page_breaks && (k as usize) + 1 < groups.len() {
                if !sheet.print.row_breaks.contains(&subtotal_row) {
                    sheet.print.row_breaks.push(subtotal_row);
                }
            }
        }

        // Grand Total row
        let grand_total_row = last_data_row + num_groups + 1;
        let gt_label = "総計".to_string();
        let style = sheet.cell(grand_total_row - 1, params.group_by_column).map(|c| c.style).unwrap_or(0);
        let cell = SheetCell {
            row: grand_total_row,
            column: params.group_by_column,
            value: CellValue::Text(gt_label.clone()),
            text: gt_label,
            kind: CellKind::Text,
            formula: None,
            style,
            runs: None,
            phonetic: None,
        };
        sheet.put_cell(grand_total_row, params.group_by_column, Some(cell));

        for &col in &params.subtotal_columns {
            let col_name = crate::sheet::cell_name(col, 0);
            let col_letters = col_name.trim_end_matches('1');
            let r1 = first_data_row + 1;
            let r2 = grand_total_row;
            let formula = format!("SUBTOTAL({},{}{}:{}{})", params.function_code, col_letters, r1, col_letters, r2);
            let style = sheet.cell(grand_total_row - 1, col).map(|c| c.style).unwrap_or(0);
            let cell = SheetCell {
                row: grand_total_row,
                column: col,
                value: CellValue::Empty,
                text: String::new(),
                kind: CellKind::Empty,
                formula: Some(formula),
                style,
                runs: None,
                phonetic: None,
            };
            sheet.put_cell(grand_total_row, col, Some(cell));
        }
    } else {
        // summary_below == false
        for k in (0..groups.len()).rev() {
            let edit = StructureEdit::Insert { sheet: sheet_idx, axis: Axis::Rows, at: groups[k].orig_start, count: 1 };
            apply_structure(presentation, &edit);
            Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);
        }
        let edit = StructureEdit::Insert { sheet: sheet_idx, axis: Axis::Rows, at: first_data_row, count: 1 };
        apply_structure(presentation, &edit);
        Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);

        let sheet = sheet_mut(presentation, slide)?;
        let gt_label = "総計".to_string();
        let style = sheet.cell(first_data_row + 1, params.group_by_column).map(|c| c.style).unwrap_or(0);
        let cell = SheetCell {
            row: first_data_row,
            column: params.group_by_column,
            value: CellValue::Text(gt_label.clone()),
            text: gt_label,
            kind: CellKind::Text,
            formula: None,
            style,
            runs: None,
            phonetic: None,
        };
        sheet.put_cell(first_data_row, params.group_by_column, Some(cell));

        let last_row_below = last_data_row + num_groups + 1;
        for &col in &params.subtotal_columns {
            let col_name = crate::sheet::cell_name(col, 0);
            let col_letters = col_name.trim_end_matches('1');
            let r1 = first_data_row + 2;
            let r2 = last_row_below + 1;
            let formula = format!("SUBTOTAL({},{}{}:{}{})", params.function_code, col_letters, r1, col_letters, r2);
            let style = sheet.cell(first_data_row + 1, col).map(|c| c.style).unwrap_or(0);
            let cell = SheetCell {
                row: first_data_row,
                column: col,
                value: CellValue::Empty,
                text: String::new(),
                kind: CellKind::Empty,
                formula: Some(formula),
                style,
                runs: None,
                phonetic: None,
            };
            sheet.put_cell(first_data_row, col, Some(cell));
        }

        for (k, group) in groups.iter().enumerate() {
            let k = k as u32;
            let subtotal_row = group.orig_start + k + 1;
            let new_data_start = subtotal_row + 1;
            let new_data_end = group.orig_end + k + 2;

            let label_text = if group.label.is_empty() {
                "集計".to_string()
            } else {
                format!("{} 集計", group.label)
            };
            let style = sheet.cell(new_data_start, params.group_by_column).map(|c| c.style).unwrap_or(0);
            let cell = SheetCell {
                row: subtotal_row,
                column: params.group_by_column,
                value: CellValue::Text(label_text.clone()),
                text: label_text,
                kind: CellKind::Text,
                formula: None,
                style,
                runs: None,
                phonetic: None,
            };
            sheet.put_cell(subtotal_row, params.group_by_column, Some(cell));

            for &col in &params.subtotal_columns {
                let col_name = crate::sheet::cell_name(col, 0);
                let col_letters = col_name.trim_end_matches('1');
                let r1 = new_data_start + 1;
                let r2 = new_data_end + 1;
                let formula = format!("SUBTOTAL({},{}{}:{}{})", params.function_code, col_letters, r1, col_letters, r2);
                let style = sheet.cell(new_data_start, col).map(|c| c.style).unwrap_or(0);
                let cell = SheetCell {
                    row: subtotal_row,
                    column: col,
                    value: CellValue::Empty,
                    text: String::new(),
                    kind: CellKind::Empty,
                    formula: Some(formula),
                    style,
                    runs: None,
                    phonetic: None,
                };
                sheet.put_cell(subtotal_row, col, Some(cell));
            }

            for r in new_data_start..=new_data_end {
                sheet.outline_rows.insert(r, 2);
            }
            sheet.outline_rows.insert(subtotal_row, 1);
        }
    }

    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

/// Create sparklines mapping a data range to a location range.
pub fn make_sparklines(data_sheet: Option<String>, data: CellRange, location: CellRange) -> Vec<Sparkline> {
    let loc_rows = location.last_row.saturating_sub(location.first_row) + 1;
    let loc_cols = location.last_column.saturating_sub(location.first_column) + 1;
    let data_rows = data.last_row.saturating_sub(data.first_row) + 1;
    let data_cols = data.last_column.saturating_sub(data.first_column) + 1;

    let mut out = Vec::new();
    if loc_rows == data_rows && loc_cols == 1 {
        // Vertical location range matching vertical data items (each row of data maps to 1 location row)
        for i in 0..loc_rows {
            let row = location.first_row + i;
            let col = location.first_column;
            let d_row = data.first_row + i;
            out.push(Sparkline {
                row,
                column: col,
                sheet: data_sheet.clone(),
                data: CellRange {
                    first_row: d_row,
                    last_row: d_row,
                    first_column: data.first_column,
                    last_column: data.last_column,
                },
            });
        }
    } else if loc_cols == data_cols && loc_rows == 1 {
        // Horizontal location range matching horizontal data items (each col of data maps to 1 location col)
        for j in 0..loc_cols {
            let row = location.first_row;
            let col = location.first_column + j;
            let d_col = data.first_column + j;
            out.push(Sparkline {
                row,
                column: col,
                sheet: data_sheet.clone(),
                data: CellRange {
                    first_row: data.first_row,
                    last_row: data.last_row,
                    first_column: d_col,
                    last_column: d_col,
                },
            });
        }
    } else {
        // General mapping: each cell in location gets the whole data range
        for r in location.first_row..=location.last_row {
            for c in location.first_column..=location.last_column {
                out.push(Sparkline {
                    row: r,
                    column: c,
                    sheet: data_sheet.clone(),
                    data,
                });
            }
        }
    }
    out
}

fn set_name(presentation: &mut Presentation, name: String, sheet: Option<usize>, formula: Option<String>) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let wb = Arc::make_mut(presentation.workbook.as_mut().ok_or(EditError::IndexOutOfRange)?);
    let at = wb.names.iter().position(|n| n.name.eq_ignore_ascii_case(&name) && n.sheet == sheet);
    let formula = formula.map(|f| f.trim().trim_start_matches('=').to_string());
    match (at, formula) {
        (Some(i), Some(f)) => wb.names[i].formula = f,
        (None, Some(f)) => wb.names.push(crate::sheet::DefinedName { name, sheet, formula: f, hidden: false }),
        (Some(i), None) => {
            wb.names.remove(i);
        }
        (None, None) => return Err(EditError::IndexOutOfRange),
    }
    // Every formula again (names reach anywhere).
    let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &all)?;
    Ok(inverse)
}

fn hide_sheet(presentation: &mut Presentation, slide: SlideId, sheet: usize, hidden: bool, shape: ShapeId) -> Result<Command, EditError> {
    check_workbook_structure_not_locked(presentation)?;
    let wb = presentation.workbook.as_ref().ok_or(EditError::IndexOutOfRange)?;
    let ws = wb.sheets.get(sheet).ok_or(EditError::IndexOutOfRange)?;
    if ws.other {
        return Err(EditError::IndexOutOfRange);
    }
    if hidden {
        let shown = wb.sheets.iter().filter(|s| s.slide.is_some()).count();
        if ws.slide.is_none() || shown <= 1 {
            return Err(EditError::LastSlide);
        }
    } else if ws.slide.is_some() || ws.hidden.is_none() || presentation.slide(slide).is_some() {
        return Err(EditError::IndexOutOfRange);
    }
    structural(presentation, StructureEdit::HideSheet { sheet, hidden, slide, shape })
}

fn rename_sheet(presentation: &mut Presentation, slide: SlideId, name: String) -> Result<Command, EditError> {
    check_workbook_structure_not_locked(presentation)?;
    let index = workbook_index(presentation, slide)?;
    if !crate::sheet_ops::valid_sheet_name(presentation, &name, Some(index)) {
        return Err(EditError::IndexOutOfRange);
    }
    structural(presentation, StructureEdit::RenameSheet { sheet: index, name })
}

fn structure(presentation: &mut Presentation, slide: SlideId, insert: bool, axis: Axis, at: u32, count: u32) -> Result<Command, EditError> {
    if count == 0 {
        return Err(EditError::IndexOutOfRange);
    }
    let sheet = sheet_mut(presentation, slide)?;
    let len = match axis {
        Axis::Rows => sheet.rows.len(),
        Axis::Columns => sheet.columns.len(),
    } as u32;
    let limit = match axis {
        Axis::Rows => ibis_calc::formula::MAX_ROW,
        Axis::Columns => ibis_calc::formula::MAX_COLUMN,
    } + 1;
    if at > len || (!insert && at + count > len) || at + count > limit {
        return Err(EditError::IndexOutOfRange);
    }
    let index = crate::sheet_calc::BookView::new(presentation).index_of_slide(slide).ok_or(EditError::IndexOutOfRange)?;
    if presentation.workbook.is_none() {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    if !insert {
        let d1904 = date1904(presentation);
        let sheet = sheet_mut(presentation, slide)?;
        let deleted: Vec<_> = sheet.spills.iter().filter(|(anchor, _)| {
            let position = if axis == Axis::Rows { anchor.0 } else { anchor.1 };
            (at..at + count).contains(&position)
        }).map(|(anchor, range)| (*anchor, *range)).collect();
        for (anchor, range) in deleted { clear_spilled(sheet, anchor, range, None, d1904); }
    }
    let edit = if insert { StructureEdit::Insert { sheet: index, axis, at, count } } else { StructureEdit::Delete { sheet: index, axis, at, count } };
    let mut changed = apply_structure(presentation, &edit);
    // A constant array may need new spill cells even when its text is unchanged.
    changed.extend(sheet_mut(presentation, slide)?.spills.keys().map(|&(r, c)| (index, r, c)));
    changed.sort_unstable();
    changed.dedup();
    Arc::make_mut(presentation.workbook.as_mut().unwrap()).structure.push(edit);
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &changed)?;
    refresh_data_tables(presentation, slide, &mut Undo::default())?;
    Ok(inverse)
}

fn put_cells(presentation: &mut Presentation, slide: SlideId, cells: Vec<PutCell>) -> Result<Command, EditError> {
    let d1904 = date1904(presentation);
    let mut undo = Undo::default();
    let before = snapshot(presentation);
    {
        let sheet = sheet_mut(presentation, slide)?;
        for c in &cells {
            check_bounds(sheet, c.row, c.column)?;
        }
        let styles_before = sheet.styles.clone();
        for c in &cells {
            let Some(style) = &c.style else { continue };
            undo.remember(slide, sheet, c.row, c.column);
            let index = sheet.style_index(style.clone());
            let cell = match sheet.cell(c.row, c.column).cloned() {
                Some(cell) => SheetCell { text: display_text(&cell.value, style, d1904), style: index, ..cell },
                None => SheetCell { runs: None, phonetic: None, row: c.row, column: c.column, value: CellValue::Empty, text: String::new(), kind: CellKind::Empty, formula: None, style: index },
            };
            sheet.put_cell(c.row, c.column, Some(cell));
        }
        undo.styles_if_grown(slide, styles_before, sheet);
    }
    let edits: Vec<CellEdit> = cells.into_iter().map(|c| CellEdit { row: c.row, column: c.column, input: c.input }).collect();
    // The contents, as typed values; its own undo is covered by this one's.
    let Command::Sheet(SheetCommand::Restore(patches)) = set_cells(presentation, slide, edits)? else {
        // Dynamic arrays about: undone from the picture taken first.
        return Ok(before);
    };
    // Earlier states first: the ones remembered here come before set_cells'.
    for p in patches {
        match undo.patches.iter_mut().find(|q| q.slide == p.slide) {
            Some(q) => {
                for slot in p.cells {
                    if !q.cells.iter().any(|s| s.row == slot.row && s.column == slot.column) {
                        q.cells.push(slot);
                    }
                }
                if q.styles.is_none() {
                    q.styles = p.styles;
                }
            }
            None => undo.patches.push(p),
        }
    }
    Ok(undo.command())
}

fn clear_formats(presentation: &mut Presentation, slide: SlideId, ranges: Vec<CellRange>) -> Result<Command, EditError> {
    let default = default_style(presentation);
    let d1904 = date1904(presentation);
    let mut undo = Undo::default();
    let sheet = sheet_mut(presentation, slide)?;
    let styles_before = sheet.styles.clone();
    let targets: Vec<SheetCell> = sheet
        .cells
        .iter()
        .filter(|c| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)))
        .cloned()
        .collect();
    for c in targets {
        undo.remember(slide, sheet, c.row, c.column);
        if c.value.is_empty() && c.formula.is_none() {
            sheet.put_cell(c.row, c.column, None);
        } else {
            let index = sheet.style_index(default.clone());
            sheet.put_cell(c.row, c.column, Some(SheetCell { text: display_text(&c.value, &default, d1904), style: index, ..c }));
        }
    }
    undo.styles_if_grown(slide, styles_before, sheet);
    Ok(undo.command())
}

/// The slide's shape of the sheet fits its grid again.
fn fit_frame(presentation: &mut Presentation, slide: SlideId) {
    if let Some(s) = presentation.slide_mut(slide) {
        let (mut xs, mut ys, mut cell_images) = (Vec::new(), Vec::new(), Vec::new());
        for shape in s.shapes.iter_mut() {
            if let ShapeKind::Sheet(sheet) = &shape.kind {
                shape.frame.width = sheet.columns.iter().sum();
                shape.frame.height = sheet.rows.iter().sum();
                xs = sheet.column_offsets();
                ys = sheet.row_offsets();
                cell_images = sheet.cell_images.clone();
            }
        }
        if !cell_images.is_empty() {
            for shape in s.shapes.iter_mut() {
                if matches!(shape.kind, ShapeKind::Picture(_)) {
                    let cx = shape.frame.x + shape.frame.width / 2;
                    let cy = shape.frame.y + shape.frame.height / 2;
                    for img in &cell_images {
                        let (col, r) = (img.column as usize, img.row as usize);
                        if col + 1 < xs.len() && r + 1 < ys.len() {
                            let (x0, y0) = (xs[col], ys[r]);
                            let (x1, y1) = (xs[col + 1], ys[r + 1]);
                            if (x0..=x1).contains(&cx) && (y0..=y1).contains(&cy) {
                                let (cw, ch) = ((x1 - x0).max(1), (y1 - y0).max(1));
                                let (w, h) = (shape.frame.width.max(1), shape.frame.height.max(1));
                                let scale = (cw as f64 / w as f64).min(ch as f64 / h as f64);
                                let (fw, fh) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
                                shape.frame = EmuRect::new(x0 + (cw - fw) / 2, y0 + (ch - fh) / 2, fw.max(1), fh.max(1));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The sizes of columns or rows `first`.. as they are, and what the hidden ones hold for each.
fn sizes_now(sheet: &Sheet, axis: Axis, first: u32, count: usize) -> (Vec<Emu>, Vec<Option<Emu>>) {
    let (sizes, hidden) = match axis {
        Axis::Rows => (&sheet.rows, &sheet.hidden_rows),
        Axis::Columns => (&sheet.columns, &sheet.hidden_columns),
    };
    let range = first as usize..first as usize + count;
    (sizes[range.clone()].to_vec(), range.map(|i| hidden.get(&(i as u32)).copied()).collect())
}

fn restore_sizes(presentation: &mut Presentation, slide: SlideId, axis: Axis, first: u32, new_sizes: Vec<Emu>, new_hidden: Vec<Option<Emu>>, new_custom: Vec<bool>) -> Result<Command, EditError> {
    let sheet = sheet_mut(presentation, slide)?;
    let len = match axis {
        Axis::Rows => sheet.rows.len(),
        Axis::Columns => sheet.columns.len(),
    };
    if first as usize + new_sizes.len() > len || new_sizes.len() != new_hidden.len() {
        return Err(EditError::IndexOutOfRange);
    }
    let (sizes, hidden_of) = sizes_now(sheet, axis, first, new_sizes.len());
    let custom_of = custom_now(sheet, axis, first, new_sizes.len());
    if axis == Axis::Columns && new_custom.len() == new_sizes.len() {
        for (i, on) in new_custom.into_iter().enumerate() {
            if on {
                sheet.custom_columns.insert(first + i as u32);
            } else {
                sheet.custom_columns.remove(&(first + i as u32));
            }
        }
    }
    let (target, hidden) = match axis {
        Axis::Rows => (&mut sheet.rows, &mut sheet.hidden_rows),
        Axis::Columns => (&mut sheet.columns, &mut sheet.hidden_columns),
    };
    for (i, (size, was)) in new_sizes.into_iter().zip(new_hidden).enumerate() {
        let at = first + i as u32;
        target[at as usize] = size;
        match was {
            Some(v) => {
                hidden.insert(at, v);
            }
            None => {
                hidden.remove(&at);
            }
        }
    }
    fit_frame(presentation, slide);
    Ok(Command::Sheet(SheetCommand::RestoreSizes { slide, axis, first, sizes, hidden: hidden_of, custom: custom_of }))
}

/// Whether each of `count` columns from `first` is fixed by the user (false for rows).
fn custom_now(sheet: &Sheet, axis: Axis, first: u32, count: usize) -> Vec<bool> {
    match axis {
        Axis::Rows => vec![false; count],
        Axis::Columns => (first..first + count as u32).map(|c| sheet.custom_columns.contains(&c)).collect(),
    }
}

/// Sets the size of rows or columns; `fixed` says a column is then one whose width the user fixed.
fn set_size_marked(presentation: &mut Presentation, slide: SlideId, axis: Axis, first: u32, last: u32, size: Emu, fixed: bool) -> Result<Command, EditError> {
    let (old, old_hidden) = {
        let sheet = sheet_mut(presentation, slide)?;
        let len = match axis {
            Axis::Rows => sheet.rows.len(),
            Axis::Columns => sheet.columns.len(),
        };
        if first > last || last as usize >= len || size < 0 {
            return Err(EditError::IndexOutOfRange);
        }
        sizes_now(sheet, axis, first, (last - first + 1) as usize)
    };
    let old_custom = custom_now(sheet_mut(presentation, slide)?, axis, first, (last - first + 1) as usize);
    let visibility_changed = old.iter().any(|&was| (was == 0) != (size == 0));
    // Hiding or showing rows calculates again (SUBTOTAL and the like), which is undone from a picture of the whole book; a size alone from the sizes it replaces (a copy of the book is a copy of every cell).
    let inverse = if axis == Axis::Rows && visibility_changed {
        snapshot(presentation)
    } else {
        Command::Sheet(SheetCommand::RestoreSizes { slide, axis, first, sizes: old.clone(), hidden: old_hidden, custom: old_custom })
    };
    let sheet = sheet_mut(presentation, slide)?;
    let sizes = match axis {
        Axis::Rows => &mut sheet.rows,
        Axis::Columns => &mut sheet.columns,
    };
    for s in &mut sizes[first as usize..=last as usize] {
        *s = size;
    }
    if axis == Axis::Columns {
        for c in first..=last {
            if fixed {
                sheet.custom_columns.insert(c);
            } else {
                sheet.custom_columns.remove(&c);
            }
        }
    }
    let hidden = match axis {
        Axis::Rows => &mut sheet.hidden_rows,
        Axis::Columns => &mut sheet.hidden_columns,
    };
    for (i, was) in (first..=last).zip(old) {
        if size > 0 {
            hidden.remove(&i);
        } else if was > 0 {
            hidden.insert(i, was);
        }
    }
    fit_frame(presentation, slide);
    if axis == Axis::Rows && visibility_changed {
        let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
        recalculate_cells(presentation, &all)?;
    }
    Ok(inverse)
}

fn move_cells(presentation: &mut Presentation, slide: SlideId, from: CellRange, row: u32, column: u32) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let index = workbook_index(presentation, slide)?;
    let name = presentation.workbook.as_ref().map(|w| w.sheets[index].name.clone()).unwrap_or_default();
    let (dr, dc) = (row as i64 - from.first_row as i64, column as i64 - from.first_column as i64);
    let (h, w) = (from.last_row - from.first_row, from.last_column - from.first_column);
    let to = CellRange { first_row: row, first_column: column, last_row: row + h, last_column: column + w };
    let inside = |r: &CellRange, c: &SheetCell| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column);
    {
        let sheet = sheet_mut(presentation, slide)?;
        check_bounds(sheet, to.last_row, to.last_column)?;
        let moved: Vec<SheetCell> = sheet.cells.iter().filter(|c| inside(&from, c)).cloned().collect();
        let covered: Vec<(u32, u32)> = sheet.cells.iter().filter(|c| inside(&from, c) || inside(&to, c)).map(|c| (c.row, c.column)).collect();
        for (r, c) in covered {
            sheet.put_cell(r, c, None);
        }
        for c in moved {
            let (r, col) = ((c.row as i64 + dr) as u32, (c.column as i64 + dc) as u32);
            sheet.put_cell(r, col, Some(SheetCell { row: r, column: col, ..c }));
        }
        // Dynamic arrays inside go with their cells.
        let spills: Vec<((u32, u32), CellRange)> = sheet.spills.iter().filter(|(a, _)| (from.first_row..=from.last_row).contains(&a.0) && (from.first_column..=from.last_column).contains(&a.1)).map(|(a, r)| (*a, *r)).collect();
        for (a, r) in spills {
            sheet.spills.remove(&a);
            let m = |v: u32, d: i64| (v as i64 + d) as u32;
            sheet.spills.insert((m(a.0, dr), m(a.1, dc)), CellRange { first_row: m(r.first_row, dr), last_row: m(r.last_row, dr), first_column: m(r.first_column, dc), last_column: m(r.last_column, dc) });
        }
        // Merged ranges inside go with the cells.
        for m in sheet.merges.iter_mut() {
            if m.first_row >= from.first_row && m.last_row <= from.last_row && m.first_column >= from.first_column && m.last_column <= from.last_column {
                *m = CellRange {
                    first_row: (m.first_row as i64 + dr) as u32,
                    last_row: (m.last_row as i64 + dr) as u32,
                    first_column: (m.first_column as i64 + dc) as u32,
                    last_column: (m.last_column as i64 + dc) as u32,
                };
            }
        }
        // Validations inside go with the cells.
        for v in sheet.validations.iter_mut() {
            for r in v.ranges.iter_mut() {
                if r.first_row >= from.first_row && r.last_row <= from.last_row && r.first_column >= from.first_column && r.last_column <= from.last_column {
                    *r = CellRange {
                        first_row: (r.first_row as i64 + dr) as u32,
                        last_row: (r.last_row as i64 + dr) as u32,
                        first_column: (r.first_column as i64 + dc) as u32,
                        last_column: (r.last_column as i64 + dc) as u32,
                    };
                }
            }
        }
        // Cell images inside go with the cells.
        for img in sheet.cell_images.iter_mut() {
            if img.row >= from.first_row && img.row <= from.last_row && img.column >= from.first_column && img.column <= from.last_column {
                img.row = (img.row as i64 + dr) as u32;
                img.column = (img.column as i64 + dc) as u32;
            }
        }
        // Data table ranges and input cells inside go with the cells.
        for dt in sheet.data_tables.iter_mut() {
            if dt.range.first_row >= from.first_row && dt.range.last_row <= from.last_row && dt.range.first_column >= from.first_column && dt.range.last_column <= from.last_column {
                dt.range = CellRange {
                    first_row: (dt.range.first_row as i64 + dr) as u32,
                    last_row: (dt.range.last_row as i64 + dr) as u32,
                    first_column: (dt.range.first_column as i64 + dc) as u32,
                    last_column: (dt.range.last_column as i64 + dc) as u32,
                };
            }
            if let Some((r, c)) = dt.row_input.as_mut()
                && (from.first_row..=from.last_row).contains(r)
                && (from.first_column..=from.last_column).contains(c)
            {
                *r = (*r as i64 + dr) as u32;
                *c = (*c as i64 + dc) as u32;
            }
            if let Some((r, c)) = dt.column_input.as_mut()
                && (from.first_row..=from.last_row).contains(r)
                && (from.first_column..=from.last_column).contains(c)
            {
                *r = (*r as i64 + dr) as u32;
                *c = (*c as i64 + dc) as u32;
            }
        }
    }
    // References wholly inside the range, from anywhere, follow it.
    let names: Vec<String> = presentation.workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
    let mut changed = crate::sheet_ops::rewrite_formulas(presentation, &|own, f| {
        let own = names.get(own).map(String::as_str).unwrap_or("");
        ibis_calc::formula::rewrite_references(f, |a| {
            let here = match &a.sheet {
                None => own.eq_ignore_ascii_case(&name),
                Some(s) => s.book.is_none() && s.last.is_none() && s.first.eq_ignore_ascii_case(&name),
            };
            let within = !a.whole_rows
                && !a.whole_columns
                && a.start.row.min(a.end.row) >= from.first_row
                && a.start.row.max(a.end.row) <= from.last_row
                && a.start.column.min(a.end.column) >= from.first_column
                && a.start.column.max(a.end.column) <= from.last_column;
            if !(here && within) {
                return Some(a.clone());
            }
            let mut b = a.clone();
            b.start.row = (b.start.row as i64 + dr) as u32;
            b.end.row = (b.end.row as i64 + dr) as u32;
            b.start.column = (b.start.column as i64 + dc) as u32;
            b.end.column = (b.end.column as i64 + dc) as u32;
            Some(b)
        })
    });
    for r in [from, to] {
        for row in r.first_row..=r.last_row {
            for column in r.first_column..=r.last_column {
                changed.push((index, row, column));
            }
        }
    }
    invalidate_calc_index(presentation);
    recalculate_cells(presentation, &changed)?;
    refresh_data_tables(presentation, slide, &mut Undo::default())?;
    fit_frame(presentation, slide);
    Ok(inverse)
}

fn hide(presentation: &mut Presentation, slide: SlideId, axis: Axis, first: u32, last: u32, hide: bool) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let (sizes, hidden, default) = match axis {
        Axis::Rows => (&mut sheet.rows, &mut sheet.hidden_rows, sheet.default_row),
        Axis::Columns => (&mut sheet.columns, &mut sheet.hidden_columns, sheet.default_column),
    };
    if first > last || last as usize >= sizes.len() {
        return Err(EditError::IndexOutOfRange);
    }
    let default = if default > 0 { default } else { sizes.iter().copied().find(|&s| s > 0).unwrap_or(EMPTY_ROW) };
    let mut changed = false;
    for i in first..=last {
        let size = &mut sizes[i as usize];
        if hide && *size > 0 {
            hidden.insert(i, *size);
            *size = 0;
            changed = true;
        } else if !hide && *size == 0 {
            *size = hidden.remove(&i).filter(|&s| s > 0).unwrap_or(default);
            changed = true;
        }
    }
    if !changed {
        return Err(EditError::IndexOutOfRange);
    }
    fit_frame(presentation, slide);
    if axis == Axis::Rows {
        // Visibility changes aggregates even though none of their inputs changed.
        let all = crate::sheet_calc::BookView::new(presentation).formula_cells();
        recalculate_cells(presentation, &all)?;
    }
    Ok(inverse)
}

fn merge(presentation: &mut Presentation, slide: SlideId, range: CellRange, center: bool) -> Result<Command, EditError> {
    if range.first_row > range.last_row || range.first_column > range.last_column || (range.first_row == range.last_row && range.first_column == range.last_column) {
        return Err(EditError::IndexOutOfRange);
    }
    let inverse = snapshot(presentation);
    let default = default_style(presentation);
    {
        let sheet = sheet_mut(presentation, slide)?;
        check_bounds(sheet, range.last_row, range.last_column)?;
        // Ranges it overlaps are replaced.
        sheet.merges.retain(|m| m.last_row < range.first_row || m.first_row > range.last_row || m.last_column < range.first_column || m.first_column > range.last_column);
        sheet.merges.push(range);
        sheet.merges.sort_by_key(|m| (m.first_row, m.first_column));
    }
    // The other cells lose their values (their formats stay).
    let mut edits = Vec::new();
    for row in range.first_row..=range.last_row {
        for column in range.first_column..=range.last_column {
            if (row, column) != (range.first_row, range.first_column) {
                edits.push(CellEdit { row, column, input: CellInput::Clear });
            }
        }
    }
    let sheet = sheet_mut(presentation, slide)?;
    let has_values = edits.iter().any(|e| sheet.cell(e.row, e.column).is_some_and(|c| !c.value.is_empty() || c.formula.is_some()));
    if has_values {
        set_cells(presentation, slide, edits)?;
    }
    if center {
        let top_left = CellRange { last_row: range.first_row, last_column: range.first_column, ..range };
        set_format(presentation, slide, vec![top_left], StyleChange::Align(CellAlign::Center))?;
    }
    let _ = default;
    Ok(inverse)
}

fn unmerge(presentation: &mut Presentation, slide: SlideId, range: CellRange) -> Result<Command, EditError> {
    let inverse = snapshot(presentation);
    let sheet = sheet_mut(presentation, slide)?;
    let before = sheet.merges.len();
    sheet.merges.retain(|m| m.last_row < range.first_row || m.first_row > range.last_row || m.last_column < range.first_column || m.first_column > range.last_column);
    if sheet.merges.len() == before {
        return Err(EditError::IndexOutOfRange);
    }
    Ok(inverse)
}

/// Synchronize `Sheet::book_context` on every slide's worksheet so single-sheet
/// evaluators (such as conditional formatting) have up-to-date defined names,
/// sheet names/indices, `date1904`, and cross-sheet cell snapshots.
pub fn sync_sheet_book_context(presentation: &mut Presentation) {
    let defined_names: std::sync::Arc<[crate::sheet::DefinedName]> =
        crate::sheet_calc::collect_defined_names(presentation).into();
    let date1904 = presentation.workbook.as_deref().is_some_and(|w| w.date1904);

    let mut sheets_vec: Vec<(String, crate::sheet::Cells)> = Vec::new();
    let mut slide_meta: std::collections::HashMap<SlideId, (usize, String)> = std::collections::HashMap::new();

    match presentation.workbook.as_deref() {
        Some(wb) => {
            for (idx, ws) in wb.sheets.iter().enumerate() {
                let cells = match ws.slide {
                    Some(id) => presentation
                        .slide(id)
                        .and_then(crate::sheet_calc::sheet_of_slide)
                        .map(|s| s.cells.clone())
                        .unwrap_or_default(),
                    None => ws.hidden.as_ref().map(|s| s.cells.clone()).unwrap_or_default(),
                };
                if let Some(id) = ws.slide {
                    slide_meta.insert(id, (idx, ws.name.clone()));
                }
                sheets_vec.push((ws.name.clone(), cells));
            }
        }
        None => {
            for (i, slide) in presentation.slides.iter().enumerate() {
                if let Some(sheet) = crate::sheet_calc::sheet_of_slide(slide) {
                    let name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", i + 1));
                    let idx = sheets_vec.len();
                    slide_meta.insert(slide.id, (idx, name.clone()));
                    sheets_vec.push((name, sheet.cells.clone()));
                }
            }
        }
    }

    let sheets: std::sync::Arc<[(String, crate::sheet::Cells)]> = sheets_vec.into();

    for (slide_pos, slide) in presentation.slides.iter_mut().enumerate() {
        let (sheet_index, sheet_name) = slide_meta.get(&slide.id).cloned().unwrap_or_else(|| {
            (slide_pos, slide.name.clone().unwrap_or_else(|| format!("Sheet{}", slide_pos + 1)))
        });
        let Some(shape) = slide.shapes.iter_mut().find(|sh| matches!(sh.kind, ShapeKind::Sheet(_))) else {
            continue;
        };
        let ShapeKind::Sheet(sheet_arc) = &mut shape.kind else {
            continue;
        };
        let new_ctx = crate::sheet::SheetBookContext {
            sheet_name,
            sheet_index,
            defined_names: defined_names.clone(),
            date1904,
            sheets: sheets.clone(),
        };
        if !sheet_arc.book_context.exact_eq(&new_ctx) {
            let sheet = Arc::make_mut(sheet_arc);
            if !sheet.conditional.is_empty() || !sheet.conditional_x14.is_empty() {
                sheet.cells.bump_content_id();
            }
            sheet.book_context = new_ctx;
        }
    }
}

/// Calculate the formula cells that have no value (a file written by a
/// program that does not calculate has none cached), and what they feed.
pub fn calculate_missing_values(presentation: &mut Presentation) -> Result<(), EditError> {
    sync_sheet_book_context(presentation);
    let view = crate::sheet_calc::BookView::new(presentation);
    let mut missing = view.formula_cells_without_value();
    let images: Vec<_> = view.image_formula_cells().into_iter().filter(|c| !missing.contains(c)).collect();
    missing.extend(images);
    drop(view);
    let res = recalculate_cells_deep(presentation, &missing, 0, true);
    sync_sheet_book_context(presentation);
    res
}

/// Calculate again the given formula cells and what depends on them.
fn recalculate_cells(presentation: &mut Presentation, changed: &[(usize, u32, u32)]) -> Result<(), EditError> {
    if calc_mode(presentation) == CalcMode::Manual {
        return Ok(());
    }
    recalculate_cells_deep(presentation, changed, 0, false)
}

fn recalculate_cells_force(presentation: &mut Presentation, changed: &[(usize, u32, u32)]) -> Result<(), EditError> {
    recalculate_cells_deep(presentation, changed, 0, false)
}

fn recalculate_cells_deep(presentation: &mut Presentation, changed: &[(usize, u32, u32)], depth: u32, only: bool) -> Result<(), EditError> {
    if changed.is_empty() {
        return Ok(());
    }
    let d1904 = date1904(presentation);
    let (results, slides) = {
        let view = crate::sheet_calc::BookView::new(presentation);
        let results = if only { view.calculate_only(changed) } else { view.recalculate(changed) };
        let count = results.iter().map(|r| r.cell.0 + 1).max().unwrap_or(0);
        let slides: Vec<Option<SlideId>> = (0..count).map(|i| view.slide_of(i)).collect();
        (results, slides)
    };
    let mut follow: Vec<(usize, u32, u32)> = Vec::new();
    for r in results {
        let (sheet_index, row, column) = r.cell;
        let target = slides.get(sheet_index).copied().flatten();
        // IMAGE: the picture asked for is kept for the cell to show (the value stays as it was).
        if let Some(t) = target {
            set_web_image(presentation, t, (row, column), r.image.as_ref())?;
        }
        if r.unsupported {
            continue;
        }
        let sheet: &mut Sheet = match target {
            Some(t) => sheet_mut(presentation, t)?,
            None => match presentation.workbook.as_mut().and_then(|w| Arc::make_mut(w).sheets.get_mut(sheet_index)).and_then(|s| s.hidden.as_mut()) {
                Some(h) => Arc::make_mut(h),
                None => continue,
            },
        };
        let Some(cell) = sheet.cell(row, column).cloned() else { continue };
        let (value, spilled) = spill(sheet, (row, column), r.value.clone(), r.spill.as_ref(), d1904);
        follow.extend(spilled.into_iter().map(|(a, b)| (sheet_index, a, b)));
        if cell.value == value {
            continue;
        }
        let text = display_text(&value, &sheet.styles[cell.style as usize], d1904);
        sheet.put_cell(row, column, Some(SheetCell { kind: CellKind::of(&value), text, value, ..cell }));
    }
    // What the spilled cells feed, a few rounds deep (spills feeding spills).
    if !follow.is_empty() && depth < 8 {
        return recalculate_cells_deep(presentation, &follow, depth + 1, only);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Document;
    use crate::geometry::EmuRect;
    use crate::model::Shape;
    use crate::sheet::CellError;

    fn workbook() -> (Document, SlideId) {
        let mut p = Presentation::empty(crate::geometry::EmuSize::WIDESCREEN);
        let id = p.new_slide_id();
        let mut slide = Slide::new(id);
        slide.name = Some("Sheet1".into());
        let shape = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 10], rows: vec![100; 20], grid_lines: true, ..Sheet::default() };
        slide.shapes.push(Shape { id: shape, frame: EmuRect::new(0, 0, 1000, 2000), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(Arc::new(sheet)), floating: None });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![crate::sheet::WorkbookSheet { name: "Sheet1".into(), state: Default::default(), slide: Some(id), hidden: None, other: false }],
            ..Default::default()
        }));
        (Document::new(p), id)
    }

    fn cell(d: &Document, slide: SlideId, row: u32, column: u32) -> Option<SheetCell> {
        let s = d.presentation().slide(slide).unwrap();
        crate::sheet_calc::sheet_of_slide(s).unwrap().cell(row, column).cloned()
    }

    fn set(d: &mut Document, slide: SlideId, row: u32, column: u32, input: CellInput) {
        d.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row, column, input }] })]).unwrap();
    }

    /// The picture IMAGE asks for is a result of the book's calculation: its address is read as any reference is
    /// (another sheet, a name, another workbook's cached cells, a table's row).
    #[test]
    fn image_addresses_are_resolved_by_the_calculation() {
        let (mut d, s1) = workbook();
        let mut p = d.presentation().clone();
        let s2 = p.new_slide_id();
        let mut slide = Slide::new(s2);
        slide.name = Some("Pictures".into());
        let shape = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 10], rows: vec![100; 20], grid_lines: true, ..Sheet::default() };
        slide.shapes.push(Shape { id: shape, frame: EmuRect::new(0, 0, 1000, 2000), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(Arc::new(sheet)), floating: None });
        p.slides.push(slide);
        let mut wb = (**p.workbook.as_ref().unwrap()).clone();
        wb.sheets.push(crate::sheet::WorkbookSheet { name: "Pictures".into(), state: Default::default(), slide: Some(s2), hidden: None, other: false });
        wb.names.push(crate::sheet::DefinedName { name: "Pic".into(), sheet: None, formula: "Pictures!$A$2".into(), hidden: false });
        let mut ext = crate::sheet::ExternalSheet { name: "S".into(), ..Default::default() };
        ext.cells.insert((0, 0), CellValue::Text("https://x.test/ext.png".into()));
        wb.external_books.push(crate::sheet::ExternalBook { name: "ext.xlsx".into(), path: "ext.xlsx".into(), sheets: vec![ext], broken: false });
        p.workbook = Some(Arc::new(wb));
        d = Document::new(p);
        set(&mut d, s2, 0, 0, CellInput::Value(CellValue::Text("https://x.test/sheet.png".into())));
        set(&mut d, s2, 1, 0, CellInput::Value(CellValue::Text("https://x.test/name.png".into())));
        set(&mut d, s1, 0, 0, CellInput::Formula("IMAGE(Pictures!A1,\"alt\",1)".into()));
        set(&mut d, s1, 1, 0, CellInput::Formula("IMAGE(Pic)".into()));
        set(&mut d, s1, 2, 0, CellInput::Formula("IMAGE([ext.xlsx]S!A1)".into()));
        set(&mut d, s1, 3, 0, CellInput::Formula("IMAGE(\"\")".into()));
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s1).unwrap()).unwrap();
        let source = |row: u32| sheet.web_images.get(&(row, 0)).map(|i| (i.source.clone(), i.sizing));
        assert_eq!(source(0), Some(("https://x.test/sheet.png".into(), 1)), "another sheet");
        assert_eq!(source(1), Some(("https://x.test/name.png".into(), 0)), "a name");
        assert_eq!(source(2), Some(("https://x.test/ext.png".into(), 0)), "another workbook");
        assert_eq!(source(3), None, "an empty address is an error");
        // The address changes: the picture follows; the formula replaced: it goes.
        set(&mut d, s2, 0, 0, CellInput::Value(CellValue::Text("https://x.test/other.png".into())));
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s1).unwrap()).unwrap();
        assert_eq!(sheet.web_images.get(&(0, 0)).map(|i| i.source.as_str()), Some("https://x.test/other.png"));
        set(&mut d, s1, 0, 0, CellInput::Formula("1+1".into()));
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s1).unwrap()).unwrap();
        assert!(!sheet.web_images.contains_key(&(0, 0)));
    }

    /// Undo and redo give back the picture IMAGE asked for along with the formula.
    #[test]
    fn image_pictures_follow_undo_and_redo() {
        let (mut d, s) = workbook();
        let at = |d: &Document| crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().web_images.get(&(0, 0)).map(|i| i.source.clone());
        set(&mut d, s, 0, 0, CellInput::Formula("IMAGE(\"https://x.test/a.png\")".into()));
        assert_eq!(at(&d).as_deref(), Some("https://x.test/a.png"));
        set(&mut d, s, 0, 0, CellInput::Formula("IMAGE(\"https://x.test/b.png\")".into()));
        assert_eq!(at(&d).as_deref(), Some("https://x.test/b.png"));
        assert!(d.undo());
        assert_eq!(at(&d).as_deref(), Some("https://x.test/a.png"), "undo: the first picture");
        assert!(d.redo());
        assert_eq!(at(&d).as_deref(), Some("https://x.test/b.png"), "redo: the second");
        // A dependent IMAGE(A2) follows an edit of A2, and its undo.
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Text("https://x.test/c.png".into())));
        set(&mut d, s, 0, 0, CellInput::Formula("IMAGE(A2)".into()));
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Text("https://x.test/d.png".into())));
        assert_eq!(at(&d).as_deref(), Some("https://x.test/d.png"));
        assert!(d.undo());
        assert_eq!(at(&d).as_deref(), Some("https://x.test/c.png"), "undo of the edit the picture depends on");
        assert!(d.undo());
        assert!(d.undo());
        assert_eq!(at(&d).as_deref(), Some("https://x.test/b.png"));
    }

    #[test]
    fn subtotal_nested_calls_do_not_match_literal_text() {
        let (mut d, s) = workbook();
        for (row, formula) in ["LEN(\"SUBTOTAL(\")", "LEN(\"AGGREGATE(\")", "SUBTOTAL(9,A1:A2)", "AGGREGATE(9,0,A1:A2)"].iter().enumerate() {
            set(&mut d, s, row as u32, 0, CellInput::Formula((*formula).into()));
        }
        for (row, formula, expected) in [(0,"SUBTOTAL(9,A1:A4)",19.0),(1,"SUBTOTAL(3,A1:A4)",2.0),(2,"AGGREGATE(9,0,A1:A4)",19.0),(3,"AGGREGATE(3,0,A1:A4)",2.0),(4,"AGGREGATE(9,4,A1:A4)",57.0)] {
            set(&mut d, s, row, 1, CellInput::Formula(formula.into()));
            assert_eq!(cell(&d,s,row,1).unwrap().value,CellValue::Number(expected),"{formula}");
        }
        assert!(d.undo()); assert!(d.redo());
        assert_eq!(cell(&d,s,4,1).unwrap().value,CellValue::Number(57.0));
    }

    #[test]
    fn subtotal_count_reference_errors() {
        let (mut d,s)=workbook();
        set(&mut d,s,0,0,CellInput::Value(CellValue::Error(CellError::NA)));
        set(&mut d,s,1,0,CellInput::Value(CellValue::Number(7.0)));
        set(&mut d,s,2,0,CellInput::Value(CellValue::Text("text".into())));
        for (row,formula,expected) in [(0,"SUBTOTAL(2,A1:A3)",CellValue::Number(1.0)),(1,"SUBTOTAL(102,A1:A3)",CellValue::Number(1.0)),(2,"SUBTOTAL(3,A1:A3)",CellValue::Number(3.0)),(3,"SUBTOTAL(103,A1:A3)",CellValue::Number(3.0)),(4,"SUBTOTAL(9,A1:A3)",CellValue::Error(CellError::NA))] {
            set(&mut d,s,row,1,CellInput::Formula(formula.into()));assert_eq!(cell(&d,s,row,1).unwrap().value,expected,"{formula}");
        }
        assert!(d.undo());assert!(d.redo());
    }

    #[test]
    fn subtotal_function_numbers_follow_row_visibility() {
        let (mut d,s)=workbook();for (row,n) in [2.0,4.0,6.0].into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Value(CellValue::Number(n)));}
        let all=[4.0,3.0,3.0,6.0,2.0,48.0,2.0,(8.0_f64/3.0).sqrt(),12.0,4.0,8.0/3.0];
        let visible=[4.0,2.0,2.0,6.0,2.0,12.0,8.0_f64.sqrt(),2.0,8.0,8.0,4.0];
        for (i,code) in (1..=11).chain(101..=111).enumerate(){set(&mut d,s,(i%11) as u32,(1+i/11) as u32,CellInput::Formula(format!("SUBTOTAL({code},A1:A3)")));}
        let check=|d:&Document,hidden:bool|{for i in 0..22 {let expected=if hidden && i>=11 {visible[i%11]}else{all[i%11]};let CellValue::Number(n)=cell(d,s,(i%11) as u32,(1+i/11) as u32).unwrap().value else{panic!("not numeric {i}")};assert!((n-expected).abs()<1e-12,"code {}: {n} != {expected}",if i<11{i+1}else{i+90});}};
        check(&d,false);d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:1,last:1,hidden:true})]).unwrap();check(&d,true);
        assert!(d.undo());check(&d,false);assert!(d.redo());check(&d,true);
        d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:1,last:1,hidden:false})]).unwrap();check(&d,false);
    }

    #[test]
    fn aggregate_function_and_option_numbers() {
        let (mut d,s)=workbook();for (row,n) in [2.0,4.0,4.0,6.0].into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Value(CellValue::Number(n)));}
        let all=[4.0,4.0,4.0,6.0,2.0,192.0,(8.0_f64/3.0).sqrt(),2.0_f64.sqrt(),16.0,8.0/3.0,2.0,4.0,4.0,4.0,4.0,3.5,3.5,2.5,2.5];
        let visible=[14.0/3.0,3.0,3.0,6.0,4.0,96.0,(4.0_f64/3.0).sqrt(),(8.0_f64/9.0).sqrt(),14.0,4.0/3.0,8.0/9.0,4.0,4.0,4.0,4.0,4.0,4.0,4.0,4.0];
        let mut edits=vec![];for function in 1..=19{for option in 0..8{let k=match function {14|15=>",2",16|18=>",0.25",17|19=>",1",_=>""};edits.push(CellEdit {row:function-1,column:option+1,input:CellInput::Formula(format!("AGGREGATE({function},{option},A1:A4{k})"))});}}
        d.execute(vec![Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
        let check=|d:&Document,hidden:bool|{for function in 1..=19{for option in 0..8{let expected=if hidden && option%2==1{visible[function as usize-1]}else{all[function as usize-1]};let CellValue::Number(n)=cell(d,s,function-1,option+1).unwrap().value else{panic!("non numeric {function}/{option}")};assert!((n-expected).abs()<1e-12,"{function}/{option}: {n} != {expected}");}}};
        check(&d,false);d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:0,last:0,hidden:true})]).unwrap();check(&d,true);assert!(d.undo());check(&d,false);assert!(d.redo());check(&d,true);
    }

    #[test]
    fn aggregate_count_options_keep_error_counting_separate_from_numeric_errors() {
        let (mut d,s)=workbook();
        for(row,input)in[(0,CellInput::Value(CellValue::Number(5.))),(1,CellInput::Value(CellValue::Error(CellError::NA))),(2,CellInput::Value(CellValue::Text("text".into()))),(3,CellInput::Formula("\"\"".into())),(4,CellInput::Formula("SUBTOTAL(3,A1:A4)".into())),(5,CellInput::Value(CellValue::Bool(true))),(7,CellInput::Value(CellValue::Number(8.)))] {set(&mut d,s,row,0,input);}
        let edits=(2..=3).flat_map(|f|(0..8).map(move |o|CellEdit {row:o,column:f-1,input:CellInput::Formula(format!("AGGREGATE({f},{o},A1:A8)"))})).collect();
        d.execute(vec![Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
        let check=|d:&Document,hidden:bool| {for o in 0..8 {let nested=o<4;let skip_errors=matches!(o,2|3|6|7);let hides=hidden&&o%2==1;let count=3-i32::from(nested)-i32::from(hides);let counta=7-i32::from(nested)-i32::from(skip_errors)-if hides{1+i32::from(!skip_errors)}else{0};assert_eq!(cell(d,s,o,1).unwrap().value,CellValue::Number(count as f64),"COUNT option{o} hidden{hidden}");assert_eq!(cell(d,s,o,2).unwrap().value,CellValue::Number(counta as f64),"COUNTA option{o} hidden{hidden}");}};
        check(&d,false);d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:1,last:1,hidden:true}),Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:7,last:7,hidden:true})]).unwrap();check(&d,true);assert!(d.undo());check(&d,false);assert!(d.redo());check(&d,true);
    }

    #[test]
    fn aggregate_options_distinguish_hidden_nested_and_errors() {
        let (mut d,s)=workbook();for (row,input) in [(0,CellInput::Value(CellValue::Number(2.0))),(1,CellInput::Value(CellValue::Number(4.0))),(2,CellInput::Formula("SUBTOTAL(9,A1:A2)".into())),(3,CellInput::Value(CellValue::Error(CellError::NA))),(4,CellInput::Formula("LEN(\"SUBTOTAL(\")".into()))]{set(&mut d,s,row,0,input);}
        for option in 0..8{set(&mut d,s,option,1,CellInput::Formula(format!("AGGREGATE(9,{option},A1:A5)")));}
        d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:1,last:1,hidden:true})]).unwrap();
        for (row,n) in [(2,15.0),(3,11.0),(6,21.0),(7,17.0)]{assert_eq!(cell(&d,s,row,1).unwrap().value,CellValue::Number(n));}
        for row in [0,1,4,5]{assert_eq!(cell(&d,s,row,1).unwrap().value,CellValue::Error(CellError::NA));}
        assert!(d.undo());assert_eq!(cell(&d,s,3,1).unwrap().value,CellValue::Number(15.0));assert!(d.redo());assert_eq!(cell(&d,s,3,1).unwrap().value,CellValue::Number(11.0));
    }

    #[test]
    fn subtotal_distinguishes_filtered_and_manual_hidden_rows() {
        let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Value(CellValue::Text("header".into())));for (row,n) in [(1,2.0),(2,4.0),(3,6.0)]{set(&mut d,s,row,0,CellInput::Value(CellValue::Number(n)));}
        for (row,code) in [9,109,2,102,3,103].into_iter().enumerate(){set(&mut d,s,row as u32,1,CellInput::Formula(format!("SUBTOTAL({code},A2:A4)")));}
        d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:2,last:2,hidden:true})]).unwrap();
        assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(12.0));assert_eq!(cell(&d,s,1,1).unwrap().value,CellValue::Number(8.0));
        d.execute(vec![Command::Sheet(SheetCommand::SetAutoFilter {slide:s,filter:Some(crate::sheet::AutoFilter {range:CellRange {first_row:0,last_row:3,first_column:0,last_column:0},columns:vec![(0,crate::sheet::ColumnFilter::Values(vec!["4".into(),"6".into()]))]})})]).unwrap();
        let check=|d:&Document|{for (row,n) in [10.0,6.0,2.0,1.0,2.0,1.0].into_iter().enumerate(){assert_eq!(cell(d,s,row as u32,1).unwrap().value,CellValue::Number(n),"row {row}");}};check(&d);
        assert!(d.undo());assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(12.0));assert!(d.redo());check(&d);
        // Editing an input does not reapply the filter.
        set(&mut d,s,1,0,CellInput::Value(CellValue::Number(6.0)));check(&d);assert!(d.undo());
        set(&mut d,s,3,0,CellInput::Value(CellValue::Number(8.0)));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(12.0));assert_eq!(cell(&d,s,1,1).unwrap().value,CellValue::Number(8.0));assert!(d.undo());check(&d);
        d.execute(vec![Command::Sheet(SheetCommand::Insert {slide:s,axis:Axis::Rows,at:1,count:1})]).unwrap();assert!(crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().filtered_rows.contains(&2));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(10.0));assert!(d.undo());check(&d);

    }

    #[test]
    fn jis_name_uses_existing_japanese_width_conversion() {
        let (mut d,s)=workbook();let cases=[("abc","ａｂｃ"),("ｱｲｳ ABC","アイウ　ＡＢＣ"),("ｶﾞﾊﾟｳﾞ","ガパウ゛"),("日本語ＡＢＣ","日本語ＡＢＣ"),("ｶA🙂","カＡ🙂"),("","")];
        for (row,(input,expected)) in cases.into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Value(CellValue::Text(input.into())));for (column,name) in [(1,"JIS"),(2,"DBCS")]{set(&mut d,s,row as u32,column,CellInput::Formula(format!("{name}(A{})",row+1)));assert_eq!(cell(&d,s,row as u32,column).unwrap().value,CellValue::Text(expected.into()),"{name}/{input}");}}
        set(&mut d,s,6,1,CellInput::Formula("JIS(123)".into()));assert_eq!(cell(&d,s,6,1).unwrap().value,CellValue::Text("１２３".into()));set(&mut d,s,7,1,CellInput::Formula("JIS(NA())".into()));assert_eq!(cell(&d,s,7,1).unwrap().value,CellValue::Error(CellError::NA));assert!(d.undo());assert!(d.redo());
        for (row,formula) in [(8,"JIS()"),(9,"JIS(\"a\",\"b\")")]{set(&mut d,s,row,1,CellInput::Formula(formula.into()));assert_eq!(cell(&d,s,row,1).unwrap().value,CellValue::Error(CellError::Value));}
        assert!(ibis_calc::functions::implemented().contains(&"JIS"));

    }

    #[test]
    fn aggregate_values_follow_zero_height_resize() {
        let (mut d,s)=workbook();for (row,n) in [2.0,4.0,6.0].into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Value(CellValue::Number(n)));}
        for (row,f) in ["SUBTOTAL(9,A1:A3)","SUBTOTAL(109,A1:A3)","AGGREGATE(9,1,A1:A3)","AGGREGATE(9,4,A1:A3)","B2*2","SUM(A1:A3)"].into_iter().enumerate(){set(&mut d,s,row as u32,1,CellInput::Formula(f.into()));}
        let check=|d:&Document,hidden:bool|{for (row,n) in (if hidden{[12.0,8.0,8.0,12.0,16.0,12.0]}else{[12.0,12.0,12.0,12.0,24.0,12.0]}).into_iter().enumerate(){assert_eq!(cell(d,s,row as u32,1).unwrap().value,CellValue::Number(n),"row {row}");}};
        d.execute(vec![Command::Sheet(SheetCommand::SetSize {slide:s,axis:Axis::Rows,first:1,last:1,size:0})]).unwrap();check(&d,true);assert!(d.undo());check(&d,false);assert!(d.redo());check(&d,true);
        d.execute(vec![Command::Sheet(SheetCommand::SetSize {slide:s,axis:Axis::Rows,first:1,last:1,size:100})]).unwrap();check(&d,false);
        d.execute(vec![Command::Sheet(SheetCommand::SetSize {slide:s,axis:Axis::Columns,first:0,last:0,size:0})]).unwrap();check(&d,false);
    }

    #[test]
    fn wrap_count_above_vector_length_has_no_padding() {
        for function in ["WRAPROWS","WRAPCOLS"]{for vector in ["{1,2}","{1;2}"]{for count in ["2","5","1E20"]{for pad in ["",",\"pad\""]{
            let (mut d,s)=workbook();let formula=format!("{function}({vector},{count}{pad})");set(&mut d,s,0,0,CellInput::Formula(formula.clone()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Number(1.0));let (r,c)=if function=="WRAPROWS"{(0,1)}else{(1,0)};assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(2.0));let sh=crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();assert_eq!(sh.spills.get(&(0,0)).copied(),Some(CellRange {first_row:0,last_row:r,first_column:0,last_column:c}),"{formula}");assert_eq!(sh.cells.len(),2);assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(2.0));
        }}}}
    }

    #[test]
    fn expand_dimensions_padding_and_unaddressable_capacity() {
        for rows in [2,3]{for columns in [2,3]{for pad in [None,Some("0"),Some("\"\""),Some("\"pad\"")]{
            let (mut d,s)=workbook();let suffix=pad.map(|p|format!(",{p}")).unwrap_or_default();let formula=format!("EXPAND({{1,2;3,4}},{rows},{columns}{suffix})");set(&mut d,s,0,0,CellInput::Formula(formula.clone()));
            for r in 0..rows{for c in 0..columns{let expected=if r<2 && c<2{CellValue::Number((r*2+c+1) as f64)}else{match pad {None=>CellValue::Error(CellError::NA),Some("0")=>CellValue::Number(0.0),Some("\"\"")=>CellValue::Text(String::new()),_=>CellValue::Text("pad".into())}};assert_eq!(cell(&d,s,r,c).unwrap().value,expected,"{formula}/{r}/{c}");}}
            assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,s,1,1).unwrap().value,CellValue::Number(4.0));
        }}}
        // These inputs are rejected by checked capacity arithmetic before allocating.
        let (mut d,s)=workbook();for (row,f) in ["EXPAND({1,2},1E20,2)","EXPAND({1;2},2,1E20)","EXPAND(1,1E20,1E20)"].into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Formula(f.into()));assert_eq!(cell(&d,s,row as u32,0).unwrap().value,CellValue::Error(CellError::Num));}
    }

    #[test]
    fn regex_lookup_evaluates_regex_without_exact_fallback() {
        for (formula, exp_dot, exp_esc) in [
            ("XLOOKUP(A1,{\"abc\",\"a.c\"},{10,20},99,3,1)", CellValue::Number(10.0), CellValue::Number(20.0)),
            ("XLOOKUP(A1,{\"abc\",\"a.c\"},{10,20},99,3,-1)", CellValue::Number(20.0), CellValue::Number(20.0)),
            ("XMATCH(A1,{\"abc\",\"a.c\"},3,1)", CellValue::Number(1.0), CellValue::Number(2.0)),
            ("XMATCH(A1,{\"abc\",\"a.c\"},3,-1)", CellValue::Number(2.0), CellValue::Number(2.0)),
        ] {
            let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Value(CellValue::Text("a.c".into())));set(&mut d,s,0,1,CellInput::Formula(formula.into()));
            // "a.c" matches "abc" via regex '.' wildcard, NOT exact match "a.c"
            assert_eq!(cell(&d,s,0,1).unwrap().value,exp_dot);
            set(&mut d,s,0,0,CellInput::Value(CellValue::Text("a\\.c".into())));
            // "a\\.c" matches literal "a.c"
            assert_eq!(cell(&d,s,0,1).unwrap().value,exp_esc);
            assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,s,0,1).unwrap().value,exp_esc);
        }
    }

    #[test]
    fn xlookup_dimension_mismatch_errors_even_when_not_found() {
        for (keys,returns) in [("{1;2}","{10;20;30}"),("{1,2}","{10,20,30}")]{for key in ["1","9"]{for mode in [-1,0,1]{for search in [-1,1]{let (mut d,s)=workbook();let f=format!("XLOOKUP({key},{keys},{returns},99,{mode},{search})");set(&mut d,s,0,0,CellInput::Formula(f.clone()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value),"{f}");assert!(d.undo());assert!(d.redo());}}}}
    }

    #[test]
    fn xlookup_scalar_not_found_keeps_na() {
        for keys in ["5","\"abc\""] {let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula(format!("XLOOKUP(1,{keys},{{10;20;30}})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::NA));}
    }

    #[test]
    fn xlookup_empty_key_is_not_numeric_zero_for_approximate_match() {
        let (mut d,s)=workbook();set(&mut d,s,0,1,CellInput::Formula("XLOOKUP(A1,{1;2;3},{2.1;3.9;6.2},99,1,1)".into()));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(99.0));set(&mut d,s,0,0,CellInput::Value(CellValue::Number(0.0)));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(2.1));assert!(d.undo());assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(99.0));assert!(d.redo());assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Number(2.1));
    }

    #[test]
    fn sortby_rejects_two_dimensional_or_mixed_direction_keys() {
        for (source,valid,bad) in [("{1;2;3;4}","{1;2;3;4}","{1,2;3,4}"),("{1,2,3,4}","{1,2,3,4}","{1,2;3,4}"),("{1;2;3;4}","{1;2;3;4}","{1,2,3,4}"),("{1,2,3,4}","{1,2,3,4}","{1;2;3;4}")]{for position in 0..3{let keys=(0..3).map(|i|if i==position{bad}else{valid}).collect::<Vec<_>>();let f=format!("SORTBY({source},{},1,{},1,{},1)",keys[0],keys[1],keys[2]);let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula(f.clone()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value),"{f}");assert!(d.undo());assert!(d.redo());}}
    }

    #[test]
    fn sortby_two_key_order_enums_in_both_directions() {
        for separator in [";",","]{for (first,second,expected) in [(1,1,[4.,2.,1.,3.]),(1,-1,[2.,4.,3.,1.]),(-1,1,[1.,3.,4.,2.]),(-1,-1,[3.,1.,2.,4.])]{let (mut d,s)=workbook();let src=format!("{{{}}}",["1","2","3","4"].join(separator));let k1=format!("{{{}}}",["2","1","2","1"].join(separator));let k2=format!("{{{}}}",["1","2","2","1"].join(separator));let f=format!("SORTBY({src},{k1},{first},{k2},{second})");set(&mut d,s,0,0,CellInput::Formula(f));for (i,n) in expected.into_iter().enumerate(){let (r,c)=if separator==";"{(i as u32,0)}else{(0,i as u32)};assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(n));}assert!(d.undo());assert!(d.redo());}}
    }

    #[test]
    fn filter_empty_include_is_false_and_follows_edits() {
        let (mut d,s)=workbook();for (row,n) in [10.,20.,30.].into_iter().enumerate(){set(&mut d,s,row as u32,0,CellInput::Value(CellValue::Number(n)));}set(&mut d,s,0,2,CellInput::Formula("FILTER(A1:A3,B1:B3,99)".into()));assert_eq!(cell(&d,s,0,2).unwrap().value,CellValue::Number(99.0));set(&mut d,s,1,1,CellInput::Value(CellValue::Bool(true)));assert_eq!(cell(&d,s,0,2).unwrap().value,CellValue::Number(20.0));assert!(d.undo());assert_eq!(cell(&d,s,0,2).unwrap().value,CellValue::Number(99.0));assert!(d.redo());assert_eq!(cell(&d,s,0,2).unwrap().value,CellValue::Number(20.0));
    }

    #[test]
    fn arraytotext_empty_array_errors_are_value_without_changing_sources() {
        for (source, src_err) in [("TAKE({1;2},0)", CellError::Value), ("FILTER({1;2},{FALSE;FALSE})", CellError::Calc)] {
            for suffix in ["", ",0", ",1"] {
                let (mut d, s) = workbook();
                set(&mut d, s, 0, 0, CellInput::Formula(source.into()));
                assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Error(src_err));
                set(&mut d, s, 0, 1, CellInput::Formula(format!("ARRAYTOTEXT({source}{suffix})")));
                assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Error(CellError::Value));
                assert!(d.undo());
                assert!(d.redo());
            }
        }
    }

    #[test]
    fn choose_lines_index_shapes_and_error_precedence() {
        let cases=[("1",vec![1]),("-1",vec![3]),("2.9",vec![2]),("{3,1,3}",vec![3,1,3]),("{3;1;3}",vec![3,1,3]),("{3,1;2,3}",vec![3,1,2,3])];for name in ["CHOOSEROWS","CHOOSECOLS"]{for (input,lines) in &cases{let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula(format!("{name}({{1,2,3;4,5,6;7,8,9}},{input})")));for (out,line) in lines.iter().enumerate(){for cross in 0..3{let (r,c,n)=if name=="CHOOSEROWS"{(out as u32,cross,((*line-1)*3+cross+1) as f64)}else{(cross,out as u32,(cross*3+*line) as f64)};assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(n),"{name}/{input}/{r}/{c}");}}assert!(d.undo());assert!(d.redo());}
            for (index,e) in [("0",CellError::Value),("4",CellError::Value),("-4",CellError::Value),("NA()",CellError::NA),("{0,#N/A}",CellError::NA),("{#N/A,0}",CellError::NA)]{let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula(format!("{name}({{1,2,3;4,5,6;7,8,9}},{index})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(e));assert!(d.undo());assert!(d.redo());}
        }
    }

    /// 重複の削除: by the chosen columns, text without regard to case; the
    /// first of each kept, the rest moving up, the range's last rows empty,
    /// the cells beside the range left; undone and redone.
    #[test]
    fn remove_duplicates_by_columns_keeps_the_first() {
        let (mut d, s) = workbook();
        let rows = [("名前", "地域", "x"), ("a", "東", "1"), ("A", "東", "2"), ("b", "西", "3"), ("a", "西", "4"), ("b", "西", "5")];
        for (row, (a, b, c)) in rows.into_iter().enumerate() {
            for (column, text) in [a, b, c].into_iter().enumerate() {
                set(&mut d, s, row as u32, column as u32, CellInput::Value(CellValue::Text(text.into())));
            }
        }
        set(&mut d, s, 5, 3, CellInput::Value(CellValue::Text("beside".into())));
        let range = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 2 };
        let text = |d: &Document, r: u32, c: u32| cell(d, s, r, c).map(|x| x.value.clone()).unwrap_or_default();
        d.execute(vec![Command::Sheet(SheetCommand::RemoveDuplicates { slide: s, range, columns: vec![0, 1], header: true })]).unwrap();
        let third: Vec<CellValue> = (1..=5).map(|r| text(&d, r, 2)).collect();
        assert_eq!(third, vec![CellValue::Text("1".into()), CellValue::Text("3".into()), CellValue::Text("4".into()), CellValue::Empty, CellValue::Empty]);
        assert_eq!(text(&d, 0, 0), CellValue::Text("名前".into()));
        assert_eq!(text(&d, 5, 3), CellValue::Text("beside".into()));
        assert!(d.undo());
        assert_eq!(text(&d, 2, 2), CellValue::Text("2".into()));
        assert!(d.redo());
        assert_eq!(text(&d, 2, 2), CellValue::Text("3".into()));
        // By the first column only, without a heading row.
        let (mut d, s) = workbook();
        for (row, v) in ["x", "y", "X", "x"].into_iter().enumerate() {
            set(&mut d, s, row as u32, 0, CellInput::Value(CellValue::Text(v.into())));
        }
        let range = CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 0 };
        d.execute(vec![Command::Sheet(SheetCommand::RemoveDuplicates { slide: s, range, columns: vec![0], header: false })]).unwrap();
        let left: Vec<CellValue> = (0..4).map(|r| cell(&d, s, r, 0).map(|x| x.value.clone()).unwrap_or_default()).collect();
        assert_eq!(left, vec![CellValue::Text("x".into()), CellValue::Text("y".into()), CellValue::Empty, CellValue::Empty]);
        // A column outside the range is refused.
        assert!(d.execute(vec![Command::Sheet(SheetCommand::RemoveDuplicates { slide: s, range, columns: vec![1], header: false })]).is_err());
    }

    #[test]
    fn multi_column_sort_header_and_order_combinations() {
        for header in [false,true]{for ascending in [false,true]{for second in [false,true]{let (mut d,s)=workbook();let data=[("Group",CellValue::Text("Score".into()),"ID"),("b",CellValue::Number(2.),"r1"),("a",CellValue::Number(2.),"r2"),("b",CellValue::Number(1.),"r3"),("a",CellValue::Number(1.),"r4")];for (row,(a,b,id)) in data.into_iter().enumerate(){for (column,value) in [CellValue::Text(a.into()),b,CellValue::Text(id.into())].into_iter().enumerate(){set(&mut d,s,row as u32,column as u32,CellInput::Value(value));}}
            let expected=match (ascending,second){(true,true)=>vec!["r4","r2","r3","r1"],(true,false)=>vec!["r2","r4","r1","r3"],(false,true)=>vec!["r3","r1","r4","r2"],(false,false)=>vec!["r1","r3","r2","r4"]};let mut expected=expected;if header || !ascending{expected.insert(0,"ID");}else{expected.push("ID");}
            d.execute(vec![Command::Sheet(SheetCommand::Sort {slide:s,range:CellRange {first_row:0,last_row:4,first_column:0,last_column:2},keys:vec![(0,ascending),(1,second)],header})]).unwrap();for (row,id) in expected.iter().enumerate(){assert_eq!(cell(&d,s,row as u32,2).unwrap().value,CellValue::Text((*id).into()));}assert!(d.undo());assert_eq!(cell(&d,s,1,2).unwrap().value,CellValue::Text("r1".into()));assert!(d.redo());for (row,id) in expected.iter().enumerate(){assert_eq!(cell(&d,s,row as u32,2).unwrap().value,CellValue::Text((*id).into()));}
        }}}
    }

    #[test]
    fn values_formulas_and_recalculation() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(2.0)));
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Number(3.0)));
        set(&mut d, s, 2, 0, CellInput::Formula("SUM(A1:A2)*10".into()));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(50.0));
        assert_eq!(cell(&d, s, 2, 0).unwrap().text, "50");
        // Changing an input calculates the formula again.
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(7.0)));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(100.0));
        // Undo puts back the input and the formula's value.
        assert!(d.undo());
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(50.0));
        assert!(d.redo());
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(100.0));
        set(&mut d, s, 1, 0, CellInput::Clear);
        assert!(cell(&d, s, 1, 0).is_none());
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(70.0));
        // A formula referring to a formula.
        set(&mut d, s, 3, 0, CellInput::Formula("A3+1".into()));
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(1.0)));
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Number(11.0));
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn rows_inserted_and_deleted() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(2.0)));
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Number(3.0)));
        set(&mut d, s, 2, 0, CellInput::Formula("SUM(A1:A2)".into()));
        set(&mut d, s, 3, 0, CellInput::Formula("A1*10".into()));
        let before = d.presentation().clone();
        d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 1, count: 1 })]).unwrap();
        assert_eq!(cell(&d, s, 3, 0).unwrap().formula.as_deref(), Some("SUM(A1:A3)"));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(3.0));
        assert!(cell(&d, s, 1, 0).is_none());
        // Deleting the first row: the reference to it is gone.
        d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
        assert_eq!(cell(&d, s, 3, 0).unwrap().formula.as_deref(), Some("#REF!*10"));
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Error(CellError::Ref));
        assert_eq!(cell(&d, s, 2, 0).unwrap().formula.as_deref(), Some("SUM(A1:A2)"));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(3.0));
        assert!(d.undo());
        assert!(d.undo());
        assert!(*d.presentation() == before);
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn pasting_and_clearing_formats() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(3.0)));
        let bold = CellStyle { bold: true, number_format: "0.00".into(), ..CellStyle::default() };
        let cells = vec![
            PutCell { row: 1, column: 0, input: CellInput::Formula("A1*2".into()), style: Some(bold.clone()) },
            PutCell { row: 1, column: 1, input: CellInput::Value(CellValue::Text("x".into())), style: None },
        ];
        d.execute(vec![Command::Sheet(SheetCommand::PutCells { slide: s, cells })]).unwrap();
        let c = cell(&d, s, 1, 0).unwrap();
        assert_eq!(c.text, "6.00");
        let range = CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 1 };
        d.execute(vec![Command::Sheet(SheetCommand::ClearFormats { slide: s, ranges: vec![range] })]).unwrap();
        assert_eq!(cell(&d, s, 1, 0).unwrap().text, "6");
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn flatten_array_ignore_and_scan_options() {
        for function in ["TOCOL", "TOROW"] {
            for ignore in 0..=3 {
                for by_column in [false, true] {
                    let (mut d, s) = workbook();
                    for (r, col, v) in [(0,0,CellValue::Number(1.)),(0,2,CellValue::Error(CellError::NA)),(1,0,CellValue::Number(2.)),(1,1,CellValue::Number(0.)),(1,2,CellValue::Text(String::new()))] {
                        set(&mut d,s,r,col,CellInput::Value(v));
                    }
                    let formula=format!("{function}(A1:C2,{ignore},{by_column})");
                    set(&mut d,s,5,0,CellInput::Formula(formula.clone()));
                    let order=if by_column {vec![(0,0),(1,0),(0,1),(1,1),(0,2),(1,2)]} else {vec![(0,0),(0,1),(0,2),(1,0),(1,1),(1,2)]};
                    let expected:Vec<_>=order.into_iter().filter_map(|(r,c)| {
                        let value=cell(&d,s,r,c).map(|c|c.value).unwrap_or_default();
                        if (value.is_empty() && matches!(ignore,1|3)) || (matches!(value,CellValue::Error(_)) && matches!(ignore,2|3)) {None}
                        else {Some(if value.is_empty(){CellValue::Number(0.)}else{value})}
                    }).collect();
                    for (i,v) in expected.iter().enumerate() {
                        let (r,c)=if function=="TOCOL" {(5+i as u32,0)}else{(5,i as u32)};
                        assert_eq!(cell(&d,s,r,c).unwrap().value,*v,"{formula} element {i}");
                    }
                    assert!(d.undo()); assert!(cell(&d,s,5,0).is_none()); assert!(d.redo());
                    assert_eq!(cell(&d,s,5,0).unwrap().formula.as_deref(),Some(formula.as_str()));
                }
            }
            for (option,error) in [("NA()",CellError::NA),("1/0",CellError::Div0),("\"invalid\"",CellError::Value)] {
                let (mut d,s)=workbook();
                set(&mut d,s,5,0,CellInput::Formula(format!("{function}({{1,2;3,4}},0,{option})")));
                assert_eq!(cell(&d,s,5,0).unwrap().value,CellValue::Error(error),"{function} scan option {option}");
                assert!(cell(&d,s,6,0).is_none());
            }
        }
    }

    #[test]
    fn weekend_string_and_numeric_options() {
        let (mut d,s)=workbook();
        set(&mut d,s,0,0,CellInput::Formula("NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,14),\"1111111\")".into()));
        assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Number(0.));
        for mask in 0u32..128 {
            let text:String=(0..7).map(|day|if mask&(1<<day)==0 {'0'}else{'1'}).collect();
            let expected=2*(7-mask.count_ones());
            for (start,end,sign) in [(1,14,1.),(14,1,-1.)] {
                set(&mut d,s,0,0,CellInput::Formula(format!("NETWORKDAYS.INTL(DATE(2024,1,{start}),DATE(2024,1,{end}),\"{text}\")")));
                assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Number(if expected==0 {0.}else{expected as f64*sign}),"weekend {text}/{start}/{end}");
            }
            for days in [-1,0,1] {
                let expected=if mask==127 {CellValue::Error(CellError::Value)} else {
                    let mut delta=0i32;
                    if days!=0 {loop {delta+=days;let weekday=delta.rem_euclid(7) as u32;if mask&(1<<weekday)==0 {break}}}
                    CellValue::Number(45292.+delta as f64)
                };
                set(&mut d,s,0,0,CellInput::Formula(format!("WORKDAY.INTL(DATE(2024,1,1),{days},\"{text}\")")));
                assert_eq!(cell(&d,s,0,0).unwrap().value,expected,"workday {text}/{days}");
            }
        }
        for (n,text) in [(1,"0000011"),(2,"1000001"),(3,"1100000"),(4,"0110000"),(5,"0011000"),(6,"0001100"),(7,"0000110"),(11,"0000001"),(12,"1000000"),(13,"0100000"),(14,"0010000"),(15,"0001000"),(16,"0000100"),(17,"0000010")] {
            for f in ["NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,14)","WORKDAY.INTL(DATE(2024,1,1),5"] {
                set(&mut d,s,0,0,CellInput::Formula(format!("{f},{n})")));let number=cell(&d,s,0,0).unwrap().value;
                set(&mut d,s,0,0,CellInput::Formula(format!("{f},\"{text}\")")));assert_eq!(cell(&d,s,0,0).unwrap().value,number,"{f}/{n}/{text}");
            }
        }
        for option in ["\"000001\"","\"00000111\"","\"00000x1\""] {
            for f in ["NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,14)","WORKDAY.INTL(DATE(2024,1,1),1"] {
                set(&mut d,s,0,0,CellInput::Formula(format!("{f},{option})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value));
            }
        }
    }

    #[test]
    fn datedif_unit_options() {
        let (mut d,s)=workbook();
        for (unit,expected) in [("Y",1.),("M",14.),("D",440.),("MD",14.),("YM",2.),("YD",75.)] {
            for spelling in [unit.to_owned(),unit.to_lowercase()] {
                set(&mut d,s,0,0,CellInput::Formula(format!("DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"{spelling}\")")));
                assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Number(expected),"unit {spelling}");
                set(&mut d,s,0,0,CellInput::Formula(format!("DATEDIF(DATE(2001,6,1),DATE(2001,6,1),\"{spelling}\")")));
                assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Number(0.));
                set(&mut d,s,0,0,CellInput::Formula(format!("DATEDIF(DATE(2002,8,15),DATE(2001,6,1),\"{spelling}\")")));
                assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Num));
            }
        }
        for (unit,error) in [("\"invalid\"",CellError::Num),("NA()",CellError::NA),("1/0",CellError::Div0)] {
            set(&mut d,s,0,0,CellInput::Formula(format!("DATEDIF(DATE(2001,6,1),DATE(2002,8,15),{unit})")));
            assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(error));
        }
    }

    #[test]
    fn information_invalid_and_retired_options() {
        let (mut d,s)=workbook();
        for formula in ["INFO(1)","INFO(-1)","INFO(\"invalid\")","CELL(1,A1)","CELL(\"invalid\",A1)"] {
            set(&mut d,s,0,1,CellInput::Formula(formula.into()));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Error(CellError::Value),"{formula}");
        }
        for option in ["memavail","memused","totmem"] {
            set(&mut d,s,0,1,CellInput::Formula(format!("INFO(\"{option}\")")));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Error(CellError::NA));
        }
        for formula in ["INFO(NA())","CELL(NA(),A1)"] {
            set(&mut d,s,0,1,CellInput::Formula(formula.into()));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Error(CellError::NA));
        }
    }

    #[test]
    fn sort_index_order_and_orientation() {
        let (mut d,s)=workbook();
        for index in [1,2] {for order in [-1,1] {for by_column in [false,true] {
            let formula=format!("SORT({{4,1;2,3}},{index},{order},{by_column})");set(&mut d,s,0,0,CellInput::Formula(formula.clone()));
            let swap=(index==1 && order==1)||(index==2 && order==-1);
            let expected=match (by_column,swap) {(false,true)=>[[2.,3.],[4.,1.]],(true,true)=>[[1.,4.],[3.,2.]],_=>[[4.,1.],[2.,3.]]};
            for (row,values) in expected.into_iter().enumerate(){for (col,value) in values.into_iter().enumerate(){assert_eq!(cell(&d,s,row as u32,col as u32).unwrap().value,CellValue::Number(value),"{formula}");}}
            assert!(d.undo());assert!(cell(&d,s,0,0).is_none());assert!(d.redo());assert_eq!(cell(&d,s,1,1).unwrap().value,CellValue::Number(expected[1][1]));assert!(d.undo());
        }}}
        for (option,error) in [("NA()",CellError::NA),("1/0",CellError::Div0),("\"invalid\"",CellError::Value)] {
            set(&mut d,s,0,0,CellInput::Formula(format!("SORT({{4,1;2,3}},1,1,{option})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(error),"scan {option}");
        }
        for formula in ["SORT({4,1;2,3},0)","SORT({4,1;2,3},3)","SORT({4,1;2,3},1,0)","SORT({4,1;2,3},1,2)"] {
            set(&mut d,s,0,0,CellInput::Formula(formula.into()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value));
        }
    }

    #[test]
    fn sort_error_values_ascending() {
        let permutations=["#N/A;#DIV/0!;#VALUE!","#N/A;#VALUE!;#DIV/0!","#DIV/0!;#N/A;#VALUE!","#DIV/0!;#VALUE!;#N/A","#VALUE!;#N/A;#DIV/0!","#VALUE!;#DIV/0!;#N/A"];
        for input in permutations {
            let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula(format!("SORT({{{input}}})")));
            for (row,error) in [CellError::Div0,CellError::Value,CellError::NA].into_iter().enumerate(){assert_eq!(cell(&d,s,row as u32,0).unwrap().value,CellValue::Error(error),"{input}");}
            assert!(d.undo());assert!(cell(&d,s,0,0).is_none());assert!(d.redo());assert_eq!(cell(&d,s,2,0).unwrap().value,CellValue::Error(CellError::NA));
        }
        let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula("SORT({#N/A;2;#DIV/0!;1;#VALUE!})".into()));
        let expected=[CellValue::Number(1.),CellValue::Number(2.),CellValue::Error(CellError::Div0),CellValue::Error(CellError::Value),CellValue::Error(CellError::NA)];
        for (row,value) in expected.into_iter().enumerate(){assert_eq!(cell(&d,s,row as u32,0).unwrap().value,value);}
    }

    #[test]
    fn textsplit_option_combinations_and_errors() {
        let (mut d,s)=workbook();
        for ignore in [false,true] {for mode in [0,1] {for custom_pad in [false,true] {
            let pad=if custom_pad {",\"pad\""}else{""};let formula=format!("TEXTSPLIT(\"aXbxxc|dXe\",\"x\",\"|\",{ignore},{mode}{pad})");
            set(&mut d,s,0,0,CellInput::Formula(formula.clone()));
            let rows=match (ignore,mode) {(false,0)=>vec![vec!["aXb","","c"],vec!["dXe"]],(true,0)=>vec![vec!["aXb","c"],vec!["dXe"]],(false,1)=>vec![vec!["a","b","","c"],vec!["d","e"]],_=>vec![vec!["a","b","c"],vec!["d","e"]]};
            let width=rows[0].len();for (r,values) in rows.iter().enumerate(){for col in 0..width {
                let expected=values.get(col).map(|v|CellValue::Text((*v).into())).unwrap_or_else(||if custom_pad {CellValue::Text("pad".into())}else{CellValue::Error(CellError::NA)});
                assert_eq!(cell(&d,s,r as u32,col as u32).unwrap().value,expected,"{formula}");
            }}
            assert!(d.undo());assert!(cell(&d,s,0,0).is_none());assert!(d.redo());assert_eq!(cell(&d,s,0,0).unwrap().formula.as_deref(),Some(formula.as_str()));assert!(d.undo());
        }}}
        for (option,error) in [("NA()",CellError::NA),("1/0",CellError::Div0),("\"invalid\"",CellError::Value)] {
            for formula in [format!("TEXTSPLIT(\"a,b\",\",\",,{option})"),format!("TEXTSPLIT(\"a,b\",\",\",,FALSE,{option})")] {
                set(&mut d,s,0,0,CellInput::Formula(formula.clone()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(error),"{formula}");
            }
        }
        for mode in [-1,99] {
            set(&mut d,s,0,0,CellInput::Formula(format!("TEXTSPLIT(\"a,b\",\",\",,FALSE,{mode})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value));
        }
    }

    #[test]
    fn textsplit_duplicate_delimiters_and_pad_shape() {
        let (mut d,s)=workbook();
        set(&mut d,s,0,0,CellInput::Formula("TEXTSPLIT(\"a,b,c\",\",\",\",\")".into()));
        for (col,text) in ["a","b","c"].into_iter().enumerate(){assert_eq!(cell(&d,s,0,col as u32).unwrap().value,CellValue::Text(text.into()));}
        assert!(cell(&d,s,1,0).is_none());assert!(d.undo());
        set(&mut d,s,0,0,CellInput::Formula("TEXTSPLIT(\"Hello World\",\"Hello World\",\"Hello World\")".into()));
        assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Text(String::new()));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Text(String::new()));assert!(cell(&d,s,1,0).is_none());assert!(d.undo());
        for (mode,expected_column) in [(0,false),(1,true)] {
            set(&mut d,s,0,0,CellInput::Formula(format!("TEXTSPLIT(\"aXb\",\"x\",\"X\",FALSE,{mode})")));
            let (row,col)=if expected_column {(0,1)}else{(1,0)};assert_eq!(cell(&d,s,row,col).unwrap().value,CellValue::Text("b".into()));assert!(d.undo());
        }
        for pad in ["{1,2;3,4}","{1,2}","{1;2}"] {
            set(&mut d,s,0,0,CellInput::Formula(format!("TEXTSPLIT(\"m\",\",\",,FALSE,0,{pad})")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Value),"unused multi-value pad {pad}");assert!(d.undo());
        }
        set(&mut d,s,0,0,CellInput::Formula("TEXTSPLIT(\"a,b|c\",\",\",\"|\",FALSE,0,{9})".into()));assert_eq!(cell(&d,s,1,1).unwrap().value,CellValue::Number(9.));
        set(&mut d,s,10,0,CellInput::Formula("TEXTSPLIT(\"m\",\",\",,FALSE,0,C1:C3)".into()));assert_eq!(cell(&d,s,10,0).unwrap().value,CellValue::Text("m".into()),"unused reference pad is not an array constant");
    }

    #[test]
    fn casefold_delimiters_keep_original_utf8_offsets() {
        let (mut d,s)=workbook();
        for prefix in ["K","İ","日本","🙂","ΟΣ|"] {
            for instance in [1,-1] {
                for (function,expected) in [("TEXTBEFORE",prefix),("TEXTAFTER","tail")] {
                    let formula=format!("{function}(\"{prefix}Xtail\",\"x\",{instance},1)");set(&mut d,s,0,0,CellInput::Formula(formula.clone()));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Text(expected.into()),"{formula}");
                }
            }
            set(&mut d,s,0,0,CellInput::Formula(format!("TEXTSPLIT(\"{prefix}Xtail\",\"x\",,FALSE,1)")));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Text(prefix.into()));assert_eq!(cell(&d,s,0,1).unwrap().value,CellValue::Text("tail".into()));
        }

    }

    #[test]
    fn dynamic_reference_arrays_resize_after_input_change() {
        for line in include_str!("../../../test/editing/xlsx-dynamic-reference.tsv").lines().skip(1) {
            let fields:Vec<_>=line.split('\t').collect();let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Value(CellValue::Number(3.0)));set(&mut d,slide,1,1,CellInput::Formula("SEQUENCE(A1)".into()));set(&mut d,slide,2,3,CellInput::Formula(fields[0].into()));
            let check=|d:&Document,text:&str|{let expected:Vec<f64>=text.split(';').map(|v|v.parse().unwrap()).collect();for r in 0..3{assert_eq!(cell(d,slide,r+2,3).map(|x|x.value).unwrap_or_default(),expected.get(r as usize).map_or(CellValue::Empty,|n|CellValue::Number(*n)),"{} {r}",fields[0]);}};
            check(&d,fields[1]);set(&mut d,slide,0,0,CellInput::Value(CellValue::Number(2.0)));check(&d,fields[2]);assert!(d.undo());check(&d,fields[1]);assert!(d.redo());check(&d,fields[2]);
        }
    }

    #[test]
    fn text_before_after_mode_and_search_end_options() {
        for line in include_str!("../../../test/editing/xlsx-text-boundary.tsv").lines().skip(1) {
            let(formula,v)=line.split_once('\t').unwrap();let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()));
            let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula(formula.into()));assert_eq!(cell(&d,slide,0,0).unwrap().value,expected,"{formula}");assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,expected);
        }
    }

    #[test]
    fn compatibility_version_mid_replace() {
        for version in [1,2] {
            for line in include_str!("../../../test/editing/xlsx-compatibility-slice.tsv").lines().skip(1) {
                let fields:Vec<_>=line.split('\t').collect();
                let text=fields[version as usize];
                let expected=CellError::ALL.iter().find(|e|e.as_str()==text).map_or_else(||CellValue::Text(text.into()),|e|CellValue::Error(*e));
                let (d,slide)=workbook();let mut p=d.presentation().clone();Arc::make_mut(p.workbook.as_mut().unwrap()).compatibility_version=Some(version);let mut d=Document::new(p);
                set(&mut d,slide,0,0,CellInput::Formula(fields[0].into()));
                assert_eq!(cell(&d,slide,0,0).unwrap().value,expected,"version {version}: {}",fields[0]);
                assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,expected);
            }
        }
    }

    #[test]
    fn day_count_basis_and_method_track_leap_year_in_both_date_bases() {
        for date1904 in [false, true] {
            let (d, slide) = workbook(); let mut p = d.presentation().clone();
            Arc::make_mut(p.workbook.as_mut().unwrap()).date1904 = date1904;
            let mut d = Document::new(p);
            let cases: Vec<_> = include_str!("../../../test/editing/xlsx-day-count-options.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
            set(&mut d, slide, 0, 1, CellInput::Formula("DATE(2019,1,1)".into()));
            set(&mut d, slide, 0, 2, CellInput::Formula("DATE(2020,1,1)".into()));
            for (row, f) in cases.iter().enumerate() { set(&mut d, slide, row as u32, 0, CellInput::Formula(f[0].into())); }
            let check = |d: &Document, col: usize| { for (row, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(CellError::Num)); assert_eq!(cell(d, slide, row as u32, 0).unwrap().value, expected, "date1904={date1904} {}", f[0]); } };
            check(&d, 1); let before = d.presentation().clone();
            d.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 0, column: 1, input: CellInput::Formula("DATE(2020,1,1)".into()) }, CellEdit { row: 0, column: 2, input: CellInput::Formula("DATE(2021,1,1)".into()) }] })]).unwrap();
            check(&d, 2); let after = d.presentation().clone();
            assert!(d.undo()); assert_eq!(*d.presentation(), before); check(&d, 1);
            assert!(d.redo()); assert_eq!(*d.presentation(), after); check(&d, 2);
        }
    }

    #[test]
    fn week_return_types_track_year_boundary_and_history() {
        let (mut d, slide) = workbook();
        let cases: Vec<_> = include_str!("../../../test/editing/xlsx-week-return-types.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
        d.execute(vec![Command::Sheet(SheetCommand::Insert { slide, axis: Axis::Rows, at: 20, count: cases.len() as u32 - 20 })]).unwrap();
        set(&mut d, slide, 0, 1, CellInput::Formula("DATE(2021,1,1)".into()));
        for (row, f) in cases.iter().enumerate() { set(&mut d, slide, row as u32, 0, CellInput::Formula(f[0].into())); }
        let check = |d: &Document, col: usize| { for (row, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(CellError::Num)); assert_eq!(cell(d, slide, row as u32, 0).unwrap().value, expected, "{}", f[0]); } };
        check(&d, 1); let before = d.presentation().clone();
        set(&mut d, slide, 0, 1, CellInput::Formula("DATE(2021,1,4)".into()));
        check(&d, 2); let after = d.presentation().clone();
        assert!(d.undo()); assert_eq!(*d.presentation(), before); check(&d, 1);
        assert!(d.redo()); assert_eq!(*d.presentation(), after); check(&d, 2);
    }

    #[test]
    fn switching_compatibility_recalculates_and_restores_spills() {
        let(mut d,slide)=workbook();
        let cases:Vec<_>=include_str!("../../../test/editing/xlsx-compatibility-toggle.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
        for(i,f)in cases.iter().enumerate(){set(&mut d,slide,i as u32*3,0,CellInput::Formula(f[0].into()));}
        let check=|d:&Document,version:u32| {
            for(i,f)in cases.iter().enumerate() {
                let text=f[version as usize+1];let expected=if f[1]=="T" {vec![CellValue::Text(text.into())]} else {text.split(';').map(|v|CellValue::Number(v.parse().unwrap())).collect()};
                for r in 0..2 {assert_eq!(cell(d,slide,i as u32*3+r,0).map(|x|x.value).unwrap_or_default(),expected.get(r as usize).cloned().unwrap_or_default(),"version {version}: {}",f[0]);}
            }
        };
        check(&d,1);
        for version in [crate::CalculationCompatibilityVersion::V2,crate::CalculationCompatibilityVersion::V1] {
            let before=d.presentation().clone();d.execute(vec![Command::Sheet(SheetCommand::SetCompatibilityVersion {version})]).unwrap();check(&d,version.number());
            let after=d.presentation().clone();assert!(d.undo());assert_eq!(*d.presentation(),before);assert!(d.redo());assert_eq!(*d.presentation(),after);
        }
    }

    #[test]
    fn compatibility_version_find_search() {
        for version in [1,2] {
            for line in include_str!("../../../test/editing/xlsx-compatibility-search.tsv").lines().skip(1) {
                let fields:Vec<_>=line.split('\t').collect();
                let text=fields[version as usize];
                let expected=text.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*CellError::ALL.iter().find(|e|e.as_str()==text).unwrap()));
                let (d,slide)=workbook();let mut p=d.presentation().clone();Arc::make_mut(p.workbook.as_mut().unwrap()).compatibility_version=Some(version);let mut d=Document::new(p);
                set(&mut d,slide,0,0,CellInput::Formula(fields[0].into()));
                assert_eq!(cell(&d,slide,0,0).unwrap().value,expected,"version {version}: {}",fields[0]);
                assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,expected);
            }
        }
    }

    #[test]
    fn typed_reference_arithmetic_keeps_arrays_and_explicit_at() {
        for formula in ["B2#*2", "B2:B4*2", "+B2#*2", "LET(values,B2#,values*2)", "IF(TRUE,B2#,0)*2"] {
            let (mut d, slide) = workbook();
            set(&mut d, slide, 1, 1, CellInput::Formula("SEQUENCE(3)".into()));
            // No dynamic flag on D3: the same stored legacy expression intersects row 3.
            let view = crate::sheet_calc::BookView::new(d.presentation());
            let expr = ibis_calc::formula::parse(formula).unwrap();
            assert_eq!(ibis_calc::eval::evaluate_cell(&view, &ibis_calc::eval::Stored, 0, 2, 3, &expr), ibis_calc::CellValue::Number(4.0), "legacy {formula}");
            for explicit in ["@B2#*2", "@B2:B4*2"] {
                let expr=ibis_calc::formula::parse(explicit).unwrap();
                assert_eq!(ibis_calc::eval::evaluate_cell(&view, &ibis_calc::eval::Stored, 0, 2, 3, &expr), ibis_calc::CellValue::Number(4.0), "explicit {explicit}");
            }
            set(&mut d, slide, 2, 3, CellInput::Formula(formula.into()));
            let check=|d:&Document|{for r in 0..3 {assert_eq!(cell(d,slide,r+2,3).map(|x|x.value).unwrap_or_default(),CellValue::Number(((r+1)*2)as f64),"dynamic {formula}/{r}");}};
            check(&d);assert!(d.undo());assert!(d.redo());check(&d);
        }
        for explicit in ["@B2#*2", "@B2:B4*2"] {
            let (mut d,slide)=workbook();set(&mut d,slide,1,1,CellInput::Formula("SEQUENCE(3)".into()));set(&mut d,slide,2,3,CellInput::Formula(explicit.into()));
            assert_eq!(cell(&d,slide,2,3).unwrap().value,CellValue::Number(4.0));assert!(cell(&d,slide,3,3).is_none_or(|x|x.value.is_empty()));
        }
    }

    #[test]
    fn cross_sheet_spills_follow_structure_and_mixed_references() {
        for axis in [Axis::Rows, Axis::Columns] {
            for (insert, at) in [(true, 0), (true, 2), (false, 1), (false, 2)] {
                let (mut d, source) = workbook();
                let observer = d.ids().new_slide_id();
                let shape = d.ids().new_shape_id();
                d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Observer".into(), slide: observer, shape })]).unwrap();
                set(&mut d, source, 1, 1, CellInput::Formula("SEQUENCE(3,3)".into()));
                for (row, formula) in [(0, "SUM(Sheet1!$B$2#)"), (1, "Sheet1!B2+Sheet1!$B2+Sheet1!B$2+Sheet1!$B$2"), (3, "MAP(Sheet1!$B$2#,LAMBDA(value,value*2))"), (7, "SUM(A4#)")] {
                    set(&mut d, observer, row, 0, CellInput::Formula(formula.into()));
                }
                assert_eq!(cell(&d, observer, 7, 0).unwrap().value, CellValue::Number(90.0), "initial chain");
                let before = d.presentation().clone();
                let command = if insert { SheetCommand::Insert { slide: source, axis, at, count: 1 } } else { SheetCommand::Delete { slide: source, axis, at, count: 1 } };
                d.execute(vec![Command::Sheet(command)]).unwrap();
                let check = |d: &Document| {
                    let deleted = !insert && at == 1;
                    for (row, expected) in [(0, 45.0), (1, 4.0), (7, 90.0)] {
                        assert_eq!(cell(d, observer, row, 0).unwrap().value, if deleted { CellValue::Error(CellError::Ref) } else { CellValue::Number(expected) }, "{axis:?}/{insert}/{at}/{row}");
                    }
                    for r in 0..3 { for c in 0..3 {
                        let expected = if deleted { if (r,c)==(0,0) { CellValue::Error(CellError::Ref) } else { CellValue::Empty } } else { CellValue::Number(((r*3+c+1)*2) as f64) };
                        assert_eq!(cell(d, observer, r+3, c).map(|x|x.value).unwrap_or_default(), expected, "{axis:?}/{insert}/{at}/{r}/{c}");
                    }}
                    let mixed = cell(d, observer, 1, 0).unwrap().formula.unwrap();
                    if insert && at==0 {
                        let expected = if axis==Axis::Rows { "Sheet1!B3+Sheet1!$B3+Sheet1!B$3+Sheet1!$B$3" } else { "Sheet1!C2+Sheet1!$C2+Sheet1!C$2+Sheet1!$C$2" };
                        assert_eq!(mixed, expected);
                    } else if deleted { assert!(mixed.contains("#REF!")); }
                };
                check(&d);
                let after=d.presentation().clone();
                assert!(d.undo()); assert_eq!(*d.presentation(), before);
                assert!(d.redo()); assert_eq!(*d.presentation(), after); check(&d);
            }
        }
    }

    #[test]
    fn inserted_rows_and_columns_rebuild_constant_spill_regions() {
        for axis in [Axis::Rows,Axis::Columns]{for at in [0,2]{let (mut d,s)=workbook();set(&mut d,s,1,1,CellInput::Formula("SEQUENCE(3,3)".into()));set(&mut d,s,5,5,CellInput::Formula("SUM($B$2#)".into()));let before=d.presentation().clone();d.execute(vec![Command::Sheet(SheetCommand::Insert {slide:s,axis,at,count:1})]).unwrap();let (row,col)=if at==0{if axis==Axis::Rows{(2,1)}else{(1,2)}}else{(1,1)};for r in 0..3{for c in 0..3{assert_eq!(cell(&d,s,row+r,col+c).map(|c|c.value).unwrap_or_default(),CellValue::Number((r*3+c+1) as f64),"{axis:?}/{at}/{r}/{c}");}}let sh=crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();assert_eq!(sh.spills.get(&(row,col)),Some(&CellRange {first_row:row,last_row:row+2,first_column:col,last_column:col+2}));let (r,c)=if axis==Axis::Rows{(6,5)}else{(5,6)};assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(45.0));if at==2{let (r,c)=if axis==Axis::Rows{(4,1)}else{(1,4)};assert!(cell(&d,s,r,c).is_none_or(|c|c.value.is_empty()));}assert!(d.undo());assert_eq!(*d.presentation(),before);assert!(d.redo());assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Number(45.0));}}
    }

    #[test]
    fn deleting_spill_anchor_clears_owned_values_and_keeps_blockers() {
        for axis in [Axis::Rows,Axis::Columns]{let (mut d,s)=workbook();set(&mut d,s,1,1,CellInput::Formula("SEQUENCE(3,3)".into()));set(&mut d,s,5,5,CellInput::Formula("SUM($B$2#)".into()));let before=d.presentation().clone();d.execute(vec![Command::Sheet(SheetCommand::Delete {slide:s,axis,at:1,count:1})]).unwrap();for row in 1..4{for col in 1..4{assert!(cell(&d,s,row,col).is_none_or(|c|c.value.is_empty()),"{axis:?}/{row}/{col}");}}let (r,c)=if axis==Axis::Rows{(4,5)}else{(5,4)};assert_eq!(cell(&d,s,r,c).unwrap().value,CellValue::Error(CellError::Ref));assert!(d.undo());assert_eq!(*d.presentation(),before);assert!(d.redo());}
        let (mut d,s)=workbook();set(&mut d,s,0,0,CellInput::Formula("SEQUENCE(3)".into()));set(&mut d,s,1,0,CellInput::Value(CellValue::Text("blocker".into())));assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Error(CellError::Spill));d.execute(vec![Command::Sheet(SheetCommand::Delete {slide:s,axis:Axis::Rows,at:0,count:1})]).unwrap();assert_eq!(cell(&d,s,0,0).unwrap().value,CellValue::Text("blocker".into()));
    }

    #[test]
    fn surviving_spill_after_partial_structure_edits() {
        for axis in [Axis::Rows, Axis::Columns] {
            for (insert, at, count) in [(false, 2, 1), (false, 2, 2), (true, 2, 2)] {
                let (mut d, slide) = workbook();
                set(&mut d, slide, 1, 1, CellInput::Formula("SEQUENCE(3,3)".into()));
                set(&mut d, slide, 8, 8, CellInput::Formula("SUM($B$2#)".into()));
                let before = d.presentation().clone();
                let command = if insert { SheetCommand::Insert { slide, axis, at, count } } else { SheetCommand::Delete { slide, axis, at, count } };
                d.execute(vec![Command::Sheet(command)]).unwrap();
                for r in 0..3 { for c in 0..3 { assert_eq!(cell(&d, slide, r+1, c+1).map(|x|x.value).unwrap_or_default(), CellValue::Number((r*3+c+1) as f64), "{axis:?}/{insert}/{count}/{r}/{c}"); } }
                let offset = if insert { 8+count } else { 8-count };
                let (r,c) = if axis == Axis::Rows { (offset,8) } else { (8,offset) };
                assert_eq!(cell(&d, slide, r, c).unwrap().value, CellValue::Number(45.0));
                let sh = crate::sheet_calc::sheet_of_slide(d.presentation().slide(slide).unwrap()).unwrap();
                assert_eq!(sh.spills.get(&(1,1)), Some(&CellRange { first_row:1, last_row:3, first_column:1, last_column:3 }));
                assert!(d.undo()); assert_eq!(*d.presentation(), before); assert!(d.redo());
                assert_eq!(cell(&d, slide, r, c).unwrap().value, CellValue::Number(45.0));
            }
        }
    }

    #[test]
    fn match_option_inputs_undo_redo() {
        for line in include_str!("../../../test/editing/xlsx-match-options.tsv").lines().skip(1) {
            let (formula,expected)=line.split_once('\t').unwrap();
            let expected=expected.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(if expected=="#N/A"{CellError::NA}else{CellError::Value}));
            let (mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula(formula.into()));assert_eq!(cell(&d,slide,0,0).unwrap().value,expected,"{formula}");assert!(d.undo());assert!(cell(&d,slide,0,0).is_none());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,expected);
        }
    }

    #[test]
    fn unique_options_dependency_undo_redo() {
        for line in include_str!("../../../test/editing/xlsx-unique-options.tsv").lines().skip(1) {
            let fields:Vec<_>=line.split('\t').collect();let (mut d,slide)=workbook();
            for (row,values) in [[1.,10.,1.],[2.,20.,2.],[1.,10.,1.]].into_iter().enumerate(){for (column,n)in values.into_iter().enumerate(){set(&mut d,slide,row as u32,column as u32,CellInput::Value(CellValue::Number(n)));}}
            set(&mut d,slide,0,4,CellInput::Formula(fields[0].into()));
            let check=|d:&Document,text:&str|{let rows:Vec<Vec<f64>>=text.split(';').map(|r|r.split(',').map(|v|v.parse().unwrap()).collect()).collect();let sh=crate::sheet_calc::sheet_of_slide(d.presentation().slide(slide).unwrap()).unwrap();assert_eq!(sh.spills.get(&(0,4)),Some(&CellRange {first_row:0,last_row:rows.len()as u32-1,first_column:4,last_column:4+rows[0].len()as u32-1}));for r in 0..3{for c in 0..3{assert_eq!(cell(d,slide,r,c+4).map(|x|x.value).unwrap_or_default(),rows.get(r as usize).and_then(|x|x.get(c as usize)).map_or(CellValue::Empty,|n|CellValue::Number(*n)),"{} {r}/{c}",fields[0]);}}};
            check(&d,fields[1]);set(&mut d,slide,2,2,CellInput::Value(CellValue::Number(3.0)));check(&d,fields[2]);assert!(d.undo());check(&d,fields[1]);assert!(d.redo());check(&d,fields[2]);
        }
    }

    #[test]
    fn makearray_dimensions_are_positive_integers() {
        for (rows,columns) in [("2.9","3"),("3","2.9"),("0","3"),("3","0"),("-1","3"),("3","-1")] {
            let formula=format!("MAKEARRAY({rows},{columns},LAMBDA(r,c,r*10+c))");let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula(formula.clone()));assert_eq!(cell(&d,slide,0,0).unwrap().value,CellValue::Error(CellError::Value),"{formula}");assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,CellValue::Error(CellError::Value));
        }
        for columns in [false,true]{let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Value(CellValue::Number(2.0)));let formula=if columns{"MAKEARRAY(3,A1,LAMBDA(r,c,r*10+c))"}else{"MAKEARRAY(A1,3,LAMBDA(r,c,r*10+c))"};set(&mut d,slide,2,2,CellInput::Formula(formula.into()));let before=d.presentation().clone();set(&mut d,slide,0,0,CellInput::Value(CellValue::Number(2.9)));assert_eq!(cell(&d,slide,2,2).unwrap().value,CellValue::Error(CellError::Value));for r in 2..5{for c in 2..5{if(r,c)!=(2,2){assert!(cell(&d,slide,r,c).is_none_or(|x|x.value.is_empty()));}}}assert!(d.undo());assert_eq!(*d.presentation(),before);assert!(d.redo());assert_eq!(cell(&d,slide,2,2).unwrap().value,CellValue::Error(CellError::Value));}
        let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula("MAKEARRAY(2,3,LAMBDA(r,c,r*10+c))".into()));for r in 0..2{for c in 0..3{assert_eq!(cell(&d,slide,r,c).unwrap().value,CellValue::Number(((r+1)*10+c+1)as f64));}}
    }

    #[test]
    fn file_lambda_parameters_do_not_become_cell_references() {
        for (file,expected)in [("_xlfn.MAP({1;2},{3;4},_xlfn.LAMBDA(_xlpm.x0,_xlpm.x1,_xlpm.x0+_xlpm.x1))",4.0),("_xlfn.LET(_xlpm.A1,5,_xlpm.A1+A1)",15.0),("_xlfn.LET(_xlpm.TRUE,5,_xlpm.TRUE+IF(TRUE,1,0))",6.0),("_xlfn.LAMBDA(_xlpm.A1,_xlpm.A1+1)(5)",6.0),("_xlfn.LET(_XLPM.A1,5,_xlpm.a1+A1)",15.0)] {
            let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Value(CellValue::Number(10.0)));let formula=ibis_calc::formula::from_file(file);set(&mut d,slide,3,3,CellInput::Formula(formula));assert_eq!(cell(&d,slide,3,3).unwrap().value,CellValue::Number(expected),"{file}");assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,3,3).unwrap().value,CellValue::Number(expected));
        }
    }

    #[test]
    fn map_mismatched_shapes_do_not_broadcast() {
        for line in include_str!("../../../test/editing/xlsx-map-shapes.tsv").lines().skip(1){let(formula,expected)=line.split_once('\t').unwrap();let(mut d,slide)=workbook();set(&mut d,slide,9,9,CellInput::Value(CellValue::Number(5.0)));set(&mut d,slide,0,0,CellInput::Formula(formula.into()));let rows:Vec<Vec<_>>=expected.split(';').map(|r|r.split(',').map(|v|v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()))).collect()).collect();let check=|d:&Document|{for row in 0..2{for col in 0..2{assert_eq!(cell(d,slide,row,col).map(|x|x.value).unwrap_or_default(),rows.get(row as usize).and_then(|r|r.get(col as usize)).cloned().unwrap_or_default(),"{formula}/{row}/{col}");}}};check(&d);assert!(d.undo());assert!(d.redo());check(&d);}
        let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula("{1;2}+{10,20}".into()));for(row,values)in [[11.,21.],[12.,22.]].into_iter().enumerate(){for(col,n)in values.into_iter().enumerate(){assert_eq!(cell(&d,slide,row as u32,col as u32).unwrap().value,CellValue::Number(n));}}
    }

    #[test]
    fn lambda_arity_and_explicit_omission() {
        for line in include_str!("../../../test/editing/xlsx-lambda-arity.tsv").lines().skip(1){let(formula,v)=line.split_once('\t').unwrap();let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()));let(mut d,slide)=workbook();set(&mut d,slide,0,0,CellInput::Formula(formula.into()));assert_eq!(cell(&d,slide,0,0).unwrap().value,expected,"{formula}");assert!(d.undo());assert!(d.redo());assert_eq!(cell(&d,slide,0,0).unwrap().value,expected);}
    }

    #[test]
    fn omitted_numeric_aggregates_keep_empty_compatibility() {
        let mut mismatches = Vec::new();
        for function in ["AVERAGE", "AVERAGEA", "MIN"] {
            let (mut d, slide) = workbook();
            let formula = format!("LAMBDA(first,value,{function}(value,2))(0,)");
            set(&mut d, slide, 0, 0, CellInput::Formula(formula.clone()));
            let actual = cell(&d, slide, 0, 0).unwrap().value;
            if actual != CellValue::Number(2.0) { mismatches.push((formula, actual)); }
        }
        assert!(mismatches.is_empty(), "omitted numeric aggregates: {mismatches:?}");
    }

    #[test]
    fn isomitted_distinguishes_data_and_nearest_binding() {
        for line in include_str!("../../../test/editing/xlsx-isomitted.tsv").lines().skip(1){let(formula,expected)=line.split_once('\t').unwrap();let(mut d,slide)=workbook();set(&mut d,slide,1,0,CellInput::Value(CellValue::Number(7.0)));set(&mut d,slide,3,3,CellInput::Formula(formula.into()));let check=|d:&Document|{for(row,v)in expected.split(';').enumerate(){assert_eq!(cell(d,slide,row as u32+3,3).unwrap().value,CellValue::Number(v.parse().unwrap()),"{formula}/{row}");}};check(&d);assert!(d.undo());assert!(d.redo());check(&d);}
    }

    #[test]
    fn dynamic_arrays_spill() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Formula("SEQUENCE(3)".into()));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Number(3.0));
        set(&mut d, s, 0, 1, CellInput::Formula("SUM(A1#)".into()));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(6.0));
        // Blocked: #SPILL!, the spilled values gone, the sum follows.
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Text("x".into())));
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Error(crate::sheet::CellError::Spill));
        assert!(cell(&d, s, 2, 0).is_none_or(|c| c.value.is_empty()));
        // Cleared again: it spills again.
        set(&mut d, s, 1, 0, CellInput::Clear);
        assert_eq!(cell(&d, s, 1, 0).unwrap().value, CellValue::Number(2.0));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(6.0));
        // A bigger array: more spills; a smaller one takes back the rest.
        set(&mut d, s, 0, 0, CellInput::Formula("SEQUENCE(5)".into()));
        assert_eq!(cell(&d, s, 4, 0).unwrap().value, CellValue::Number(5.0));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(15.0));
        set(&mut d, s, 0, 0, CellInput::Formula("SEQUENCE(2)".into()));
        assert!(cell(&d, s, 4, 0).is_none_or(|c| c.value.is_empty()));
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn moving_cells() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(3.0)));
        set(&mut d, s, 0, 1, CellInput::Formula("A1*2".into()));
        set(&mut d, s, 0, 2, CellInput::Formula("SUM(A1:B1)".into()));
        let from = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
        d.execute(vec![Command::Sheet(SheetCommand::MoveCells { slide: s, from, row: 5, column: 0 })]).unwrap();
        assert!(cell(&d, s, 0, 0).is_none());
        assert_eq!(cell(&d, s, 5, 0).unwrap().value, CellValue::Number(3.0));
        assert_eq!(cell(&d, s, 0, 1).unwrap().formula.as_deref(), Some("A6*2"));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(6.0));
        assert_eq!(cell(&d, s, 0, 2).unwrap().formula.as_deref(), Some("SUM(A1:B1)"), "partly inside: stays");
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn formats() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(1234.5)));
        let range = CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 1 };
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change: StyleChange::Bold(true) })]).unwrap();
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change: StyleChange::NumberFormat("#,##0.00".into()) })]).unwrap();
        assert_eq!(cell(&d, s, 0, 0).unwrap().text, "1,234.50");
        assert!(cell(&d, s, 1, 1).is_some());
        while d.undo() {}
        assert!(*d.presentation() == start);
    }

    #[test]
    fn encodeurl_and_japanese_numerals() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();

        // ENCODEURL: encodes non-alphanumeric/non-unreserved characters to %XX
        set(&mut d, s, 0, 0, CellInput::Formula("ENCODEURL(\"https://example.com/search?q=東京&tag=a~b_c-d.e\")".into()));
        assert_eq!(
            cell(&d, s, 0, 0).unwrap().value,
            CellValue::Text("https%3A%2F%2Fexample.com%2Fsearch%3Fq%3D%E6%9D%B1%E4%BA%AC%26tag%3Da~b_c-d.e".into())
        );

        // NUMBERSTRING: Japanese numeral conversions (kinds 1, 2, 3)
        set(&mut d, s, 1, 0, CellInput::Formula("NUMBERSTRING(12345, 1)".into()));
        assert_eq!(cell(&d, s, 1, 0).unwrap().value, CellValue::Text("一万二千三百四十五".into()));

        set(&mut d, s, 2, 0, CellInput::Formula("NUMBERSTRING(12345, 2)".into()));
        assert_eq!(cell(&d, s, 2, 0).unwrap().value, CellValue::Text("壱萬弐阡参百四拾伍".into()));

        set(&mut d, s, 3, 0, CellInput::Formula("NUMBERSTRING(12345, 3)".into()));
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Text("一二三四五".into()));

        // Invalid NUMBERSTRING parameters give #NUM!
        set(&mut d, s, 4, 0, CellInput::Formula("NUMBERSTRING(-1, 1)".into()));
        assert_eq!(cell(&d, s, 4, 0).unwrap().value, CellValue::Error(CellError::Num));

        set(&mut d, s, 5, 0, CellInput::Formula("NUMBERSTRING(100, 4)".into()));
        assert_eq!(cell(&d, s, 5, 0).unwrap().value, CellValue::Error(CellError::Num));

        // DATESTRING: format date serials to Japanese era dates
        set(&mut d, s, 6, 0, CellInput::Formula("DATESTRING(DATE(2023, 1, 1))".into()));
        assert_eq!(cell(&d, s, 6, 0).unwrap().value, CellValue::Text("令和5年1月1日".into()));

        set(&mut d, s, 7, 0, CellInput::Formula("DATESTRING(DATE(1989, 1, 8))".into()));
        assert_eq!(cell(&d, s, 7, 0).unwrap().value, CellValue::Text("平成1年1月8日".into()));

        set(&mut d, s, 8, 0, CellInput::Formula("DATESTRING(-1)".into()));
        assert_eq!(cell(&d, s, 8, 0).unwrap().value, CellValue::Error(CellError::Num));

        while d.undo() {}
        assert!(*d.presentation() == start);
        while d.redo() {}
        assert_eq!(cell(&d, s, 6, 0).unwrap().value, CellValue::Text("令和5年1月1日".into()));
    }

    #[test]
    fn randarray_and_anchorarray() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();

        // RANDARRAY spills a 2x3 grid of random numbers in [0, 1)
        set(&mut d, s, 0, 0, CellInput::Formula("RANDARRAY(2, 3)".into()));
        let mut grid_sum = 0.0;
        for r in 0..2 {
            for c in 0..3 {
                match cell(&d, s, r, c).unwrap().value {
                    CellValue::Number(n) if (0.0..1.0).contains(&n) => grid_sum += n,
                    other => panic!("{other:?}"),
                }
            }
        }
        let sh = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sh.spills.get(&(0, 0)), Some(&CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 2 }));

        // SUM over spill area using A1#
        set(&mut d, s, 5, 0, CellInput::Formula("SUM(A1#)".into()));
        assert!(matches!(cell(&d, s, 5, 0).unwrap().value, CellValue::Number(n) if (0.0..6.0).contains(&n)));
        let _ = grid_sum;

        // ANCHORARRAY explicit function call
        set(&mut d, s, 6, 0, CellInput::Formula("SUM(ANCHORARRAY(A1))".into()));
        assert!(matches!(cell(&d, s, 6, 0).unwrap().value, CellValue::Number(n) if (0.0..6.0).contains(&n)));

        // RANDARRAY with integer mode and bounds
        set(&mut d, s, 10, 0, CellInput::Formula("RANDARRAY(2, 2, 5, 10, TRUE)".into()));
        for r in 10..12 {
            for c in 0..2 {
                assert!(matches!(cell(&d, s, r, c).unwrap().value, CellValue::Number(n) if (5.0..=10.0).contains(&n) && n.fract() == 0.0));
            }
        }

        // Out-of-bounds parameter validation
        set(&mut d, s, 15, 0, CellInput::Formula("RANDARRAY(-1, 2)".into()));
        assert_eq!(cell(&d, s, 15, 0).unwrap().value, CellValue::Error(CellError::Value));

        set(&mut d, s, 16, 0, CellInput::Formula("RANDARRAY(2, 2, 10, 5)".into()));
        assert_eq!(cell(&d, s, 16, 0).unwrap().value, CellValue::Error(CellError::Value));

        while d.undo() {}
        assert!(*d.presentation() == start);
        while d.redo() {}
        assert!(matches!(cell(&d, s, 5, 0).unwrap().value, CellValue::Number(n) if (0.0..6.0).contains(&n)));
    }

    #[test]
    fn trimrange_array_and_options() {
        let (mut d, s) = workbook();
        let start = d.presentation().clone();

        // Populate a 2x2 data island surrounded by empty cells:
        // Rows 2 and 3, Columns 1 and 2
        set(&mut d, s, 2, 1, CellInput::Value(CellValue::Number(10.0)));
        set(&mut d, s, 2, 2, CellInput::Value(CellValue::Number(20.0)));
        set(&mut d, s, 3, 1, CellInput::Value(CellValue::Number(30.0)));
        set(&mut d, s, 3, 2, CellInput::Value(CellValue::Number(40.0)));

        // Default TRIMRANGE(A1:D5) trims both leading and trailing empty rows & cols -> 2x2 spilled result
        set(&mut d, s, 5, 0, CellInput::Formula("TRIMRANGE(A1:D5)".into()));
        assert_eq!(cell(&d, s, 5, 0).unwrap().value, CellValue::Number(10.0));
        assert_eq!(cell(&d, s, 5, 1).unwrap().value, CellValue::Number(20.0));
        assert_eq!(cell(&d, s, 6, 0).unwrap().value, CellValue::Number(30.0));
        assert_eq!(cell(&d, s, 6, 1).unwrap().value, CellValue::Number(40.0));
        let sh = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sh.spills.get(&(5, 0)), Some(&CellRange { first_row: 5, last_row: 6, first_column: 0, last_column: 1 }));

        // SUM over the trimmed spill area
        set(&mut d, s, 7, 0, CellInput::Formula("SUM(A6#)".into()));
        assert_eq!(cell(&d, s, 7, 0).unwrap().value, CellValue::Number(100.0));

        // Options: trim leading rows only (1), no col trim (0) -> rows 2..=4, cols 0..=3 (3 rows x 4 cols)
        set(&mut d, s, 8, 0, CellInput::Formula("TRIMRANGE(A1:D5, 1, 0)".into()));
        let sh = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sh.spills.get(&(8, 0)), Some(&CellRange { first_row: 8, last_row: 10, first_column: 0, last_column: 3 }));
        assert_eq!(cell(&d, s, 8, 1).unwrap().value, CellValue::Number(10.0));

        // Entirely empty range returns #REF!
        set(&mut d, s, 12, 0, CellInput::Formula("TRIMRANGE(E1:G4)".into()));
        assert_eq!(cell(&d, s, 12, 0).unwrap().value, CellValue::Error(CellError::Ref));

        // Invalid options return #VALUE!
        set(&mut d, s, 13, 0, CellInput::Formula("TRIMRANGE(A1:D5, -1)".into()));
        assert_eq!(cell(&d, s, 13, 0).unwrap().value, CellValue::Error(CellError::Value));

        set(&mut d, s, 14, 0, CellInput::Formula("TRIMRANGE(A1:D5, 1, 4)".into()));
        assert_eq!(cell(&d, s, 14, 0).unwrap().value, CellValue::Error(CellError::Value));

        while d.undo() {}
        assert!(*d.presentation() == start);
        while d.redo() {}
        assert_eq!(cell(&d, s, 7, 0).unwrap().value, CellValue::Number(100.0));
    }

    /// A one-column sheet: a heading, then these values (numbers or text)
    /// in column A, styled by index.
    fn filter_sheet(values: &[(&str, u32)]) -> Sheet {
        let mut sheet = Sheet { columns: vec![100; 2], rows: vec![100; values.len() + 1], ..Sheet::default() };
        sheet.styles = vec![CellStyle::default(), CellStyle { fill: Some(Rgb(0xFFFF00)), ..CellStyle::default() }, CellStyle { color: Rgb(0xFF0000), ..CellStyle::default() }];
        let mut put = |row: u32, text: &str, style: u32| {
            if text.is_empty() {
                return;
            }
            let value = text.parse::<f64>().map_or(CellValue::Text(text.into()), CellValue::Number);
            sheet.put_cell(row, 0, Some(SheetCell { runs: None, phonetic: None, row, column: 0, value, text: text.into(), kind: CellKind::Text, formula: None, style }));
        };
        put(0, "見出し", 0);
        for (i, (text, style)) in values.iter().enumerate() {
            put(i as u32 + 1, text, *style);
        }
        sheet
    }

    /// The rows (1 = the first under the heading) a filter keeps.
    fn kept(sheet: &Sheet, filter: crate::sheet::ColumnFilter, today: Option<i64>) -> Vec<u32> {
        let range = CellRange { first_row: 0, last_row: sheet.rows.len() as u32 - 1, first_column: 0, last_column: 0 };
        let out = filtered_rows_on(sheet, &crate::sheet::AutoFilter { range, columns: vec![(0, filter)] }, today, false);
        (1..sheet.rows.len() as u32).filter(|r| !out.contains(r)).collect()
    }

    #[test]
    fn text_number_and_wildcard_filters() {
        use crate::sheet::ColumnFilter;
        assert!(wildcard_match("*店", "駅前店") && wildcard_match("駅?店", "駅前店") && !wildcard_match("駅?店", "駅前本店"));
        assert!(wildcard_match("a~*b", "A*b") && !wildcard_match("a~*b", "axb") && wildcard_match("*", ""));
        let sheet = filter_sheet(&[("駅前店", 0), ("本店", 0), ("10", 0), ("", 0), ("25", 0), ("駅南", 0)]);
        let custom = |and: bool, c: &[(&str, &str)]| ColumnFilter::Custom { and, conditions: c.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() };
        // Contains, begins with, ends with, does not contain.
        assert_eq!(kept(&sheet, custom(false, &[("equal", "*前*")]), None), vec![1]);
        assert_eq!(kept(&sheet, custom(false, &[("equal", "駅*")]), None), vec![1, 6]);
        assert_eq!(kept(&sheet, custom(false, &[("equal", "*店")]), None), vec![1, 2]);
        assert_eq!(kept(&sheet, custom(false, &[("notEqual", "*店*")]), None), vec![3, 4, 5, 6]);
        // Between (both), either of two, and numbers with text beside them.
        assert_eq!(kept(&sheet, custom(true, &[("greaterThanOrEqual", "10"), ("lessThan", "25")]), None), vec![3]);
        assert_eq!(kept(&sheet, custom(false, &[("equal", "本店"), ("greaterThan", "20")]), None), vec![2, 5]);
        // 空白以外: notEqual a space.
        assert_eq!(kept(&sheet, custom(false, &[("notEqual", " ")]), None), vec![1, 2, 3, 5, 6]);
        // Blank cells, as a value of the list.
        assert_eq!(kept(&sheet, ColumnFilter::Values(vec!["".into(), "本店".into()]), None), vec![2, 4]);
    }

    #[test]
    fn top_period_date_and_colour_filters() {
        use crate::sheet::{ColumnFilter, DateGroup};
        use ibis_calc::eval::convert::serial_from_date;
        let sheet = filter_sheet(&[("5", 0), ("40", 0), ("15", 1), ("30", 2), ("20", 0)]);
        let top = |top: bool, percent: bool, value: &str| ColumnFilter::Top { top, percent, value: value.into() };
        assert_eq!(kept(&sheet, top(true, false, "2"), None), vec![2, 4]);
        assert_eq!(kept(&sheet, top(false, false, "1"), None), vec![1]);
        assert_eq!(kept(&sheet, top(true, true, "40"), None), vec![2, 4]);
        assert_eq!(kept(&sheet, ColumnFilter::Color { cell: true, color: Some(Rgb(0xFFFF00)) }, None), vec![3]);
        assert_eq!(kept(&sheet, ColumnFilter::Color { cell: false, color: Some(Rgb(0xFF0000)) }, None), vec![4]);
        assert_eq!(kept(&sheet, ColumnFilter::Color { cell: true, color: None }, None), vec![1, 2, 4, 5]);
        let day = |y, m, d| serial_from_date(y, m, d, false).to_string();
        let dates = [day(2026, 10, 7), day(2026, 10, 1), day(2026, 9, 30), day(2026, 10, 4), day(2025, 10, 7), day(2026, 1, 15)];
        let sheet = filter_sheet(&dates.iter().map(|d| (d.as_str(), 0)).collect::<Vec<_>>());
        let today = Some(serial_from_date(2026, 10, 7, false));
        let period = |k: &str| kept(&sheet, ColumnFilter::Period(k.into()), today);
        assert_eq!(period("today"), vec![1]);
        assert_eq!(period("thisMonth"), vec![1, 2, 4]);
        assert_eq!(period("lastMonth"), vec![3]);
        // 2026-10-07 is a Wednesday: the week from Sunday the 4th.
        assert_eq!(period("thisWeek"), vec![1, 4]);
        assert_eq!(period("lastWeek"), vec![2, 3]);
        assert_eq!(period("thisQuarter"), vec![1, 2, 4]);
        assert_eq!(period("lastQuarter"), vec![3]);
        assert_eq!(period("thisYear"), vec![1, 2, 3, 4, 6]);
        assert_eq!(period("lastYear"), vec![5]);
        assert_eq!(period("M10"), vec![1, 2, 4, 5]);
        assert_eq!(period("Q1"), vec![6]);
        assert_eq!(kept(&sheet, ColumnFilter::Period("today".into()), None), Vec::<u32>::new());
        let group = |grouping: &str, year, month, day| DateGroup { grouping: grouping.into(), year, month, day, hour: 0, minute: 0, second: 0 };
        let by = |groups| kept(&sheet, ColumnFilter::Dates { values: Vec::new(), groups }, None);
        assert_eq!(by(vec![group("month", 2026, 10, 0)]), vec![1, 2, 4]);
        assert_eq!(by(vec![group("year", 2025, 0, 0), group("day", 2026, 9, 30)]), vec![3, 5]);
    }

    #[test]
    fn sorting_by_colour_puts_its_rows_first_or_last() {
        let (d, s) = workbook();
        let mut p = d.presentation().clone();
        {
            let sheet = sheet_mut(&mut p, s).unwrap();
            *sheet = Sheet { rows: vec![100; 6], columns: vec![100; 2], ..filter_sheet(&[("1", 0), ("2", 1), ("3", 2), ("4", 1), ("5", 0)]) };
        }
        let mut d = Document::new(p);
        let range = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 };
        let order = |d: &Document| (1..6).map(|r| cell(d, s, r, 0).unwrap().text).collect::<Vec<_>>().join(",");
        d.execute(vec![Command::Sheet(SheetCommand::SortBy { slide: s, range, keys: vec![(0, SortBy::CellColor { color: Some(Rgb(0xFFFF00)), top: true })], header: true, match_case: false })]).unwrap();
        assert_eq!(order(&d), "2,4,1,3,5");
        d.undo();
        d.execute(vec![Command::Sheet(SheetCommand::SortBy { slide: s, range, keys: vec![(0, SortBy::FontColor { color: Some(Rgb(0xFF0000)), top: false }), (0, SortBy::Value(false))], header: true, match_case: false })]).unwrap();
        assert_eq!(order(&d), "5,4,2,1,3");
    }

    #[test]
    fn sorting_by_icon_puts_its_rows_first_or_last() {
        let (d, s) = workbook();
        let mut p = d.presentation().clone();
        {
            let sheet = sheet_mut(&mut p, s).unwrap();
            *sheet = Sheet {
                rows: vec![100; 6],
                columns: vec![100; 2],
                conditional: vec![crate::sheet::ConditionalFormat {
                    source_index: None,
                    ranges: vec![CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 }],
                    rules: vec![crate::sheet::CfRule {
                        kind: crate::sheet::CfKind::IconSet {
                            set: "3TrafficLights1".into(),
                            values: vec![
                                crate::sheet::CfValue { kind: "num".into(), value: Some("0".into()), gte: true },
                                crate::sheet::CfValue { kind: "num".into(), value: Some("30".into()), gte: true },
                                crate::sheet::CfValue { kind: "num".into(), value: Some("70".into()), gte: true },
                            ],
                            reverse: false,
                            show_value: true,
                            custom: false,
                            icons: Vec::new(),
                        },
                        priority: 1,
                        stop: false,
                        style: None,
                    }],
                }],
                ..filter_sheet(&[("10", 0), ("80", 0), ("50", 0), ("90", 0), ("20", 0)])
            };
        }
        let mut d = Document::new(p);
        let range = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 };
        let order = |d: &Document| (1..6).map(|r| cell(d, s, r, 0).unwrap().text).collect::<Vec<_>>().join(",");
        // 10 -> slot 0 (red)
        // 80 -> slot 2 (green)
        // 50 -> slot 1 (yellow)
        // 90 -> slot 2 (green)
        // 20 -> slot 0 (red)
        // Sort by icon_id: 2 (green) on top: 80 and 90 come first, then others: 80,90,10,50,20
        d.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s,
            range,
            keys: vec![(0, SortBy::Icon { icon_set: "3TrafficLights1".into(), icon_id: 2, top: true })],
            header: true,
            match_case: false,
        })]).unwrap();
        assert_eq!(order(&d), "80,90,10,50,20");
        d.undo();
        assert_eq!(order(&d), "10,80,50,90,20");
        // Sort by icon_id: 0 (red) at bottom: non-red first (80,50,90), red at bottom (10,20)
        d.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s,
            range,
            keys: vec![(0, SortBy::Icon { icon_set: "3TrafficLights1".into(), icon_id: 0, top: false })],
            header: true,
            match_case: false,
        })]).unwrap();
        assert_eq!(order(&d), "80,50,90,10,20");
    }

    #[test]
    fn icon_filter_keeps_matching_icon_rows() {
        let (d, s) = workbook();
        let mut p = d.presentation().clone();
        {
            let sheet = sheet_mut(&mut p, s).unwrap();
            *sheet = Sheet {
                rows: vec![100; 6],
                columns: vec![100; 2],
                conditional: vec![crate::sheet::ConditionalFormat {
                    source_index: None,
                    ranges: vec![CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 }],
                    rules: vec![crate::sheet::CfRule {
                        kind: crate::sheet::CfKind::IconSet {
                            set: "3TrafficLights1".into(),
                            values: vec![
                                crate::sheet::CfValue { kind: "num".into(), value: Some("0".into()), gte: true },
                                crate::sheet::CfValue { kind: "num".into(), value: Some("30".into()), gte: true },
                                crate::sheet::CfValue { kind: "num".into(), value: Some("70".into()), gte: true },
                            ],
                            reverse: false,
                            show_value: true,
                            custom: false,
                            icons: Vec::new(),
                        },
                        priority: 1,
                        stop: false,
                        style: None,
                    }],
                }],
                ..filter_sheet(&[("10", 0), ("80", 0), ("50", 0), ("90", 0), ("20", 0)])
            };
        }
        let sheet = sheet_mut(&mut p, s).unwrap();
        let filter = crate::sheet::AutoFilter {
            range: CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 },
            columns: vec![(0, crate::sheet::ColumnFilter::Icon { icon_set: "3TrafficLights1".into(), icon_id: 2 })],
        };
        // icon_id 2 matches rows with 80 (row 2) and 90 (row 4). Excluded rows: 1, 3, 5.
        let hidden = filtered_rows(sheet, &filter, false);
        assert_eq!(hidden.into_iter().collect::<Vec<_>>(), vec![1, 3, 5]);
    }

    #[test]
    fn case_sensitive_sort_distinguishes_case() {
        let (d, s) = workbook();
        let mut p = d.presentation().clone();
        {
            let sheet = sheet_mut(&mut p, s).unwrap();
            *sheet = Sheet { rows: vec![100; 5], columns: vec![100; 2], ..filter_sheet(&[("b", 0), ("B", 0), ("a", 0), ("A", 0)]) };
        }
        let mut d = Document::new(p);
        let range = CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 };
        let order = |d: &Document| (1..5).map(|r| cell(d, s, r, 0).unwrap().text).collect::<Vec<_>>().join(",");

        // Case-sensitive sort ascending: uppercase precedes lowercase (A, a, B, b)
        d.execute(vec![Command::Sheet(SheetCommand::SortBy { slide: s, range, keys: vec![(0, SortBy::Value(true))], header: true, match_case: true })]).unwrap();
        assert_eq!(order(&d), "A,a,B,b");
        assert!(d.undo());
        assert_eq!(order(&d), "b,B,a,A");
        assert!(d.redo());
        assert_eq!(order(&d), "A,a,B,b");

        // Case-sensitive sort descending: b, B, a, A
        d.execute(vec![Command::Sheet(SheetCommand::SortBy { slide: s, range, keys: vec![(0, SortBy::Value(false))], header: true, match_case: true })]).unwrap();
        assert_eq!(order(&d), "b,B,a,A");

        // Case-insensitive sort ascending: stable ordering between identical case-folded letters (a from row 3, A from row 4; b from row 1, B from row 2)
        d.execute(vec![Command::Sheet(SheetCommand::SortBy { slide: s, range, keys: vec![(0, SortBy::Value(true))], header: true, match_case: false })]).unwrap();
        assert_eq!(order(&d), "a,A,b,B");
    }

    #[test]
    fn custom_list_sort_orders_by_list_and_handles_unmatched() {
        let (d, s) = workbook();
        let mut p = d.presentation().clone();
        {
            let sheet = sheet_mut(&mut p, s).unwrap();
            *sheet = Sheet {
                rows: vec![100; 7],
                columns: vec![100; 2],
                ..filter_sheet(&[("水", 0), ("月", 0), ("金", 0), ("日", 0), ("その他", 0), ("火", 0)])
            };
        }
        let mut d = Document::new(p);
        let range = CellRange { first_row: 0, last_row: 6, first_column: 0, last_column: 0 };
        let order = |d: &Document| (1..7).map(|r| cell(d, s, r, 0).unwrap().text).collect::<Vec<_>>().join(",");

        let days = vec![
            "日".into(), "月".into(), "火".into(), "水".into(), "木".into(), "金".into(), "土".into(),
        ];

        // Custom list ascending: matched items follow list order (日, 月, 火, 水, 金), unmatched item ("その他") at end
        d.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s,
            range,
            keys: vec![(0, SortBy::CustomList { list: days.clone(), ascending: true })],
            header: true,
            match_case: false,
        })]).unwrap();
        assert_eq!(order(&d), "日,月,火,水,金,その他");

        // Undo restores original order
        assert!(d.undo());
        assert_eq!(order(&d), "水,月,金,日,その他,火");

        // Redo restores custom list sorted order
        assert!(d.redo());
        assert_eq!(order(&d), "日,月,火,水,金,その他");

        // Custom list descending: reverse list order (金, 水, 火, 月, 日), unmatched item ("その他") at end
        d.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s,
            range,
            keys: vec![(0, SortBy::CustomList { list: days, ascending: false })],
            header: true,
            match_case: false,
        })]).unwrap();
        assert_eq!(order(&d), "金,水,火,月,日,その他");

        // Case-sensitivity test:
        let (d2, s2) = workbook();
        let mut p2 = d2.presentation().clone();
        {
            let sheet = sheet_mut(&mut p2, s2).unwrap();
            *sheet = Sheet {
                rows: vec![100; 6],
                columns: vec![100; 2],
                ..filter_sheet(&[("Sun", 0), ("mon", 0), ("TUE", 0), ("Wed", 0), ("other", 0)])
            };
        }
        let mut d2 = Document::new(p2);
        let range2 = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 };
        let order2 = |d: &Document| (1..6).map(|r| cell(d, s2, r, 0).unwrap().text).collect::<Vec<_>>().join(",");
        let eng_days = vec!["Sun".into(), "Mon".into(), "Tue".into(), "Wed".into()];

        // Case-insensitive custom list sort: "mon" and "TUE" match "Mon" and "Tue"
        d2.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s2,
            range: range2,
            keys: vec![(0, SortBy::CustomList { list: eng_days.clone(), ascending: true })],
            header: true,
            match_case: false,
        })]).unwrap();
        assert_eq!(order2(&d2), "Sun,mon,TUE,Wed,other");

        // Case-sensitive custom list sort: "mon" and "TUE" do NOT match, only "Sun" and "Wed" match
        // Non-matching items "mon", "other", "TUE" sort alphabetically after matching items
        d2.execute(vec![Command::Sheet(SheetCommand::SortBy {
            slide: s2,
            range: range2,
            keys: vec![(0, SortBy::CustomList { list: eng_days, ascending: true })],
            header: true,
            match_case: true,
        })]).unwrap();
        assert_eq!(order2(&d2), "Sun,Wed,mon,other,TUE");
    }

    #[test]
    fn duplicate_and_delete_conditional_rule() {
        let (mut d, s) = workbook();
        let r1 = crate::sheet::ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 10, first_column: 0, last_column: 1 }],
            rules: vec![crate::sheet::CfRule { kind: crate::sheet::CfKind::Expression("A1>10".into()), priority: 1, stop: true, style: None }],
        };
        let r2 = crate::sheet::ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 10, first_column: 0, last_column: 1 }],
            rules: vec![crate::sheet::CfRule { kind: crate::sheet::CfKind::Expression("A1>0".into()), priority: 2, stop: false, style: None }],
        };
        d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: r1 })]).unwrap();
        d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: r2 })]).unwrap();

        // Duplicate rule 0 (priority 1)
        d.execute(vec![Command::Sheet(SheetCommand::DuplicateConditionalRule { slide: s, index: 0, rule: 0 })]).unwrap();
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 3);
        assert_eq!(sheet.conditional[0].rules[0].priority, 1);
        assert_eq!(sheet.conditional[1].rules[0].priority, 3);
        assert_eq!(sheet.conditional[2].rules[0].priority, 2);
        assert_eq!(sheet.conditional[2].rules[0].kind, crate::sheet::CfKind::Expression("A1>10".into()));
        assert!(sheet.conditional[2].rules[0].stop);
        assert_eq!(sheet.conditional[2].ranges, sheet.conditional[0].ranges);

        // Undo duplication
        assert!(d.undo());
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 2);
        assert_eq!(sheet.conditional[0].rules[0].priority, 1);
        assert_eq!(sheet.conditional[1].rules[0].priority, 2);

        // Redo duplication
        assert!(d.redo());
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 3);

        // Delete duplicated rule (index 2, rule 0, priority 2)
        d.execute(vec![Command::Sheet(SheetCommand::DeleteConditionalRule { slide: s, index: 2, rule: 0 })]).unwrap();
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 2);
        assert_eq!(sheet.conditional[0].rules[0].priority, 1);
        assert_eq!(sheet.conditional[1].rules[0].priority, 2);

        // Undo deletion
        assert!(d.undo());
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 3);

        // Redo deletion
        assert!(d.redo());
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.conditional.len(), 2);
    }

    #[test]
    fn calculation_modes_manual_and_calculate_now() {
        let (mut d, s) = workbook();
        // A1 = 10, B1 = =A1*2
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(10.0)));
        set(&mut d, s, 0, 1, CellInput::Formula("A1*2".into()));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(20.0));

        // Switch to Manual
        d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: CalcMode::Manual })]).unwrap();
        assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, CalcMode::Manual);

        // Edit A1 = 50: B1 should NOT recalculate in Manual mode
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(50.0)));
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Number(50.0));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(20.0));

        // CalculateNow (F9): B1 should update to 100
        d.execute(vec![Command::Sheet(SheetCommand::CalculateNow)]).unwrap();
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));

        // Undo CalculateNow: B1 reverts to 20
        assert!(d.undo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(20.0));

        // Redo CalculateNow: B1 becomes 100 again
        assert!(d.redo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));

        // Edit A1 = 70: B1 remains 100
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(70.0)));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));

        // CalculateSheet (Shift+F9): B1 updates to 140
        d.execute(vec![Command::Sheet(SheetCommand::CalculateSheet { slide: s })]).unwrap();
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(140.0));

        // Switch back to Auto: recalculates to current state
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(30.0)));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(140.0)); // still 140 before switch
        d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: CalcMode::Auto })]).unwrap();
        assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, CalcMode::Auto);
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(60.0)); // recalculated!

        // Now in Auto mode, editing A1 = 5 immediately updates B1 = 10
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(5.0)));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(10.0));
    }

    #[test]
    fn calculation_modes_auto_no_table() {
        let (mut d, s) = workbook();
        // A1 = 10, B1 = =A1*2
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(10.0)));
        set(&mut d, s, 0, 1, CellInput::Formula("A1*2".into()));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(20.0));

        // Switch to AutoNoTable
        d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: CalcMode::AutoNoTable })]).unwrap();
        assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, CalcMode::AutoNoTable);

        // Regular formulas still auto-calculate
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(25.0)));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(50.0));

        // Undo & Redo of SetCalcMode
        assert!(d.undo()); // undo set(A1=25)
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(20.0));
        assert!(d.undo()); // undo SetCalcMode
        assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, CalcMode::Auto);
        assert!(d.redo()); // redo SetCalcMode
        assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, CalcMode::AutoNoTable);
    }

    #[test]
    fn defined_names_formulas_and_constants() {
        let (mut d, s) = workbook();
        // A1 = 100, A2 = 200, A3 = 300
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(100.0)));
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Number(200.0)));
        set(&mut d, s, 2, 0, CellInput::Value(CellValue::Number(300.0)));

        // Define constant name TaxRate = "=0.10" (leading '=' should be trimmed)
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TaxRate".into(), sheet: None, formula: Some("=0.10".into()) })]).unwrap();
        assert_eq!(d.presentation().workbook.as_deref().unwrap().names.iter().find(|n| n.name == "TaxRate").unwrap().formula, "0.10");

        // Cell B1 = =A1 * TaxRate -> 10.0
        set(&mut d, s, 0, 1, CellInput::Formula("A1 * TaxRate".into()));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(10.0));

        // Define formula name SubTotal = "=SUM(A1:A3)"
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "SubTotal".into(), sheet: None, formula: Some("=SUM(A1:A3)".into()) })]).unwrap();

        // Cell B2 = =SubTotal -> 600.0
        set(&mut d, s, 1, 1, CellInput::Formula("SubTotal".into()));
        assert_eq!(cell(&d, s, 1, 1).unwrap().value, CellValue::Number(600.0));

        // Define composite formula name TotalWithTax = "=SubTotal * (1 + TaxRate)"
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TotalWithTax".into(), sheet: None, formula: Some("=SubTotal * (1 + TaxRate)".into()) })]).unwrap();

        // Cell B3 = =TotalWithTax -> 660.0
        set(&mut d, s, 2, 1, CellInput::Formula("TotalWithTax".into()));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(660.0));

        // Define LAMBDA formula name DoubleVal = "=LAMBDA(x, x * 2)"
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "DoubleVal".into(), sheet: None, formula: Some("=LAMBDA(x, x * 2)".into()) })]).unwrap();
        set(&mut d, s, 3, 1, CellInput::Formula("DoubleVal(25)".into()));
        assert_eq!(cell(&d, s, 3, 1).unwrap().value, CellValue::Number(50.0));

        // Edit TaxRate: change from 0.10 to 0.05
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TaxRate".into(), sheet: None, formula: Some("0.05".into()) })]).unwrap();
        // B1: 100 * 0.05 = 5.0
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(5.0));
        // B3: 600 * (1 + 0.05) = 630.0
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(630.0));

        // Undo edit of TaxRate: restores to 0.10
        assert!(d.undo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(10.0));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(660.0));

        // Redo edit of TaxRate: restores to 0.05
        assert!(d.redo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(5.0));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(630.0));

        // Delete TaxRate: formulas referencing it now evaluate to #NAME?
        d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TaxRate".into(), sheet: None, formula: None })]).unwrap();
        assert!(matches!(cell(&d, s, 0, 1).unwrap().value, CellValue::Error(ibis_calc::CellError::Name)));
        assert!(matches!(cell(&d, s, 2, 1).unwrap().value, CellValue::Error(ibis_calc::CellError::Name)));
        // SubTotal (B2) and DoubleVal (B4) still valid
        assert_eq!(cell(&d, s, 1, 1).unwrap().value, CellValue::Number(600.0));
        assert_eq!(cell(&d, s, 3, 1).unwrap().value, CellValue::Number(50.0));

        // Undo deletion: TaxRate restored!
        assert!(d.undo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(5.0));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(630.0));
    }

    #[test]
    fn whatif_scenarios_and_summary_report() {
        let (mut d, s) = workbook();
        // B1: 100, B2: 0.20, B3: =B1*(1+B2) -> 120.0
        set(&mut d, s, 0, 1, CellInput::Value(CellValue::Number(100.0)));
        set(&mut d, s, 1, 1, CellInput::Value(CellValue::Number(0.20)));
        set(&mut d, s, 2, 1, CellInput::Formula("B1*(1+B2)".into()));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(120.0));

        let sc1 = crate::sheet::SheetScenario {
            name: "現状".into(),
            comment: Some("ベースライン".into()),
            user: Some("test".into()),
            locked: false,
            hidden: false,
            input_cells: vec![
                crate::sheet::ScenarioInputCell { r: "B1".into(), val: "100".into(), num_fmt_id: None, deleted: false },
                crate::sheet::ScenarioInputCell { r: "B2".into(), val: "0.20".into(), num_fmt_id: None, deleted: false },
            ],
        };
        let sc2 = crate::sheet::SheetScenario {
            name: "好景気".into(),
            comment: Some("高需要".into()),
            user: Some("test".into()),
            locked: false,
            hidden: false,
            input_cells: vec![
                crate::sheet::ScenarioInputCell { r: "B1".into(), val: "150".into(), num_fmt_id: None, deleted: false },
                crate::sheet::ScenarioInputCell { r: "B2".into(), val: "0.25".into(), num_fmt_id: None, deleted: false },
            ],
        };
        let sc3 = crate::sheet::SheetScenario {
            name: "不景気".into(),
            comment: Some("低需要".into()),
            user: Some("test".into()),
            locked: false,
            hidden: false,
            input_cells: vec![
                crate::sheet::ScenarioInputCell { r: "B1".into(), val: "80".into(), num_fmt_id: None, deleted: false },
                crate::sheet::ScenarioInputCell { r: "B2".into(), val: "0.15".into(), num_fmt_id: None, deleted: false },
            ],
        };

        // 1. Add scenarios
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s, scenario: sc1 })]).unwrap();
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s, scenario: sc2 })]).unwrap();
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s, scenario: sc3 })]).unwrap();

        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.scenarios.as_ref().unwrap().scenarios.len(), 3);

        // 2. Show scenario 1 (好景気): B1=150, B2=0.25 -> B3=187.5
        d.execute(vec![Command::Sheet(SheetCommand::ShowScenario { slide: s, index: 1 })]).unwrap();
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(150.0));
        assert_eq!(cell(&d, s, 1, 1).unwrap().value, CellValue::Number(0.25));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(187.5));

        // 3. Undo/Redo ShowScenario
        assert!(d.undo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));
        assert_eq!(cell(&d, s, 1, 1).unwrap().value, CellValue::Number(0.20));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(120.0));

        assert!(d.redo());
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(150.0));
        assert_eq!(cell(&d, s, 2, 1).unwrap().value, CellValue::Number(187.5));

        // 4. Edit scenario 2 (不景気 -> B1=70, B2=0.10)
        let mut sc3_edit = sheet_of(d.presentation(), s).unwrap().scenarios.as_ref().unwrap().scenarios[2].clone();
        sc3_edit.input_cells[0].val = "70".into();
        sc3_edit.input_cells[1].val = "0.10".into();
        d.execute(vec![Command::Sheet(SheetCommand::EditScenario { slide: s, index: 2, scenario: sc3_edit })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().scenarios.as_ref().unwrap().scenarios[2].input_cells[0].val, "70");

        // 5. Delete scenario 0 (現状)
        d.execute(vec![Command::Sheet(SheetCommand::DeleteScenario { slide: s, index: 0 })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().scenarios.as_ref().unwrap().scenarios.len(), 2);

        // 6. Create Scenario Summary report
        d.execute(vec![Command::Sheet(SheetCommand::CreateScenarioSummary { slide: s, result_cells: vec!["B3".into()] })]).unwrap();
        // New sheet added! Total sheets is now 2
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);
        let sum_slide = d.presentation().slides[0].id;
        let sum_sheet = sheet_of(d.presentation(), sum_slide).unwrap();
        // Cell B2 (row 1, col 1) has title
        assert_eq!(sum_sheet.cell(1, 1).map(|c| c.value.clone()), Some(CellValue::Text("シナリオの要約".into())));
        // Result row has evaluated results for scenarios:
        // 好景気 (col 3): 150 * 1.25 = 187.5
        // 不景気 (col 4): 70 * 1.10 = 77.0
        let res_row = sum_sheet.cells.iter().find(|c| c.column == 1 && c.text == "$B$3").map(|c| c.row).expect("B3 result row");
        assert_eq!(sum_sheet.cell(res_row, 3).map(|c| c.value.clone()), Some(CellValue::Number(187.5)));
        assert_eq!(sum_sheet.cell(res_row, 4).map(|c| c.value.clone()), Some(CellValue::Number(77.0)));

        // 7. Undo summary sheet creation
        assert!(d.undo());
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 1);

        // 8. Redo summary sheet creation
        assert!(d.redo());
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);
    }

    #[test]
    fn merge_scenarios_command_and_undo() {
        let (mut d, s1) = workbook();

        // Add scenario to sheet 1
        let sc1 = crate::sheet::SheetScenario {
            name: "ベース".into(),
            comment: None,
            user: None,
            locked: false,
            hidden: false,
            input_cells: vec![crate::sheet::ScenarioInputCell { r: "A1".into(), val: "10".into(), num_fmt_id: None, deleted: false }],
        };
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s1, scenario: sc1 })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s1).unwrap().scenarios.as_ref().unwrap().scenarios.len(), 1);

        // Add sheet 2 with 2 scenarios
        let s2 = d.ids().new_slide_id();
        let sh2 = d.ids().new_shape_id();
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Sheet2".into(), slide: s2, shape: sh2 })]).unwrap();
        let sc2 = crate::sheet::SheetScenario {
            name: "高成長".into(),
            comment: None,
            user: None,
            locked: false,
            hidden: false,
            input_cells: vec![crate::sheet::ScenarioInputCell { r: "A1".into(), val: "20".into(), num_fmt_id: None, deleted: false }],
        };
        let sc3 = crate::sheet::SheetScenario {
            name: "低成長".into(),
            comment: None,
            user: None,
            locked: false,
            hidden: false,
            input_cells: vec![crate::sheet::ScenarioInputCell { r: "A1".into(), val: "5".into(), num_fmt_id: None, deleted: false }],
        };
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s2, scenario: sc2 })]).unwrap();
        d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s2, scenario: sc3 })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s2).unwrap().scenarios.as_ref().unwrap().scenarios.len(), 2);

        // Merge scenarios from sheet 2 into sheet 1
        d.execute(vec![Command::Sheet(SheetCommand::MergeScenarios { slide: s1, from_slide: s2 })]).unwrap();
        let sc_list = &sheet_of(d.presentation(), s1).unwrap().scenarios.as_ref().unwrap().scenarios;
        assert_eq!(sc_list.len(), 3);
        assert_eq!(sc_list[0].name, "ベース");
        assert_eq!(sc_list[1].name, "高成長");
        assert_eq!(sc_list[2].name, "低成長");

        // Undo merge
        assert!(d.undo());
        assert_eq!(sheet_of(d.presentation(), s1).unwrap().scenarios.as_ref().unwrap().scenarios.len(), 1);

        // Redo merge
        assert!(d.redo());
        assert_eq!(sheet_of(d.presentation(), s1).unwrap().scenarios.as_ref().unwrap().scenarios.len(), 3);
    }

    #[test]
    fn sparkline_commands_and_clear() {
        use crate::sheet::{SparklineGroup, SparklineKind};
        let (mut d, s) = workbook();

        // 1. make_sparklines mapping
        let data = CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 2 }; // A1:C3
        let location = CellRange { first_row: 0, last_row: 2, first_column: 3, last_column: 3 }; // D1:D3
        let sparklines = make_sparklines(None, data, location);
        assert_eq!(sparklines.len(), 3);
        assert_eq!((sparklines[0].row, sparklines[0].column), (0, 3));
        assert_eq!((sparklines[0].data.first_row, sparklines[0].data.last_row), (0, 0));
        assert_eq!((sparklines[2].row, sparklines[2].column), (2, 3));
        assert_eq!((sparklines[2].data.first_row, sparklines[2].data.last_row), (2, 2));

        // 2. AddSparklineGroup
        let group = SparklineGroup {
            kind: SparklineKind::Line,
            markers: true,
            high: true,
            low: true,
            sparklines: sparklines.clone(),
            ..Default::default()
        };
        d.execute(vec![Command::Sheet(SheetCommand::AddSparklineGroup { slide: s, group })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 1);
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines[0].sparklines.len(), 3);

        // 3. Undo/Redo AddSparklineGroup
        assert!(d.undo());
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 0);
        assert!(d.redo());
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 1);

        // 4. EditSparklineGroup: change kind to Column
        let mut edited = sheet_of(d.presentation(), s).unwrap().sparklines[0].clone();
        edited.kind = SparklineKind::Column;
        d.execute(vec![Command::Sheet(SheetCommand::EditSparklineGroup { slide: s, index: 0, group: edited })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines[0].kind, SparklineKind::Column);

        // 5. Clear specific sparkline (D1 only, row 0, col 3)
        let clear_range = CellRange { first_row: 0, last_row: 0, first_column: 3, last_column: 3 };
        d.execute(vec![Command::Sheet(SheetCommand::ClearSparklines { slide: s, ranges: vec![clear_range], whole_group: false })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines[0].sparklines.len(), 2);
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines[0].sparklines[0].row, 1);

        // Undo clear
        assert!(d.undo());
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines[0].sparklines.len(), 3);

        // 6. Clear whole group
        d.execute(vec![Command::Sheet(SheetCommand::ClearSparklines { slide: s, ranges: vec![clear_range], whole_group: true })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 0);

        // Undo whole group clear
        assert!(d.undo());
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 1);

        // 7. DeleteSparklineGroup
        d.execute(vec![Command::Sheet(SheetCommand::DeleteSparklineGroup { slide: s, index: 0 })]).unwrap();
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 0);
        assert!(d.undo());
        assert_eq!(sheet_of(d.presentation(), s).unwrap().sparklines.len(), 1);
    }

    #[test]
    fn subtotals_insert_calculate_remove_undo_redo() {
        let (mut d, s) = workbook();
        // Setup table:
        // Row 0: Dept, Item, Amount
        // Row 1: Sales, A, 10
        // Row 2: Sales, B, 20
        // Row 3: Support, C, 30
        // Row 4: Support, D, 40
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Text("Dept".into())));
        set(&mut d, s, 0, 1, CellInput::Value(CellValue::Text("Item".into())));
        set(&mut d, s, 0, 2, CellInput::Value(CellValue::Text("Amount".into())));

        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Text("Sales".into())));
        set(&mut d, s, 1, 1, CellInput::Value(CellValue::Text("A".into())));
        set(&mut d, s, 1, 2, CellInput::Value(CellValue::Number(10.0)));

        set(&mut d, s, 2, 0, CellInput::Value(CellValue::Text("Sales".into())));
        set(&mut d, s, 2, 1, CellInput::Value(CellValue::Text("B".into())));
        set(&mut d, s, 2, 2, CellInput::Value(CellValue::Number(20.0)));

        set(&mut d, s, 3, 0, CellInput::Value(CellValue::Text("Support".into())));
        set(&mut d, s, 3, 1, CellInput::Value(CellValue::Text("C".into())));
        set(&mut d, s, 3, 2, CellInput::Value(CellValue::Number(30.0)));

        set(&mut d, s, 4, 0, CellInput::Value(CellValue::Text("Support".into())));
        set(&mut d, s, 4, 1, CellInput::Value(CellValue::Text("D".into())));
        set(&mut d, s, 4, 2, CellInput::Value(CellValue::Number(40.0)));

        let range = CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 2 };
        let params = SubtotalParams {
            range,
            group_by_column: 0,
            function_code: 9, // SUM
            subtotal_columns: vec![2],
            replace_existing: true,
            summary_below: true,
            page_breaks: false,
        };

        // 1. Apply subtotal
        d.execute(vec![Command::Sheet(SheetCommand::ApplySubtotal { slide: s, params: params.clone() })]).unwrap();

        // Verify rows:
        // Row 0: Dept, Item, Amount
        // Row 1: Sales, A, 10
        // Row 2: Sales, B, 20
        // Row 3: "Sales 集計", empty, SUBTOTAL(9, C2:C3) -> 30
        // Row 4: Support, C, 30
        // Row 5: Support, D, 40
        // Row 6: "Support 集計", empty, SUBTOTAL(9, C5:C6) -> 70
        // Row 7: "総計", empty, SUBTOTAL(9, C2:C7) -> 100
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Text("Sales 集計".into()));
        assert_eq!(cell(&d, s, 3, 2).unwrap().formula.as_deref(), Some("SUBTOTAL(9,C2:C3)"));
        assert_eq!(cell(&d, s, 3, 2).unwrap().value, CellValue::Number(30.0));

        assert_eq!(cell(&d, s, 6, 0).unwrap().value, CellValue::Text("Support 集計".into()));
        assert_eq!(cell(&d, s, 6, 2).unwrap().formula.as_deref(), Some("SUBTOTAL(9,C5:C6)"));
        assert_eq!(cell(&d, s, 6, 2).unwrap().value, CellValue::Number(70.0));

        assert_eq!(cell(&d, s, 7, 0).unwrap().value, CellValue::Text("総計".into()));
        assert_eq!(cell(&d, s, 7, 2).unwrap().formula.as_deref(), Some("SUBTOTAL(9,C2:C7)"));
        assert_eq!(cell(&d, s, 7, 2).unwrap().value, CellValue::Number(100.0));

        // Verify outline levels
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.outline_rows.get(&1).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&2).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&3).copied(), Some(1));
        assert_eq!(sheet.outline_rows.get(&4).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&5).copied(), Some(2));
        assert_eq!(sheet.outline_rows.get(&6).copied(), Some(1));
        assert_eq!(sheet.outline_rows.get(&7).copied(), None);

        // 2. Undo
        assert!(d.undo());
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Text("Support".into()));
        assert_eq!(cell(&d, s, 3, 2).unwrap().value, CellValue::Number(30.0));
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert!(sheet.outline_rows.is_empty());

        // 3. Redo
        assert!(d.redo());
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Text("Sales 集計".into()));
        assert_eq!(cell(&d, s, 7, 2).unwrap().value, CellValue::Number(100.0));
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.outline_rows.get(&3).copied(), Some(1));

        // 4. Remove Subtotal
        let full_range = CellRange { first_row: 0, last_row: 7, first_column: 0, last_column: 2 };
        d.execute(vec![Command::Sheet(SheetCommand::RemoveSubtotal { slide: s, range: full_range })]).unwrap();
        assert_eq!(cell(&d, s, 3, 0).unwrap().value, CellValue::Text("Support".into()));
        assert_eq!(cell(&d, s, 4, 0).unwrap().value, CellValue::Text("Support".into()));
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert!(sheet.outline_rows.is_empty());

        // 5. Replace Existing with COUNT (code 2)
        let mut count_params = params.clone();
        count_params.function_code = 2; // COUNT
        d.execute(vec![Command::Sheet(SheetCommand::ApplySubtotal { slide: s, params: count_params })]).unwrap();
        assert_eq!(cell(&d, s, 3, 2).unwrap().value, CellValue::Number(2.0));
        assert_eq!(cell(&d, s, 6, 2).unwrap().value, CellValue::Number(2.0));
        assert_eq!(cell(&d, s, 7, 2).unwrap().value, CellValue::Number(4.0));
    }

    #[test]
    fn workbook_protection_blocks_structural_edits_and_undo() {
        use crate::sheet::WorkbookProtection;

        let (mut d, s) = workbook();

        // 1. Unprotected: adding a sheet works
        let (s2, sh2) = (d.ids().new_slide_id(), d.ids().new_shape_id());
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {
            index: 1,
            name: "Sheet2".into(),
            slide: s2,
            shape: sh2,
        })]).unwrap();
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);

        // 2. Protect workbook structure
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(WorkbookProtection {
                lock_structure: true,
                lock_windows: false,
                lock_revision: false,
                hash: None,
                legacy: None,
            }),
        })]).unwrap();
        assert!(d.presentation().workbook.as_ref().unwrap().protection.as_ref().unwrap().lock_structure);

        // 3. Structural operations are blocked
        let (s3, sh3) = (d.ids().new_slide_id(), d.ids().new_shape_id());
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::AddSheet {
                index: 2,
                name: "Sheet3".into(),
                slide: s3,
                shape: sh3,
            })]),
            Err(EditError::WorkbookProtected)
        );

        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: s2 })]),
            Err(EditError::WorkbookProtected)
        );

        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::RenameSheet {
                slide: s,
                name: "NewName".into(),
            })]),
            Err(EditError::WorkbookProtected)
        );

        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::MoveSheet { slide: s, to: 1 })]),
            Err(EditError::WorkbookProtected)
        );

        let (s4, sh4) = (d.ids().new_slide_id(), d.ids().new_shape_id());
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::CopySheet {
                slide: s,
                name: "CopyOfSheet1".into(),
                new_slide: s4,
                shape: sh4,
            })]),
            Err(EditError::WorkbookProtected)
        );

        let (_s5, sh5) = (d.ids().new_slide_id(), d.ids().new_shape_id());
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::HideSheet {
                slide: s2,
                sheet: 1,
                hidden: true,
                shape: sh5,
            })]),
            Err(EditError::WorkbookProtected)
        );

        // 4. Undo unprotects and allows adding sheet
        assert!(d.undo());
        assert!(d.presentation().workbook.as_ref().unwrap().protection.is_none());
        let (s6, sh6) = (d.ids().new_slide_id(), d.ids().new_shape_id());
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {
            index: 2,
            name: "Sheet3".into(),
            slide: s6,
            shape: sh6,
        })]).unwrap();
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 3);

        // 5. Redo protects again
        assert!(d.undo()); // undo add sheet3
        assert!(d.redo()); // redo add sheet3
        // Protect again
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(WorkbookProtection {
                lock_structure: true,
                lock_windows: true,
                lock_revision: false,
                hash: None,
                legacy: None,
            }),
        })]).unwrap();
        assert!(d.presentation().workbook.as_ref().unwrap().protection.as_ref().unwrap().lock_windows);

        // 6. Explicitly unprotect via SetWorkbookProtection { protection: None }
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: None,
        })]).unwrap();
        assert!(d.presentation().workbook.as_ref().unwrap().protection.is_none());
    }

    #[test]
    fn move_cells_moves_validation_ranges() {
        use crate::sheet::Validation;

        let (mut d, s) = workbook();
        let range = CellRange { first_row: 1, first_column: 1, last_row: 2, last_column: 2 };
        let rule = Validation {
            ranges: vec![range],
            kind: "whole".into(),
            operator: "between".into(),
            formula1: Some("1".into()),
            formula2: Some("10".into()),
            allow_blank: true,
            dropdown: false,
            prompt: None,
            error: None,
        };
        d.execute(vec![Command::Sheet(SheetCommand::SetValidation {
            slide: s,
            ranges: vec![range],
            rule: Some(rule),
        })]).unwrap();

        // Move cells from B2:C3 (1,1..2,2) to E5:F6 (4,4..5,5)
        d.execute(vec![Command::Sheet(SheetCommand::MoveCells {
            slide: s,
            from: range,
            row: 4,
            column: 4,
        })]).unwrap();

        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.validations.len(), 1);
        let expected = CellRange { first_row: 4, first_column: 4, last_row: 5, last_column: 5 };
        assert_eq!(sheet.validations[0].ranges, vec![expected]);

        // Undo restores original validation range
        assert!(d.undo());
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.validations[0].ranges, vec![range]);
    }

    #[test]
    fn protected_ranges_command_and_undo() {
        use crate::sheet::ProtectedRange;

        let (mut d, s) = workbook();
        let pr = ProtectedRange {
            name: "AllowedRange1".into(),
            ranges: vec![CellRange { first_row: 0, first_column: 1, last_row: 5, last_column: 2 }],
            hash: None,
            legacy: None,
        };
        d.execute(vec![Command::Sheet(SheetCommand::SetProtectedRanges {
            slide: s,
            ranges: vec![pr.clone()],
        })]).unwrap();

        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.protected_ranges.len(), 1);
        assert_eq!(sheet.protected_ranges[0].name, "AllowedRange1");

        // Undo
        assert!(d.undo());
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert!(sheet.protected_ranges.is_empty());

        // Redo
        assert!(d.redo());
        let sheet = sheet_of(d.presentation(), s).unwrap();
        assert_eq!(sheet.protected_ranges.len(), 1);
        assert_eq!(sheet.protected_ranges[0].name, "AllowedRange1");
    }

    #[test]
    fn full_precision_command_and_rounding() {
        let (mut d, s) = workbook();

        // Set A1 = 10.456 with format 0.00
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Number(10.456)));
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
            slide: s,
            ranges: vec![CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }],
            change: StyleChange::NumberFormat("0.00".into()),
        })]).unwrap();

        // B1 = =A1*10
        set(&mut d, s, 0, 1, CellInput::Formula("A1*10".into()));
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Number(10.456));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(104.56));
        assert!(d.presentation().workbook.as_deref().unwrap().full_precision);

        // Turn on precision as displayed (full_precision: false)
        d.execute(vec![Command::Sheet(SheetCommand::SetFullPrecision { full_precision: false })]).unwrap();
        assert!(!d.presentation().workbook.as_deref().unwrap().full_precision);
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Number(10.46));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(10.46 * 10.0));

        // Undo
        assert!(d.undo());
        assert!(d.presentation().workbook.as_deref().unwrap().full_precision);
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Number(10.456));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(104.56));

        // Redo
        assert!(d.redo());
        assert!(!d.presentation().workbook.as_deref().unwrap().full_precision);
        assert_eq!(cell(&d, s, 0, 0).unwrap().value, CellValue::Number(10.46));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(10.46 * 10.0));

        // Enter new cell C1 while precision as displayed is active
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
            slide: s,
            ranges: vec![CellRange { first_row: 0, first_column: 2, last_row: 0, last_column: 2 }],
            change: StyleChange::NumberFormat("0.0".into()),
        })]).unwrap();
        set(&mut d, s, 0, 2, CellInput::Value(CellValue::Number(5.67)));
        assert_eq!(cell(&d, s, 0, 2).unwrap().value, CellValue::Number(5.7));
    }

    #[test]
    fn iterative_calculation_command_and_undo() {
        let (mut d, s) = workbook();

        // B1 = =B1+1 (circular reference)
        set(&mut d, s, 0, 1, CellInput::Formula("B1+1".into()));
        // With iterative calculation off (default), Excel and ibisOffice show 0.0
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(0.0));
        assert!(!d.presentation().workbook.as_deref().unwrap().iterate);

        // Turn on iterative calculation (100 iterations)
        d.execute(vec![Command::Sheet(SheetCommand::SetIterativeCalculation {
            iterate: true,
            max_iterations: 100,
            max_change: 0.001.into(),
        })]).unwrap();
        assert!(d.presentation().workbook.as_deref().unwrap().iterate);
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));

        // Undo
        assert!(d.undo());
        assert!(!d.presentation().workbook.as_deref().unwrap().iterate);
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(0.0));

        // Redo
        assert!(d.redo());
        assert!(d.presentation().workbook.as_deref().unwrap().iterate);
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Number(100.0));
    }

    #[test]
    fn show_pivot_details_command_and_undo() {
        use crate::pivot::{PivotDataField, PivotField, PivotSource, PivotTable};

        let (mut d, s) = workbook();

        // 1. Setup source data in A1:C4 (rows 0..=3, cols 0..=2)
        // A1: "品名", B1: "地域", C1: "売上"
        set(&mut d, s, 0, 0, CellInput::Value(CellValue::Text("品名".into())));
        set(&mut d, s, 0, 1, CellInput::Value(CellValue::Text("地域".into())));
        set(&mut d, s, 0, 2, CellInput::Value(CellValue::Text("売上".into())));
        // Row 1: りんご, 東, 100
        set(&mut d, s, 1, 0, CellInput::Value(CellValue::Text("りんご".into())));
        set(&mut d, s, 1, 1, CellInput::Value(CellValue::Text("東".into())));
        set(&mut d, s, 1, 2, CellInput::Value(CellValue::Number(100.0)));
        // Row 2: みかん, 西, 150
        set(&mut d, s, 2, 0, CellInput::Value(CellValue::Text("みかん".into())));
        set(&mut d, s, 2, 1, CellInput::Value(CellValue::Text("西".into())));
        set(&mut d, s, 2, 2, CellInput::Value(CellValue::Number(150.0)));
        // Row 3: りんご, 西, 200
        set(&mut d, s, 3, 0, CellInput::Value(CellValue::Text("りんご".into())));
        set(&mut d, s, 3, 1, CellInput::Value(CellValue::Text("西".into())));
        set(&mut d, s, 3, 2, CellInput::Value(CellValue::Number(200.0)));

        // 2. Build pivot table in A6:B9 (rows 5..=8, cols 0..=1)
        let pivot = PivotTable {
            name: "ピボット1".into(),
            part: None,
            cache_part: None,
            location: CellRange { first_row: 5, first_column: 0, last_row: 8, last_column: 1 },
            first_header_row: 1,
            first_data_row: 1,
            first_data_col: 1,
            source: PivotSource {
                sheet: None,
                range: Some(CellRange { first_row: 0, first_column: 0, last_row: 3, last_column: 2 }),
                name: None,
            },
            fields: vec![
                PivotField { name: "品名".into(), ..Default::default() },
                PivotField { name: "地域".into(), ..Default::default() },
                PivotField { name: "売上".into(), ..Default::default() },
            ],
            row_fields: vec![0],
            col_fields: vec![],
            page_fields: vec![],
            data_fields: vec![PivotDataField {
                name: "合計 / 売上".into(),
                field: 2,
                function: "sum".into(),
                show_as: None,
                number_format: None,
            }],
            row_items: vec![],
            col_items: vec![],
            row_grand_totals: true,
            col_grand_totals: true,
            compact: true,
            outline: true,
            row_header_caption: None,
            col_header_caption: None,
            grand_total_caption: None,
            data_caption: None,
            missing_caption: None,
            missing_zero: false,
            error_caption: None,
            fill_down_labels: false,
            filters: vec![],
            style: None,
            data_on_rows: false,
            refreshed: true,
            refresh_on_load: false,
            buttons: vec![],
            corner: None,
            merge_item: false,
            auto_fit_columns: true,
            preserve_formatting: true,
            multiple_field_filters: false,
            calculated_items: vec![],
            show_drill: true,
        };

        let sheet_data = crate::pivot::source_data(sheet_of(d.presentation(), s).unwrap(), pivot.source.range.unwrap());
        let built = pivot.build(&sheet_data);
        let default_st = default_style(d.presentation());
        let mut new_sheet = sheet_of(d.presentation(), s).unwrap().clone();
        new_sheet.pivots.push(pivot.clone());
        new_sheet.put_pivot(0, &built, &default_st, false);
        d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: s, sheet: Box::new(new_sheet) })]).unwrap();

        // Built table layout:
        // Row 5: 行ラベル | 合計 / 売上
        // Row 6: みかん | 150
        // Row 7: りんご | 300
        // Row 8: 総計   | 450
        assert_eq!(cell(&d, s, 6, 1).unwrap().value, CellValue::Number(150.0));
        assert_eq!(cell(&d, s, 7, 1).unwrap().value, CellValue::Number(300.0));
        assert_eq!(cell(&d, s, 8, 1).unwrap().value, CellValue::Number(450.0));

        // 3. Drill-down on "りんご" value cell (Row 7, Col 1)
        d.execute(vec![Command::Sheet(SheetCommand::ShowPivotDetails {
            slide: s,
            column: 1,
            row: 7,
        })]).unwrap();

        // 4. Verify drill-down sheet "展開1" created
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets[0].name, "展開1");
        let detail_slide = d.presentation().slides[0].id;
        let detail_sheet = sheet_of(d.presentation(), detail_slide).unwrap();

        // Check headings at Row 0
        assert_eq!(detail_sheet.cell(0, 0).map(|c| c.value.clone()), Some(CellValue::Text("品名".into())));
        assert_eq!(detail_sheet.cell(0, 1).map(|c| c.value.clone()), Some(CellValue::Text("地域".into())));
        assert_eq!(detail_sheet.cell(0, 2).map(|c| c.value.clone()), Some(CellValue::Text("売上".into())));

        // Check 2 "りんご" records at Row 1 and Row 2
        // りんご, 東, 100
        assert_eq!(detail_sheet.cell(1, 0).map(|c| c.value.clone()), Some(CellValue::Text("りんご".into())));
        assert_eq!(detail_sheet.cell(1, 1).map(|c| c.value.clone()), Some(CellValue::Text("東".into())));
        assert_eq!(detail_sheet.cell(1, 2).map(|c| c.value.clone()), Some(CellValue::Number(100.0)));
        // りんご, 西, 200
        assert_eq!(detail_sheet.cell(2, 0).map(|c| c.value.clone()), Some(CellValue::Text("りんご".into())));
        assert_eq!(detail_sheet.cell(2, 1).map(|c| c.value.clone()), Some(CellValue::Text("西".into())));
        assert_eq!(detail_sheet.cell(2, 2).map(|c| c.value.clone()), Some(CellValue::Number(200.0)));
        // Row 3 should be empty
        assert!(detail_sheet.cell(3, 0).is_none());

        // 5. Test Undo removes detail sheet
        assert!(d.undo());
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 1);

        // 6. Test Redo restores detail sheet
        assert!(d.redo());
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);
        let detail_sheet_restored = sheet_of(d.presentation(), d.presentation().slides[0].id).unwrap();
        assert_eq!(detail_sheet_restored.cell(1, 0).map(|c| c.value.clone()), Some(CellValue::Text("りんご".into())));
        assert_eq!(detail_sheet_restored.cell(2, 2).map(|c| c.value.clone()), Some(CellValue::Number(200.0)));
    }

    #[test]
    fn custom_views_add_show_delete_undo_redo_and_table_restriction() {
        use crate::sheet::{CustomSheetView, CustomWorkbookView, SheetTable};

        let mut p = Presentation::empty(crate::geometry::EmuSize::WIDESCREEN);
        let s = p.new_slide_id();
        let mut slide = Slide::new(s);
        slide.name = Some("Sheet1".into());
        let shape = p.new_shape_id();
        let mut sheet = Sheet {
            columns: vec![80 * 36_000; 10],
            rows: vec![100 * 36_000; 10],
            default_row: 100 * 36_000,
            default_column: 80 * 36_000,
            zoom: Some(120),
            grid_lines: true,
            headings: true,
            ..Sheet::default()
        };
        sheet.print.landscape = true;
        sheet.rows[2] = 0;
        sheet.hidden_rows.insert(2, 100 * 36_000);
        sheet.columns[1] = 0;
        sheet.hidden_columns.insert(1, 80 * 36_000);

        let sheet_snap = CustomSheetView {
            guid: "{11111111-2222-3333-4444-555555555555}".into(),
            name: "LandscapeView".into(),
            zoom: sheet.zoom,
            grid_lines: sheet.grid_lines,
            headings: sheet.headings,
            show_ruler: sheet.show_ruler,
            page_break_preview: sheet.page_break_preview,
            frozen: sheet.frozen,
            split: sheet.split,
            active_cell: Some((0, 0)),
            selection_range: None,
            print: Some(sheet.print.clone()),
            hidden_rows: Some(sheet.hidden_rows.clone()),
            hidden_columns: Some(sheet.hidden_columns.clone()),
            auto_filter: Some(sheet.auto_filter.clone()),
            filtered_rows: Some(sheet.filtered_rows.clone()),
            show_formulas: sheet.show_formulas,
            show_zeros: sheet.show_zeros,
        };

        let wb_view = CustomWorkbookView {
            name: "LandscapeView".into(),
            guid: "{11111111-2222-3333-4444-555555555555}".into(),
            include_print_settings: true,
            include_hidden: true,
            active_sheet_index: 0,
        };

        slide.shapes.push(Shape {
            id: shape,
            frame: EmuRect::new(0, 0, 1000, 2000),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Sheet(Arc::new(sheet)),
            floating: None,
        });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![crate::sheet::WorkbookSheet {
                name: "Sheet1".into(),
                state: Default::default(),
                slide: Some(s),
                hidden: None,
                other: false,
            }],
            ..Default::default()
        }));

        let mut d = Document::new(p);

        // 2. Add custom view
        d.execute(vec![Command::Sheet(SheetCommand::AddCustomView {
            view: wb_view,
            sheet_views: vec![(s, sheet_snap)],
        })]).unwrap();

        assert_eq!(d.presentation().workbook.as_ref().unwrap().custom_views.len(), 1);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().custom_views[0].name, "LandscapeView");
        assert_eq!(sheet_of(d.presentation(), s).unwrap().custom_views.len(), 1);

        // 3. Mutate the sheet state via commands!
        d.execute(vec![
            Command::Sheet(SheetCommand::SetZoom { slide: s, zoom: Some(80) }),
            Command::Sheet(SheetCommand::SetSize { slide: s, axis: Axis::Rows, first: 2, last: 2, size: 100 * 36_000 }),
            Command::Sheet(SheetCommand::SetSize { slide: s, axis: Axis::Columns, first: 1, last: 1, size: 80 * 36_000 }),
        ]).unwrap();

        // 4. Show (apply) custom view
        d.execute(vec![Command::Sheet(SheetCommand::ShowCustomView {
            name: "LandscapeView".into(),
        })]).unwrap();

        {
            let sheet = sheet_of(d.presentation(), s).unwrap();
            assert_eq!(sheet.zoom, Some(120));
            assert!(sheet.print.landscape);
            assert_eq!(sheet.rows[2], 0);
            assert!(sheet.hidden_rows.contains_key(&2));
            assert_eq!(sheet.columns[1], 0);
            assert!(sheet.hidden_columns.contains_key(&1));
        }

        // 5. Test Undo
        assert!(d.undo());
        {
            let sheet = sheet_of(d.presentation(), s).unwrap();
            assert_eq!(sheet.zoom, Some(80));
            assert_ne!(sheet.rows[2], 0);
        }

        // 6. Test Redo
        assert!(d.redo());
        {
            let sheet = sheet_of(d.presentation(), s).unwrap();
            assert_eq!(sheet.zoom, Some(120));
            assert!(sheet.print.landscape);
            assert_eq!(sheet.rows[2], 0);
        }

        // 7. Test Table Exclusion (XLSX-VIEW-018)
        let mut p_with_table = d.presentation().clone();
        sheet_mut(&mut p_with_table, s).unwrap().tables.push(SheetTable {
            name: "Table1".into(),
            range: CellRange { first_row: 0, first_column: 0, last_row: 5, last_column: 5 },
            header_rows: 1,
            totals_rows: 0,
            columns: vec!["Col1".into()],
            style: None,
            filter: None,
            totals: Vec::new(),
        });
        let mut d_table = Document::new(p_with_table);

        // Attempting to show custom view with table must fail with CustomViewsDisabledByTable
        let show_res = d_table.execute(vec![Command::Sheet(SheetCommand::ShowCustomView {
            name: "LandscapeView".into(),
        })]);
        assert_eq!(show_res, Err(EditError::CustomViewsDisabledByTable));

        // Attempting to add custom view with table must fail
        let add_res = d_table.execute(vec![Command::Sheet(SheetCommand::AddCustomView {
            view: CustomWorkbookView { name: "Another".into(), ..Default::default() },
            sheet_views: Vec::new(),
        })]);
        assert_eq!(add_res, Err(EditError::CustomViewsDisabledByTable));

        // 8. Delete Custom View on `d` (which has no table)
        d.execute(vec![Command::Sheet(SheetCommand::DeleteCustomView {
            name: "LandscapeView".into(),
        })]).unwrap();
        assert_eq!(d.presentation().workbook.as_ref().unwrap().custom_views.len(), 0);
        assert_eq!(sheet_of(d.presentation(), s).unwrap().custom_views.len(), 0);

        // Undo delete restores view
        assert!(d.undo());
        assert_eq!(d.presentation().workbook.as_ref().unwrap().custom_views.len(), 1);
    }

    #[test]
    fn window_hide_unhide_undo_redo_and_protection() {
        use crate::sheet::WorkbookProtection;

        let (mut d, _s) = workbook();
        assert!(!d.presentation().is_workbook_window_hidden());
        assert!(!d.presentation().is_workbook_windows_locked());

        // 1. Hide workbook window
        d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: true })]).unwrap();
        assert!(d.presentation().is_workbook_window_hidden());

        // 2. Undo -> restored to visible
        assert!(d.undo());
        assert!(!d.presentation().is_workbook_window_hidden());

        // 3. Redo -> hidden again
        assert!(d.redo());
        assert!(d.presentation().is_workbook_window_hidden());

        // 4. Unhide
        d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: false })]).unwrap();
        assert!(!d.presentation().is_workbook_window_hidden());

        // 5. Undo unhide -> hidden again
        assert!(d.undo());
        assert!(d.presentation().is_workbook_window_hidden());

        // 6. Redo unhide -> visible
        assert!(d.redo());
        assert!(!d.presentation().is_workbook_window_hidden());

        // 7. Protect workbook with lock_windows = true
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(WorkbookProtection {
                lock_structure: false,
                lock_windows: true,
                lock_revision: false,
                hash: None,
                legacy: None,
            }),
        })]).unwrap();
        assert!(d.presentation().is_workbook_windows_locked());

        // 8. Attempting to hide or unhide while windows are locked must fail
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: true })]),
            Err(EditError::WorkbookProtected)
        );
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: false })]),
            Err(EditError::WorkbookProtected)
        );

        // 9. Remove workbook protection
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: None,
        })]).unwrap();
        assert!(!d.presentation().is_workbook_windows_locked());

        // 10. Now hiding succeeds again
        d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: true })]).unwrap();
        assert!(d.presentation().is_workbook_window_hidden());
    }

    #[test]
    fn chart_move_to_existing_sheet_and_new_sheet_with_undo_redo_and_protection() {
        use crate::chart::Chart;
        use crate::sheet::WorkbookProtection;

        // 1. Create a workbook with 1 sheet
        let (mut d, s1) = workbook();
        let c_id = d.ids().new_shape_id();
        let chart = Chart {
            groups: Vec::new(),
            axes: Vec::new(),
            legend: None,
            title: None,
            plot_area: None,
            background: crate::drawing::Fill::None,
            border: None,
            plot_background: crate::drawing::Fill::None,
            plot_border: None,
            disp_blanks_as: None,
            plot_vis_only: true,
            data_table: None,
            style: None,
            pivot_source: None,
            rounded_corners: false,
            title_overlay: false,
            ex: None,
            ex_format: None,
            excel_layout: false,
            word_layout: false,
            cell_refs: Vec::new(),
            ex_source: None,
        };
        let chart_shape = Shape {
            id: c_id,
            frame: EmuRect::new(100, 100, 500, 400),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Chart(Box::new(chart)),
            floating: None,
        };
        // Add chart to s1
        d.execute(vec![Command::InsertShape { slide: s1, index: 1, shape: chart_shape }]).unwrap();

        // Add second sheet s2
        let s2 = d.ids().new_slide_id();
        let sh2 = d.ids().new_shape_id();
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {
            index: 1,
            name: "Sheet2".to_string(),
            slide: s2,
            shape: sh2,
        })]).unwrap();

        assert_eq!(d.presentation().slide(s1).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 1);

        // 2. Move chart to existing sheet s2
        d.execute(vec![Command::Sheet(SheetCommand::MoveChart {
            from_slide: s1,
            shape: c_id,
            target: ChartMoveTarget::ExistingSheet { slide: s2 },
        })]).unwrap();

        assert_eq!(d.presentation().slide(s1).unwrap().shapes.len(), 1);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes[1].id, c_id);

        // 3. Undo move to existing sheet
        assert!(d.undo());
        assert_eq!(d.presentation().slide(s1).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 1);

        // 4. Redo move to existing sheet
        assert!(d.redo());
        assert_eq!(d.presentation().slide(s1).unwrap().shapes.len(), 1);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 2);

        // 5. Move chart from s2 to NewSheet ("ChartSheet")
        let s3 = d.ids().new_slide_id();
        let sh3 = d.ids().new_shape_id();
        d.execute(vec![Command::Sheet(SheetCommand::MoveChart {
            from_slide: s2,
            shape: c_id,
            target: ChartMoveTarget::NewSheet {
                name: "ChartSheet".to_string(),
                slide: s3,
                sheet_shape: sh3,
            },
        })]).unwrap();

        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 1);
        assert_eq!(d.presentation().slide(s3).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().slide(s3).unwrap().shapes[1].id, c_id);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 3);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets[2].name, "ChartSheet");

        // 6. Undo move to new sheet -> s3 deleted, chart returns to s2
        assert!(d.undo());
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().slide(s2).unwrap().shapes[1].id, c_id);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 2);

        // 7. Redo move to new sheet
        assert!(d.redo());
        assert_eq!(d.presentation().slide(s2).unwrap().shapes.len(), 1);
        assert_eq!(d.presentation().slide(s3).unwrap().shapes.len(), 2);
        assert_eq!(d.presentation().workbook.as_ref().unwrap().sheets.len(), 3);

        // 8. Protection test: if workbook structure is locked, moving to new sheet must fail
        d.undo(); // back to s2
        d.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(WorkbookProtection {
                lock_structure: true,
                lock_windows: false,
                lock_revision: false,
                hash: None,
                legacy: None,
            }),
        })]).unwrap();

        let s4 = d.ids().new_slide_id();
        let sh4 = d.ids().new_shape_id();
        assert_eq!(
            d.execute(vec![Command::Sheet(SheetCommand::MoveChart {
                from_slide: s2,
                shape: c_id,
                target: ChartMoveTarget::NewSheet {
                    name: "ChartSheet2".to_string(),
                    slide: s4,
                    sheet_shape: sh4,
                },
            })]),
            Err(EditError::WorkbookProtected)
        );
    }

    #[test]
    fn phonetic_commands_undo_redo_and_formula_recalculation() {
        let (mut d, s) = workbook();

        // 1. A1 = "東京", B1 = "=PHONETIC(A1)"
        d.execute(vec![Command::Sheet(SheetCommand::SetCells {
            slide: s,
            edits: vec![
                CellEdit { row: 0, column: 0, input: CellInput::Value(CellValue::Text("東京".into())) },
                CellEdit { row: 0, column: 1, input: CellInput::Formula("PHONETIC(A1)".into()) },
            ],
        })]).unwrap();

        // B1 uses cell text because phonetic is None
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Text("東京".into()));

        // 2. Set A1 phonetic to "トウキョウ"
        d.execute(vec![Command::Sheet(SheetCommand::SetCellPhonetic {
            slide: s,
            row: 0,
            column: 0,
            phonetic: Some("トウキョウ".into()),
        })]).unwrap();

        assert_eq!(cell(&d, s, 0, 0).unwrap().phonetic.as_deref(), Some("トウキョウ"));
        // Recalculated: B1 should now be "トウキョウ"
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Text("トウキョウ".into()));

        // 3. Undo: reverts phonetic and recalculates B1 to "東京"
        assert!(d.undo());
        assert_eq!(cell(&d, s, 0, 0).unwrap().phonetic, None);
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Text("東京".into()));

        // 4. Redo: restores phonetic and B1 to "トウキョウ"
        assert!(d.redo());
        assert_eq!(cell(&d, s, 0, 0).unwrap().phonetic.as_deref(), Some("トウキョウ"));
        assert_eq!(cell(&d, s, 0, 1).unwrap().value, CellValue::Text("トウキョウ".into()));

        // 5. SetShowPhonetic toggle and undo/redo
        let get_sheet = |doc: &Document| crate::sheet_calc::sheet_of_slide(doc.presentation().slide(s).unwrap()).cloned().unwrap();
        assert!(!get_sheet(&d).show_phonetic);
        d.execute(vec![Command::Sheet(SheetCommand::SetShowPhonetic { slide: s, show: true })]).unwrap();
        assert!(get_sheet(&d).show_phonetic);
        assert!(d.undo());
        assert!(!get_sheet(&d).show_phonetic);
        assert!(d.redo());
        assert!(get_sheet(&d).show_phonetic);

        // 6. SetPhoneticProperties and undo/redo
        use crate::sheet::{PhoneticProperties, PhoneticType, PhoneticAlignment};
        let props = PhoneticProperties {
            font_id: 0,
            phonetic_type: PhoneticType::Hiragana,
            alignment: PhoneticAlignment::Center,
        };
        d.execute(vec![Command::Sheet(SheetCommand::SetPhoneticProperties { slide: s, properties: Some(props) })]).unwrap();
        assert_eq!(get_sheet(&d).phonetic_pr, Some(props));
        assert!(d.undo());
        assert_eq!(get_sheet(&d).phonetic_pr, None);
        assert!(d.redo());
        assert_eq!(get_sheet(&d).phonetic_pr, Some(props));
    }

    #[test]
    fn split_panes_command_undo_redo_and_mutual_exclusion() {
        let (mut d, s) = workbook();
        let get_sheet = |doc: &Document| crate::sheet_calc::sheet_of_slide(doc.presentation().slide(s).unwrap()).cloned().unwrap();

        assert_eq!(get_sheet(&d).split, None);
        assert_eq!(get_sheet(&d).frozen, None);

        // 1. Split at column 2, row 3
        d.execute(vec![Command::Sheet(SheetCommand::Split { slide: s, split: Some((2, 3)) })]).unwrap();
        assert_eq!(get_sheet(&d).split, Some((2, 3)));
        assert_eq!(get_sheet(&d).frozen, None);

        // 2. Undo reverts split
        assert!(d.undo());
        assert_eq!(get_sheet(&d).split, None);

        // 3. Redo restores split
        assert!(d.redo());
        assert_eq!(get_sheet(&d).split, Some((2, 3)));

        // 4. Freeze panes un-splits and sets frozen
        d.execute(vec![Command::Sheet(SheetCommand::Freeze { slide: s, frozen: Some((1, 1)) })]).unwrap();
        assert_eq!(get_sheet(&d).frozen, Some((1, 1)));
        assert_eq!(get_sheet(&d).split, None);

        // 5. Undo restores split and clears frozen
        assert!(d.undo());
        assert_eq!(get_sheet(&d).split, Some((2, 3)));
        assert_eq!(get_sheet(&d).frozen, None);

        // 6. Split at (4, 5) clears frozen if set
        d.execute(vec![Command::Sheet(SheetCommand::Freeze { slide: s, frozen: Some((2, 2)) })]).unwrap();
        assert_eq!(get_sheet(&d).frozen, Some((2, 2)));
        d.execute(vec![Command::Sheet(SheetCommand::Split { slide: s, split: Some((4, 5)) })]).unwrap();
        assert_eq!(get_sheet(&d).split, Some((4, 5)));
        assert_eq!(get_sheet(&d).frozen, None);
    }

    #[test]
    fn auto_expand_grid_on_cell_edit_or_format() {
        let (mut d, s) = workbook();
        let get_sheet = |doc: &Document| {
            let slide = doc.presentation().slide(s).unwrap();
            crate::sheet_calc::sheet_of_slide(slide).unwrap().clone()
        };
        let initial_rows = get_sheet(&d).rows.len();
        let initial_cols = get_sheet(&d).columns.len();
        assert!(initial_rows <= 100);
        assert!(initial_cols <= 50);

        // Edit cell at (column 30, row 150), which is beyond initial rows and columns.
        let target_col = (initial_cols + 10) as u32;
        let target_row = (initial_rows + 20) as u32;
        d.execute(vec![Command::Sheet(SheetCommand::SetCells {
            slide: s,
            edits: vec![CellEdit {
                row: target_row,
                column: target_col,
                input: CellInput::Value(CellValue::Text("Hello Excel Grid".into())),
            }],
        })]).unwrap();

        let sheet = get_sheet(&d);
        assert!(sheet.rows.len() > target_row as usize);
        assert!(sheet.columns.len() > target_col as usize);
        assert_eq!(sheet.cell(target_row, target_col).unwrap().text, "Hello Excel Grid");

        // Format cell even further (e.g. bold at row 200, col 50)
        let format_row = target_row + 30;
        let format_col = target_col + 15;
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
            slide: s,
            ranges: vec![CellRange {
                first_row: format_row,
                first_column: format_col,
                last_row: format_row,
                last_column: format_col,
            }],
            change: StyleChange::Bold(true),
        })]).unwrap();

        let sheet2 = get_sheet(&d);
        assert!(sheet2.rows.len() > format_row as usize);
        assert!(sheet2.columns.len() > format_col as usize);
    }
}


#[cfg(test)]
#[path="tests/validation_ranges.rs"]
mod validation_range_tests;
