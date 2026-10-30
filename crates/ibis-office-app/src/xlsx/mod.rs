// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Workbooks (Excel): cells, the formula bar, sheet tabs and headers, and
//! the workbook's editing — the spreadsheet担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

pub(crate) mod autosum;
pub(crate) mod cell_edit;
pub(crate) mod clipboard;
mod function_dialog;
mod thesaurus_panel;
mod word_count;
mod quick_analysis;
pub(crate) mod paste_special_dialog;
pub(crate) mod copy_picture;
pub(crate) mod names;
pub(crate) mod page_layout;
pub(crate) mod print;
pub(crate) mod validation;
pub(crate) mod conditional;
pub(crate) mod conditional_edit;
pub(crate) mod sort_edit;
pub(crate) mod dedupe_edit;
pub(crate) mod edit_panels;
pub(crate) mod phonetic;
pub(crate) mod spelling_panel;
pub(crate) mod error_check;
pub(crate) mod external_links;
pub(crate) mod export_xls;
pub(crate) mod import_text;
pub(crate) mod text_columns;
pub(crate) mod goto_special;
pub(crate) mod split_view;
pub(crate) mod cell_styles;
pub(crate) mod flash_fill;
pub(crate) mod notes;
pub(crate) mod password;
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
pub(crate) mod chart_template;
pub(crate) mod comment_card;
pub(crate) mod conditional_quick;
pub(crate) mod table_edit;
pub(crate) mod filter;
pub(crate) mod format_more;
pub(crate) mod autocomplete;
pub(crate) mod pivot;
pub(crate) mod row_fit;
pub(crate) mod table_design;
pub(crate) mod header_footer;
pub(crate) mod header_footer_edit;
pub(crate) mod find;
pub(crate) mod links;
pub(crate) mod headers;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/xlsx/scenarios.rs"]
pub(crate) mod scenarios;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "../tests/xlsx/corpus_report.rs"]
pub(crate) mod corpus_report;
pub(crate) mod sheet_ui;
mod sheet_tab_scroll;
mod right_pane;
pub(crate) mod sheet_view;
pub(crate) mod slicer;
pub(crate) mod slicer_view;
pub(crate) mod control_view;
pub(crate) mod control;
pub(crate) mod control_insert;
pub(crate) mod timeline_view;
pub(crate) mod timeline;
pub(crate) mod timeline_insert;
pub(crate) mod toolbar;
pub(crate) mod advanced_filter;
pub(crate) mod analysis_panel;
pub(crate) mod solver_panel;
pub(crate) mod autocorrect;
pub(crate) mod ink;
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
pub(crate) mod pivot_options;
pub(crate) mod trace;
pub(crate) mod formula_audit;
pub(crate) mod formula_reference;
pub(crate) mod properties_panel;
pub(crate) mod pivot_calc_field;
pub(crate) mod pivot_calc_item;
pub(crate) mod alt_text_panel;
pub(crate) mod shape_edit;
pub(crate) mod chart_insert;
pub(crate) mod threaded_comments;
pub(crate) mod accessibility_panel;
pub(crate) mod compatibility_panel;
pub(crate) mod selection_pane;
pub(crate) mod frame_panel;
pub(crate) mod theme_panel;
pub(crate) mod window_hide;
pub(crate) mod arrange_windows;
pub(crate) mod backup_save;
pub(crate) mod repair;
pub(crate) mod xml_maps;

pub(crate) use print::PrintWhat;

use std::collections::{HashMap, HashSet};

use gpui_kit::*;
use ibis_doc_model::{Presentation, SlideId};

use crate::shell::editor::{SlideEditor, WorkspaceContent};
use crate::shell::format::{FormatEditor, FormatState};

