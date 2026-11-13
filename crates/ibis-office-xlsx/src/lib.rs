// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "scenarios"), allow(dead_code))]
//! Workbooks (Excel): cells, the formula bar, sheet tabs and headers, and
//! the workbook's editing — the spreadsheet担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

// The crates below, under the names this crate's code uses (as in ibis-office-app and ibis-office-shell): the shell is a crate of
// its own (tasks/doc/crate-split-plan.md, 12), and what it took down from the app's root is under the same names here.
pub(crate) use ibis_office_base::clock;
pub(crate) use clock::debug_timing;
pub(crate) use ibis_office_render as render;
pub(crate) use ibis_office_shell as shell;
pub(crate) use shell::actions::*;
pub(crate) use shell::app::*;
#[cfg(all(not(target_family = "wasm"), feature = "scenarios"))]
pub(crate) use shell::read_file::*;
pub(crate) use shell::{native_shape_clipboard, spelling};
// The browser's file pickers (the shell's), under the name the formats use.
#[cfg(target_family = "wasm")]
pub(crate) use shell::web;

pub(crate) mod autosum;
/// The extension traits of this folder's `impl SlideEditor` blocks, written by `scripts/gen-editor-ext.py`.
pub mod ext;
/// The workbook's own lines of the menus.
pub(crate) mod menu_items;
/// The screen's rows of the sidebar table.
pub(crate) mod sidebars;
pub mod cell_edit;
pub(crate) mod clipboard;
pub(crate) mod clipboard_html;
pub(crate) mod clipboard_native;
pub(crate) mod clipboard_xmlss;
mod function_dialog;
mod thesaurus_panel;
mod word_count;
mod quick_analysis;
pub(crate) mod paste_placement;
pub mod paste_special_dialog;
pub mod copy_picture;
pub(crate) mod names;
pub(crate) mod names_list;
pub(crate) mod names_panel;
pub(crate) mod paste_names;
pub(crate) mod paste_names_dialog;
pub(crate) mod sheet_transfer;
pub mod page_layout;
pub(crate) mod print;
pub(crate) mod validation;
pub mod conditional;
pub(crate) mod conditional_edit;
pub(crate) mod sort_edit;
pub(crate) mod dedupe_edit;
pub(crate) mod edit_panels;
pub(crate) mod phonetic;
pub(crate) mod spelling_panel;
pub(crate) mod error_check;
pub(crate) mod external_links;
pub(crate) use crate::shell::xls_note as export_xls;
pub(crate) mod import_text;
pub(crate) mod text_columns;
pub(crate) mod goto_special;
pub(crate) mod split_view;
pub(crate) mod cell_styles;
pub(crate) mod flash_fill;
pub(crate) mod notes;
pub use crate::shell::legacy_password as password;
pub(crate) mod protection;
pub(crate) mod series;
pub(crate) mod goal_seek;
pub(crate) mod data_table;
pub(crate) mod custom_format;
pub(crate) mod fill;
pub(crate) mod chart_edit;
pub(crate) mod chart_style;
pub(crate) mod chart_series;
pub(crate) mod chart_dropdown;
pub(crate) mod chart_type_compatibility;
pub(crate) mod range_field;
pub(crate) mod move_chart;
pub mod chart_template;
pub(crate) mod comment_card;
pub(crate) mod conditional_quick;
pub(crate) mod table_edit;
pub(crate) mod filter;
pub(crate) mod format_more;
pub(crate) mod autocomplete;
pub mod pivot;
pub(crate) mod pivot_fields;
pub(crate) mod pivot_pane;
pub(crate) mod pivot_protect;
pub mod row_fit;
pub(crate) mod table_design;
pub(crate) mod table_style_preview;
pub mod header_footer;
pub mod header_footer_edit;
pub(crate) mod find;
pub(crate) mod links;
pub(crate) mod drag_autoscroll;
pub(crate) mod headers;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/scenarios.rs"]
pub mod scenarios;
pub mod export_hooks;
pub mod sheet_ui;
mod sheet_tab_scroll;
pub(crate) mod sheet_tab_drag;
mod right_pane;
pub mod sheet_view;
pub(crate) mod view_position;
pub(crate) mod replace_fonts;
pub(crate) mod view_state;
pub(crate) mod slicer;
pub mod slicer_view;
pub mod control_view;
pub(crate) mod checkbox;
pub(crate) mod control;
pub(crate) mod control_insert;
pub mod timeline_view;
pub(crate) mod timeline;
pub(crate) mod timeline_insert;
pub mod toolbar;
pub(crate) mod advanced_filter;
pub(crate) mod analysis_panel;
pub(crate) mod solver_panel;
pub(crate) mod autocorrect;
// The pen and recognition code is shared by the three formats: shell/ink.rs (kept under this name for this directory and its tests).
pub(crate) use crate::shell::ink;
mod ink_panel;
pub(crate) mod toggle;
pub(crate) mod slicer_insert;
pub(crate) mod slicer_settings;
pub(crate) mod forecast_sheet;
pub(crate) mod query_ops;
pub(crate) mod query_editor;
pub(crate) mod cells_dialog;
pub(crate) mod consolidate;
pub(crate) mod context_menu;
pub(crate) mod scenario_manager;
pub(crate) mod custom_views;
pub(crate) mod sparkline_dialog;
pub(crate) mod row_col_size;
pub(crate) mod subtotal_dialog;
pub(crate) mod pivot_group;
pub(crate) mod pivot_drill;
pub(crate) mod pivot_options;
pub(crate) mod trace;
pub mod formula_audit;
pub mod formula_reference;
pub(crate) mod properties_panel;
pub(crate) mod pivot_calc_field;
pub(crate) mod pivot_calc_item;
pub(crate) mod alt_text_panel;
pub(crate) mod shape_edit;
pub(crate) mod chart_insert;
pub mod threaded_comments;
pub(crate) mod accessibility_panel;
pub(crate) mod compatibility_panel;
pub(crate) mod selection_pane;
pub(crate) mod frame_panel;
pub(crate) mod theme_panel;
pub(crate) mod window_hide;
pub mod arrange_windows;
pub use crate::shell::backup_save;
mod backup_save_panel;
pub(crate) mod repair;
pub(crate) mod xml_maps;

pub use print::PrintWhat;

use std::collections::{HashMap, HashSet};

use gpui_kit::*;
use ibis_doc_model::{Presentation, SlideId};

use crate::shell::editor::{SlideEditor, WorkspaceContent};
use crate::shell::format::{FormatEditor, FormatState, impl_format_state};
use crate::ext::*;