/// What only an open workbook has.
pub(crate) struct State {
    pub(crate) range_pick: Option<range_field::RangePick>,
    pub(crate) range_pick_click_release: bool,
    /// The selected cell of each worksheet, as (column, row) of the cell or
    /// of the top-left cell of its merged range. A sheet not in here has A1.
    pub(crate) cells: HashMap<SlideId, (u32, u32)>,
    /// The other corner of each worksheet's selected range, when more than
    /// the selected cell is selected (the selected cell is the anchor).
    pub(crate) cell_ends: HashMap<SlideId, (u32, u32)>,
    /// A range of cells is being selected by dragging.
    pub(crate) cell_drag: bool,
    /// Dragging with ⌘ held to deselect cells from the current selection.
    pub(crate) deselect_drag: Option<sheet_ui::DeselectDrag>,
    /// Dragging a split bar to change window split position.
    pub(crate) split_drag: Option<split_view::SplitDrag>,
    /// The name box and the ジャンプ dialog.
    pub(crate) jump: sheet_ui::CellJump,
    /// The cell being typed into.
    pub(crate) cell_edit: Option<cell_edit::CellEdit>,
    /// Where the formula bar's text field is (set when painted), to find the character pressed.
    pub(crate) bar_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The formula bar widened to several lines (the ▼ at its right end), as Excel's: all the lines of a cell are shown and typed into; closed, it shows one line.
    pub(crate) bar_expanded: bool,
    pub(crate) bar_visual_lines: Vec<(usize,String)>,
    pub(crate) bar_caret_row: Option<usize>,
    pub(crate) bar_shapes: Vec<std::rc::Rc<ShapedLine>>,
    /// The scrolling of the widened bar.
    pub(crate) bar_scroll: ScrollHandle,
    /// Cells copied or cut.
    pub(crate) clip: Option<clipboard::Clip>,
    /// Where the row and column headers were painted (for pressing them).
    pub(crate) header_geometry: headers::SharedGeometry,
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
    pub(crate) fill_source: Option<ibis_doc_model::CellRange>,
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
    pub(crate) names: Option<names::NamesPanel>,
    /// The data validation panel, when open in the sidebar.
    pub(crate) validation: Option<validation::ValidationPanel>,
    /// A pending Yes/No/Cancel confirmation for a `warning`-style data validation rule.
    pub(crate) validation_warning: Option<validation::ValidationWarning>,
    /// A list's drop-down, while open.
    pub(crate) list_popup: Option<validation::ListPopup>,
    pub(crate) validation_choices: Option<validation::ValidationChoices>,
    pub(crate) validation_rejected: Option<(u32, u32)>,
    pub(crate) validation_message_bounds: Option<Bounds<Pixels>>,
    /// A filter heading's ▼ panel, while open.
    pub(crate) filter_panel: Option<filter::FilterPanel>,
    /// The cell whose comment (thread or note) shows, the pointer on it.
    pub(crate) comment_hover: Option<(u32, u32)>,
    /// テーブルの作成, while open.
    pub(crate) table_dialog: Option<table_edit::TableDialog>,
    /// グラフの書式, when open in the sidebar.
    pub(crate) chart_edit: Option<chart_edit::ChartPanel>,
    /// The last fill (source, the cell dragged to, how), for オートフィル オプション.
    pub(crate) last_fill: Option<(ibis_doc_model::CellRange, u32, u32, fill::FillMode)>,
    /// The last paste of copied cells, for 貼り付けのオプション (`clipboard.rs`).
    pub(crate) last_paste: Option<clipboard::LastPaste>,
    /// 検索と置換, when open in the sidebar.
    pub(crate) find: Option<find::FindPanel>,
    /// スペル チェック, when open in the sidebar.
    pub(crate) spelling: Option<spelling_panel::SpellPanel>,
    /// The link panel, when open in the sidebar.
    pub(crate) link: Option<links::LinkPanel>,
    /// ページ設定, when open in the sidebar.
    pub(crate) page_setup: bool,
    /// カスタム ヘッダー・フッター編集, when open in the sidebar.
    pub(crate) header_footer_edit: Option<header_footer_edit::HeaderFooterEditPanel>,
    /// 表示形式（ユーザー定義）, when open in the sidebar.
    pub(crate) custom_format: Option<custom_format::FormatPanel>,
    /// Which sections of セルの書式設定 are open (表示形式・配置・フォント・罫線・塗りつぶし・保護), kept while the window lives.
    pub(crate) format_open: [bool; 6],
    pub(crate) conditional_edit: Option<conditional_edit::ConditionalPanel>,
    pub(crate) sort_edit: Option<sort_edit::SortPanel>,
    /// The panels of the editing basics (重複の削除, 区切り位置, …).
    pub(crate) panels: edit_panels::EditPanels,
    /// 起動時のリンクの確認: the bar that tells the workbook has links to other workbooks (their values are the
    /// cached ones until updated) shows until dismissed.
    pub(crate) links_notice: bool,
    pub(crate) border_line: ibis_doc_model::BorderLine,
    pub(crate) border_color: ibis_doc_model::Rgb,
    /// A sheet tab being dragged.
    pub(crate) tab_drag: Option<SlideId>,
    /// What the right pane was last made for (`right_pane.rs`).
    pub(crate) right_key: Option<right_pane::RightKey>,
    /// Scrolling of the sheet tabs along the bottom.
    pub(crate) tab_scroll: ScrollHandle,
    pub(crate) tab_reveal: Option<SlideId>,
    pub(crate) tab_last_click: Option<(SlideId, usize)>,
    /// Whether the formula bar is shown.
    pub(crate) show_formula_bar: bool,
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
    /// 選択中の図形・画像・グラフのID一覧（複数選択・グループ化・整列用）.
    pub(crate) shape_selection: Vec<ibis_doc_model::ShapeId>,
    /// このブックで常にバックアップを作成する (XLSX-FILE-019). None の場合は全体設定に従う.
    pub(crate) always_create_backup: Option<bool>,
    /// すべてのメモを表示する (XLSX-SEC-009 / Show All Notes).
    pub(crate) show_all_notes: bool,
    /// フォーム コントロールのデザイン モード (Design Mode).
    pub(crate) design_mode: bool,
    /// ステータス バーの集計表示ビットマスク (XLSX-VIEW-006: 平均, データの個数, 数値の個数, 最小値, 最大値, 合計).
    pub(crate) status_summary_mask: u8,
}