/// What only an open workbook has.
pub struct State {
    pub(crate) range_pick: Option<range_field::RangePick>,
    pub(crate) range_pick_click_release: bool,
    /// The selected cell of each worksheet, as (column, row) of the cell or
    /// of the top-left cell of its merged range. A sheet not in here has A1.
    pub cells: HashMap<SlideId, (u32, u32)>,
    /// The other corner of each worksheet's selected range, when more than
    /// the selected cell is selected (the selected cell is the anchor).
    pub cell_ends: HashMap<SlideId, (u32, u32)>,
    /// A range of cells is being selected by dragging.
    pub(crate) cell_drag: bool,
    /// The pointer is held past the edge of the cells during a drag: the view scrolls on a timer (#186).
    pub(crate) drag_scroll: Option<drag_autoscroll::DragScroll>,
    /// Dragging with ⌘ held to deselect cells from the current selection.
    pub(crate) deselect_drag: Option<sheet_ui::DeselectDrag>,
    /// Dragging a split bar to change window split position.
    pub split_drag: Option<split_view::SplitDrag>,
    /// The name box and the ジャンプ dialog.
    pub jump: sheet_ui::CellJump,
    /// The cell being typed into.
    pub cell_edit: Option<cell_edit::CellEdit>,
    /// Where the formula bar's text field is (set when painted), to find the character pressed.
    pub bar_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The formula bar widened to several lines (the ▼ at its right end), as Excel's: all the lines of a cell are shown and typed into; closed, it shows one line.
    pub(crate) bar_expanded: bool,
    pub bar_visual_lines: Vec<(usize,String)>,
    pub(crate) bar_caret_row: Option<usize>,
    pub(crate) bar_shapes: Vec<std::rc::Rc<ShapedLine>>,
    /// The scrolling of the widened bar.
    pub(crate) bar_scroll: ScrollHandle,
    /// Cells copied or cut.
    pub clip: Option<clipboard::Clip>,
    /// The copied cells' dashed outline is shown (Excel's marching ants after ⌘C, the user's K0011): from a copy until Esc or
    /// typing into a cell. (A cut shows it while the cut is pending.)
    pub(crate) copy_outline: bool,
    /// Where the row and column headers were painted (for pressing them).
    pub header_geometry: headers::SharedGeometry,
    /// A header's line being dragged (sizing a column or row).
    pub(crate) header_drag: Option<headers::HeaderDrag>,
    /// A timeline selection grip, period range, or scrollbar thumb being dragged.
    pub(crate) timeline_drag: Option<timeline::TimelineDrag>,
    /// A form control scrollbar thumb being dragged.
    pub(crate) control_drag: Option<control::ControlThumbDrag>,
    /// A margin handle on the ruler being dragged in page layout view (adjusting page margins).
    pub(crate) ruler_margin_drag: Option<page_layout::RulerMarginDrag>,
    /// Whole columns or rows being selected by dragging along the headers.
    pub(crate) header_select: Option<ibis_doc_model::sheet_command::Axis>,
    /// The pointer is on a header's line (the resize pointer shows).
    pub(crate) header_hover: Option<ibis_doc_model::sheet_command::Axis>,
    /// The header's menu (the right button), while open.
    pub(crate) header_popup: Option<headers::HeaderPopup>,
    /// The range the fill handle is being dragged from.
    pub fill_source: Option<ibis_doc_model::CellRange>,
    /// The selection's border being dragged to move or copy it.
    pub(crate) cell_move: Option<fill::CellMove>,
    /// Option is held while the fill handle is dragged (a copy, not a series).
    pub(crate) fill_copy: bool,
    /// A range let go over cells with data, waiting for the answer of Excel's question.
    pub(crate) move_overwrite: Option<fill::CellMove>,
    /// Where a run of Tabs began (its column) and the cell it reached, for Enter.
    pub(crate) tab_start: Option<(u32, (u32, u32))>,
    /// The formats 書式のコピー picked up (rows of the range), waiting for a range.
    pub(crate) painter: Option<Vec<Vec<ibis_doc_model::CellStyle>>>,
    /// The name manager, when open in the sidebar.
    pub(crate) names: Option<names_panel::NamesPanel>,
    /// The data validation panel, when open in the sidebar.
    pub validation: Option<validation::ValidationPanel>,
    /// A pending Yes/No/Cancel confirmation for a `warning`-style data validation rule.
    pub(crate) validation_warning: Option<validation::ValidationWarning>,
    /// A list's drop-down, while open.
    pub list_popup: Option<validation::ListPopup>,
    pub(crate) validation_choices: Option<validation::ValidationChoices>,
    pub(crate) validation_rejected: Option<(u32, u32)>,
    pub(crate) validation_message_bounds: Option<Bounds<Pixels>>,
    /// A filter heading's ▼ panel, while open.
    pub filter_panel: Option<filter::FilterPanel>,
    /// The cell whose comment (thread or note) shows, the pointer on it.
    pub comment_hover: Option<(u32, u32)>,
    /// The hover card of the selected cell's note (notes.rs).
    pub(crate) note_card: notes::NoteCard,
    /// The scroll of the hover card of a comment that is taller than the card may be.
    pub comment_scroll: ScrollHandle,
    /// How long the pointer has been on the comment mark, and the timers that keep the card (comment_card.rs).
    pub comment_popup: comment_card::PopupTimers,
    /// テーブルの作成, while open.
    pub(crate) table_dialog: Option<table_edit::TableDialog>,
    /// グラフの書式, when open in the sidebar.
    pub chart_edit: Option<chart_edit::ChartPanel>,
    /// The last fill (source, the cell dragged to, how), for オートフィル オプション.
    pub(crate) last_fill: Option<(ibis_doc_model::CellRange, u32, u32, fill::FillMode)>,
    /// The last paste of copied cells, for 貼り付けのオプション (`clipboard.rs`).
    pub(crate) last_paste: Option<clipboard::LastPaste>,
    /// 検索と置換, when open in the sidebar.
    pub find: Option<find::FindPanel>,
    /// スペル チェック, when open in the sidebar.
    pub(crate) spelling: Option<spelling_panel::SpellPanel>,
    /// The link panel, when open in the sidebar.
    pub link: Option<links::LinkPanel>,
    /// ページ設定, when open in the sidebar.
    pub page_setup: bool,
    /// ファイル > 印刷 の印刷ダイアログ, when open.
    pub print_panel: bool,
    /// カスタム ヘッダー・フッター編集, when open in the sidebar.
    pub header_footer_edit: Option<header_footer_edit::HeaderFooterEditPanel>,
    /// 表示形式（ユーザー定義）, when open in the sidebar.
    pub custom_format: Option<custom_format::FormatPanel>,
    /// Which sections of セルの書式設定 are open (表示形式・配置・フォント・罫線・塗りつぶし・保護), kept while the window lives.
    pub format_tab: usize,
    pub(crate) conditional_edit: Option<conditional_edit::ConditionalPanel>,
    pub sort_edit: Option<sort_edit::SortPanel>,
    /// The panels of the editing basics (重複の削除, 区切り位置, …).
    pub panels: edit_panels::EditPanels,
    /// 起動時のリンクの確認: the bar that tells the workbook has links to other workbooks (their values are the
    /// cached ones until updated) shows until dismissed.
    pub(crate) links_notice: bool,
    pub(crate) border_line: ibis_doc_model::BorderLine,
    pub border_color: ibis_doc_model::Rgb,
    /// A sheet tab being dragged.
    pub(crate) tab_drag: Option<sheet_tab_drag::TabDrag>,
    /// What the right pane was last made for (`right_pane.rs`).
    pub(crate) right_key: Option<right_pane::RightKey>,
    /// The fields pane the current cell opened in a pivot table (`pivot_pane.rs`).
    pub(crate) pivot_pane: pivot_pane::PivotPane,
    /// Scrolling of the sheet tabs along the bottom.
    pub tab_scroll: ScrollHandle,
    pub(crate) tab_reveal: Option<SlideId>,
    pub(crate) tab_last_click: Option<(SlideId, usize)>,
    /// Whether the formula bar is shown.
    pub show_formula_bar: bool,
    /// データの入力規則のマーク表示 (Circle Invalid Data).
    pub(crate) validation_circles: bool,
    /// 範囲の編集の許可でセッション中にロック解除された保護範囲の名前.
    pub(crate) unlocked_ranges: HashSet<String>,
    /// トレース矢印 (参照元・参照先のトレース).
    pub(crate) trace_arrows: Vec<trace::TraceArrow>,
    /// Whether background calc index warmup has started after the first render.
    pub(crate) calc_warmup_started: bool,
    /// 印刷およびPDF書き出しの対象 (XLSX-SEC-019).
    pub(crate) print_what: PrintWhat,
    /// このブックで常にバックアップを作成する (XLSX-FILE-019). None の場合は全体設定に従う.
    pub(crate) always_create_backup: Option<bool>,
    /// すべてのメモを表示する (XLSX-SEC-009 / Show All Notes).
    pub(crate) show_all_notes: bool,
    /// フォーム コントロールのデザイン モード (Design Mode).
    pub(crate) design_mode: bool,
    /// ステータス バーの集計表示ビットマスク (XLSX-VIEW-006: 平均, データの個数, 数値の個数, 最小値, 最大値, 合計).
    pub(crate) status_summary_mask: u8,
    /// How far each worksheet was scrolled when it was left (the workspace's scroll offset), restored when it is shown again, as Excel's:
    /// kept until the workbook closes. By the sheet's id, so adding, deleting, moving and renaming sheets cannot mix them up (#145).
    pub(crate) sheet_scrolls: HashMap<SlideId, Point<Pixels>>,
    /// The scroll the file left the first sheet at, put in force when the workspace has been laid out (`xlsx/view_position.rs`:
    /// before that the content has no size, and the scroll would be held at the top).
    pub(crate) pending_view_scroll: Option<Point<Pixels>>,
}

pub(crate) const STATUS_SUMMARY_AVG: u8 = 1 << 0;
pub(crate) const STATUS_SUMMARY_COUNT: u8 = 1 << 1;
pub(crate) const STATUS_SUMMARY_NUM_COUNT: u8 = 1 << 2;
pub(crate) const STATUS_SUMMARY_MIN: u8 = 1 << 3;
pub(crate) const STATUS_SUMMARY_MAX: u8 = 1 << 4;
pub(crate) const STATUS_SUMMARY_SUM: u8 = 1 << 5;
pub(crate) const STATUS_SUMMARY_DEFAULT: u8 = STATUS_SUMMARY_AVG | STATUS_SUMMARY_COUNT | STATUS_SUMMARY_SUM;

/// The workbook's keys (in the sheet's context).
pub fn key_bindings() -> Vec<KeyBinding> {
    use sheet_ui::*;
    let c = Some(crate::shell::editor::SHEET_CONTEXT);
    let p = Some(find::FIND_PANEL_CONTEXT);
    let m = |k: &str| format!("{}-{k}", crate::secondary_modifier());
    vec![
        // Typing into cells (cell_edit.rs): F2 edits; Backspace and Delete
        // take a character while typing, else clear the cells.
        KeyBinding::new("f2", EditCell, c),
        KeyBinding::new("f4", CycleReference, c),
        KeyBinding::new("backspace", CellBackspace, c),
        KeyBinding::new("delete", CellDelete, c),
        KeyBinding::new(&m("b"), CellBold, c),
        KeyBinding::new(&m("t"), InsertTable, c),
        KeyBinding::new(&m("i"), CellItalic, c),
        KeyBinding::new(&m("u"), CellUnderline, c),
        KeyBinding::new(&m("c"), CellCopy, c),
        KeyBinding::new(&m("x"), CellCut, c),
        KeyBinding::new(&m("v"), CellPaste, c),
        KeyBinding::new(&m("shift-v"), CellPastePlain, c),
        KeyBinding::new(&m("alt-v"), CellPasteSpecialDialog, c),
        KeyBinding::new("ctrl-alt-v", CellPasteSpecialDialog, c),
        KeyBinding::new(&m("\\"), ClearCellFormats, c),
        KeyBinding::new("tab", CellTab, c),
        KeyBinding::new(&m("k"), InsertLink, c),
        KeyBinding::new(&m("f"), OpenSheetFind, c),
        KeyBinding::new(&m("shift-h"), OpenSheetFind, c),
        // The same keys with the focus in the find panel's fields.
        KeyBinding::new(&m("f"), OpenSheetFind, p),
        KeyBinding::new(&m("shift-h"), OpenSheetFind, p),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-g", SheetFindNext, p),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-g", SheetFindPrevious, p),
        // ⌘G / ⇧⌘G: Mac only (Excel for Mac). Windows and Linux have no such key here.
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-g", SheetFindNext, c),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-g", SheetFindPrevious, c),
        // F3 is Find Next (Shift+F3 is Find Previous while the find panel is open, and Insert Function otherwise: see the InsertFunction handler).
        KeyBinding::new("f3", SheetFindNext, c),
        KeyBinding::new("f3", SheetFindNext, p),
        KeyBinding::new("shift-f3", SheetFindPrevious, p),
        KeyBinding::new(&m("a"), SelectAllCells, c),
        KeyBinding::new("enter", CellEnter, c),
        KeyBinding::new("ctrl-enter", CellFillSelection, c),
        KeyBinding::new(&m("1"), OpenCellFormat, c),
        KeyBinding::new("ctrl-shift-l", ToggleFilter, c),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-f", ToggleFilter, c),
        KeyBinding::new(&m("d"), FillDown, c),
        KeyBinding::new(&m("r"), FillRight, c),
        KeyBinding::new("ctrl-;", InsertDate, c),
        KeyBinding::new("ctrl-shift-;", InsertTime, c),
        KeyBinding::new("ctrl-:", InsertTime, c),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-;", InsertTime, c),
        KeyBinding::new("ctrl-space", SelectColumns, c),
        KeyBinding::new("shift-space", SelectRows, c),
        // A line break in the cell: Alt+Enter as on Windows, and Excel for
        // Mac's ⌃⌥Return and ⌥⌘Return.
        KeyBinding::new("alt-enter", CellLineBreak, c),
        KeyBinding::new("ctrl-alt-enter", CellLineBreak, c),
        KeyBinding::new(&m("alt-enter"), CellLineBreak, c),
        KeyBinding::new("alt-down", PickFromList, c),
        KeyBinding::new("ctrl-e", FlashFill, c),
        KeyBinding::new(&m("enter"), CellFillSelection, c),
        KeyBinding::new("ctrl-9", HideRows, c),
        KeyBinding::new("ctrl-0", HideColumns, c),
        KeyBinding::new("ctrl-shift-9", UnhideRows, c),
        KeyBinding::new("ctrl-shift-0", UnhideColumns, c),
        KeyBinding::new(&m("`"), ToggleShowFormulas, c),
        KeyBinding::new("ctrl-`", ToggleShowFormulas, c),
        KeyBinding::new("shift-f2", EditNote, c),
        KeyBinding::new("shift-f3", InsertFunction, c),
        // ⌘+ (⌃+): 挿入 (Insert Selection / Rows / Columns)
        KeyBinding::new(&m("+"), SheetInsert, c),
        KeyBinding::new(&m("="), SheetInsert, c),
        KeyBinding::new("ctrl-+", SheetInsert, c),
        KeyBinding::new("ctrl-=", SheetInsert, c),
        KeyBinding::new("ctrl-shift-=", SheetInsert, c),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-=", SheetInsert, c),
        // ⌘- (⌃-): 削除 (Delete Selection / Rows / Columns)
        KeyBinding::new(&m("-"), SheetDelete, c),
        KeyBinding::new("ctrl--", SheetDelete, c),
    ]
}

/// A workbook's editor: shown with sheet tabs, a formula bar and cell
/// selection, like Excel.
pub struct Workbook;

mod dialog_hooks;
mod event_hooks;
mod workspace_hooks;
mod infobar_hooks;
impl_format_state!(State, ibis_pptx::Kind::Workbook);

#[ibis_office_macros::editor_ext(Xlsx)]
impl SlideEditor {
    /// A workbook's state, when a workbook is open.
    pub fn sheet_state(&self) -> Option<&State> {
        self.format_state::<State>()
    }

    pub fn sheet_state_mut(&mut self) -> Option<&mut State> {
        self.format_state_mut::<State>()
    }

    /// The selected cell of the current worksheet: (column, row) of the cell
    /// or of the top-left cell of its merged range.
    pub fn current_cell(&self) -> Option<(u32, u32)> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.sheet_state()?.cells.get(&self.current_slide).copied().unwrap_or((0, 0));
        Some(sheet.merge_at(row, column).map_or((column, row), |m| (m.first_column, m.first_row)))
    }

    /// The cells the selection covers: from the selected cell to the other
    /// corner of the range, grown to take in whole merged ranges.
    pub fn selected_range(&self) -> Option<ibis_doc_model::CellRange> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        let (end_column, end_row) = self.sheet_state()?.cell_ends.get(&self.current_slide).copied().unwrap_or((column, row));
        let mut range = ibis_doc_model::CellRange {
            first_row: row.min(end_row),
            first_column: column.min(end_column),
            last_row: row.max(end_row),
            last_column: column.max(end_column),
        };
        // Merged ranges it cuts into are taken in whole, until none is left.
        loop {
            let grown = sheet.merges.iter().fold(range, |r, m| {
                let meets = m.first_row <= r.last_row && m.last_row >= r.first_row && m.first_column <= r.last_column && m.last_column >= r.first_column;
                if !meets {
                    return r;
                }
                ibis_doc_model::CellRange {
                    first_row: r.first_row.min(m.first_row),
                    first_column: r.first_column.min(m.first_column),
                    last_row: r.last_row.max(m.last_row),
                    last_column: r.last_column.max(m.last_column),
                }
            });
            if grown == range {
                return Some(range);
            }
            range = grown;
        }
    }
}

/// The context menus of what a workbook's selected cell is in (`FormatInfo::outer_contexts`).
fn outer_contexts(state: &crate::shell::menu_context::ContextState) -> Vec<crate::shell::menu_context::ContextMenu> {
    use crate::shell::menu_context::ContextMenu::*;
    let mut out = Vec::new();
    if state.in_sheet_table {
        out.push(TableDesign);
    }
    if state.in_pivot {
        out.extend([PivotAnalyze, PivotDesign]);
    }
    if state.in_sparkline {
        out.push(Sparkline);
    }
    out
}