pub(crate) const STATUS_SUMMARY_AVG: u8 = 1 << 0;
pub(crate) const STATUS_SUMMARY_COUNT: u8 = 1 << 1;
pub(crate) const STATUS_SUMMARY_NUM_COUNT: u8 = 1 << 2;
pub(crate) const STATUS_SUMMARY_MIN: u8 = 1 << 3;
pub(crate) const STATUS_SUMMARY_MAX: u8 = 1 << 4;
pub(crate) const STATUS_SUMMARY_SUM: u8 = 1 << 5;
pub(crate) const STATUS_SUMMARY_DEFAULT: u8 = STATUS_SUMMARY_AVG | STATUS_SUMMARY_COUNT | STATUS_SUMMARY_SUM;

/// The workbook's keys (in the sheet's context).
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    use sheet_ui::*;
    let c = Some(crate::shell::editor::SHEET_CONTEXT);
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
pub(crate) struct Workbook;

impl FormatEditor for Workbook {
    const PAGE_NOUN: &'static str = "_xlsx.sheet";
    const FIT_TOOLTIP: &'static str = "_xlsx.fit-sheet-to-window";
    const CAN_FIT: bool = false;

    /// The sidebar: the name manager when it is open.
    /// Opened with no panel chosen, the pane shows the selection's attributes (right_pane.rs).
    fn sidebar_on_request() -> bool {
        true
    }

    fn sidebar(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        let chain = |editor: &mut SlideEditor, cx: &mut Context<SlideEditor>| {
            editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel")).or_else(|| editor.custom_format_panel(cx)).or_else(|| editor.page_setup_panel(cx).map(|panel| panel.modal_named("page_setup_panel"))).or_else(|| editor.header_footer_edit_panel(cx)).or_else(|| editor.cell_link_panel(cx).map(|panel| panel.modal_named("cell_link_panel"))).or_else(|| editor.sheet_find_panel(cx)).or_else(|| editor.sheet_spelling_panel(cx).map(|panel| panel.modal_named("sheet_spelling_panel"))).or_else(|| editor.validation_panel(cx).map(|panel| panel.modal_named("validation_panel"))).or_else(|| editor.names_panel(cx).map(|panel|panel.modal_named("names_panel"))).or_else(|| editor.conditional_panel(cx).map(|panel|panel.modal_named("conditional_panel"))).or_else(|| editor.sort_panel(cx).map(|panel| if editor.wb().sort_edit.as_ref().is_some_and(|p|p.editor.is_some()){panel.modal_named("sort_level_editor")}else{panel})).or_else(|| editor.edit_panel(cx)).or_else(|| editor.sheet_chart_panel(cx))
        };
        let panel = chain(editor, cx);
        // No tabs: the pane shows what is selected (right_pane.rs).
        if editor.sync_right_pane(panel.is_some(), window, cx) { chain(editor, cx) } else { panel }
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

    fn new_state(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> FormatState {
        // The sheets' conditional formats, worked out ahead (they take the
        // longest of drawing a sheet the first time).
        let sheets = presentation.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some(sheet.clone()),
            _ => None,
        });
        conditional::apply_ahead(sheets.collect());
        FormatState::Workbook(State { range_pick: None, range_pick_click_release: false, cells: HashMap::new(), cell_ends: HashMap::new(), cell_drag: false, deselect_drag: None, split_drag: None, jump: sheet_ui::CellJump::new(window, cx), cell_edit: None, bar_bounds: Default::default(), bar_expanded: false, bar_visual_lines: Vec::new(), bar_caret_row:None, bar_shapes:Vec::new(), bar_scroll: ScrollHandle::new(), clip: None, header_geometry: Default::default(), header_drag: None, timeline_drag: None, control_drag: None, ruler_margin_drag: None, header_select: None, header_hover: None, header_popup: None, fill_source: None, cell_move: None, fill_copy: false, move_overwrite: None, tab_start: None, painter: None, names: None, validation: None, validation_warning: None, list_popup: None, validation_choices: None, validation_rejected: None, validation_message_bounds: None, filter_panel: None, comment_hover: None, table_dialog: None, chart_edit: None, last_fill: None, last_paste: None, find: None, spelling: None, link: None, page_setup: false, header_footer_edit: None, custom_format: None, format_open: [true; 6], conditional_edit: None, sort_edit: None, panels: Default::default(), links_notice: crate::shell::settings_window::links_warning_enabled() && presentation.workbook.as_ref().is_some_and(|w| w.external_books.iter().any(|b| !b.broken && !b.name.is_empty())), border_line: ibis_doc_model::BorderLine::Thin, border_color: ibis_doc_model::Rgb::BLACK, tab_drag: None, tab_scroll: ScrollHandle::new(), tab_reveal: None, tab_last_click: None, right_key: None, show_formula_bar: true, validation_circles: false, unlocked_ranges: HashSet::new(), trace_arrows: Vec::new(), calc_warmup_started: false, print_what: PrintWhat::ActiveSheet, shape_selection: Vec::new(), always_create_backup: None, show_all_notes: false, design_mode: false, status_summary_mask: STATUS_SUMMARY_DEFAULT })
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

    /// A worksheet opens at its top-left corner, at its own zoom.
    fn show_page(editor: &mut SlideEditor, slide: SlideId, _: &mut Window, _: &mut Context<SlideEditor>) -> bool {
        if editor.current_slide != slide {
            editor.current_slide = slide;
            if let Some((_, sheet)) = editor.current_sheet() {
                editor.zoom = Some(sheet_zoom(sheet));
            }
            editor.workspace_scroll.set_offset(Point::default());
            editor.clamp_workspace_scroll();
        }
        editor.set_selection(Vec::new());
        if let Some(index) = editor.presentation().slide_index(slide) {
            editor.list_scroll.scroll_to_item(index);
        }
        true
    }

    fn toolbar_right(editor: &SlideEditor) -> Option<Vec<crate::shell::chrome::ToolItem>> {
        Some(toolbar::toolbar_right(editor))
    }

    fn menus(editor: &SlideEditor, menus: &mut crate::shell::chrome::Menus) {
        toolbar::menus(editor, menus);
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
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

    fn bounds_for_range(editor: &mut SlideEditor, _: std::ops::Range<usize>, element: Bounds<Pixels>, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        if editor.editing.is_some() {
            return editor.caret_bounds();
        }
        editor.cell_bounds_for_range(element)
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
        body
            .child(div().flex().flex_row().flex_1().min_h_0().child(editor.render_workspace(window, cx)))
            .child(editor.render_sheet_tabs(cx))
            .into_any_element()
    }
}

/// A worksheet's own zoom, 1.0 being 100%.
fn sheet_zoom(sheet: &ibis_doc_model::Sheet) -> f32 {
    sheet.zoom.map_or(1.0, |z| z as f32 / 100.0)
}

impl SlideEditor {
    /// The workbook's state. Only for code that runs with a workbook open
    /// (the workbook's views and actions); elsewhere use `sheet_state`.
    pub(crate) fn wb(&self) -> &State {
        self.sheet_state().expect("a workbook is open")
    }

    pub(crate) fn wb_mut(&mut self) -> &mut State {
        self.sheet_state_mut().expect("a workbook is open")
    }

    /// Scroll a worksheet by the workspace's height, down or up, moving the selected cell with the page.
    pub(crate) fn scroll_sheet_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        // Typing (Edit mode) into text of several lines: the caret goes a page of lines (issue #17).
        if self.cell_caret_page(forward, cx) {
            return;
        }
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let offset = self.workspace_scroll.offset();
        // Less the column headers, which cover the top of the view.
        let headings = self.current_sheet().map_or(true, |(_, s)| s.headings);
        let head_h = if headings { sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 };
        let page = (f32::from(ws.size.height) - head_h).max(1.0);
        let scale = content.scale;
        let default_row = self.current_sheet().map_or(20 * ibis_doc_model::EMU_PER_POINT, |(_, s)| {
            if s.default_row > 0 { s.default_row } else { 20 * ibis_doc_model::EMU_PER_POINT }
        });
        let row_h_px = (default_row as f32 * scale).max(1.0);
        let step_rows = ((page / row_h_px).round() as u32).max(1);

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
}

pub(crate) mod formula_layout;