impl FormatEditor for Workbook {
    fn note_open_failed(path: &std::path::Path) {
        repair::note_open_failed(path);
    }
    fn note_opened(path: &std::path::Path) {
        repair::note_opened(path);
    }
    const INFO: &'static crate::shell::format::FormatInfo = &crate::shell::format::FormatInfo {
        order: 1,
        extension: "xlsx",
        new_file_name: || "Book1.xlsx".into(),
        untitled_key: "Book1",
        office: crate::shell::format::OfficeApp {
            name: "Excel",
            setting_key: "office_path.excel",
            #[cfg(target_os = "macos")]
            bundle: "Microsoft Excel.app",
            #[cfg(target_os = "windows")]
            exe: "EXCEL.EXE",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            open_key: "window.open-in-excel",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            logo_svg: include_bytes!("../logo/open_in_excel.svg"),
        },
        new_command: "file.new.workbook",
        command_menus: {
            use crate::shell::chrome::MenuId as M;
            &[M::File, M::Edit, M::View, M::Insert, M::Format, M::Data, M::Tools, M::Window, M::Help]
        },
        icon: "table_chart",
        color: 0x1e8e3e,
        context_tones: [
            crate::shell::menu_context::ContextTone { fg: 0x157332, bg: 0x1e8e3e, bg_alpha: 0.12 },
            crate::shell::menu_context::ContextTone { fg: 0x7fd48f, bg: 0x7fd48f, bg_alpha: 0.16 },
        ],
        menus: &crate::shell::menu_layout::SHEET_MENUS,
        fold_order: {
            use crate::shell::chrome::MenuId;
            &[MenuId::PageLayout, MenuId::Formulas, MenuId::Review, MenuId::Data]
        },
        sidebars: sidebars::SIDEBARS,
        mask: crate::shell::commands::KindMask::XLSX,
        several_windows: Some(crate::shell::format::SeveralWindows { compare_left_right: false, follow_shape_edits: true, number_in_title_bar: true }),
        shapes: crate::shell::format::ShapeRules { in_shell: false, several: true, try_first: false, cmyk: false, break_is_typed: false },
        outer_contexts,
        font_substitutes: false,
        drawing_canvas: false,
        new_badge_color: 0x1e8e3e,
        search_history_key: "menu-search.history.spreadsheet",
        px_per_inch_at_100: crate::shell::editor::SHEET_PX_PER_INCH_AT_100,
        #[cfg(not(target_family = "wasm"))]
        save_formats: &[("file.save-format.xlsx", "xlsx"), ("file.save-format.xltx", "xltx"), ("file.save-format.xls", "xls"), ("file.save-format.ods", "ods"), ("file.save-format.csv", "csv"), ("file.save-format.tsv", "tsv")],
    };

    fn theme_panel(editor: &SlideEditor) -> Option<&crate::shell::theme_panel::ThemePanel> {
        editor.sheet_state()?.panels.theme.as_ref()
    }

    fn theme_panel_open(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.open_xlsx_theme_panel(window, cx)
    }

    fn theme_panel_apply(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_xlsx_theme_panel(window, cx)
    }

    fn theme_palette_apply(editor: &mut SlideEditor, id: &str, _window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_xlsx_theme_palette(id, cx)
    }

    fn open_print_panel(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.commit_edit(window, cx);
        editor.open_xlsx_print_panel(cx);
        true
    }

    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(editor: &SlideEditor, _printing: bool) -> Result<crate::shell::format::PdfJob, String> {
        editor.xlsx_pdf_job()
    }

    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(editor: &SlideEditor, path: &std::path::Path, source: &ibis_pptx::Source, copy: bool) -> Option<Result<crate::shell::format::FileBytes, String>> {
        editor.xlsx_bytes_for_path(path, source, copy)
    }

    #[cfg(not(target_family = "wasm"))]
    fn new_file_save_folder(_editor: &SlideEditor) -> Option<std::path::PathBuf> {
        crate::shell::backup_save::custom_default_save_location()
    }

    fn sidebar_sync(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.sync_pivot_pane(window, cx);
    }

    fn dialog_background_panel(editor: &mut SlideEditor, child_key: Option<&'static str>, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        let retain_chart = editor.sheet_state().is_some_and(|state| state.chart_edit.is_some())
            && matches!(child_key, Some("move_chart_panel" | "chart_template_panel" | "chart_series_editor"));
        let retain_sort = child_key == Some("sort_level_editor");
        let retain_names = child_key == Some("names_dialog");
        let retain_pivot = matches!(child_key, Some("pivot_value_dialog" | "pivot_field_dialog"));
        if retain_pivot { editor.pivot_fields_background(cx) } else if retain_chart { editor.sheet_chart_panel(cx) } else if retain_sort { editor.sort_background_panel(cx) } else if retain_names { editor.names_list_panel(cx) } else { None }
    }

    fn dialog_parent_panel(editor: &mut SlideEditor, child_key: Option<&'static str>, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        if child_key == Some("conditional_rule_editor") { editor.conditional_parent_panel(cx) } else { None }
    }

    fn extra_dialogs(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Vec<crate::shell::format::ExtraDialog> {
        use crate::shell::dialog::DialogMode;
        use crate::shell::format::ExtraDialog;
        if !editor.dialog.active {
            if let Some(panel) = editor.chart_series_editor_panel(cx) {
                let mode = if editor.sheet_range_pick_active() { DialogMode::Modeless } else { DialogMode::Modal };
                return vec![ExtraDialog { panel, key: "chart_series_editor", mode, footer_from_sidebar: false, resets_panel_mode: true }];
            }
            if editor.sheet_range_pick_active() {
                if let Some(panel) = editor.generic_range_pick_panel(cx) {
                    return vec![ExtraDialog { panel, key: "generic_range_picker", mode: DialogMode::Modeless, footer_from_sidebar: true, resets_panel_mode: false }];
                }
            }
        }
        Vec::new()
    }

    fn sidebar_blocked_by_dialog(editor: &SlideEditor) -> bool {
        editor.dialog.active
            && matches!(editor.dialog.key, Some("chart_series_editor" | "move_chart_panel" | "chart_template_panel" | "sort_level_editor" | "names_dialog" | "pivot_value_dialog" | "pivot_field_dialog"))
    }

    fn document_loaded(editor: &mut SlideEditor) {
        editor.restore_sheet_views();
    }

    fn own_edit_open(editor: &SlideEditor) -> bool {
        editor.cell_edit().is_some()
    }

    fn has_pending_edit(editor: &SlideEditor) -> bool {
        editor.cell_edit().is_some_and(|e| e.mode == crate::shell::edit_mode::EditMode::Enter || e.pending.is_some())
    }

    /// What is being typed in a cell is put in, so it is saved too.
    fn enter_pending_edit(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.commit_cell_edit(cx);
    }

    fn formula_edit_key(editor: &mut SlideEditor, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.sheet_state().is_some() && editor.formula_edit_key(event, window, cx)
    }

    fn set_font_size(editor: &mut SlideEditor, pt: f32, _: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.xlsx_set_font_size(pt, cx);
    }

    fn cancel_tool_early(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        // Esc in a sheet tab's name field (#174): the sheet keeps its name.
        editor.cancel_sheet_rename_if_open(window, cx)
    }

    fn cancel_tool_late(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.xlsx_cancel_tool_late(window, cx)
    }

    fn pointer_move_early(editor: &mut SlideEditor, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.move_formula_reference_drag(event, cx)
    }

    fn pointer_move(editor: &mut SlideEditor, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.xlsx_pointer_move(event, window, cx)
    }

    fn pointer_up_early(editor: &mut SlideEditor) {
        if let Some(edit) = editor.cell_edit_mut() {
            edit.reference_drag = false;
        }
    }

    fn pointer_up(editor: &mut SlideEditor, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.xlsx_pointer_up(event, window, cx)
    }

    fn range_pick_move(editor: &mut SlideEditor, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.sheet_range_pick_move(event, window, cx)
    }

    fn range_pick_down(editor: &mut SlideEditor, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.sheet_range_pick_down(event, window, cx)
    }

    fn range_pick_up(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.sheet_range_pick_up(window, cx)
    }

    fn range_pick_key(editor: &mut SlideEditor, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.sheet_range_pick_key(keystroke, window, cx)
    }

    fn range_pick_suspend_drag(editor: &mut SlideEditor, window: &mut Window) {
        editor.suspend_sheet_range_drag(window);
    }

    fn root_event_surface(_editor: &SlideEditor, cx: &Context<SlideEditor>) -> Option<AnyElement> {
        Some(range_field::event_surface(cx).into_any_element())
    }

    fn format_key(editor: &mut SlideEditor, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.validation_list_key(keystroke, cx) || editor.names_list_key(keystroke, window, cx)
    }

    fn zoom_set(editor: &mut SlideEditor, zoom: Option<f32>) {
        editor.keep_sheet_zoom(zoom);
    }

    fn view_from_model(editor: &mut SlideEditor) {
        editor.sync_sheet_zoom();
    }

    /// The other window's edit: a sheet or a shape it removed is no longer shown or selected here, and the zoom follows.
    fn model_from_peer(editor: &mut SlideEditor) {
        editor.fix_selection();
        editor.sync_sheet_zoom();
    }

    /// フォントの置換: the fonts of the cells' formats.
    fn used_fonts(editor: &SlideEditor) -> Vec<String> {
        replace_fonts::workbook_used_fonts(editor.presentation())
    }

    fn replace_fonts(editor: &mut SlideEditor, target: &str, replacement: &str, cx: &mut Context<SlideEditor>) -> usize {
        let (commands, count) = replace_fonts::workbook_replace_commands(editor.presentation(), target, replacement);
        if count > 0 {
            editor.run(commands, cx);
        }
        count
    }

    /// A window's view over a reload: the shown sheet and every sheet's cursor (`view_state.rs`).
    fn view_kept(editor: &SlideEditor) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(editor.keep_sheet_view()))
    }

    fn view_put_back(editor: &mut SlideEditor, kept: &dyn std::any::Any, window: &mut Window, cx: &mut Context<SlideEditor>) {
        if let Some(view) = kept.downcast_ref::<view_state::KeptView>() {
            editor.put_sheet_view_back(view, window, cx);
        }
    }

    fn page_when_current_gone(editor: &SlideEditor) -> Option<SlideId> {
        editor.sheet_state().and_then(|s| s.panels.analysis.as_ref()).map(|p| p.slide)
    }

    fn history_applied_early(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.refresh_chart_source_field_after_history(window, cx);
    }

    fn history_applied(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.sheet_spelling_refresh(cx);
    }

    fn modifiers_changed(editor: &mut SlideEditor, event: &ModifiersChangedEvent, cx: &mut Context<SlideEditor>) {
        editor.xlsx_modifiers_changed(event, cx);
    }

    fn workspace_wheel(editor: &mut SlideEditor, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.split_panes().is_some() && !editor.page_layout_shown() && editor.scroll_split_pane(event, false, false, cx)
    }

    fn shape_press_early(editor: &mut SlideEditor, event: &MouseDownEvent, cx: &mut Context<SlideEditor>) -> bool {
        editor.ink_press(event.position, cx)
    }

    fn shape_press(editor: &mut SlideEditor, shape: ibis_doc_model::ShapeId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.xlsx_shape_press(shape, event, window, cx)
    }

    fn register_actions(root: Stateful<Div>, cx: &mut Context<SlideEditor>) -> Stateful<Div> {
        let root = root
            .on_action(cx.listener(|this, _: &crate::SelectCellUp, window, cx| this.move_cell(0, -1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellDown, window, cx| this.move_cell(0, 1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellLeft, window, cx| this.move_cell(-1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellRight, window, cx| this.move_cell(1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsUp, _, cx| this.extend_cells(0, -1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsDown, _, cx| this.extend_cells(0, 1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsLeft, _, cx| this.extend_cells(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsRight, _, cx| this.extend_cells(1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::split_axis::ToggleSplit, _, cx| this.toggle_split(cx)))
        ;
        SlideEditor::cell_jump_actions(root, cx)
    }

    fn workspace_page_layout_shown(editor: &SlideEditor) -> bool {
        editor.page_layout_shown()
    }

    fn workspace_page_layout(editor: &SlideEditor, content: WorkspaceContent, cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        editor.render_page_layout(content, cx).map(|papers| papers.into_any_element())
    }

    fn workspace_main_pane_lead(editor: &SlideEditor) -> Point<Pixels> {
        editor.main_pane_lead()
    }

    fn workspace_cell_editor(editor: &mut SlideEditor, content: WorkspaceContent, cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        editor.render_cell_editor(content, cx).map(|e| e.into_any_element())
    }

    fn workspace_frozen_panes(editor: &SlideEditor, layout: crate::shell::editor::SlideLayout, content: WorkspaceContent, cx: &mut Context<SlideEditor>) -> Vec<AnyElement> {
        if editor.page_layout_shown() {
            return Vec::new();
        }
        editor.render_frozen_panes(layout, content, cx)
    }

    fn workspace_headers(editor: &mut SlideEditor, content: WorkspaceContent, cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        editor.render_sheet_headers(content, cx).map(|e| e.into_any_element())
    }

    fn workspace_split_scrollbars(editor: &SlideEditor, content: WorkspaceContent, cx: &mut Context<SlideEditor>) -> Option<Vec<AnyElement>> {
        (editor.split_panes().is_some() && !editor.page_layout_shown()).then(|| editor.render_split_scrollbars(content, cx))
    }

    fn workspace_page_break_preview_on(editor: &SlideEditor) -> bool {
        editor.current_sheet().is_some_and(|(_, s)| s.page_break_preview) && !editor.page_layout_shown()
    }

    fn workspace_sheet_frame(editor: &SlideEditor) -> Option<ibis_doc_model::EmuRect> {
        editor.current_sheet().map(|(shape, _)| shape.frame)
    }

    fn workspace_hide_ink(editor: &SlideEditor) -> bool {
        editor.current_sheet().map(|(_, s)| s.hide_ink).unwrap_or(false)
    }

    fn workspace_sheet_overlays(editor: &SlideEditor, scale: f32, cx: &mut Context<SlideEditor>) -> crate::shell::format::SheetOverlays {
        let mut over_sheet: Vec<AnyElement> = Vec::new();
        if let Some(cursor) = editor.render_cell_cursor(scale, cx) {
            over_sheet.push(cursor.into_any_element());
        }
        // Page break preview greys the sheet outside the print area, under the selected cell.
        if let Some(preview) = editor.render_page_break_preview(scale, false) {
            over_sheet.push(preview.into_any_element());
        }
        // Validation circles over invalid data.
        if let Some(circles) = editor.render_validation_circles(scale) {
            over_sheet.push(circles.into_any_element());
        }
        // Trace arrows over cells (参照元・参照先のトレース).
        if let Some(arrows) = editor.render_trace_arrows(scale) {
            over_sheet.push(arrows.into_any_element());
        }
        // The stroke being drawn with the pen (描画).
        if let Some(stroke) = editor.render_ink_preview(scale) {
            over_sheet.push(stroke.into_any_element());
        }
        // A chart's data ranges (データの編集), over the cells.
        if let Some(frames) = editor.render_chart_ranges(scale) {
            over_sheet.push(frames.into_any_element());
        }
        crate::shell::format::SheetOverlays { over_sheet, page_lines: editor.render_page_break_preview(scale, true).map(|lines| lines.into_any_element()) }
    }

    fn workspace_shape_design_mode(editor: &SlideEditor) -> bool {
        editor.is_design_mode_active()
    }

    fn workspace_shape_has_right_click(_editor: &SlideEditor) -> bool {
        true
    }

    fn workspace_shape_right_down(editor: &mut SlideEditor, id: ibis_doc_model::ShapeId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.on_sheet_shape_right_down(id, event, window, cx);
    }

    /// A press on the cells: the left button selects (or draws with the pen), the right one opens the cells' menu.
    fn workspace_grid_down(editor: &mut SlideEditor, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) {
        match event.button {
            MouseButton::Left => editor.on_sheet_down(event, window, cx),
            MouseButton::Right => editor.on_sheet_right_down(event, window, cx),
            _ => {}
        }
    }

    /// The sheet's keys (the arrows move the cell).
    fn workspace_key_context() -> &'static str {
        crate::shell::editor::SHEET_CONTEXT
    }

    /// The grid is the whole workspace: no shadow of a page.
    fn workspace_page_shadow() -> bool {
        false
    }

    fn infobar_kind(editor: &SlideEditor) -> Option<crate::shell::infobar::InfoBarKind> {
        editor.xlsx_infobar_kind()
    }

    fn infobar_action(editor: &mut SlideEditor, kind: crate::shell::infobar::InfoBarKind, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.xlsx_infobar_action(kind, window, cx);
    }

    /// The workbook's own words for 最終版.
    fn infobar_spec(kind: crate::shell::infobar::InfoBarKind) -> Option<crate::shell::infobar::InfoBarSpec> {
        use crate::shell::infobar::InfoBarKind;
        match kind {
            InfoBarKind::MarkAsFinal => Some(("xlsx-infobar-final", "infobar-btn-edit-anyway", "xlsx.infobar.final.title", "xlsx.infobar.final.message", "xlsx.infobar.final.edit-anyway")),
            _ => None,
        }
    }

    fn range_pick_active(editor: &SlideEditor) -> bool {
        editor.sheet_range_pick_active()
    }

    fn finish_range_pick(editor: &mut SlideEditor, accept: bool, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.finish_sheet_range_pick(accept, window, cx);
    }

    fn range_pick_panel(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        editor.generic_range_pick_panel(cx)
    }

    fn close_dialog(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.xlsx_close_dialog(window, cx)
    }

    fn discard_dialog(editor: &mut SlideEditor, key: &'static str) {
        editor.discard_xlsx_dialog(key);
    }

    fn keeps_sidebar_after_dialog(editor: &SlideEditor) -> bool {
        // The Draw panel stays on screen when a dialog (the settings) opened over it is closed: its pen is still the mode.
        editor.wb().panels.ink.is_some() || editor.sheet_state().is_some_and(|state| state.chart_edit.is_some()) && matches!(editor.dialog.key, Some("move_chart_panel" | "chart_template_panel"))
    }

    fn autocorrect_dialog_parent_open(editor: &SlideEditor) -> bool {
        editor.wb().panels.autocorrect.as_ref().is_some_and(|p| !p.entry_open)
    }

    fn autocorrect_panel_mut(editor: &mut SlideEditor) -> Option<&mut crate::shell::autocorrect::AutoCorrectPanel> {
        editor.wb_mut().panels.autocorrect.as_mut()
    }

    /// The file is written with where each sheet is scrolled to and what is selected in it (`xlsx/view_position.rs`).
    fn model_for_save(editor: &SlideEditor) -> std::borrow::Cow<'_, Presentation> {
        editor.workbook_for_save()
    }

    /// The workbook's theme (`xlsx/theme_panel.rs`).
    fn palette_theme_colors(editor: &SlideEditor) -> [ibis_doc_model::Rgb; 10] {
        editor.current_theme_palette_colors()
    }

    /// A worksheet's chart takes its data from the sheet's cells: no data grid of its own is kept for it.
    fn chart_data_slot(_editor: &mut SlideEditor) -> Option<&mut Option<crate::shell::chart_data::DataWindow>> {
        None
    }

    /// バックアップ コピーを作成する: the app's setting, or this workbook's own choice (`xlsx/backup_save_panel.rs`).
    #[cfg(not(target_family = "wasm"))]
    fn backup_before_write(editor: &SlideEditor, path: &std::path::Path) {
        let override_opt = editor.sheet_state().and_then(|s| s.always_create_backup);
        if let Ok(Some(backup_path)) = crate::shell::backup_save::create_backup_copy_if_enabled(path, override_opt) {
            log::info!("Created backup copy at: {}", backup_path.display());
        }
    }

    fn dialog_arrow_step(editor: &mut SlideEditor, step: i32, cx: &mut Context<SlideEditor>) -> bool {
        editor.cells_dialog_step(step, cx)
    }

    fn dialog_field_owns_enter(editor: &SlideEditor, window: &Window, cx: &App) -> bool {
        editor.import_text_path_focused(window, cx)
    }

    fn panel_message_shown(editor: &mut SlideEditor) {
        if let Some(s) = editor.sheet_state_mut() {
            s.validation_rejected = None;
            s.validation_message_bounds = None;
        }
    }

    fn panel_message_dismissed(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.xlsx_panel_message_dismissed(cx);
    }

    fn panel_message_origin(editor: &SlideEditor) -> Option<Point<Pixels>> {
        editor.xlsx_panel_message_origin()
    }

    fn panel_message_anchored(editor: &SlideEditor) -> bool {
        editor.xlsx_panel_message_anchored()
    }

    fn panel_message_measured(editor: &mut SlideEditor, bounds: Bounds<Pixels>, cx: &mut Context<SlideEditor>) {
        if let Some(s) = editor.sheet_state_mut() {
            if s.validation_rejected.is_some() && s.validation_message_bounds != Some(bounds) {
                s.validation_message_bounds = Some(bounds);
                cx.notify();
            }
        }
    }

    fn prepare_model(presentation: &mut Presentation, resources: &crate::render::shape_view::Resources, text_system: &TextSystem, _: Option<&str>, _: bool) {
        pivot::fit_built_pivots(presentation, resources, text_system);
        row_fit::fit_measured_rows(presentation, resources);
    }

    const PAGE_NOUN: &'static str = "_xlsx.sheet";
    const FIT_TOOLTIP: &'static str = "_xlsx.fit-sheet-to-window";
    const CAN_FIT: bool = false;

    /// The sidebar: the name manager when it is open.
    /// Opened with no panel chosen, the pane shows the selection's attributes (right_pane.rs).
    fn sidebar_on_request() -> bool {
        true
    }

    // Shown, the pane is for what is selected (the render's `sync_right_pane`).
    fn ink_panel_open(editor: &SlideEditor) -> bool {
        editor.sidebar_open && editor.sheet_state().is_some_and(|s| s.panels.ink.is_some())
    }

    fn close_ink_panel(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        if let Some(s) = editor.sheet_state_mut() {
            s.panels.ink = None;
        }
        cx.notify();
    }

    fn sidebar_toggled(editor: &mut SlideEditor) {
        if let Some(s) = editor.sheet_state_mut() {
            s.right_key = None;
        }
    }

    fn sidebar(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        let chain = |editor: &mut SlideEditor, cx: &mut Context<SlideEditor>| {
            editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel")).or_else(|| editor.custom_format_panel(cx)).or_else(|| editor.page_setup_panel(cx).map(|panel| panel.modal_named("page_setup_panel"))).or_else(|| editor.xlsx_print_panel(cx).map(|panel| panel.modal_named("xlsx_print_panel"))).or_else(|| editor.header_footer_edit_panel(cx)).or_else(|| editor.cell_link_panel(cx).map(|panel| panel.modal_named("cell_link_panel"))).or_else(|| editor.sheet_find_panel(cx)).or_else(|| editor.sheet_spelling_panel(cx).map(|panel| panel.modal_named("sheet_spelling_panel"))).or_else(|| editor.validation_panel(cx).map(|panel| panel.modal_named("validation_panel"))).or_else(|| editor.names_panel(cx)).or_else(|| editor.conditional_panel(cx).map(|panel|panel.modal_named(if editor.wb().conditional_edit.as_ref().is_some_and(|p|!p.manager&&p.parent.is_some()){"conditional_rule_editor"}else{"conditional_panel"}))).or_else(|| editor.sort_panel(cx).map(|panel| if editor.wb().sort_edit.as_ref().is_some_and(|p|p.editor.is_some()){panel.modal_named("sort_level_editor")}else{panel})).or_else(|| editor.edit_panel(cx)).or_else(|| editor.sheet_chart_panel(cx))
        };
        // The pane's input fields show the document's values (Undo, Redo, another cell, an external update).
        editor.sync_format_panel(window, cx);
        let panel = chain(editor, cx);
        // No tabs: the pane shows what is selected (right_pane.rs).
        if editor.sync_right_pane(panel.is_some(), window, cx) { chain(editor, cx) } else { panel }
    }

    /// The title row's 「デザイン モードを閉じる」 (the shell's red pill, as the slide master's) while the workbook is in design
    /// mode: a click leaves the mode.
    fn title_mode_button(editor: &SlideEditor, weak: &WeakEntity<SlideEditor>, _icon_only: bool) -> Option<AnyElement> {
        if !editor.is_design_mode_active() {
            return None;
        }
        let close: crate::shell::chrome::Handler = std::rc::Rc::new(|this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| {
            if this.is_design_mode_active() {
                this.toggle_design_mode(cx);
            }
        });
        Some(crate::shell::chrome::close_mode_button("xlsx-design-mode-close", "xlsx.design-mode.close", weak, &close))
    }

    /// The title bar's コメントを追加 (⌥⌘M): open the threaded comment panel on the selected cell.
    fn add_comment(_: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        Some(std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.open_threaded_comment_panel(window, cx)))
    }

    /// The title bar's コメントを表示: toggle the threaded comment panel.
    fn show_comments(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let shown = editor.sidebar_open && editor.sheet_state().is_some_and(|s| s.panels.threaded_comment.is_some());
        Some((
            shown,
            std::rc::Rc::new(move |this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| {
                if shown {
                    if let Some(state) = this.sheet_state_mut() {
                        state.panels.threaded_comment = None;
                    }
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                } else {
                    this.open_threaded_comment_panel(window, cx);
                }
            }),
        ))
    }

    /// Pasted shapes go to the selected cell; the same data pasted again steps from the last paste.
    fn place_pasted_shapes(editor: &mut SlideEditor, shapes: &mut [ibis_doc_model::Shape], stamp: u64) {
        editor.position_sheet_paste(shapes, stamp);
    }

    /// The Draw menu on a sheet: a pen opens the Ink panel with the pen chosen; the ink selected becomes a shape.
    fn draw_command(_: &SlideEditor, command: crate::shell::draw_menu::DrawCommand) -> Option<crate::shell::chrome::Handler> {
        use crate::shell::draw_menu::DrawCommand;
        let handler: crate::shell::chrome::Handler = match command {
            DrawCommand::Pen(pen) => std::rc::Rc::new(move |this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| {
                this.open_ink_panel(window, cx);
                this.ink_set_pen(pen, cx);
            }),
            DrawCommand::ToShape => std::rc::Rc::new(|this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.ink_to_shape(cx)),
        };
        Some(handler)
    }

    /// 書式のコピー/貼り付け: on while formats are picked up.
    fn format_paint(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let on = editor.sheet_state().is_some_and(|s| s.painter.is_some());
        Some((on, std::rc::Rc::new(|this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.toggle_format_paint(cx))))
    }

    /// Excel's status bar for a range: the average, the count of cells
    /// with something in them and the sum, of the cells shown.
    fn status_items(editor: &SlideEditor) -> Vec<SharedString> {
        editor.range_summary().map(|s| vec![s.into()]).unwrap_or_default()
    }

    /// The summary of the selection sits at the right, next to the zoom (Excel's).
    fn status_items_at_right() -> bool {
        true
    }

    /// A cell being typed into: Excel's 入力 (Enter) or 編集 (Edit), which the arrows tell apart.
    fn status_hint(editor: &SlideEditor) -> Option<SharedString> {
        let edit = editor.cell_edit()?;
        Some(crate::shell::i18n::text(match edit.mode {
            crate::shell::edit_mode::EditMode::Edit => "xlsx.status.mode-edit",
            _ => "xlsx.status.mode-enter",
        }))
    }

    /// The sheet tabs' scroll: the status bar gives it a scrollbar.
    fn status_tab_scroll(editor: &SlideEditor) -> Option<ScrollHandle> {
        editor.sheet_state().map(|state| state.tab_scroll.clone())
    }

    fn new_state(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState> {
        // The sheets' conditional formats, worked out ahead (they take the
        // longest of drawing a sheet the first time).
        let sheets = presentation.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some(sheet.clone()),
            _ => None,
        });
        conditional::apply_ahead(sheets.collect());
        Box::new(State { range_pick: None, range_pick_click_release: false, cells: HashMap::new(), cell_ends: HashMap::new(), cell_drag: false, drag_scroll: None, deselect_drag: None, split_drag: None, jump: sheet_ui::CellJump::new(window, cx), cell_edit: None, bar_bounds: Default::default(), bar_expanded: false, bar_visual_lines: Vec::new(), bar_caret_row:None, bar_shapes:Vec::new(), bar_scroll: ScrollHandle::new(), clip: None, copy_outline: false, header_geometry: Default::default(), header_drag: None, timeline_drag: None, control_drag: None, ruler_margin_drag: None, header_select: None, header_hover: None, header_popup: None, fill_source: None, cell_move: None, fill_copy: false, move_overwrite: None, tab_start: None, painter: None, names: None, validation: None, validation_warning: None, list_popup: None, validation_choices: None, validation_rejected: None, validation_message_bounds: None, filter_panel: None, comment_hover: None, note_card: Default::default(), comment_scroll: ScrollHandle::new(), comment_popup: Default::default(), table_dialog: None, chart_edit: None, last_fill: None, last_paste: None, find: None, spelling: None, link: None, page_setup: false, print_panel: false, header_footer_edit: None, custom_format: None, format_tab: 0, conditional_edit: None, sort_edit: None, panels: Default::default(), links_notice: crate::shell::settings_dialog::links_warning_enabled() && presentation.workbook.as_ref().is_some_and(|w| w.external_books.iter().any(|b| !b.broken && !b.name.is_empty())), border_line: ibis_doc_model::BorderLine::Thin, border_color: ibis_doc_model::Rgb::BLACK, tab_drag: None, tab_scroll: ScrollHandle::new(), tab_reveal: None, tab_last_click: None, right_key: None, pivot_pane: Default::default(), show_formula_bar: true, validation_circles: false, unlocked_ranges: HashSet::new(), trace_arrows: Vec::new(), calc_warmup_started: false, print_what: PrintWhat::ActiveSheet, always_create_backup: None, show_all_notes: false, design_mode: false, status_summary_mask: STATUS_SUMMARY_DEFAULT, sheet_scrolls: HashMap::new(), pending_view_scroll: None })
    }

    /// A worksheet at its own zoom, as Excel shows it.
    fn initial_zoom(editor: &SlideEditor) -> Option<f32> {
        Some(editor.current_sheet().map_or(1.0, |(_, sheet)| sheet_zoom(sheet)))
    }

    /// A worksheet sits in the top-left corner, after the row and column
    /// headers, as in Excel, and scrolls over its whole grid, past the
    /// cells it uses.
    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        let mut page = editor.presentation().size_of(editor.slide());
        let headings = editor.current_sheet().map_or(true, |(_, s)| s.headings);
        if let Some((shape, sheet)) = editor.current_sheet() {
            let default_row = if sheet.default_row > 0 { sheet.default_row } else { 20 * ibis_doc_model::EMU_PER_POINT };
            let default_col = if sheet.default_column > 0 { sheet.default_column } else { 54 * ibis_doc_model::EMU_PER_POINT };

            let scroll_offset = editor.workspace_scroll.offset();
            // Pixels per EMU, as `page_content` works it out (the zoom alone
            // is not it: the scroll was read as EMU, 1/12700 of what it is,
            // and the grid never grew as the wheel went down).
            let scale = editor.zoom.unwrap_or(1.0) * editor.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32;
            // The bottom and right edges of the view, in EMU from the sheet's origin.
            let scroll_emu_x = if scale > 0.0 { ((-f32::from(scroll_offset.x) + f32::from(workspace.width)) / scale).max(0.0) as i64 } else { 0 };
            let scroll_emu_y = if scale > 0.0 { ((-f32::from(scroll_offset.y) + f32::from(workspace.height)) / scale).max(0.0) as i64 } else { 0 };

            let (sel_emu_x, sel_emu_y) = if let Some(range) = editor.selected_range() {
                let xs = sheet.column_offsets();
                let ys = sheet.row_offsets();
                let x = if (range.last_column as usize) < xs.len() {
                    xs[range.last_column as usize]
                } else {
                    xs.last().copied().unwrap_or(0) + (range.last_column as usize - xs.len().saturating_sub(1)) as i64 * default_col
                };
                let y = if (range.last_row as usize) < ys.len() {
                    ys[range.last_row as usize]
                } else {
                    ys.last().copied().unwrap_or(0) + (range.last_row as usize - ys.len().saturating_sub(1)) as i64 * default_row
                };
                (x + default_col, y + default_row)
            } else {
                (0, 0)
            };

            let min_rows_buffer = 1_000 * default_row;
            let min_cols_buffer = 50 * default_col;

            // The grid reaches 1,000 rows (50 columns) past the bottom (right)
            // edge of the view, so the wheel and the scroll bar go on into
            // the rows below, up to Excel's last row (and column).
            let extra_h = min_rows_buffer
                .max(scroll_emu_y + 1_000 * default_row - shape.frame.bottom())
                .max(sel_emu_y + 100 * default_row - shape.frame.bottom());
            let extra_w = min_cols_buffer
                .max(scroll_emu_x + 50 * default_col - shape.frame.right())
                .max(sel_emu_x + 20 * default_col - shape.frame.right());

            let max_total_h = sheet_ui::MAX_SHEET_ROWS as i64 * default_row;
            let max_total_w = sheet_ui::MAX_SHEET_COLUMNS as i64 * default_col;

            let target_w = (shape.frame.right() + extra_w).min(shape.frame.x + max_total_w);
            let target_h = (shape.frame.bottom() + extra_h).min(shape.frame.y + max_total_h);

            page.width = page.width.max(target_w);
            page.height = page.height.max(target_h);
        }
        let before = if headings {
            let row_header_w = editor.current_sheet().map_or(sheet_ui::ROW_HEADER_WIDTH, |(_, s)| sheet_ui::row_header_width(s));
            point(row_header_w, sheet_ui::COLUMN_HEADER_HEIGHT)
        } else {
            point(0.0, 0.0)
        };
        // The page layout view: the papers instead of the grid.
        if let Some(content) = editor.page_layout_content(workspace) {
            return content;
        }
        editor.page_content(workspace, page, before, point(0.0, 0.0), true)
    }

    /// PageDown / PageUp scroll a screen (Control+PageDown changes sheets).
    fn step_page(editor: &mut SlideEditor, forward: bool, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.scroll_sheet_page(forward, cx);
        true
    }

    fn link_under_pointer(editor: &SlideEditor, position: Point<Pixels>) -> bool {
        editor.link_cell_under(position).is_some()
    }

    /// The context menus' state: the shapes selected on the sheet, and the table, the pivot table and the sparklines the
    /// selected cell is in.
    fn selection_context(editor: &SlideEditor, state: &mut crate::shell::menu_context::ContextState) {
        state.objects = editor.selected_shapes().into_iter().map(crate::shell::menu_context::ObjectKind::of).collect();
        state.ink_selected = editor.ink_is_selected();
        state.in_sheet_table = editor.table_at_selection().is_some();
        state.in_pivot = editor.pivot_at_selection().is_some();
        state.in_sparkline = editor.sparkline_group_at_selection().is_some();
    }

    /// A worksheet opens where it was left (its scroll; its selected cell, its split panes and its zoom are kept too), at the top-left corner
    /// the first time, at its own zoom.
    fn show_page(editor: &mut SlideEditor, slide: SlideId, _: &mut Window, _: &mut Context<SlideEditor>) -> bool {
        editor.set_current_slide(slide);
        editor.set_selection(Vec::new());
        if let Some(index) = editor.presentation().slide_index(slide) {
            editor.list_scroll.scroll_to_item(index);
        }
        true
    }

    /// The one place a worksheet's view is kept and put back (`SlideEditor::set_current_slide` calls it for every change of the active
    /// sheet): the scroll of the sheet left is saved under its `SlideId` (not its position, so deleting or moving sheets shifts nothing;
    /// a deleted sheet's is dropped), the new sheet's own zoom is shown and its scroll comes back, at the top-left corner the first time.
    fn page_changed(editor: &mut SlideEditor, from: SlideId, to: SlideId) {
        let offset = editor.workspace_scroll.offset();
        let from_exists = editor.presentation().slide(from).is_some();
        let remembered = editor.sheet_state_mut().and_then(|s| {
            if from_exists {
                s.sheet_scrolls.insert(from, offset);
            } else {
                s.sheet_scrolls.remove(&from);
            }
            s.sheet_scrolls.get(&to).copied()
        });
        if let Some((_, sheet)) = editor.current_sheet() {
            editor.zoom = Some(sheet_zoom(sheet));
        }
        editor.workspace_scroll.set_offset(remembered.unwrap_or_default());
        // Rows or columns may have been deleted since: the scroll is kept only as far as the sheet goes.
        editor.clamp_workspace_scroll();
    }

    fn menu_items(editor: &SlideEditor, part: crate::shell::chrome::MenuPart) -> Vec<crate::shell::chrome::MenuEntry> {
        menu_items::menu_items(editor, part)
    }

    fn toolbar_right(editor: &SlideEditor) -> Vec<crate::shell::chrome::ToolItem> {
        toolbar::toolbar_right(editor)
    }

    fn menus(editor: &SlideEditor, menus: &mut crate::shell::chrome::Menus) {
        toolbar::menus(editor, menus);
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn ignored_scenarios() -> Vec<(&'static str, fn())> {
        Vec::new()
    }

    // ----- Typing into cells (cell_edit.rs) or shapes (text_edit.rs) -------

    fn wants_text_input(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        if editor.editing.is_some() {
            return true;
        }
        editor.cells_take_typing(window, cx)
    }

    fn text_for_range(editor: &mut SlideEditor, range: std::ops::Range<usize>, adjusted: &mut Option<std::ops::Range<usize>>, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<String> {
        if editor.editing.is_some() {
            return editor.input_text_for_range(range, adjusted);
        }
        editor.cell_text_for_range(range)
    }

    fn selected_text_range(editor: &mut SlideEditor, _: bool, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        if editor.editing.is_some() {
            return editor.input_selected_range();
        }
        editor.cell_selected_range()
    }

    fn marked_text_range(editor: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<std::ops::Range<usize>> {
        if editor.editing.is_some() {
            return editor.input_marked_range();
        }
        editor.cell_marked_range()
    }

    fn unmark_text(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.editing.is_some() {
            editor.input_unmark(cx);
            return;
        }
        editor.cell_unmark();
    }

    fn replace_text_in_range(editor: &mut SlideEditor, range: Option<std::ops::Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.editing.is_some() {
            editor.input_replace(range, text, cx);
            return;
        }
        editor.cell_replace(range, text, None, cx);
    }

    fn replace_and_mark_text_in_range(editor: &mut SlideEditor, range: Option<std::ops::Range<usize>>, text: &str, selected: Option<std::ops::Range<usize>>, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.editing.is_some() {
            if let Some(range) = range {
                editor.input_replace(Some(range), "", cx);
            }
            editor.input_mark(text, cx);
            return;
        }
        editor.cell_replace(range, text, Some(selected), cx);
    }

    fn set_ime_clauses(editor: &mut SlideEditor, clauses: Vec<ImeClause>, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.editing.is_some() {
            editor.input_set_clauses(clauses, cx);
            return;
        }
        if let Some(edit) = editor.cell_edit_mut() {
            edit.marked_clauses = if edit.marked.is_some() { clauses } else { Vec::new() };
            cx.notify();
        }
    }

    fn bounds_for_range(editor: &mut SlideEditor, _: std::ops::Range<usize>, element: Bounds<Pixels>, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        if editor.editing.is_some() {
            return editor.caret_bounds();
        }
        editor.cell_bounds_for_range(element)
    }

    /// A character picked in the symbols panel goes into the cell being edited; the active cell's edit starts first when none is.
    fn insert_symbol(editor: &mut SlideEditor, c: char, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.cell_edit().is_none() {
            editor.begin_cell_edit(crate::shell::edit_mode::EditMode::Edit, true, cx);
        }
        editor.cell_replace(None, &c.to_string(), None, cx);
    }

    fn character_index_for_point(editor: &mut SlideEditor, point: Point<Pixels>, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<usize> {
        if editor.editing.is_some() {
            return editor.input_index_for_point(point);
        }
        None
    }

    /// After the first render completes, warm up the formula dependency index on a background thread.
    fn before_render(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.sheet_range_pick_active() { editor.clear_sheet_range_handle_bounds(); editor.update_sheet_range_pick(window, cx); }
        if let Some(state) = editor.sheet_state_mut() {
            if !state.calc_warmup_started {
                state.calc_warmup_started = true;
                let presentation = editor.presentation().clone();
                window.on_next_frame(move |_, _| {
                    ibis_doc_model::warmup_calc_index_ahead(presentation);
                });
            }
        }
    }

    /// Excel's layout: the formula bar over the sheet, the sheet tabs under
    /// it, and no page list.
    fn render_body(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        if editor.is_workbook_window_hidden() {
            return editor.render_hidden_workbook_view(window, cx);
        }
        // The クエリ エディター takes the place of the sheet while it is open.
        if let Some(view) = editor.render_query_editor(window, cx) {
            return view;
        }
        let show_formula_bar = editor.sheet_state().map_or(true, |s| s.show_formula_bar);
        let mut body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0();
        if let Some(bar) = editor.render_infobar(cx) {
            body = body.child(bar);
        }
        if show_formula_bar {
            body = body.child(editor.render_formula_bar(window, cx));
        }
        if let Some(banner) = editor.render_recovery_banner(cx) {
            body = body.child(banner);
        }
        if editor.is_repaired {
            body = body.child(editor.render_repair_banner(window, cx));
        }
        if let Some(banner) = editor.render_links_banner(cx) {
            body = body.child(banner);
        }
        // The data grid embedded in a chart's pane (its data lives on one sheet) shows no sheet tabs; a workbook
        // with several sheets (one that was read, whose series point into more than one) shows them, to reach and add sheets.
        let tabs = editor.shows_sheet_tabs();
        let body = body.child(div().flex().flex_row().flex_1().min_h_0().child(editor.render_workspace(window, cx)));
        if tabs {
            body.child(editor.render_sheet_tabs(cx)).into_any_element()
        } else {
            body.into_any_element()
        }
    }
}

/// A worksheet's own zoom, 1.0 being 100%.
fn sheet_zoom(sheet: &ibis_doc_model::Sheet) -> f32 {
    sheet.zoom.map_or(1.0, |z| z as f32 / 100.0)
}

#[ibis_office_macros::editor_ext(Xlsx2)]
impl SlideEditor {
    /// Whether the sheet tabs are shown: always, except in the data grid embedded in a chart's pane, where only a workbook of
    /// several sheets has them (a chart's data is on one sheet, and a new sheet would be out of its series' reach).
    pub fn shows_sheet_tabs(&self) -> bool {
        !self.grid_only || self.presentation().slides.len() > 1
    }

    /// The workbook's state. Only for code that runs with a workbook open
    /// (the workbook's views and actions); elsewhere use `sheet_state`.
    pub fn wb(&self) -> &State {
        self.sheet_state().expect("a workbook is open")
    }

    pub fn wb_mut(&mut self) -> &mut State {
        self.sheet_state_mut().expect("a workbook is open")
    }

    /// Scroll a worksheet by the workspace's height, down or up, moving the selected cell with the page.
    pub(crate) fn scroll_sheet_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        // Typing (Edit mode) into text of several lines: the caret goes a page of lines (issue #17).
        if self.cell_caret_page(forward, cx) {
            return;
        }
        let Some((page, step_rows, default_row)) = self.sheet_page_geometry() else { return };
        let Some(ws) = self.workspace_bounds else { return };
        let offset = self.workspace_scroll.offset();
        let scale = self.workspace_content().map_or(1.0, |c| c.scale);

        let (column, row) = self.current_cell().unwrap_or((0, 0));
        let new_row = if forward {
            (row + step_rows).min(sheet_ui::MAX_SHEET_ROWS - 1)
        } else {
            row.saturating_sub(step_rows)
        };
        self.set_selection(Vec::new());
        self.set_cell(column, new_row);

        let max_total_h_px = (sheet_ui::MAX_SHEET_ROWS as i64 * default_row) as f32 * scale;
        let max = (max_total_h_px - f32::from(ws.size.height)).max(0.0);
        let top = (-f32::from(offset.y) + if forward { page } else { -page }).clamp(0.0, max);
        self.workspace_scroll.set_offset(point(offset.x, px(-top)));
        self.scroll_cell_into_view();
        cx.notify();
    }

    /// The height of a screen of cells in pixels (less the column headers, which cover the top of the view), the
    /// number of default-height rows in it, and the default row height in EMU.
    fn sheet_page_geometry(&self) -> Option<(f32, u32, i64)> {
        let (content, ws) = (self.workspace_content()?, self.workspace_bounds?);
        let headings = self.current_sheet().map_or(true, |(_, s)| s.headings);
        let head_h = if headings { sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 };
        let page = (f32::from(ws.size.height) - head_h).max(1.0);
        let default_row = self.current_sheet().map_or(20 * ibis_doc_model::EMU_PER_POINT, |(_, s)| {
            if s.default_row > 0 { s.default_row } else { 20 * ibis_doc_model::EMU_PER_POINT }
        });
        let row_h_px = (default_row as f32 * content.scale).max(1.0);
        Some((page, ((page / row_h_px).round() as u32).max(1), default_row))
    }

    /// Shift+PageDown / Shift+PageUp: stretch the selected range by a screen, the end that moves staying in view.
    pub(crate) fn extend_sheet_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.editing.is_some() || self.cell_edit().is_some() {
            return;
        }
        let (Some((_, step_rows, _)), Some((column, row))) = (self.sheet_page_geometry(), self.current_cell()) else { return };
        let end_row = self.sheet_state().and_then(|s| s.cell_ends.get(&self.current_slide).copied()).map_or(row, |e| e.1);
        let new_row = if forward { (end_row + step_rows).min(sheet_ui::MAX_SHEET_ROWS - 1) } else { end_row.saturating_sub(step_rows) };
        let end_column = self.sheet_state().and_then(|s| s.cell_ends.get(&self.current_slide).copied()).map_or(column, |e| e.0);
        let slide = self.current_slide;
        let was_kept = self.selected_ranges().len() > 1;
        if (end_column, new_row) == (column, row) {
            self.wb_mut().cell_ends.remove(&slide);
        } else {
            self.wb_mut().cell_ends.insert(slide, (end_column, new_row));
        }
        self.extra_ranges_follow(was_kept);
        // A screen scrolls with the selection, and the end is then brought into view.
        if let (Some((page, _, _)), Some(ws)) = (self.sheet_page_geometry(), self.workspace_bounds) {
            let offset = self.workspace_scroll.offset();
            let (default_row, scale) = (self.current_sheet().map_or(20 * ibis_doc_model::EMU_PER_POINT, |(_, s)| s.default_row.max(1)), self.workspace_content().map_or(1.0, |c| c.scale));
            let max = ((sheet_ui::MAX_SHEET_ROWS as i64 * default_row) as f32 * scale - f32::from(ws.size.height)).max(0.0);
            let top = (-f32::from(offset.y) + if forward { page } else { -page }).clamp(0.0, max);
            self.workspace_scroll.set_offset(point(offset.x, px(-top)));
        }
        self.scroll_selection_end_into_view();
        cx.notify();
    }
}

pub(crate) mod formula_layout;
