// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The Excel-like parts of the window for a workbook: the formula bar with
//! the name box, the ジャンプ dialog, the row and column headers, the
//! selected cell and the sheet tabs, and moving about the cells.
//!
//! They are drawn in the editor only; the worksheet itself (and what is
//! exported) is drawn by `sheet_view.rs`.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CellRange, SlideId, cell_name, sheet_command::Axis};

use crate::shell::icons::Material;
use super::sheet_tab_scroll::{TAB_BUTTON_WIDTH, reveal_offset};
use crate::shell::editor::{SlideEditor, SlideLayout, Tool, WorkspaceContent, sheet_of};

/// Dragging with ⌘ held to deselect (unselect) a rectangle of cells.
#[derive(Clone, Debug)]
pub(crate) struct DeselectDrag {
    pub(crate) anchor: (u32, u32),
    pub(crate) base_ranges: Vec<CellRange>,
}

actions!(
    ibis_office,
    [
        /// Open the ジャンプ dialog (Excel's Go To).
        GoTo,
        /// Open テーブルの作成 (Excel's ⌘T).
        InsertTable,
        /// Select the cell at the edge of the data region above (Excel's Ctrl+↑).
        JumpCellUp,
        /// Select the cell at the edge of the data region below.
        JumpCellDown,
        /// Select the cell at the edge of the data region to the left.
        JumpCellLeft,
        /// Select the cell at the edge of the data region to the right.
        JumpCellRight,
        /// Stretch the selected range to the edge of the data region above.
        ExtendJumpUp,
        /// Stretch the selected range to the edge of the data region below.
        ExtendJumpDown,
        /// Stretch the selected range to the edge of the data region to the left.
        ExtendJumpLeft,
        /// Stretch the selected range to the edge of the data region to the right.
        ExtendJumpRight,
        /// Select the cell in column A of the selected row (Excel's Home).
        SelectRowStart,
        SelectRowEnd,
        ExtendRowEnd,
        /// Stretch the selected range to column A.
        ExtendRowStart,
        /// Select A1, or the top-left cell past frozen panes (Excel's Ctrl+Home).
        SelectFirstCell,
        /// Stretch the selected range to that cell.
        ExtendFirstCell,
        /// Select the last cell in use (Excel's Ctrl+End).
        SelectLastCell,
        /// Stretch the selected range to the last cell in use.
        ExtendLastCell,
        /// Edit the selected cell's content (Excel's F2).
        EditCell,
        /// Backspace: a character while typing, else the selected cells' values.
        CellBackspace,
        /// Delete: a character while typing, else the selected cells' values.
        CellDelete,
        /// Bold on or off for the selected cells (⌘B).
        CellBold,
        /// Italic on or off (⌘I).
        CellItalic,
        /// Underline on or off (⌘U).
        CellUnderline,
        /// Copy the selected cells (⌘C).
        CellCopy,
        /// Cut the selected cells (⌘X).
        CellCut,
        /// Paste at the selected cell (⌘V).
        CellPaste,
        /// Paste values only (⇧⌘V).
        CellPastePlain,
        /// Open the Paste Special dialog (⌘⌥V / ⌃⌥V).
        CellPasteSpecialDialog,
        /// Take the formats off the selected cells (⌘\\).
        ClearCellFormats,
        /// Tab: finish the entry and go right (remembering where the Tabs began).
        CellTab,
        /// Select the whole sheet (⌘A).
        SelectAllCells,
        /// Open the link panel (⌘K).
        InsertLink,
        /// F4: cycle the reference while typing a formula.
        CycleReference,
        /// Enter: finish the entry and go down (to where the Tabs began, after Tabs).
        CellEnter,
        /// ⌘1 (⌃1): the cell format settings (セルの書式設定).
        OpenCellFormat,
        /// ⌃⇧L (⌘⇧F on a Mac): the filter ▼ on or off.
        ToggleFilter,
        /// ⌘D (⌃D): copy the selection's first row down over it.
        FillDown,
        /// ⌘R (⌃R): copy the selection's first column right over it.
        FillRight,
        /// Copy the selection's last row up over it.
        FillUp,
        /// Copy the selection's last column left over it.
        FillLeft,
        /// ⌃; today's date into the cell (Excel's current date).
        InsertDate,
        /// ⌘; (⌃⇧; on Windows) the time now into the cell.
        InsertTime,
        /// ⌃Space: the columns of the selection, whole.
        SelectColumns,
        /// ⇧Space: the rows of the selection, whole.
        SelectRows,
        /// ⌃Enter: put the entry into every selected cell (and the ranges
        /// 条件を選択してジャンプ selected with them), staying put.
        CellFillSelection,
        /// ⌥Enter (Excel's Alt+Enter): a line break in what is being typed.
        CellLineBreak,
        /// ⌥↓ (Excel's Alt+↓): Pick from Drop-down List (or validation list).
        PickFromList,
        /// ⌃E: フラッシュ フィル.
        FlashFill,
        /// Hide the selected rows (⌃9), as Excel.
        HideRows,
        /// Hide the selected columns (⌃0).
        HideColumns,
        /// Show the hidden rows in the selection (⌃⇧9).
        UnhideRows,
        /// Show the hidden columns in the selection (⌃⇧0).
        UnhideColumns,
        /// F9: calculate all formula cells and data tables in the workbook.
        CalculateNow,
        /// Shift+F9: calculate active worksheet.
        CalculateSheet,
        /// ⌃` (⌘`): toggle show formulas in sheet cells.
        ToggleShowFormulas,
        /// ⌥⌘L (⌃⌥L): reapply active autofilter or table filter.
        ReapplyFilter,
        /// ⇧F2: insert or edit note (新しいメモ / メモの編集).
        EditNote,
        /// ⇧F3: insert function (関数の挿入).
        InsertFunction,
        /// Go to previous comment or note on active sheet.
        PreviousComment,
        /// Go to next comment or note on active sheet.
        NextComment,
        /// Toggle show all notes.
        ToggleShowAllNotes,
        /// ⌘+ (⌃+): insert cells/rows/columns according to selection.
        SheetInsert,
        /// ⌘- (⌃-): delete cells/rows/columns according to selection.
        SheetDelete,
    ]
);

/// Height of the column headers (A, B, C…), in pixels.
pub(crate) const COLUMN_HEADER_HEIGHT: f32 = 20.0;
/// Width of the row headers (1, 2, 3…), in pixels (base width for up to 4 digits).
pub(crate) const ROW_HEADER_WIDTH: f32 = 44.0;
/// Excel's maximum row count per worksheet (1-indexed: 1..=1,048,576; 0-indexed: 0..1,048,576).
pub(crate) const MAX_SHEET_ROWS: u32 = 1_048_576;
/// Excel's maximum column count per worksheet (1-indexed: 1..=16,384; 0-indexed: 0..16,384).
pub(crate) const MAX_SHEET_COLUMNS: u32 = 16_384;

/// Width of the row headers dynamically sized based on sheet row count digits (Excel specification).
pub(crate) fn row_header_width(sheet: &ibis_doc_model::Sheet) -> f32 {
    let max_row = sheet.rows.len().max(sheet.row_offsets().len().saturating_sub(1));
    row_header_width_for_rows(max_row)
}

pub(crate) fn row_header_width_for_rows(max_row: usize) -> f32 {
    let digits = max_row.max(1).to_string().len().max(4);
    // 4 digits (up to 9,999): base 44.0 px
    // Each additional digit adds 8.0 px (character width ~7.0px at 11pt + margin)
    // 5 digits (10k+): 52.0px, 6 digits (100k+): 60.0px, 7 digits (1M+, e.g. 1,048,576): 68.0px
    ROW_HEADER_WIDTH + (digits.saturating_sub(4) as f32) * 8.0
}
/// Room kept for the workspace's scrollbars when a cell is scrolled into view.
pub(super) const SCROLLBAR_ROOM: f32 = 14.0;

/// The tint over the selected range (Excel's translucent grey).
const RANGE_TINT: u32 = 0x2020201a;
/// Excel's green, for the selected cell and sheet tab.
pub(crate) const SELECTION_COLOR: u32 = 0x107c41;
/// The line along the edge of frozen panes.
pub(crate) const FROZEN_LINE: u32 = 0x9b9b9b;
/// Page break preview: the grey over the cells outside the print area (a
/// translucent grey, so their contents still show, as in Excel) and the
/// blue line around the print area.
const OUTSIDE_PRINT_AREA: u32 = 0x5f5f5f99;
const PRINT_AREA_LINE: u32 = 0x2f5ce0;
/// Blue border around a dynamic array spill range (solid when active, dashed on `#SPILL!`).
pub(crate) const SPILL_BORDER_COLOR: u32 = 0x2f5ce0;
/// Width of the line around the print area, in pixels.
const PRINT_AREA_LINE_WIDTH: f32 = 2.0;

/// If `(row, column)` is a non-anchor destination cell inside an active (unblocked) dynamic array
/// spill range on `sheet`, returns the anchor cell's formula string (with leading `=`) to display
/// as a gray ghost formula in the formula bar (#122 / issue #24).
pub(crate) fn spilled_ghost_formula_on(sheet: &ibis_doc_model::Sheet, row: u32, column: u32) -> Option<String> {
    if sheet.cell(row, column).is_some_and(|c| c.formula.is_some()) {
        return None;
    }
    for (&anchor, &range) in &sheet.spills {
        if anchor == (row, column) {
            continue;
        }
        if (range.first_row..=range.last_row).contains(&row) && (range.first_column..=range.last_column).contains(&column) {
            let anchor_cell = sheet.cell(anchor.0, anchor.1)?;
            if anchor_cell.value == ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill) {
                continue;
            }
            let formula = anchor_cell.formula.as_deref()?;
            return Some(format!("={}", sheet.formula_shown(anchor.0, anchor.1, formula)));
        }
    }
    None
}

/// Returns the active dynamic array spill outlines `(range, is_blocked)` to draw for the current selection (#122 / issue #24):
/// - `(range, false)` when the selection enters/intersects an unblocked multi-cell spill range (drawn with a thin solid blue border + outer drop shadow).
/// - `(range, true)` when a `#SPILL!` cell is selected (drawn with a thin dashed blue border + outer drop shadow around the intended spill range).
pub(crate) fn active_spill_outlines_on(
    presentation: &ibis_doc_model::Presentation,
    slide_id: SlideId,
    sheet: &ibis_doc_model::Sheet,
    current_cell: Option<(u32, u32)>,
    selected_ranges: &[CellRange],
) -> Vec<(CellRange, bool)> {
    let mut out = Vec::new();
    let range_intersects = |r: CellRange| {
        current_cell.is_some_and(|(col, row)| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&col))
            || selected_ranges.iter().any(|sel| {
                sel.first_row <= r.last_row
                    && sel.last_row >= r.first_row
                    && sel.first_column <= r.last_column
                    && sel.last_column >= r.first_column
            })
    };
    let cell_selected = |row: u32, col: u32| {
        current_cell == Some((col, row))
            || selected_ranges.iter().any(|sel| (sel.first_row..=sel.last_row).contains(&row) && (sel.first_column..=sel.last_column).contains(&col))
    };

    // 1. Unblocked multi-cell spill ranges whose area intersects the current selection.
    for (&anchor, &range) in &sheet.spills {
        let is_blocked = sheet
            .cell(anchor.0, anchor.1)
            .is_some_and(|c| c.value == ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill));
        if !is_blocked && (range.last_row > range.first_row || range.last_column > range.first_column) && range_intersects(range) {
            out.push((range, false));
        }
    }

    // 2. Blocked (#SPILL!) formula cells currently selected: show their intended spill range with a dashed border.
    for cell in &sheet.cells {
        if cell.value != ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill) {
            continue;
        }
        if !cell_selected(cell.row, cell.column) {
            continue;
        }
        let stored = sheet.spills.get(&(cell.row, cell.column)).copied();
        let intended = match stored {
            Some(r) if r.last_row > r.first_row || r.last_column > r.first_column => Some(r),
            _ if cell.formula.is_some() => {
                let view = ibis_doc_model::sheet_calc::BookView::new(presentation);
                view.index_of_slide(slide_id)
                    .and_then(|sheet_idx| view.calculate_only(&[(sheet_idx, cell.row, cell.column)]).into_iter().next())
                    .and_then(|res| res.spill)
                    .map(|(rows, cols, _)| CellRange {
                        first_row: cell.row,
                        first_column: cell.column,
                        last_row: (cell.row + (rows as u32).saturating_sub(1)).min(MAX_SHEET_ROWS - 1),
                        last_column: (cell.column + (cols as u32).saturating_sub(1)).min(MAX_SHEET_COLUMNS - 1),
                    })
                    .or(stored)
            }
            other => other,
        };
        if let Some(r) = intended {
            out.push((r, true));
        }
    }
    out
}
/// The headers' background, lines and labels.
const HEADER_BACKGROUND: u32 = 0xf3f3f3;
const HEADER_LINE: u32 = 0xd4d4d4;
const HEADER_TEXT: u32 = 0x444444;
/// The headers of the selected rows and columns.
const HEADER_SELECTED: u32 = 0xd3e7d9;
/// Size of the labels in the headers, in pixels.
const HEADER_FONT_SIZE: f32 = 11.0;

/// The letters of a column, such as `AB`.
pub(crate) fn column_letters(column: u32) -> String {
    cell_name(column, 0).trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
}

use super::cell_edit::BAR_LINE_HEIGHT;

/// The height of the formula bar's text field when widened: four lines.
pub(crate) const BAR_EXPANDED_HEIGHT: f32 = BAR_LINE_HEIGHT * 4.0 + 4.0;

/// The lines of the formula bar's text: widened, in a column that scrolls; closed, the one line.
fn bar_text(rows: Vec<impl IntoElement>, expanded: bool, scroll: Option<&ScrollHandle>) -> AnyElement {
    if !expanded {
        return h_flex().truncate().children(rows).into_any_element();
    }
    let mut column = div().id("formula-bar-text").flex().flex_col().flex_1().min_w_0().h_full().overflow_y_scroll().children(rows);
    if let Some(handle) = scroll {
        column = column.track_scroll(handle);
    }
    column.into_any_element()
}

/// Build the searchable menu entries for the bottom-left sheet list button (#98).
pub(crate) fn sheet_list_menu_entries(editor: &SlideEditor) -> Vec<crate::shell::chrome::MenuEntry> {
    let wb_sheets = editor.presentation().workbook.as_ref().map(|w| &w.sheets);
    editor
        .presentation()
        .slides
        .iter()
        .enumerate()
        .filter(|(_, slide)| {
            wb_sheets.is_none_or(|ws| ws.iter().find(|s| s.slide == Some(slide.id)).is_none_or(|s| s.hidden.is_none()))
        })
        .map(|(index, slide)| {
            let id = slide.id;
            let current = id == editor.current_slide;
            let name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", index + 1));
            let tab_color = sheet_of(slide).and_then(|(_, sheet)| sheet.tab_color);
            crate::shell::chrome::MenuItem::new("xlsx.sheet-list.item", name)
                .checked(current)
                .swatch_opt(tab_color)
                .on(move |this, window, cx| {
                    this.select_slide(id, window, cx);
                    this.focus_handle.focus(window, cx);
                    this.request_sheet_tab_reveal(id, cx);
                })
                .into()
        })
        .collect()
}

impl SlideEditor {
    /// The ▼ of the formula bar: widen it to several lines, or close it again.
    pub(crate) fn toggle_formula_bar_expanded(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.bar_expanded = !state.bar_expanded;
            state.bar_scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    /// 関数の挿入 (fx / ⇧F3 / xlsx.insert-function): the panel to choose a function and fill in its arguments
    /// (`function_dialog.rs`).
    pub(crate) fn insert_function_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_function_panel(window, cx);
    }

    /// Insert a function call (e.g. `SUM(`) into the active cell:
    /// - If not currently editing, starts editing with `={name}(`
    /// - If editing, appends or inserts `{name}(` at caret
    pub(crate) fn insert_function_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.cell_edit().is_none() && self.selection.is_none() && self.current_cell().is_some() {
            self.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx);
        }
        if let Some(edit) = self.cell_edit_mut() {
            let func_call = format!("{name}(");
            if edit.text.is_empty() {
                edit.text = format!("={func_call}");
            } else if !edit.text.starts_with('=') {
                edit.text.insert(0, '=');
                edit.text.push_str(&func_call);
            } else {
                let caret = edit.caret.min(edit.text.encode_utf16().count());
                let b = super::cell_edit::byte_at(&edit.text, caret);
                edit.text.insert_str(b, &func_call);
            }
            edit.caret = edit.text.encode_utf16().count();
            edit.anchor = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The name box and the content of the selected cell: its formula
    /// (with `=`) if it has one, otherwise its value as displayed. The
    /// content is read-only; a reference typed in the name box is selected.
    pub(crate) fn render_formula_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_name_box(window, cx);
        if self.wb().jump.pending_scroll && self.workspace_bounds.is_some() {
            // Jumped before the workspace was laid out.
            self.wb_mut().jump.pending_scroll = false;
            self.scroll_range_into_view(true);
        }
        let theme = cx.theme();
        let editing_text = self.cell_edit().filter(|e| e.slide == self.current_slide).map(|e| e.text.clone());
        // While typed into, the bar shows the caret and the selection as the cell does.
        let bar_rect = self.sheet_state().map(|s| s.bar_bounds.clone());
        let expanded = self.sheet_state().is_some_and(|s| s.bar_expanded);
        let bar_scroll = self.sheet_state().map(|s| s.bar_scroll.clone());
        let bar_height=self.formula_bar_height(window);
        let width=self.wb().bar_bounds.get().map_or((f32::from(window.viewport_size().width)-210.).max(1.),|b|(f32::from(b.size.width)-20.).max(1.));
        let editing_lines: Option<Vec<(usize,String)>>=editing_text.as_ref().map(|text|super::formula_layout::visual_lines(text,width,window));
        if let Some(lines)=&editing_lines {self.wb_mut().bar_visual_lines=lines.clone();self.wb_mut().bar_shapes=lines.iter().map(|(_,l)|std::rc::Rc::new(super::formula_layout::shape_bar_line(l,window))).collect();}
        let editing_bar_lines=self.cell_edit().zip(editing_lines.as_ref()).map(|(edit,lines)|{
            let row=self.wb().bar_caret_row.filter(|&i|lines.get(i).is_some_and(|(a,l)|(*a..=*a+l.encode_utf16().count()).contains(&edit.caret))).unwrap_or_else(||lines.iter().rposition(|(start,_)|*start<=edit.caret).unwrap_or(0));
            if expanded {if let Some(handle)=&bar_scroll {let top=row as f32*BAR_LINE_HEIGHT;let seen=-f32::from(handle.offset().y);let shown=bar_height-4.;if top<seen{handle.set_offset(point(px(0.),px(-top)));}else if top+BAR_LINE_HEIGHT>seen+shown{handle.set_offset(point(px(0.),px(-(top+BAR_LINE_HEIGHT-shown))));}}}
            lines.iter().enumerate().filter(|(i,_)|expanded||*i==row).map(|(i, (a,l))|{let mut line=edit.bar_line(*a,l);if i!=row{line.caret_byte=None;}line}).collect::<Vec<_>>()
        });
        let ghost_formula = (self.selection.is_none() && editing_text.is_none() && !self.formula_hidden_here())
            .then(|| self.current_sheet().zip(self.current_cell()).and_then(|((_, sheet), (column, row))| spilled_ghost_formula_on(sheet, row, column)))
            .flatten();
        let is_ghost_formula = ghost_formula.is_some();
        let content = match (self.selection, self.current_sheet(), self.current_cell()) {
            _ if editing_text.is_some() => editing_text.clone().unwrap_or_default(),
            // 数式を表示しない on a protected sheet: nothing.
            (None, Some(_), Some(_)) if self.formula_hidden_here() => String::new(),
            _ if ghost_formula.is_some() => ghost_formula.unwrap().replace("\r\n", "\n").replace('\r', "\n"),
            // A cell of a data table shows its TABLE formula, as Excel's bar does.
            (None, Some((_, sheet)), Some((column, row))) if sheet.data_tables.iter().any(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column)) => {
                let t = sheet.data_tables.iter().find(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column)).unwrap();
                let name = |c: Option<(u32, u32)>| c.map(|(r, c)| ibis_doc_model::cell_name(c, r)).unwrap_or_default();
                format!("{{=TABLE({},{})}}", name(t.row_input), name(t.column_input))
            }
            (None, Some((_, sheet)), Some((column, row))) => {
                let content = sheet.cell(row, column).map(|cell| match &cell.formula {
                    Some(formula) => format!("={}", sheet.formula_shown(row, column, formula)),
                    // A number as it is (a date or time as shown), as Excel's bar shows it.
                    None => match &cell.value {
                        ibis_doc_model::CellValue::Number(n) if !ibis_calc::numfmt::parsed(&sheet.styles[cell.style as usize].number_format).first().is_some_and(|s| s.is_date()) => ibis_calc::eval::number_to_text(*n),
                        _ => cell.text.clone(),
                    },
                });
                // Line breaks stay: closed, the bar shows the first line; widened, all of them.
                content.unwrap_or_default().replace("\r\n", "\n").replace('\r', "\n")
            }
            (Some(shape_id), _, _) => {
                let formula = self.slide().shape_formulas.get(&shape_id);
                match formula {
                    Some(f) => if f.starts_with('=') { f.clone() } else { format!("={f}") },
                    None => String::new(),
                }
            }
            _ if !self.selected_shapes().is_empty() => {
                let shapes = self.selected_shapes();
                let formula = shapes.first().and_then(|s| self.slide().shape_formulas.get(&s.id));
                match formula {
                    Some(f) => if f.starts_with('=') { f.clone() } else { format!("={f}") },
                    None => String::new(),
                }
            }
            _ => String::new(),
        };
        if editing_text.is_none(){let lines=super::formula_layout::visual_lines(&content,width,window);self.wb_mut().bar_shapes=lines.iter().map(|(_,l)|std::rc::Rc::new(super::formula_layout::shape_bar_line(l,window))).collect();self.wb_mut().bar_visual_lines=lines;}
        let field = || {
            div().cursor_text()
                .h(px(if expanded { bar_height } else { 22. }))
                .px_2()
                .flex()
                .when(!expanded, |d| d.items_center())
                .border_1()
                .border_color(theme.border)
                .rounded_sm()
                .bg(theme.background)
        };
        let bar=h_flex()
            .h(px(if expanded { bar_height + 8. } else { 30. }))
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .when(!expanded, |d| d.items_center())
            .when(expanded, |d| d.items_start().py(px(4.)))
            .text_sm()
            .bg(theme.title_bar)
            .border_b_1()
            .border_color(theme.border)
            .child({
                let arrow_color = theme.muted_foreground;
                div()
                    .relative()
                    .w(px(110.))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w_full()
                            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                                this.wb_mut().jump.select_on_release = !this.wb().jump.name_box.read(cx).focus_handle(cx).is_focused(window);
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    if std::mem::take(&mut this.wb_mut().jump.select_on_release) {
                                        this.wb().jump.name_box.update(cx, |input, cx| input.select_all(window, cx));
                                    }
                                }),
                            )
                            .child(Input::new(&self.wb().jump.name_box).small()),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(2.))
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .child(
                                {
                                    let current=self.name_box_text();
                                    let entries=self.name_box_defined_names().into_iter().map(|name| {
                                        let value=name.clone();
                                        crate::shell::chrome::MenuItem::new("xlsx.name-box-item",name.clone()).checked(name.eq_ignore_ascii_case(&current))
                                            .on(move|e,w,cx| {
                                                if e.go_to_reference(&value,w,cx) {
                                                    e.wb().jump.name_box.update(cx,|input,cx|input.set_value(value.clone(),w,cx));
                                                    e.wb_mut().jump.name_box_shown=value.clone();e.focus_handle.focus(w,cx);
                                                } else {e.status=Some(crate::shell::i18n::format("_xlsx.invalid-reference-text",&[("text",&value)]).into());}
                                                cx.notify();
                                            }).into()
                                    }).collect();
                                    crate::shell::chrome::Dropdown::with_button("name-box-dropdown",
                                        Button::new("name-box-dropdown-btn").ghost().xsmall().w(px(18.)).h(px(20.)).px(px(0.)).rounded(px(3.))
                                            .child(crate::shell::chrome::icon("arrow_drop_down",16.,arrow_color)),entries,true,0,&cx.weak_entity()).offset_x(px(18.+2.-110.)).into_any_element()
                                },
                            ),
                    )
            })
            .child(
                div()
                    .id("fx-button")
                    .px_1()
                    .italic()
                    .text_color(theme.muted_foreground)
                    .hover(|d| d.text_color(theme.foreground).bg(theme.accent))
                    .rounded_sm()
                    .cursor_pointer()
                    .child("fx")
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.insert_function_action(window, cx);
                    })),
            )
            .child(
                field()
                    .border_color(if editing_text.is_some()&&self.focus_handle.is_focused(window){crate::shell::palette::rgb(crate::shell::chrome::PRIMARY_BUTTON_BG).into()}else{theme.border})
                    .flex_1()
                    .min_w_0()
                    // A press on the bar types into the selected cell, as in Excel (the keys go to the cell).
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, window, cx| {
                            if this.cell_edit().is_none() && this.selection.is_none() && this.current_cell().is_some() {
                                this.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx);
                                if let Some(edit)=this.cell_edit_mut(){edit.caret=0;}
                                this.wb_mut().bar_caret_row=None;
                            }
                            // The caret goes where the bar was pressed.
                            if this.cell_edit().is_some() {
                                this.press_in_bar_text(e.position, e.click_count, e.modifiers.shift, window, cx);
                            }
                            this.focus_handle.focus(window, cx);
                        }),
                    )
                    .when_some(bar_rect, |d, rect| d.child(canvas(|_, _, _| {}, move |bounds, _, w, _| {if rect.replace(Some(bounds))!=Some(bounds){w.refresh();}}).absolute().size_full()))
                    .relative()
                    .overflow_hidden()
                    .child(match editing_bar_lines {
                        Some(lines) => {
                            let fg = theme.foreground;
                            let bar_focus=self.focus_handle.clone();
                            let rows = lines.into_iter().map(|bar_line| {
                                let text = bar_line.text;
                                let bar_focus=bar_focus.clone();
                                let sel = bar_line.selection_bytes;
                                let caret = bar_line.caret_byte;
                                let overlay_text = text.clone();
                                h_flex()
                                    .relative()
                                    .h(px(BAR_LINE_HEIGHT))
                                    .w_full().min_w_0()
                                    .flex_shrink_0()
                                    .items_center()
                                    .whitespace_nowrap()
                                    // The caret at the end of the text sits on the row's right edge, where it would be clipped.
                                    .child(div().pr(px(2.)).child(SharedString::from(text)))
                                    .when(sel.is_some() || caret.is_some(), |d| {
                                        d.child(
                                            canvas(
                                                |_, _, _| {},
                                                move |bounds, _, window, _cx| {
                                                    let style = window.text_style();
                                                    let size = window.rem_size() * 0.875;
                                                    let shaped = if overlay_text.is_empty() {
                                                        None
                                                    } else {
                                                        let run = TextRun {
                                                            len: overlay_text.len(),
                                                            font: style.font(),
                                                            color: style.color,
                                                            background_color: None,
                                                            underline: None,
                                                            strikethrough: None,
                                                        };
                                                        Some(window.text_system().shape_line(SharedString::from(overlay_text.clone()), size, &[run], None))
                                                    };
                                                    let x_at = |b: usize| -> Pixels {
                                                        shaped.as_ref().map_or(px(0.), |s| s.x_for_index(b.min(overlay_text.len())))
                                                    };
                                                    let top = bounds.origin.y + (bounds.size.height - px(14.)) * 0.5;
                                                    if let Some(r) = &sel {
                                                        let x0 = x_at(r.start);
                                                        let x1 = x_at(r.end);
                                                        if x1 > x0 {
                                                            window.paint_quad(fill(
                                                                Bounds::new(point(bounds.origin.x + x0, top), gpui_kit::size(x1 - x0, px(14.))),
                                                                rgba(0x3390ff55),
                                                            ));
                                                        }
                                                    }
                                                    if let Some(c) = caret.filter(|_| bar_focus.is_focused(window) && crate::shell::caret::visible(format!("bar:{caret:?}:{}",overlay_text.len()),window,_cx)) {
                                                        // Align the thin quad to device pixels: glyph advances are fractional,
                                                        // and a subpixel quad may disappear during rasterization.
                                                        let caret_x = window.pixel_snap(bounds.origin.x + x_at(c));
                                                        let caret_top = window.pixel_snap(top);
                                                        window.paint_quad(fill(
                                                            Bounds::new(point(caret_x, caret_top), gpui_kit::size(px(1.), px(14.))),
                                                            fg,
                                                        ));
                                                    }
                                                },
                                            )
                                            .absolute()
                                            .size_full(),
                                        )
                                    })
                            });
                            bar_text(rows.collect::<Vec<_>>(), expanded, bar_scroll.as_ref())
                        }
                        None => {
                            // Closed: the first line; widened: all of them.
                            // Spilled ghost formulas (#122 / issue #24) are rendered in gray (`theme.muted_foreground`).
                            let rows: Vec<Div> = if expanded {
                                super::formula_layout::visual_lines(&content,width,window).into_iter()
                                    .map(|(_,l)| {
                                        div()
                                            .h(px(BAR_LINE_HEIGHT))
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .when(is_ghost_formula, |d| d.text_color(theme.muted_foreground))
                                            .child(SharedString::from(l.to_string()))
                                    })
                                    .collect()
                            } else {
                                vec![
                                    div()
                                        .truncate()
                                        .when(is_ghost_formula, |d| d.text_color(theme.muted_foreground))
                                        .child(SharedString::from(content.split('\n').next().unwrap_or("").to_string())),
                                ]
                            };
                            bar_text(rows, expanded, bar_scroll.as_ref())
                        }
                    }),
            )
            // The ▼ at the right end: the bar widened to several lines and back (as Excel's 数式バーの展開).
            .child(
                Button::new("formula-bar-expand")
                    .ghost()
                    .small()
                    .icon(Icon::new(Material(if expanded { "keyboard_arrow_up" } else { "keyboard_arrow_down" })).size(px(16.)))
                    .tooltip(crate::shell::i18n::text(if expanded { "fix.formula-bar.collapse" } else { "fix.formula-bar.expand" }))
                    .w(px(24.))
                    .h(px(22.))
                    .flex_shrink_0()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_formula_bar_expanded(cx);
                        this.focus_handle.focus(window, cx);
                    })),
            )
            // Over the sheet, under the name box.
            .when(self.wb().jump.goto_open, |d| d.relative().child(deferred(self.render_goto(cx)).with_priority(1)))
            .when(self.wb().table_dialog.is_some(), |d| d.relative().children(self.render_table_dialog(cx)));
        div().flex().flex_col().flex_shrink_0().child(bar).when(expanded,|d|d.child(self.formula_bar_splitter(cx)))
    }

    fn request_sheet_tab_reveal(&mut self, id: SlideId, cx: &mut Context<Self>) {
        self.wb_mut().tab_reveal = Some(id);
        cx.notify();
    }

    fn reveal_sheet_tab_after_layout(&mut self, id: SlideId, cx: &mut Context<Self>) {
        if self.wb().tab_reveal != Some(id) { return; }
        self.wb_mut().tab_reveal = None;
        let Some(index) = self.presentation().slide_index(id) else { return; };
        let scroll = &self.wb().tab_scroll;
        let Some(tab) = scroll.bounds_for_item(index) else { return; };
        let view = scroll.bounds();
        let offset = scroll.offset();
        let x = reveal_offset(f32::from(view.left()), f32::from(view.size.width), f32::from(tab.left()), f32::from(tab.size.width), f32::from(offset.x), f32::from(scroll.max_offset().x));
        if px(x) != offset.x {
            scroll.set_offset(point(px(x), offset.y));
            cx.notify();
        }
    }

    /// The tabs of the worksheets along the bottom, as in Excel.
    pub(crate) fn render_sheet_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let green = rgb(SELECTION_COLOR);
        let (background, border, muted, hover) = (theme.background, theme.border, theme.muted_foreground, theme.muted);
        let tabs = self.presentation().slides.iter().enumerate().map(|(index, slide)| {
            let id = slide.id;
            let current = id == self.current_slide;
            let name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", index + 1));
            // The sheet's tab colour: a bar of it under the sheet name both
            // when active and when inactive.
            let tab_color = sheet_of(slide).and_then(|(_, sheet)| sheet.tab_color);
            let tab_fill = tab_color.map(|c| rgb(c.0));
            div()
                .id(("sheet-tab", id.0))
                .relative()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h_full()
                .px_4()
                .border_r_1()
                .border_color(border)
                .cursor_pointer()
                .map(|d| {
                    if current {
                        let bar = tab_fill.unwrap_or(green);
                        d.bg(background).text_color(green).font_weight(FontWeight::SEMIBOLD).child(
                            div().absolute().left_0().right_0().bottom_0().h(px(if tab_fill.is_some() { 4. } else { 2. })).bg(bar),
                        )
                    } else if let Some(fill) = tab_fill {
                        d.text_color(muted).hover(move |s| s.bg(hover)).child(
                            div().absolute().left_0().right_0().bottom_0().h(px(4.)).bg(fill),
                        )
                    } else {
                        d.text_color(muted).hover(move |s| s.bg(hover))
                    }
                })
                // A tab dragged onto another moves there (as in Excel).
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.tab_drag = Some(id);
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, window, cx| this.on_tab_right_down(id, e, window, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _: &MouseUpEvent, window, cx| {
                    let Some(from) = this.sheet_state_mut().and_then(|s| s.tab_drag.take()) else { return };
                    if from == id {
                        return;
                    }
                    if this.workbook_protection_refuses(cx) {
                        return;
                    }
                    let Some(wb) = this.presentation().workbook.clone() else { return };
                    let Some(to) = wb.sheets.iter().position(|s| s.slide == Some(id)) else { return };
                    if this.current_slide != from {
                        this.select_slide(from, window, cx);
                    }
                    let slide = this.current_slide;
                    if let Err(err) = this.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::MoveSheet { slide, to })]) {
                        log::warn!("moving the sheet rejected: {err}");
                    }
                    cx.notify();
                }))
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    let clicks = event.click_count();
                    let previous = this.wb_mut().tab_last_click.replace((id, clicks));
                    // OS click counts follow the pointer, even when scrolling puts
                    // a different tab there. Renaming requires consecutive clicks
                    // on the same sheet within that OS click sequence.
                    if clicks >= 2 && previous == Some((id, clicks - 1)) {
                        return this.begin_sheet_rename(id, window, cx);
                    }
                    this.select_slide(id, window, cx);
                    this.request_sheet_tab_reveal(id, cx);
                    this.focus_handle.focus(window, cx);
                }))
                .map(|d| match self.wb().jump.renaming == Some(id) {
                    true => d.child(div().w(px(140.)).child(Input::new(&self.wb().jump.tab_rename).small())),
                    false => d.child(SharedString::from(name)),
                })
        });
        // At the left, as Excel: ◀ ▶ scroll the tabs, ≡ lists the sheets.
        let scroll = self.wb().tab_scroll.clone();
        let button = |id: &'static str, icon: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(TAB_BUTTON_WIDTH))
                .h_full()
                .cursor_pointer()
                .text_color(muted)
                .hover(move |s| s.bg(hover))
                .child(Icon::new(Material(icon)).size(px(18.)))
        };
        let step = {
            let scroll = scroll.clone();
            move |direction: f32, cx: &mut Context<Self>| {
                let (offset, max) = (scroll.offset(), scroll.max_offset().x);
                let x = (f32::from(offset.x) - direction * 120.0).clamp(-f32::from(max), 0.0);
                scroll.set_offset(point(px(x), offset.y));
                cx.notify();
            }
        };
        let (left_step, right_step) = (step.clone(), step);
        let left = button("sheet-tabs-left", "arrow_left").on_click(cx.listener(move |_, _, _, cx| left_step(-1.0, cx)));
        let right = button("sheet-tabs-right", "arrow_right").on_click(cx.listener(move |_, _, _, cx| right_step(1.0, cx)));
        let make_add = |id: &'static str| {
            button(id, "add").on_click(cx.listener(|this, _, window, cx| {
                this.insert_sheet(window, cx);
                // The new tab shows.
                this.request_sheet_tab_reveal(this.current_slide, cx);
                this.focus_handle.focus(window, cx);
            }))
        };
        let editor = cx.entity().downgrade();
        crate::shell::chrome::set_current_toolbar_open(self.font_size_menu_open, self.toolbar_search_open);
        let trigger = Button::new("sheet-list")
            .ghost()
            .small()
            .icon(Icon::new(Material("menu")).size(px(18.)))
            .w(px(TAB_BUTTON_WIDTH))
            .h_full()
            .flex_shrink_0();
        let list = crate::shell::chrome::Dropdown::with_button(
            "sheet-list",
            trigger,
            sheet_list_menu_entries(self),
            true,
            0,
            &editor,
        ).into_any_element();

        // Determine whether the + button is placed inline (right of the last sheet tab)
        // or fixed on the right edge of the scroll view.
        let slide_count = self.presentation().slides.len();
        let last_tab_bounds = if slide_count > 0 {
            scroll.bounds_for_item(slide_count - 1)
        } else {
            None
        };
        let scroll_bounds = scroll.bounds();

        let is_plus_fixed = if let (Some(last_tab), true) = (last_tab_bounds, scroll_bounds.size.width > px(0.)) {
            // Right edge of the last tab from the content origin.
            let tabs_right = last_tab.right() - scroll_bounds.left() - scroll.offset().x;
            // Check if inline + button (28px) fits within the scroll viewport width.
            let fits_inline = (tabs_right + px(TAB_BUTTON_WIDTH)) <= scroll_bounds.size.width;
            !fits_inline || scroll.max_offset().x > px(0.5) || scroll.offset().x < px(-0.5)
        } else {
            // Fallback for initial render before bounds are laid out.
            slide_count > 14
        };

        let mut tab_elements: Vec<AnyElement> = tabs.map(|t| t.into_any_element()).collect();
        if !is_plus_fixed {
            tab_elements.push(make_add("sheet-add").into_any_element());
        }

        // Soft shadows at the ends the tabs scroll towards.
        let (offset, max) = (f32::from(scroll.offset().x), f32::from(scroll.max_offset().x));
        let (more_left, more_right) = (offset < -0.5, offset > -max + 0.5 && max > 0.5);
        let shadow = |from_left: bool| {
            let (dark, clear) = (hsla(0.0, 0.0, 0.0, 0.22), hsla(0.0, 0.0, 0.0, 0.0));
            let (a, b) = if from_left { (dark, clear) } else { (clear, dark) };
            div().absolute().top_0().bottom_0().w(px(18.)).map(|d| if from_left { d.left_0() } else { d.right_0() }).bg(linear_gradient(90., linear_color_stop(a, 0.), linear_color_stop(b, 1.)))
        };
        h_flex()
            .id("sheet-tabs")
            .h(px(30.))
            .flex_shrink_0()
            .text_sm()
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(border)
            .child(left)
            .child(right)
            .child(list)
            .child(div().w(px(1.)).h_full().flex_shrink_0().bg(border))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .id("sheet-tab-scroll")
                            .size_full()
                            .overflow_x_scroll()
                            .track_scroll(&scroll)
                            .children(tab_elements),
                    )
                    .when_some(self.wb().tab_reveal, |d, id| {
                        let editor = cx.entity().downgrade();
                        d.child(canvas(|_, _, _| (), move |_, _, _, cx| {
                            let editor = editor.clone();
                            // Run after the scroll container has recorded the newly selected
                            // tab's layout, including its selected font weight and new tabs.
                            cx.defer(move |cx| { let _ = editor.update(cx, |this, cx| this.reveal_sheet_tab_after_layout(id, cx)); });
                        }).absolute().inset_0())
                    })
                    .when(more_left, |d| d.child(shadow(true)))
                    .when(more_right, |d| d.child(shadow(false))),
            )
            .when(is_plus_fixed, |d| {
                d.child(div().w(px(1.)).h_full().flex_shrink_0().bg(border))
                    .child(make_add("sheet-add"))
            })
    }

    /// The row and column headers, laid over the top and left edges of the
    /// workspace. They follow the sheet as it scrolls sideways (the columns)
    /// or up and down (the rows), and highlight the selected cell's.
    pub(crate) fn render_sheet_headers(&self, content: WorkspaceContent, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (shape, sheet) = self.current_sheet()?;
        let show_headings = sheet.headings;
        let scale = content.scale;
        let to_px = |origin: Pixels, at: i64, offset: i64| f32::from(origin) + ((at + offset) as f32 * scale).round();
        let mut xs: Vec<f32> = sheet.column_offsets().into_iter().map(|o| to_px(content.slide_origin.x, shape.frame.x, o)).collect();
        let mut ys: Vec<f32> = sheet.row_offsets().into_iter().map(|o| to_px(content.slide_origin.y, shape.frame.y, o)).collect();
        let target_right_px = to_px(content.slide_origin.x, shape.frame.x, 0) + f32::from(content.size.width) + 2000.0;
        let target_bottom_px = to_px(content.slide_origin.y, shape.frame.y, 0) + f32::from(content.size.height) + 2000.0;
        let last_column_emu: i64 = sheet.columns.iter().sum();
        let last_row_emu: i64 = sheet.rows.iter().sum();
        let mut cur_x = *xs.last().unwrap_or(&0.0);
        let mut col_count = sheet.columns.len();
        while cur_x < target_right_px && col_count < 16_384 {
            let extra = (col_count + 1 - sheet.columns.len()) as i64 * sheet.default_column;
            cur_x = to_px(content.slide_origin.x, shape.frame.x, last_column_emu + extra);
            xs.push(cur_x);
            col_count += 1;
        }
        let mut cur_y = *ys.last().unwrap_or(&0.0);
        let mut row_count = sheet.rows.len();
        while cur_y < target_bottom_px && row_count < 1_048_576 {
            let extra = (row_count + 1 - sheet.rows.len()) as i64 * sheet.default_row;
            cur_y = to_px(content.slide_origin.y, shape.frame.y, last_row_emu + extra);
            ys.push(cur_y);
            row_count += 1;
        }
        let range = self.selected_range().filter(|_| self.selection.is_none() || self.sheet_range_pick_active());
        let all_selected = range.is_some_and(|r| {
            r.first_column == 0
                && r.first_row == 0
                && r.last_column as usize + 1 >= sheet.columns.len().max(1)
                && r.last_row as usize + 1 >= sheet.rows.len().max(1)
        });
        let scroll = self.workspace_scroll.clone();
        let font = font(cx.theme().font_family.clone());
        let (head_w, head_h) = (px(row_header_width(sheet)), px(COLUMN_HEADER_HEIGHT));
        // Frozen columns and rows stay put; the others scroll past them.
        let (frozen_columns, frozen_rows) = sheet.frozen.unwrap_or((0, 0));
        let split = self.frozen_split(content).unwrap_or((None, None));
        // Split panes (表示 > 分割) scroll on their own, all the lines in each.
        let (pane_shift, main_lead, all_lines) = (self.pane_shift(), self.main_pane_lead(), self.split_panes().is_some());
        let (split_x, split_y) = (split.0.map_or(head_w, px), split.1.map_or(head_h, px));
        let geometry = self.wb().header_geometry.clone();
        if !show_headings {
            *geometry.borrow_mut() = Default::default();
        }
        let paint_geometry = geometry.clone();
        let drag = self.wb().header_drag;
        let (primary, bg_color) = (cx.theme().primary, cx.theme().background);
        let extra_shape_frames: Vec<AnyElement> = self
            .selected_shapes()
            .into_iter()
            .filter(|s| Some(s.id) != self.selection)
            .map(|s| {
                let id = s.id;
                let frame = self.display_frame(s);
                let base_x = f32::from(content.slide_origin.x + scroll.offset().x) + frame.x as f32 * scale;
                let base_y = f32::from(content.slide_origin.y + scroll.offset().y) + frame.y as f32 * scale;
                let w = frame.width as f32 * scale;
                let h = frame.height as f32 * scale;
                let outline = div()
                    .absolute()
                    .left(px(base_x))
                    .top(px(base_y))
                    .w(px(w))
                    .h(px(h))
                    .border_1()
                    .border_color(primary);
                let handles = ibis_doc_model::ResizeHandle::ALL.map(|handle| {
                    let (ax, ay) = handle.anchor();
                    let hx = base_x + w * ax as f32 / 2.0 - 3.0;
                    let hy = base_y + h * ay as f32 / 2.0 - 3.0;
                    div()
                        .id(("multi-resize-handle", (id.0 << 4) | handle as u64))
                        .absolute()
                        .left(px(hx))
                        .top(px(hy))
                        .size(px(6.0))
                        .border_1()
                        .border_color(primary)
                        .bg(bg_color)
                });
                div().absolute().top_0().left_0().size_full().child(outline).children(handles).into_any_element()
            })
            .collect();

        let painter = move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
            let offset = scroll.offset();
            let o = bounds.origin;
            let geometry = paint_geometry.clone();
            *geometry.borrow_mut() = super::headers::HeaderGeometry { scale, origin: (f32::from(o.x), f32::from(o.y)), row_header_width: f32::from(head_w), ..Default::default() };
            let (background, line, text, selected, green) =
                (rgb(HEADER_BACKGROUND), rgb(HEADER_LINE), rgb(HEADER_TEXT), rgb(HEADER_SELECTED), rgb(SELECTION_COLOR));
            let label = |window: &mut Window, cx: &mut App, s: String, color: Rgba, x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                let run = TextRun {
                    len: s.len(),
                    font: font.clone(),
                    color: color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(s.into(), px(HEADER_FONT_SIZE), &[run], None);
                if w < px(1.) || h < px(1.) {
                    return;
                }
                // A label that fits is centred; one that does not starts at
                // the header's left and is cut off at its edges.
                let fits = shaped.width <= w - px(2.);
                let origin = point(if fits { x + (w - shaped.width) / 2.0 } else { x + px(1.) }, y);
                let cell = Bounds::new(point(x, y), size(w, h));
                window.with_content_mask(Some(ContentMask { bounds: cell }), |window| {
                    shaped.paint(origin, h, TextAlign::Left, None, window, cx).ok();
                });
            };

            // Column headers: the frozen ones unscrolled, then the others
            // scrolled and clipped to the right of them.
            window.paint_quad(fill(Bounds::new(o, size(bounds.size.width, head_h)), background));
            let columns = |window: &mut Window, cx: &mut App, strip: Bounds<Pixels>, shift: Pixels, frozen: bool| {
                window.with_content_mask(Some(ContentMask { bounds: strip }), |window| {
                    for (column, pair) in xs.windows(2).enumerate() {
                        if !all_lines && (column < frozen_columns as usize) != frozen {
                            continue;
                        }
                        let (left, right) = (o.x + shift + px(pair[0]), o.x + shift + px(pair[1]));
                        if right >= strip.left() && left <= strip.right() {
                            geometry.borrow_mut().columns.push((column as u32, f32::from(left), f32::from(right)));
                        }
                        if right <= strip.left() || right - left < px(0.5) {
                            continue;
                        }
                        if left >= strip.right() {
                            break;
                        }
                        let is_selected =
                            range.is_some_and(|r| (r.first_column..=r.last_column).contains(&(column as u32)));
                        let cell = Bounds::new(point(left, o.y), size(right - left, head_h));
                        if is_selected {
                            window.paint_quad(fill(cell, selected));
                            window.paint_quad(fill(Bounds::new(point(left, o.y + head_h - px(2.)), size(right - left, px(2.))), green));
                        }
                        window.paint_quad(fill(Bounds::new(point(right - px(1.), o.y), size(px(1.), head_h)), line));
                        let color = if is_selected { green } else { text };
                        label(window, cx, column_letters(column as u32), color, left, o.y, right - left, head_h);
                    }
                });
            };
            let frozen_strip = Bounds::new(point(o.x + head_w, o.y), size(split_x - head_w, head_h));
            columns(window, cx, frozen_strip, pane_shift.x, true);
            let strip = Bounds::new(point(o.x + split_x, o.y), size(bounds.size.width - split_x, head_h));
            columns(window, cx, strip, offset.x + main_lead.x, false);
            window.paint_quad(fill(Bounds::new(point(o.x, o.y + head_h - px(1.)), size(bounds.size.width, px(1.))), line));

            // Row headers, likewise.
            window.paint_quad(fill(Bounds::new(point(o.x, o.y + head_h), size(head_w, bounds.size.height - head_h)), background));
            let rows = |window: &mut Window, cx: &mut App, strip: Bounds<Pixels>, shift: Pixels, frozen: bool| {
                window.with_content_mask(Some(ContentMask { bounds: strip }), |window| {
                    for (row, pair) in ys.windows(2).enumerate() {
                        if !all_lines && (row < frozen_rows as usize) != frozen {
                            continue;
                        }
                        let (top, bottom) = (o.y + shift + px(pair[0]), o.y + shift + px(pair[1]));
                        if bottom >= strip.top() && top <= strip.bottom() {
                            geometry.borrow_mut().rows.push((row as u32, f32::from(top), f32::from(bottom)));
                        }
                        if bottom <= strip.top() || bottom - top < px(0.5) {
                            continue;
                        }
                        if top >= strip.bottom() {
                            break;
                        }
                        let is_selected = range.is_some_and(|r| (r.first_row..=r.last_row).contains(&(row as u32)));
                        let cell = Bounds::new(point(o.x, top), size(head_w, bottom - top));
                        if is_selected {
                            window.paint_quad(fill(cell, selected));
                            window.paint_quad(fill(Bounds::new(point(o.x + head_w - px(2.), top), size(px(2.), bottom - top)), green));
                        }
                        window.paint_quad(fill(Bounds::new(point(o.x, bottom - px(1.)), size(head_w, px(1.))), line));
                        let color = if is_selected { green } else { text };
                        label(window, cx, (row + 1).to_string(), color, o.x, top, head_w, bottom - top);
                    }
                });
            };
            let frozen_strip = Bounds::new(point(o.x, o.y + head_h), size(head_w, split_y - head_h));
            rows(window, cx, frozen_strip, pane_shift.y, true);
            let strip = Bounds::new(point(o.x, o.y + split_y), size(head_w, bounds.size.height - split_y));
            rows(window, cx, strip, offset.y + main_lead.y, false);
            window.paint_quad(fill(Bounds::new(point(o.x + head_w - px(1.), o.y), size(px(1.), bounds.size.height)), line));

            // The corner where they meet (the Select All button: a right-angled triangle in its bottom-right corner).
            window.paint_quad(fill(
                Bounds::new(o, size(head_w - px(1.), head_h - px(1.))),
                if all_selected { selected } else { background },
            ));
            let (tx1, ty1) = (o.x + head_w - px(4.), o.y + head_h - px(4.));
            let tri_size = px(10.);
            if let Some(tri) = crate::render::shape_view::polygon(
                &[point(tx1 - tri_size, ty1), point(tx1, ty1 - tri_size), point(tx1, ty1)],
                None,
                true,
            ) {
                window.paint_path(tri, if all_selected { green } else { rgb(0xb8b8b8) });
            }

            // A line being dragged: where the column or row will end, across the sheet.
            if let Some(d) = drag {
                // (Where the pointer is, kept within the largest size when it moved: `on_header_move`.)
                let max_px = match d.axis {
                    Axis::Rows => super::headers::MAX_ROW_HEIGHT_PT * ibis_doc_model::EMU_PER_POINT as f32 * scale,
                    Axis::Columns => super::headers::max_column_width_pt(11.0) * ibis_doc_model::EMU_PER_POINT as f32 * scale,
                };
                let at = px(d.current.clamp(d.start, d.start + max_px));
                let guide = match d.axis {
                    Axis::Columns => Bounds::new(point(at, o.y), size(px(1.), bounds.size.height)),
                    Axis::Rows => Bounds::new(point(o.x, at), size(bounds.size.width, px(1.))),
                };
                window.paint_quad(fill(guide, green));
            }
        };
        // The header's menu, open at the pointer.
        let popup = self.wb().header_popup.as_ref().map(|p| deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2));
        // The resize pointer while a line is dragged or the pointer is on one.
        let on_line = |axis: Axis| self.wb().header_drag.is_some_and(|d| d.axis == axis) || self.wb().header_hover == Some(axis);
        let (drag_columns, drag_rows) = (on_line(Axis::Columns), on_line(Axis::Rows));
        let active_drag_cursor = if let Some(m) = self.cell_move() {
            Some(if m.copy { CursorStyle::DragCopy } else { CursorStyle::ClosedHand })
        } else if self.sheet_state().is_some_and(|s| s.fill_source.is_some()) {
            Some(if self.sheet_state().is_some_and(|s| s.fill_copy) { CursorStyle::DragCopy } else { CursorStyle::Crosshair })
        } else {
            None
        };

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .when(self.link_hover,|d|d.cursor_pointer()).when_some(active_drag_cursor, |d, cursor| d.cursor(cursor))
                .when(show_headings, |d| {
                    d.child(canvas(|_, _, _| {}, move |bounds, _, window, cx| painter(bounds, window, cx)).absolute().size_full())
                        // The headers take the pointer from the sheet under them,
                        // but let the wheel through to scroll it.
                        .child(
                            div()
                                .id("column-headers")
                                .absolute()
                                .top_0()
                                .left(head_w)
                                .right_0()
                                .h(head_h)
                                .block_mouse_except_scroll()
                                .when(drag_columns, |d| d.cursor(CursorStyle::ResizeLeftRight))
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.on_header_down(Axis::Columns, e, window, cx)))
                                // The strip hides the workspace under it from the pointer: moves over it too.
                                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                                    this.on_header_move(e, window, cx);
                                    this.on_pointer_move(e, window, cx);
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, window, cx| {
                                    this.on_header_up(cx);
                                    this.on_pointer_up(e, window, cx);
                                }))
                                .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, window, cx| this.on_header_right_down(Axis::Columns, e, window, cx))),
                        )
                        .child(
                            div()
                                .id("row-headers")
                                .absolute()
                                .top(head_h)
                                .left_0()
                                .bottom_0()
                                .w(head_w)
                                .block_mouse_except_scroll()
                                .when(drag_rows, |d| d.cursor(CursorStyle::ResizeUpDown))
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.on_header_down(Axis::Rows, e, window, cx)))
                                // The strip hides the workspace under it from the pointer: moves over it too.
                                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                                    this.on_header_move(e, window, cx);
                                    this.on_pointer_move(e, window, cx);
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, window, cx| {
                                    this.on_header_up(cx);
                                    this.on_pointer_up(e, window, cx);
                                }))
                                .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, window, cx| this.on_header_right_down(Axis::Rows, e, window, cx))),
                        )
                        // The corner (Select All button) selects every cell of the sheet on click.
                        .child(
                            div()
                                .id("header-corner")
                                .absolute()
                                .top_0()
                                .left_0()
                                .w(head_w)
                                .h(head_h)
                                .block_mouse_except_scroll()
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.on_header_corner_down(e, window, cx)))
                                .on_mouse_move(cx.listener(Self::on_pointer_move))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up)),
                        )
                })
                // Dragging a header's line or along the headers goes on over the sheet.
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| this.on_header_move(e, window, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    this.on_header_up(cx);
                    this.on_split_up(e.position, cx);
                    this.finish_cell_move(cx);
                    this.finish_fill(e.modifiers.alt, cx);
                    this.finish_format_paint(cx);
                    if let Some((column, row)) = this.cell_under(e.position) {
                        this.finish_pointing(column, row, cx);
                    }
                }))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    this.on_header_up(cx);
                    this.on_split_up(e.position, cx);
                    this.finish_cell_move(cx);
                    this.finish_fill(e.modifiers.alt, cx);
                }))
                .children(extra_shape_frames)
                .children(self.filter_buttons(cx))
                .children(self.pivot_buttons(cx))
                .children(popup)
                .children(self.wb().list_popup.as_ref().map(|p| deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2)))
                .children(self.render_validation_choices(cx))
                .children(self.render_filter_panel(cx))
                .children(self.render_pivot_popup(cx))
                .children(self.render_formula_references(cx))
                .children(self.render_completion(cx))
                .children(self.render_comment_card(cx))
                .children(self.render_all_notes(cx))
                .into_any_element(),
        )
    }

    /// The frame around the selected range, in the slide's coordinates:
    /// the range tinted except its selected cell, as in Excel, and the fill
    /// handle at its corner, which drags out a range.
    pub(crate) fn render_cell_cursor(&self, scale: f32, cx: &mut Context<Self>) -> Option<Div> {
        if self.selection.is_some() && !self.sheet_range_pick_active() {
            return None;
        }
        let (shape, sheet) = self.current_sheet()?;
        let range = self.selected_range()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at_x = |offsets: &[i64], i: u32| {
            let len = offsets.len();
            if (i as usize) < len {
                offsets[i as usize]
            } else {
                let last = offsets.last().copied().unwrap_or(0);
                last + (i as usize - (len - 1)) as i64 * sheet.default_column
            }
        };
        let at_y = |offsets: &[i64], i: u32| {
            let len = offsets.len();
            if (i as usize) < len {
                offsets[i as usize]
            } else {
                let last = offsets.last().copied().unwrap_or(0);
                last + (i as usize - (len - 1)) as i64 * sheet.default_row
            }
        };
        let x = |c: u32| shape.frame.x as f32 * scale + (at_x(&xs, c) as f32 * scale).round();
        let y = |r: u32| shape.frame.y as f32 * scale + (at_y(&ys, r) as f32 * scale).round();
        let (x0, x1, y0, y1) = (x(range.first_column), x(range.last_column + 1), y(range.first_row), y(range.last_row + 1));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let green = rgb(SELECTION_COLOR);
        let editing_here = self.cell_edit().is_some_and(|e| e.slide == self.current_slide);
        let cut_source=self.wb().clip.as_ref().filter(|c|c.cut&&c.slide==self.current_slide).map(|c|c.range);
        let cut_active=cut_source==Some(range);
        let cut_frame=cut_source.filter(|r|*r!=range).map(|r|div().absolute().left(px(x(r.first_column)-x0-1.)).top(px(y(r.first_row)-y0-1.)).w(px(x(r.last_column+1)-x(r.first_column))).h(px(y(r.last_row+1)-y(r.first_row))).border_2().border_dashed().border_color(green));
        let window_alt = self.sheet_state().is_some_and(|s| s.fill_copy || s.cell_move.is_some_and(|m| m.copy));
        // Option held while hovering or dragging the fill handle: a copy, not a series (#11).
        let fill_copy = self.sheet_state().is_some_and(|s| s.fill_copy);
        // The tint: the range less the selected cell (or its merged range).
        let mut tint = Vec::new();
        if let Some((column, row)) = self.current_cell() {
            let single = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
            let cell = sheet.merge_at(row, column).unwrap_or(single);
            if cell != range {
                let (cx0, cx1, cy0, cy1) = (x(cell.first_column), x(cell.last_column + 1), y(cell.first_row), y(cell.last_row + 1));
                for (l, t, r, b) in [(x0, y0, x1, cy0), (x0, cy1, x1, y1), (x0, cy0, cx0, cy1), (cx1, cy0, x1, cy1)] {
                    if r > l && b > t {
                        tint.push(div().absolute().left(px(l - x0 - 1.0)).top(px(t - y0 - 1.0)).w(px(r - l)).h(px(b - t)).bg(rgba(RANGE_TINT)));
                    }
                }
            }
        }
        // The ranges 条件を選択してジャンプ selected with it: tinted.
        for r in self.selected_ranges().into_iter().skip(1) {
            let (l, t, rr, b) = (x(r.first_column), y(r.first_row), x(r.last_column + 1), y(r.last_row + 1));
            if rr > l && b > t {
                tint.push(div().absolute().left(px(l - x0 - 1.0)).top(px(t - y0 - 1.0)).w(px(rr - l)).h(px(b - t)).bg(rgba(RANGE_TINT)));
            }
        }
        // The cell being typed into: its text over the cell, laid out as the cell is when not edited (`sheet_view::edit_layout`), with the caret.
        let editor = self.cell_edit().filter(|e| e.slide == self.current_slide).map(|e| {
            let caret_focus=self.focus_handle.clone();
            let (ex0, ex1, ey0, ey1) = if let Some(m) = sheet.merge_at(e.row, e.column) {
                (x(m.first_column), x(m.last_column + 1), y(m.first_row), y(m.last_row + 1))
            } else {
                (x(e.column), x(e.column + 1), y(e.row), y(e.row + 1))
            };
            let default_style = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref());
            let style = super::sheet_view::cell_or_default_style(sheet, default_style, e.row, e.column);
            let px_per_point = ibis_doc_model::EMU_PER_POINT as f32 * scale;
            e.pixels_per_point.set(px_per_point);
            let cell_size = (style.font_size as f32 / 100.0 * px_per_point).max(6.0);
            // The sheet's own coordinates, which the cell's text is placed in when not edited.
            let origin = (ex0 - shape.frame.x as f32 * scale, ey0 - shape.frame.y as f32 * scale);
            let runs = e.runs.clone().map(std::sync::Arc::new);
            let ts = WindowTextSystem::new(cx.text_system().clone());
            let layout = std::rc::Rc::new(super::sheet_view::edit_layout(&ts, sheet, default_style, &self.resources, e.row, e.column, &e.text, runs, origin, (ex1 - ex0, ey1 - ey0), px_per_point, self.cell_editor_width(ex0)));
            *e.layout.borrow_mut() = Some(layout.clone());
            let default_font_name = sheet.styles.first().and_then(|s| s.font.as_deref()).unwrap_or("游ゴシック"); // i18n-ok（既定のフォント名）
            let effective_font = style.font.as_deref().unwrap_or(default_font_name);
            let ghost = self
                .flash_fill_preview()
                .first()
                .filter(|(r, _)| *r == e.row)
                .and_then(|(_, pred)| pred.strip_prefix(&e.text).filter(|s| !s.is_empty()).map(|s| s.to_string()))
                .map(|suffix| (suffix, self.resources.font(&ibis_doc_model::TextStyle { font: Some(effective_font.to_string()), ..Default::default() })));
            let selection = e.selection().map(|r| (super::cell_edit::byte_at(&e.text, r.start), super::cell_edit::byte_at(&e.text, r.end)));
            let caret = e.caret_byte();
            let rect = e.rect.clone();
            let painted = layout.clone();
            let (box_w, box_h) = (layout.width, layout.height);
            // Lines more than fit down to the bottom of the sheet view (above the sheet tab bar): the box stops there
            // and scrolls with a scroll bar always shown, following the caret when it moves (issue #17).
            const SCROLLBAR_W: f32 = 10.0;
            let max_box_h = self.max_edit_box_height_px(e.row, ey0, ey1, px_per_point);
            let visible_h = box_h.min(max_box_h);
            let max_scroll = (box_h - visible_h).max(0.0);
            let overflows = max_scroll > 0.5;
            let mut scroll_y = e.scroll_y.get().clamp(0.0, max_scroll);
            if overflows && e.followed_caret.get() != Some(e.caret) {
                if let Some(l) = layout.lines.get(layout.line_of(caret)) {
                    let (line_top, line_bottom) = (l.baseline - l.ascent - 2.0, l.baseline + l.descent + 2.0);
                    if line_top < scroll_y {
                        scroll_y = line_top.max(0.0);
                    } else if line_bottom > scroll_y + visible_h {
                        scroll_y = (line_bottom - visible_h).clamp(0.0, max_scroll);
                    }
                }
            }
            e.followed_caret.set(Some(e.caret));
            e.scroll_y.set(if overflows { scroll_y } else { 0.0 });
            let scroll_cell = e.scroll_y.clone();
            let paint_scroll = e.scroll_y.clone();
            let bar_w = if overflows { SCROLLBAR_W } else { 0.0 };
            div()
                .absolute()
                .left(px(ex0 - x0 - 1.0))
                .top(px(ey0 - y0 - 1.0))
                .cursor_text()
                .w(px(box_w + 1.0 + bar_w))
                .h(px(visible_h + 1.0))
                .bg(style.fill.map_or_else(|| white(), |c| rgb(c.0).into()))
                .when(self.panel_message.is_some() && self.sheet_state().and_then(|s|s.validation_rejected)==Some((e.column,e.row)), |d|d.bg(rgb(0xfff1f5)))
                .max_h(px(max_box_h + 1.0))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(SELECTION_COLOR))
                // The wheel over a box of more lines than it shows scrolls them (not the sheet).
                .on_scroll_wheel(cx.listener(move |_this, ev: &ScrollWheelEvent, _window, cx| {
                    if !overflows {
                        return;
                    }
                    cx.stop_propagation();
                    let dy = f32::from(ev.delta.pixel_delta(px(20.)).y);
                    scroll_cell.set((scroll_cell.get() - dy).clamp(0.0, max_scroll));
                    cx.notify();
                }))
                // A press anywhere in the box (over the cells beside and below it too) places the caret and starts a selection by dragging: it never reaches the cells under it.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, e: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.focus_handle.focus(window, cx);
                        this.set_selection(Vec::new());
                        this.press_in_cell_text(e.position, e.click_count, e.modifiers.shift, cx);
                    }),
                )
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, cx| {
                            // The box's border is a pixel wide and the box starts a pixel before the cell: the text is placed from the cell's corner.
                            let o = bounds.origin - point(px(1.0), px(1.0));
                            rect.set(Some(Bounds::new(o, bounds.size)));
                            // The scroll bar at the right, always shown while the text goes below the box.
                            if overflows {
                                let track = Bounds::new(point(bounds.origin.x + bounds.size.width - px(SCROLLBAR_W), bounds.origin.y), gpui_kit::size(px(SCROLLBAR_W), bounds.size.height));
                                window.paint_quad(fill(track, rgb(0xF0F0F0)));
                                let track_h = f32::from(bounds.size.height);
                                let thumb_h = (track_h * visible_h / box_h).clamp(16.0_f32.min(track_h), track_h);
                                let thumb_y = (track_h - thumb_h) * (paint_scroll.get() / max_scroll).clamp(0.0, 1.0);
                                let thumb = Bounds::new(point(track.origin.x + px(2.0), track.origin.y + px(thumb_y)), gpui_kit::size(px(SCROLLBAR_W - 4.0), px(thumb_h)));
                                window.paint_quad(fill(thumb, rgb(0xA0A0A0)).corner_radii(px((SCROLLBAR_W - 4.0) / 2.0)));
                            }
                            // Scrolled, the text moves up by as much.
                            let o = o - point(px(0.0), px(paint_scroll.get()));
                            if let Some((b0, b1)) = selection {
                                for l in painted.lines.iter() {
                                    let (a, b) = (b0.max(l.start), b1.min(l.end));
                                    if a < b {
                                        let (xa, xb) = (l.x_at(a), l.x_at(b));
                                        window.paint_quad(fill(
                                            Bounds::new(point(o.x + px(xa), o.y + px(l.baseline - l.ascent)), gpui_kit::size(px(xb - xa), px(l.ascent + l.descent))),
                                            rgba(0x3390ff55),
                                        ));
                                    }
                                }
                            }
                            for l in painted.lines.iter() {
                                for p in &l.pieces {
                                    if let Some(s) = &p.shaped {
                                        s.paint(point(o.x + px(p.x), o.y + px(l.baseline) - s.ascent), s.ascent + s.descent, TextAlign::Left, None, window, cx).ok();
                                    }
                                }
                            }
                            if let Some(l) = painted.lines.last() {
                                if let Some((suffix, font)) = &ghost {
                                    let run = TextRun { len: suffix.len(), font: font.clone(), color: rgb(0x888888).into(), background_color: None, underline: None, strikethrough: None };
                                    let shaped = window.text_system().shape_line(SharedString::from(suffix.clone()), px(cell_size), &[run], None);
                                    shaped.paint(point(o.x + px(l.x_at(l.end)), o.y + px(l.baseline) - shaped.ascent), shaped.ascent + shaped.descent, TextAlign::Left, None, window, cx).ok();
                                }
                            }
                            if let Some(l) = painted.lines.get(painted.line_of(caret)).filter(|_| caret_focus.is_focused(window) && crate::shell::caret::visible(format!("cell:{caret}:{}",painted.lines.len()),window,cx)) {
                                let cx0 = l.x_at(caret);
                                window.paint_quad(fill(Bounds::new(point(o.x + px(cx0), o.y + px(l.baseline - l.ascent)), gpui_kit::size(px(1.0), px(l.ascent + l.descent))), rgb(0x000000)));
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
        });
        let flash_ghosts: Vec<AnyElement> = self
            .flash_fill_preview()
            .into_iter()
            .filter(|(r, _)| Some(*r) != self.cell_edit().map(|e| e.row))
            .map(|(r, pred)| {
                let col = self.cell_edit().map_or(0, |e| e.column);
                let (gx0, gx1, gy0, gy1) = (x(col), x(col + 1), y(r), y(r + 1));
                let style = sheet.cell(r, col).map(|c| sheet.styles[c.style as usize].clone()).unwrap_or_default();
                let size = (style.font_size as f32 / 100.0 * ibis_doc_model::EMU_PER_POINT as f32 * scale).max(6.0);
                let default_font_name = sheet.styles.first().and_then(|s| s.font.as_deref()).unwrap_or("游ゴシック"); // i18n-ok（既定のフォント名）
                let effective_font = style.font.as_deref().unwrap_or(default_font_name).to_string();
                div()
                    .absolute()
                    .left(px(gx0 - x0))
                    .top(px(gy0 - y0))
                    .w(px(gx1 - gx0))
                    .h(px(gy1 - gy0))
                    .pl(px(2.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(size))
                    .text_color(rgb(0x888888))
                    .font_family(effective_font)
                    .child(SharedString::from(pred))
                    .into_any_element()
            })
            .collect();
        // データの入力規則 of the selected cell: a list's drop-down button at
        // its right, and the message under it.
        let validation = self.current_rule().filter(|_| self.cell_edit().is_none()).map(|rule| {
            let (cx0, cx1, cy0, cy1) = self.current_cell().map_or((x0, x1, y0, y1), |(c, r)| (x(c), x(c + 1), y(r), y(r + 1)));
            let mut parts = div().absolute().left(px(cx0 - x0)).top(px(cy0 - y0)).w(px(cx1 - cx0)).h(px(cy1 - cy0));
            if rule.kind == "list" && rule.dropdown {
                let side = (cy1 - cy0).clamp(12.0, 20.0);
                parts = parts.child(
                    div()
                        .id("validation-dropdown")
                        .absolute()
                        .left(px(cx1 - cx0 + 1.0))
                        .bottom(px(0.))
                        .size(px(side))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(HEADER_BACKGROUND))
                        .rounded(px(4.))
                        .hover(|d| d.bg(rgb(0xd6d6d6)))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .text_size(px(side * 0.6))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_validation_list(e.position, window, cx);
                        }))
                        .child(crate::shell::chrome::icon("arrow_drop_down", 18., rgb(0x333333).into())),
                );
            }
            if let Some((title, text)) = rule.prompt.clone() {
                parts = parts.child(
                    v_flex()
                        .absolute()
                        .left(px(12.))
                        .top(px(cy1 - cy0 + 6.0))
                        .max_w(px(220.))
                        .p(px(6.))
                        .bg(rgb(0xFFFFE1))
                        .border_1()
                        .border_color(rgb(0x767676))
                        .text_size(px(12.))
                        .text_color(rgb(0x000000))
                        .when(!title.is_empty(), |d| d.child(div().font_weight(FontWeight::BOLD).child(SharedString::from(title))))
                        .child(SharedString::from(text)),
                );
            }
            parts
        });
        let validation_warning = self
            .sheet_state()
            .and_then(|s| s.validation_warning.clone())
            .filter(|w| w.edit.slide == self.current_slide)
            .map(|warn| {
                let (wx0, wy1) = (x(warn.edit.column), y(warn.edit.row + 1));
                let btn = |id: &'static str, label: SharedString, primary: bool, on_click: fn(&mut Self, &mut Context<Self>)| {
                    div()
                        .id(id)
                        .px(px(8.))
                        .py(px(2.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(rgb(if primary { 0x107C41 } else { 0xA6A6A6 }))
                        .bg(rgb(if primary { 0x107C41 } else { 0xF2F2F2 }))
                        .text_color(rgb(if primary { 0xFFFFFF } else { 0x000000 }))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                on_click(this, cx);
                            }),
                        )
                        .child(label)
                };
                v_flex()
                    .id("validation-warning-card")
                    .absolute()
                    .left(px(wx0 - x0))
                    .top(px(wy1 - y0 + 4.0))
                    .w(px(260.))
                    .p(px(8.))
                    .gap(px(6.))
                    .bg(rgb(0xFFFDF0))
                    .border_1()
                    .border_color(rgb(0xD9A300))
                    .rounded(px(4.))
                    .shadow_md()
                    .text_size(px(12.))
                    .text_color(rgb(0x000000))
                    .child(SharedString::from(warn.message))
                    .child(div().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("fix.validation.continue")))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(btn("validation-warning-yes", crate::shell::i18n::text("fix.update.yes"), true, Self::validation_warning_yes))
                            .child(btn("validation-warning-no", crate::shell::i18n::text("fix.update.no"), false, Self::validation_warning_no))
                            .child(btn("validation-warning-cancel", crate::shell::i18n::text("fix.validation.cancel"), false, Self::validation_warning_cancel)),
                    )
            });
        let table_total = self.current_cell_table_total().filter(|_| self.cell_edit().is_none()).map(|(table_idx, col_idx)| {
            let (_cx0, cx1, cy0, cy1) = self.current_cell().map_or((x0, x1, y0, y1), |(c, r)| (x(c), x(c + 1), y(r), y(r + 1)));
            let side = (cy1 - cy0 - 2.0).clamp(12.0, 18.0);
            div()
                .id("table-total-dropdown")
                .absolute()
                .left(px(cx1 - x0 - side - 1.0))
                .top(px(cy0 - y0 + 1.0))
                .size(px(side))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0xF2F2F2))
                .border_1()
                .border_color(rgb(0xA6A6A6))
                .text_size(px(side * 0.6))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_table_total_menu(table_idx, col_idx, e.position, window, cx);
                }))
                .child("▼")
        });
        // Dynamic array spill range outline + outer drop shadow (#122 / issue #24):
        // - Unblocked spill range: thin solid blue border + outer drop shadow
        // - Blocked (#SPILL!) spill range: thin dashed blue border + outer drop shadow
        let mut spill_frames: Vec<Div> = Vec::new();
        for (sr, is_blocked) in active_spill_outlines_on(
            self.presentation(),
            self.current_slide,
            sheet,
            self.current_cell(),
            &self.selected_ranges(),
        ) {
            let (sx0, sx1, sy0, sy1) = (x(sr.first_column), x(sr.last_column + 1), y(sr.first_row), y(sr.last_row + 1));
            let (sw, sh) = (sx1 - sx0, sy1 - sy0);
            if sw > 0.0 && sh > 0.0 {
                // Outer drop shadow rings strictly outside the 1px blue border (keeping the interior 100% clear).
                for (ring_offset, alpha) in [(1.0_f32, 0.20_f32), (2.0, 0.12), (3.0, 0.06), (4.0, 0.02)] {
                    spill_frames.push(
                        div()
                            .absolute()
                            .left(px(sx0 - x0 - 2.0 - ring_offset))
                            .top(px(sy0 - y0 - 2.0 - ring_offset))
                            .w(px(sw + 2.0 + 2.0 * ring_offset))
                            .h(px(sh + 2.0 + 2.0 * ring_offset))
                            .rounded(px(ring_offset * 0.5))
                            .border_1()
                            .border_color(hsla(222.0 / 360.0, 0.45, 0.15, alpha)),
                    );
                }
                let border_div = div()
                    .absolute()
                    .left(px(sx0 - x0 - 2.0))
                    .top(px(sy0 - y0 - 2.0))
                    .w(px(sw + 2.0))
                    .h(px(sh + 2.0))
                    .border_1()
                    .when(is_blocked, |d| d.border_dashed())
                    .border_color(rgb(SPILL_BORDER_COLOR));
                spill_frames.push(border_div);
            }
        }
        Some(
            div()
                .absolute()
                .left(px(x0 - 1.0))
                .top(px(y0 - 1.0))
                .w(px(x1 - x0 + 2.0))
                .h(px(y1 - y0 + 2.0))
                .border_2()
                // While typing the box has its own frame: the selection's is not drawn.
                .border_color(if editing_here { gpui::transparent_black() } else { green.into() })
                .when(cut_active,|d|d.border_dashed())
                .children(spill_frames)
                .children(cut_frame)
                .when(!editing_here, |d| {
                    // Paint the active selection's 2px green frame over any overlapping spill border edge.
                    d.child(
                        div()
                            .absolute()
                            .left(px(-2.0))
                            .top(px(-2.0))
                            .w(px(x1 - x0 + 2.0))
                            .h(px(y1 - y0 + 2.0))
                            .border_2()
                            .border_color(green)
                            .when(cut_active,|d|d.border_dashed()),
                    )
                })
                .children(tint)
                .children(editor)
                .children(flash_ghosts)
                .children(validation)
                .children(validation_warning)
                .children(table_total)
                // エラー チェック: the button at the left of the selected cell that has an error.
                .children(self.current_error().filter(|_| self.cell_edit().is_none()).map(|_| {
                    let (cx0, cy0) = self.current_cell().map_or((x0, y0), |(c, r)| (x(c), y(r)));
                    div()
                        .id("error-check")
                        .absolute()
                        .left(px(cx0 - x0 - 20.0))
                        .top(px(cy0 - y0))
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0xFFD966))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .rounded(px(3.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0x7F6000))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_error_menu(e.position, window, cx);
                        }))
                        .child("!")
                }))
                // オートフィル オプション, just past the filled range's corner.
                .children(self.fill_options_range().map(|_| {
                    div()
                        .id("fill-options")
                        .absolute()
                        .right(px(-26.))
                        .bottom(px(-24.))
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .rounded(px(3.))
                        .text_size(px(11.))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_fill_options(e.position, window, cx);
                        }))
                        .child("▼")
                }))
                // 貼り付けのオプション, just past the pasted range's corner.
                .children(self.paste_options_range().map(|_| {
                    div()
                        .id("paste-options")
                        .absolute()
                        .right(px(-26.))
                        .bottom(px(-24.))
                        .h(px(20.))
                        .px(px(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(1.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .rounded(px(3.))
                        .text_size(px(11.))
                        .cursor_pointer()
                        .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("xlsx.paste-options-button")).build(window, cx))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_paste_options(e.position, window, cx);
                        }))
                        .child(Icon::new(Material("content_paste")).size(px(14.)))
                        .child("▼")
                }))
                // The border: dragged, it moves the range (Option: copies it), #11.
                // (Not while a pen of the Draw panel is on: its press draws, over the border too.)
                .when(!editing_here && self.ink_pen().is_none() && self.cell_move().is_none_or(|m| m.source == range), |d| {
                    let (w, h) = (x1 - x0, y1 - y0);
                    let hover = |id: &'static str, l: f32, t: f32, sw: f32, sh: f32, cx: &mut Context<Self>| {
                        div()
                            .id(id)
                            .absolute()
                            .left(px(l))
                            .top(px(t))
                            .w(px(sw))
                            .h(px(sh))
                            .cursor(self.cell_move_cursor(window_alt))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.begin_cell_move(e.position, window, cx);
                            }))
                    };
                    d.child(hover("border-top", -4.0, -4.0, w + 8.0, 6.0, cx))
                        .child(hover("border-bottom", -4.0, h - 4.0, w + 8.0, 6.0, cx))
                        .child(hover("border-left", -4.0, -4.0, 6.0, h + 8.0, cx))
                        .child(hover("border-right", w - 4.0, -4.0, 6.0, h + 8.0, cx))
                })
                // Excel's question when the range was let go over cells with data.
                .children(self.sheet_state().and_then(|s| s.move_overwrite).map(|m| {
                    let t = m.target;
                    let (l, b) = (x(t.first_column), y(t.last_row + 1));
                    let btn = |id: &'static str, label: SharedString, primary: bool, ok: bool, cx: &mut Context<Self>| {
                        div()
                            .id(id)
                            .px(px(12.))
                            .py(px(2.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(0xA6A6A6))
                            .when(primary, |d| d.bg(rgb(SELECTION_COLOR)).text_color(rgb(0xFFFFFF)))
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.answer_move_overwrite(ok, cx);
                            }))
                            .child(label)
                    };
                    v_flex()
                        .id("move-overwrite-card")
                        .absolute()
                        .left(px(l - x0 - 1.0))
                        .top(px(b - y0 + 4.0))
                        .w(px(280.))
                        .p(px(8.))
                        .gap(px(6.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0x767676))
                        .rounded(px(4.))
                        .shadow_md()
                        .text_size(px(12.))
                        .text_color(rgb(0x000000))
                        .child(crate::shell::i18n::text("fix.move.overwrite"))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(btn("move-overwrite-ok", crate::shell::i18n::text("fix.move.ok"), true, true, cx))
                                .child(btn("move-overwrite-cancel", crate::shell::i18n::text("fix.validation.cancel"), false, false, cx)),
                        )
                }))
                // Where the range dragged by its border would land.
                .children(self.cell_move().filter(|m| m.moved).map(|m| {
                    let t = m.target;
                    let (l, tp, r, b) = (x(t.first_column), y(t.first_row), x(t.last_column + 1), y(t.last_row + 1));
                    div().absolute().left(px(l - x0 - 2.0)).top(px(tp - y0 - 2.0)).w(px(r - l + 2.0)).h(px(b - tp + 2.0)).border_2().border_color(green)
                }))
                .children(self.sheet_range_pick_handles(x1-x0,y1-y0))
                .when(!editing_here && !self.sheet_range_pick_active(), |d| {
                    d.child(
                        // A little larger than it looks, to be easy to catch.
                        div()
                            .id("fill-handle")
                            .absolute()
                            .right(px(-6.))
                            .bottom(px(-6.))
                            .size(px(11.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(if fill_copy { CursorStyle::DragCopy } else { CursorStyle::Crosshair })
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_fill_handle_down))
                            .child(div().size(px(7.)).bg(green).border_1().border_color(white())),
                    )
                }),
        )
    }

    /// Press on a worksheet: select the cell under the pointer.
    pub(crate) fn on_sheet_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool == Tool::TextBox {
            // A text box may be drawn over the sheet.
            return self.on_workspace_down(event, window, cx);
        }
        // The pen of the Draw panel draws (or erases) over the cells.
        if self.ink_press(event.position, cx) {
            cx.stop_propagation();
            return;
        }
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        self.set_selection(Vec::new());
        // A press in the box of the text being typed (which may reach over other cells) stays in it.
        if self.cell_edit().is_some_and(|e| e.slide == self.current_slide && e.rect.get().is_some_and(|r| r.contains(&event.position)))
            && self.press_in_cell_text(event.position, event.click_count, event.modifiers.shift, cx)
        {
            return;
        }
        let Some((column, row)) = self.cell_under(event.position) else { return cx.notify() };
        if let (Some(ws), Some(content)) = (self.workspace_bounds, self.workspace_content())
            && let Some((split_x, split_y)) = self.frozen_split(content)
        {
            let local = event.position - ws.origin;
            let slide = self.current_slide;
            if let Some(p) = self.wb_mut().panels.splits.get_mut(&slide) {
                p.active_pane = (
                    split_x.is_none_or(|sx| f32::from(local.x) >= sx),
                    split_y.is_none_or(|sy| f32::from(local.y) >= sy),
                );
            }
        }
        // A press on the text being typed puts the caret there (and selects by dragging).
        if self.cell_edit().is_some_and(|e| (e.column, e.row) == (column, row) && e.slide == self.current_slide)
            && self.press_in_cell_text(event.position, event.click_count, event.modifiers.shift, cx)
        {
            return;
        }
        // Typing a formula where a reference may go (after `=`, `(`, `,` or
        // an operator): the cell pressed goes into it (dragging makes it a range).
        if self.point_reference(column, row, cx) {
            if let Some(edit)=self.cell_edit_mut(){edit.reference_drag=true;}
            return;
        }
        // A double-click on the cell being typed into leaves it be; on
        // another cell, it edits that cell's content.
        if self.cell_edit().is_some_and(|e| (e.column, e.row) == (column, row) && e.slide == self.current_slide) {
            return;
        }
        self.commit_cell_edit(cx);
        if self.panel_message.is_some() { return; }
        if event.click_count >= 2 && !event.modifiers.shift {
            self.set_cell(column, row);
            if self.show_pivot_details(column, row, cx) {
                return;
            }
            let old_offset = self.workspace_scroll.offset();
            self.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx);
            let new_offset = self.workspace_scroll.offset();
            self.seed_edit_geometry(cx);
            let adjusted = point(event.position.x + (new_offset.x - old_offset.x), event.position.y + (new_offset.y - old_offset.y));
            self.press_in_cell_text(adjusted, 1, false, cx);
            return;
        }
        if event.click_count==1&&!event.modifiers.shift&&!event.modifiers.platform&&self.current_sheet().is_some_and(|(_,s)|s.hyperlinks.iter().any(|h|h.range.first_row<=row&&row<=h.range.last_row&&h.range.first_column<=column&&column<=h.range.last_column)){
            self.set_cell(column,row);self.follow_cell_link(window,cx);return;
        }
        // ⌘-click on a linked cell follows the link; elsewhere it adds a
        // range to those selected (Excel's Ctrl+クリック), or deselects if already selected.
        if event.modifiers.platform && !event.modifiers.shift {
            let current = self.selected_ranges();
            let is_in_selection = current.iter().any(|r| {
                (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)
            });
            let total_cells: u32 = current.iter().map(|r| (r.last_row - r.first_row + 1) * (r.last_column - r.first_column + 1)).sum();

            if is_in_selection {
                if total_cells > 1 {
                    let base_ranges = current.clone();
                    let deselect_rect = CellRange { first_row: row, last_row: row, first_column: column, last_column: column };
                    let remaining = super::goto_special::subtract_ranges(&base_ranges, deselect_rect);
                    if !remaining.is_empty() {
                        self.set_selected_ranges(&remaining);
                        let state = self.wb_mut();
                        state.deselect_drag = Some(DeselectDrag { anchor: (column, row), base_ranges });
                        state.cell_drag = true;
                    }
                }
                return cx.notify();
            }

            let kept = self.selected_ranges();
            self.set_cell(column, row);
            if self.follow_cell_link(window, cx) {
                return;
            }
            self.keep_with_selection(kept);
            let state = self.wb_mut();
            state.deselect_drag = None;
            state.cell_drag = true;
            return cx.notify();
        }
        if event.modifiers.shift {
            // Shift extends the selection to the cell pressed.
            let was_kept = self.selected_ranges().len() > 1;
            let slide = self.current_slide;
            self.wb_mut().cell_ends.insert(slide, (column, row));
            self.extra_ranges_follow(was_kept);
        } else {
            self.set_cell(column, row);
        }
        // Dragging on from here selects a range.
        self.wb_mut().cell_drag = true;
        cx.notify();
    }

    pub(crate) fn cell_move(&self) -> Option<super::fill::CellMove> {
        self.sheet_state().and_then(|s| s.cell_move)
    }

    /// The pointer over the selection's border: the hand, a closed hand
    /// while it is dragged, the copy pointer with Option (#11).
    pub(crate) fn cell_move_cursor(&self, copy: bool) -> CursorStyle {
        match self.cell_move() {
            Some(m) if m.copy || copy => CursorStyle::DragCopy,
            Some(_) => CursorStyle::ClosedHand,
            None if copy => CursorStyle::DragCopy,
            None => CursorStyle::OpenHand,
        }
    }

    /// Press on the fill handle at the selection's corner: dragging it
    /// selects a range from the selected cell, or double-clicking fills down
    /// to the end of adjacent data.
    pub(crate) fn on_fill_handle_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.commit_cell_edit(cx);
        self.focus_handle.focus(window, cx);
        if event.click_count >= 2 {
            if self.double_click_fill_handle(cx) {
                self.wb_mut().cell_drag = false;
                if let Some(state) = self.sheet_state_mut() {
                    state.fill_source = None;
                }
                cx.notify();
                return;
            }
        }
        self.begin_fill();
        let state = self.wb_mut();
        state.fill_copy = event.modifiers.alt;
        state.cell_drag = true;
        cx.notify();
    }

    /// Where the frozen panes of the current worksheet end, in the
    /// workspace (which is also where they are in the unscrolled content):
    /// the x after the frozen columns and the y below the frozen rows, in
    /// pixels, each `None` if that axis has none frozen.
    pub(crate) fn frozen_split(&self, content: WorkspaceContent) -> Option<(Option<f32>, Option<f32>)> {
        let (shape, sheet) = self.current_sheet()?;
        let (columns, rows) = sheet.frozen.or_else(|| self.split_panes().map(|p| p.at))?;
        let at = |offsets: Vec<i64>, i: u32| offsets[(i as usize).min(offsets.len() - 1)];
        let head_w = if sheet.headings { row_header_width(sheet) } else { 0.0 };
        let head_h = if sheet.headings { COLUMN_HEADER_HEIGHT } else { 0.0 };
        let x = (columns > 0).then(|| {
            (f32::from(content.slide_origin.x) + (shape.frame.x + at(sheet.column_offsets(), columns)) as f32 * content.scale)
                .max(head_w)
        });
        let y = (rows > 0).then(|| {
            (f32::from(content.slide_origin.y) + (shape.frame.y + at(sheet.row_offsets(), rows)) as f32 * content.scale)
                .max(head_h)
        });
        (x.is_some() || y.is_some()).then_some((x, y))
    }

    /// The slide's layout for a window position: over the frozen panes the
    /// sheet does not scroll on their axis.
    fn sheet_layout_at(&self, position: Point<Pixels>) -> Option<SlideLayout> {
        // The page layout view: the page under the pointer (none over a margin or a gap).
        if self.page_layout_shown() {
            return self.page_layout_at(position);
        }
        let mut layout = self.slide_layout()?;
        let (Some(ws), Some(content)) = (self.workspace_bounds, self.workspace_content()) else { return Some(layout) };
        let Some((split_x, split_y)) = self.frozen_split(content) else { return Some(layout) };
        let local = position - ws.origin;
        let shift = self.pane_shift();
        let main_lead = self.main_pane_lead();
        if split_x.is_some_and(|x| f32::from(local.x) < x) {
            layout.origin.x = ws.origin.x + content.slide_origin.x + shift.x;
        } else {
            layout.origin.x += main_lead.x;
        }
        if split_y.is_some_and(|y| f32::from(local.y) < y) {
            layout.origin.y = ws.origin.y + content.slide_origin.y + shift.y;
        } else {
            layout.origin.y += main_lead.y;
        }
        Some(layout)
    }

    /// The window position of a cell's centre (unfrozen part), for scripted clicks.
    #[allow(dead_code)] // For the scenario tests.
    pub(crate) fn cell_center(&self, column: u32, row: u32) -> Option<Point<Pixels>> {
        if column >= MAX_SHEET_COLUMNS || row >= MAX_SHEET_ROWS {
            return None;
        }
        if self.page_layout_shown() {
            return self.page_layout_cell_center(column, row);
        }
        let layout = self.slide_layout()?;
        let main_lead = self.main_pane_lead();
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at_x = |i: u32| {
            if (i as usize) < xs.len() {
                xs[i as usize]
            } else {
                let last = xs.last().copied().unwrap_or(0);
                let def = sheet.default_column.max(1);
                last + (i as usize - (xs.len().saturating_sub(1))) as i64 * def
            }
        };
        let at_y = |i: u32| {
            if (i as usize) < ys.len() {
                ys[i as usize]
            } else {
                let last = ys.last().copied().unwrap_or(0);
                let def = sheet.default_row.max(1);
                last + (i as usize - (ys.len().saturating_sub(1))) as i64 * def
            }
        };
        let x = shape.frame.x + (at_x(column) + at_x(column + 1)) / 2;
        let y = shape.frame.y + (at_y(row) + at_y(row + 1)) / 2;
        Some(point(layout.origin.x + main_lead.x + layout.to_px(x), layout.origin.y + main_lead.y + layout.to_px(y)))
    }

    /// The box of the cell being typed into (window position) and its text's layout, as the next paint
    /// makes them, so a press that starts the edit (a double-click) can find the character under it
    /// before anything has been painted.
    pub(crate) fn seed_edit_geometry(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.cell_edit().cloned() else { return };
        let Some(layout) = self.slide_layout() else { return };
        let Some((shape, sheet)) = self.current_sheet() else { return };
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |offsets: &[i64], i: u32, default: i64| {
            if (i as usize) < offsets.len() {
                offsets[i as usize]
            } else {
                offsets.last().copied().unwrap_or(0) + (i as usize - offsets.len().saturating_sub(1)) as i64 * default.max(1)
            }
        };
        let range = sheet.merge_at(e.row, e.column).unwrap_or(CellRange { first_row: e.row, first_column: e.column, last_row: e.row, last_column: e.column });
        let (l, r) = (at(&xs, range.first_column, sheet.default_column), at(&xs, range.last_column + 1, sheet.default_column));
        let (t, b) = (at(&ys, range.first_row, sheet.default_row), at(&ys, range.last_row + 1, sheet.default_row));
        let scale = f32::from(layout.to_px(1_000_000)) / 1_000_000.0;
        let px_per_point = ibis_doc_model::EMU_PER_POINT as f32 * scale;
        let origin = point(layout.origin.x + layout.to_px(shape.frame.x + l), layout.origin.y + layout.to_px(shape.frame.y + t));
        let (w, h) = (r - l, b - t);
        let ts = WindowTextSystem::new(cx.text_system().clone());
        let runs = e.runs.clone().map(std::sync::Arc::new);
        let sheet_origin = (l as f32 * scale, t as f32 * scale);
        let default_style = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref());
        let laid = super::sheet_view::edit_layout(&ts, sheet, default_style, &self.resources, e.row, e.column, &e.text, runs, sheet_origin, (w as f32 * scale, h as f32 * scale), px_per_point, e.layout.borrow().as_ref().map_or(f32::MAX,|l|l.width));
        e.pixels_per_point.set(px_per_point);
        e.rect.set(Some(Bounds::new(origin, size(px(laid.width), px(laid.height)))));
        *e.layout.borrow_mut() = Some(std::rc::Rc::new(laid));
    }

    /// The window position of a cell's bottom-right corner (where the fill
    /// handle is when it ends the selection), for scripted drags.
    #[allow(dead_code)] // For the scenario tests.
    pub(crate) fn cell_corner(&self, column: u32, row: u32) -> Option<Point<Pixels>> {
        if column >= MAX_SHEET_COLUMNS || row >= MAX_SHEET_ROWS {
            return None;
        }
        let layout = self.slide_layout()?;
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at_x = |i: u32| {
            if (i as usize) < xs.len() {
                xs[i as usize]
            } else {
                let last = xs.last().copied().unwrap_or(0);
                let def = sheet.default_column.max(1);
                last + (i as usize - (xs.len().saturating_sub(1))) as i64 * def
            }
        };
        let at_y = |i: u32| {
            if (i as usize) < ys.len() {
                ys[i as usize]
            } else {
                let last = ys.last().copied().unwrap_or(0);
                let def = sheet.default_row.max(1);
                last + (i as usize - (ys.len().saturating_sub(1))) as i64 * def
            }
        };
        let x = shape.frame.x + at_x(column + 1);
        let y = shape.frame.y + at_y(row + 1);
        Some(point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(y)))
    }

    /// The cell under a window position, if it is on the worksheet.
    pub(crate) fn cell_under(&self, position: Point<Pixels>) -> Option<(u32, u32)> {
        let (layout, (shape, sheet)) = self.sheet_layout_at(position).zip(self.current_sheet())?;
        let p = layout.to_emu(position);
        sheet.cell_at(p.x - shape.frame.x, p.y - shape.frame.y)
    }

    /// Page break preview (Excel's 改ページ プレビュー): the cells outside
    /// the print area greyed and a blue line around it, in the slide's
    /// coordinates. Without a print area, the page the sheet prints is used.
    pub(crate) fn render_page_break_preview(&self, scale: f32, lines_layer: bool) -> Option<Div> {
        let (shape, sheet) = self.current_sheet()?;
        if !sheet.page_break_preview && !sheet.page_layout {
            return None;
        }
        // (The page layout view draws the pages themselves, where it can: page_layout.rs.)
        if self.page_layout_shown() {
            return None;
        }
        let layout_view = sheet.page_layout;
        let frame = shape.frame;
        let (x0, y0, x1, y1) = match sheet.print_area {
            Some(area) => {
                let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
                // Whole columns or rows (an open end) reach the grid's end.
                let at = |offsets: &[i64], i: u32| offsets[(i as usize).min(offsets.len() - 1)];
                (
                    frame.x + at(&xs, area.first_column),
                    frame.y + at(&ys, area.first_row),
                    frame.x + at(&xs, area.last_column.saturating_add(1)),
                    frame.y + at(&ys, area.last_row.saturating_add(1)),
                )
            }
            None => {
                let page = self.presentation().size_of(self.slide());
                (frame.x, frame.y, frame.x + page.width.min(frame.width), frame.y + page.height.min(frame.height))
            }
        };
        let to_px = |emu: i64| emu as f32 * scale;
        let (fl, ft, fr, fb) = (to_px(frame.x), to_px(frame.y), to_px(frame.right()), to_px(frame.bottom()));
        let (pl, pt, pr, pb) = (to_px(x0), to_px(y0), to_px(x1), to_px(y1));
        let grey = rgba(OUTSIDE_PRINT_AREA);
        let rect = |l: f32, t: f32, r: f32, b: f32| {
            (r > l && b > t).then(|| div().absolute().left(px(l)).top(px(t)).w(px(r - l)).h(px(b - t)).bg(grey))
        };
        let shades = if layout_view || lines_layer { [None, None, None, None] } else { [
            rect(fl, ft, fr, pt),
            rect(fl, pb, fr, fb),
            rect(fl, pt.max(ft), pl, pb.min(fb)),
            rect(pr, pt.max(ft), fr, pb.min(fb)),
        ] };
        let w = PRINT_AREA_LINE_WIDTH;
        // Where the pages break: a dashed blue line at an automatic break, a
        // solid one at a manual break, and the page's number in grey.
        // (The pages come from the exporter, which is not in every build: without it, no page lines.)
        #[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
        let (print_scale, pages) = crate::export::preview_pages(self.presentation(), self.slide(), &self.resources);
        #[cfg(not(all(any(target_os = "macos", target_os = "windows"), feature = "export")))]
        let (print_scale, pages): (f64, Vec<ibis_doc_model::EmuRect>) = (1.0, Vec::new());
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let manual_x: Vec<i64> = sheet.print.column_breaks.iter().filter_map(|b| xs.get(*b as usize)).map(|x| frame.x + x).collect();
        let manual_y: Vec<i64> = sheet.print.row_breaks.iter().filter_map(|b| ys.get(*b as usize)).map(|y| frame.y + y).collect();
        let blue = rgb(PRINT_AREA_LINE);
        let mut lines: Vec<Div> = Vec::new();
        for page in pages.iter().filter(|_| lines_layer) {
            let (l, t, r, b) = (to_px(page.x), to_px(page.y), to_px(page.x + page.width), to_px(page.y + page.height));
            // The left and top edges (the previous page's right and bottom) that are not the area's start.
            if page.x > x0 {
                let line = div().absolute().left(px(l - 0.5)).top(px(t)).w(px(1.0)).h(px(b - t));
                lines.push(if manual_x.contains(&page.x) { line.w(px(2.0)).left(px(l - 1.0)).bg(blue) } else { line.border_l_1().border_dashed().border_color(blue) });
            }
            if page.y > y0 {
                let line = div().absolute().left(px(l)).top(px(t - 0.5)).w(px(r - l)).h(px(1.0));
                lines.push(if manual_y.contains(&page.y) { line.h(px(2.0)).top(px(t - 1.0)).bg(blue) } else { line.border_t_1().border_dashed().border_color(blue) });
            }
        }
        let numbers = pages.iter().enumerate().filter(|_| lines_layer).map(|(i, page)| {
            let (l, t, r, b) = (to_px(page.x), to_px(page.y), to_px(page.x + page.width), to_px(page.y + page.height));
            div()
                .absolute()
                .left(px(l))
                .top(px(t))
                .w(px(r - l))
                .h(px(b - t))
                .flex()
                .items_center()
                .justify_center()
                .whitespace_nowrap().overflow_hidden().text_size(px(((r - l) * 0.18).clamp(10.0, 160.0)))
                .text_color(rgba(0x80808060))
                .child(SharedString::from(crate::shell::i18n::format("xlsx.page-break-preview-page", &[("page", &(i + 1))])))
        });
        // Page layout view: each page's printed area outlined, and the paper's
        // edge (the margins, which print at the print scale) around all of them.
        let mut frames: Vec<Div> = Vec::new();
        if layout_view && lines_layer && !pages.is_empty() {
            let grey = rgb(0x9a9a9a);
            for page in pages.iter() {
                let (l, t, r, b) = (to_px(page.x), to_px(page.y), to_px(page.x + page.width), to_px(page.y + page.height));
                frames.push(div().absolute().left(px(l)).top(px(t)).w(px(r - l)).h(px(b - t)).border_1().border_color(grey));
            }
            let [ml, mr, mt, mb] = sheet.print.margins;
            let k = 1.0 / print_scale.max(0.1);
            let l = pages.iter().map(|p| p.x).min().unwrap_or(0) as f64 - ml as f64 * k;
            let t = pages.iter().map(|p| p.y).min().unwrap_or(0) as f64 - mt as f64 * k;
            let r = pages.iter().map(|p| p.x + p.width).max().unwrap_or(0) as f64 + mr as f64 * k;
            let b = pages.iter().map(|p| p.y + p.height).max().unwrap_or(0) as f64 + mb as f64 * k;
            let (l, t, r, b) = (to_px(l as i64), to_px(t as i64), to_px(r as i64), to_px(b as i64));
            frames.push(div().absolute().left(px(l)).top(px(t)).w(px(r - l)).h(px(b - t)).border_1().border_dashed().border_color(grey));
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .children(shades.into_iter().flatten())
                .children(numbers)
                .children(lines)
                .children(frames)
                .when(pr > pl && pb > pt && !layout_view && !lines_layer, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(pl - w / 2.0))
                            .top(px(pt - w / 2.0))
                            .w(px(pr - pl + w))
                            .h(px(pb - pt + w))
                            .border(px(w))
                            .border_color(rgb(PRINT_AREA_LINE)),
                    )
                }),
        )
    }

    /// Stretch the selected range to the cell under `position` (the nearest
    /// cell at the sheet's edges).
    pub(crate) fn extend_cells_to(&mut self, position: Point<Pixels>) {
        let Some((layout, (shape, sheet))) = self.sheet_layout_at(position).zip(self.current_sheet()) else { return };
        let p = layout.to_emu(position);
        let clamp = |offsets: Vec<i64>, v: i64| -> u32 {
            let last = offsets.len().saturating_sub(2);
            (offsets.partition_point(|o| *o <= v).saturating_sub(1)).min(last) as u32
        };
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let (px_, py_) = (p.x - shape.frame.x, p.y - shape.frame.y);
        let mut column = clamp(xs.clone(), px_);
        let mut row = clamp(ys.clone(), py_);
        // Dragging the fill handle goes one way (`fill_direction`).
        if let Some(source) = self.sheet_state().and_then(|s| s.fill_source) {
            (column, row) = super::fill::fill_target(source, (column, row), (px_, py_), &xs, &ys);
        }
        if let Some(drag) = self.sheet_state().and_then(|s| s.deselect_drag.clone()) {
            let deselect_rect = CellRange {
                first_row: drag.anchor.1.min(row),
                last_row: drag.anchor.1.max(row),
                first_column: drag.anchor.0.min(column),
                last_column: drag.anchor.0.max(column),
            };
            self.deselect_range(&drag.base_ranges, deselect_rect);
            return;
        }
        let anchor = self.current_cell();
        let was_kept = self.selected_ranges().len() > 1;
        if anchor == Some((column, row)) {
            let slide = self.current_slide;
            self.wb_mut().cell_ends.remove(&slide);
        } else {
            let slide = self.current_slide;
            self.wb_mut().cell_ends.insert(slide, (column, row));
        }
        self.extra_ranges_follow(was_kept);
    }

    /// Move the selection one cell (or past the merged range it is on),
    /// skipping hidden rows and columns and stopping at the sheet's edge.
    pub(crate) fn move_cell(&mut self, dx: i32, dy: i32, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(d) = self.sheet_state_mut().and_then(|s| s.panels.cells_dialog.as_mut()) {
            if dy > 0 {
                d.choice = (d.choice + 1).min(3);
                cx.notify();
                return;
            } else if dy < 0 {
                d.choice = d.choice.saturating_sub(1);
                cx.notify();
                return;
            }
        }
        if self.editing.is_none() && !self.selected_ids().is_empty(){self.nudge_selected(dx as i64,dy as i64,false,cx);return;}
        if self.editing.is_some() {
            return;
        }
        if dx==0 && self.cell_edit().is_some_and(|e|e.dragging_in_bar) && self.formula_edit_command(if dy<0{"up"}else{"down"},Modifiers::default(),cx){return;}
        // While typing: ← → move the caret in Edit mode; otherwise the entry is finished first.
        if self.cell_edit().is_some() {
            if dx==0 && self.completion_move(dy,cx){return;}
            if self.formula_reference_key(dx,dy,false,false,cx){return;}
            if dy == 0 && self.cell_caret(dx, cx) {
                return;
            }
            if dx == 0 && self.cell_caret_line(dy, false, cx) {
                return;
            }
            if dx == 0 && self.completion_move(dy, cx) {
                return;
            }
            self.commit_cell_edit(cx);
            if self.cell_edit().is_some() { return; }
        }
        let (Some((_, sheet)), Some((column, row))) = (self.current_sheet(), self.current_cell()) else {
            return;
        };
        // From the selected cell (or its merged range), not the whole range.
        let range = sheet.merge_at(row, column).unwrap_or(CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
        let (columns, rows) = (sheet.columns.clone(), sheet.rows.clone());
        let step = |at: u32, first: u32, last: u32, d: i32, sizes: &[i64], limit: u32| -> Option<u32> {
            if d == 0 {
                return Some(at);
            }
            let mut i = if d > 0 { last as i64 + 1 } else { first as i64 - 1 };
            while (0..limit as i64).contains(&i) {
                let is_hidden = (0..sizes.len() as i64).contains(&i) && sizes[i as usize] == 0;
                if !is_hidden {
                    break;
                }
                i += d.signum() as i64;
            }
            (0..limit as i64).contains(&i).then_some(i as u32)
        };
        let next = step(column, range.first_column, range.last_column, dx, &columns, MAX_SHEET_COLUMNS)
            .zip(step(row, range.first_row, range.last_row, dy, &rows, MAX_SHEET_ROWS));
        let Some((column, row)) = next else { return };
        self.set_selection(Vec::new());
        self.set_cell(column, row);
        self.scroll_cell_into_view();
        cx.notify();
    }

    /// Tab: as → (after finishing the entry), remembering the column the
    /// Tabs began at for Enter.
    pub(crate) fn cell_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The formula's list open: Tab puts in the item chosen.
        if self.completion_accept(cx) {
            return;
        }
        let Some((column, _)) = self.current_cell() else { return };
        let start = self.wb().tab_start.filter(|t| Some(t.1) == self.current_cell()).map_or(column, |t| t.0);
        self.commit_cell_edit(cx);
        if self.cell_edit().is_some() { return; }
        self.move_cell(1, 0, window, cx);
        let here = self.current_cell();
        self.wb_mut().tab_start = here.map(|h| (start, h));
    }

    /// Enter: down a row; after Tabs, back to the column they began at (Excel).
    pub(crate) fn cell_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_state().is_some_and(|s| s.panels.cells_dialog.is_some()) {
            self.apply_cells_dialog(cx);
            return;
        }
        let tab = self.wb_mut().tab_start.take().filter(|t| Some(t.1) == self.current_cell());
        match tab {
            Some((start, (_, row))) => {
                self.commit_cell_edit(cx);
                if self.cell_edit().is_some() { return; }
                if row + 1 < MAX_SHEET_ROWS {
                    self.set_selection(Vec::new());
                    self.set_cell(start, row + 1);
                    self.scroll_cell_into_view();
                }
                cx.notify();
            }
            None => {
                // Enter finishes typing and goes to the next cell (↓ in a cell of many lines would move the caret instead).
                if self.cell_edit().is_some() {
                    if self.accept_flash_fill_preview(cx) {
                        self.move_cell(0, 1, window, cx);
                        return;
                    }
                    self.commit_cell_edit(cx);
                    if self.cell_edit().is_some() {
                        return;
                    }
                }
                self.move_cell(0, 1, window, cx)
            }
        }
    }

    /// Shift+arrow: move the range's far corner (the one away from the
    /// selected cell) a cell, past hidden rows and columns, as Excel does.
    pub(crate) fn extend_cells(&mut self, dx: i32, dy: i32, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        if dx==0 && self.cell_edit().is_some_and(|e|e.dragging_in_bar) && self.formula_edit_command(if dy<0{"up"}else{"down"},Modifiers{shift:true,..Default::default()},cx){return;}
        if self.formula_reference_key(dx,dy,true,false,cx){return;}
        // Typing in Edit mode: Shift+← → select text.
        if dy == 0 && self.cell_extend_selection(dx, cx) {
            return;
        }
        if dx == 0 && self.cell_caret_line(dy, true, cx) {
            return;
        }
        let (Some((_, sheet)), Some(range), Some((column, row))) = (self.current_sheet(), self.selected_range(), self.current_cell()) else {
            return;
        };
        // The far edge in each direction: the side of the range the selected
        // cell is not on.
        let far_column = if column > range.first_column { range.first_column } else { range.last_column };
        let far_row = if row > range.first_row { range.first_row } else { range.last_row };
        let step = |at: u32, d: i32, sizes: &[i64], limit: u32| -> u32 {
            if d == 0 {
                return at;
            }
            let mut i = at as i64 + d as i64;
            while (0..limit as i64).contains(&i) {
                let is_hidden = (0..sizes.len() as i64).contains(&i) && sizes[i as usize] == 0;
                if !is_hidden {
                    break;
                }
                i += d.signum() as i64;
            }
            if (0..limit as i64).contains(&i) { i as u32 } else { at }
        };
        let end = (step(far_column, dx, &sheet.columns, MAX_SHEET_COLUMNS), step(far_row, dy, &sheet.rows, MAX_SHEET_ROWS));
        let was_kept = self.selected_ranges().len() > 1;
        if end == (column, row) {
            let slide = self.current_slide;
            self.wb_mut().cell_ends.remove(&slide);
        } else {
            let slide = self.current_slide;
            self.wb_mut().cell_ends.insert(slide, end);
        }
        self.extra_ranges_follow(was_kept);
        self.scroll_cell_into_view();
        cx.notify();
    }

    /// Select a cell of the current worksheet (its merged range's top-left
    /// cell if it is merged).
    pub(crate) fn set_cell(&mut self, column: u32, row: u32) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let cell = sheet.merge_at(row, column).map_or((column, row), |m| (m.first_column, m.first_row));
        let slide = self.current_slide;
        self.wb_mut().cells.insert(slide, cell);
        let slide = self.current_slide;
        self.wb_mut().cell_ends.remove(&slide);
    }

    /// Scroll the workspace so the selected cell is in view, clear of the headers.
    pub(crate) fn scroll_cell_into_view(&mut self) {
        self.scroll_range_into_view(false);
    }

    /// Scroll the workspace so the selected range is in view, clear of the
    /// headers: as little as it takes, or (`top_left`, as Excel's ジャンプ
    /// and name box do) on each axis it is not in view, with the range at
    /// the view's top or left, past frozen panes.
    pub(crate) fn scroll_range_into_view(&mut self, top_left: bool) {
        if self.page_layout_shown() {
            self.page_layout_scroll_into_view();
            return;
        }
        let (Some(content), Some(ws), Some((shape, sheet)), Some(range)) =
            (self.workspace_content(), self.workspace_bounds, self.current_sheet(), self.selected_range())
        else {
            return;
        };
        let scale = content.scale;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let (frozen_columns, frozen_rows) = sheet.frozen.unwrap_or((0, 0));
        let (split_x, split_y) = self.frozen_split(content).unwrap_or((None, None));
        let split_panes = self.split_panes();
        let head_w = if sheet.headings { row_header_width(sheet) } else { 0.0 };
        let head_h = if sheet.headings { COLUMN_HEADER_HEIGHT } else { 0.0 };
        let default_column = sheet.default_column;
        let default_row = sheet.default_row;
        let frame_x = shape.frame.x;
        let frame_y = shape.frame.y;
        let at_x = |offsets: &[i64], i: u32| {
            let len = offsets.len();
            if (i as usize) < len {
                offsets[i as usize]
            } else {
                let last = offsets.last().copied().unwrap_or(0);
                last + (i as usize - (len - 1)) as i64 * default_column
            }
        };
        let at_y = |offsets: &[i64], i: u32| {
            let len = offsets.len();
            if (i as usize) < len {
                offsets[i as usize]
            } else {
                let last = offsets.last().copied().unwrap_or(0);
                last + (i as usize - (len - 1)) as i64 * default_row
            }
        };
        let span = |origin: Pixels, frame: i64, offsets: &[i64], first: u32, last: u32, is_y: bool| {
            let (f, l) = if is_y { (at_y(offsets, first), at_y(offsets, last + 1)) } else { (at_x(offsets, first), at_x(offsets, last + 1)) };
            let start = f32::from(origin) + (frame + f) as f32 * scale;
            let end = f32::from(origin) + (frame + l) as f32 * scale;
            (start, end)
        };
        // `visible` is the scrolled-away amount; the view shows content from
        // `visible + head` to `visible + view`.
        let axis = |visible: f32, (start, end): (f32, f32), head: f32, view: f32, content: f32| {
            let visible = if start - head < visible {
                start - head
            } else if end > visible + view - SCROLLBAR_ROOM && top_left {
                start - head
            } else if end > visible + view - SCROLLBAR_ROOM {
                (end - view + SCROLLBAR_ROOM).min(start - head)
            } else {
                visible
            };
            -visible.clamp(0.0, (content - view).max(0.0))
        };
        let offset = self.workspace_scroll.offset();
        let (start_x, end_x) = span(content.slide_origin.x, frame_x, &xs, range.first_column, range.last_column, false);
        let (start_y, end_y) = span(content.slide_origin.y, frame_y, &ys, range.first_row, range.last_row, true);

        if let Some(mut panes) = split_panes {
            let mut ws_x = f32::from(offset.x);
            let mut ws_y = f32::from(offset.y);
            let ws_w = f32::from(ws.size.width);
            let ws_h = f32::from(ws.size.height);
            let content_w = f32::from(content.size.width);
            let content_h = f32::from(content.size.height);
            let main_axis = |cur_shift: f32, (start, end): (f32, f32), split: f32, head: f32, view: f32, content_sz: f32| -> (f32, f32) {
                let max_lead = (split - head).max(0.0);
                let min_shift = -(content_sz - view).max(0.0);
                let next_shift = if start + cur_shift < split {
                    split - start
                } else if end + cur_shift > view - SCROLLBAR_ROOM && top_left {
                    split - start
                } else if end + cur_shift > view - SCROLLBAR_ROOM {
                    (view - SCROLLBAR_ROOM - end).max(split - start)
                } else {
                    cur_shift
                }
                .clamp(min_shift, max_lead);
                (next_shift.min(0.0), next_shift.max(0.0))
            };
            if top_left {
                panes.main_lead = Point::default();
                if let Some(sx) = split_x {
                    panes.shift.x = px(axis(-f32::from(panes.shift.x), (start_x, end_x), head_w, sx, content_w));
                }
                if let Some(sy) = split_y {
                    panes.shift.y = px(axis(-f32::from(panes.shift.y), (start_y, end_y), head_h, sy, content_h));
                }
                let head_x = if range.last_column < panes.at.0 { head_w } else { split_x.unwrap_or(head_w) };
                let head_y = if range.last_row < panes.at.1 { head_h } else { split_y.unwrap_or(head_h) };
                ws_x = axis(-ws_x, (start_x, end_x), head_x, ws_w, content_w);
                ws_y = axis(-ws_y, (start_y, end_y), head_y, ws_h, content_h);
            } else {
                if let Some(sx) = split_x {
                    if !panes.active_pane.0 {
                        panes.shift.x = px(axis(-f32::from(panes.shift.x), (start_x, end_x), head_w, sx, content_w));
                    } else {
                        let (wx, lx) = main_axis(ws_x + f32::from(panes.main_lead.x), (start_x, end_x), sx, head_w, ws_w, content_w);
                        ws_x = wx;
                        panes.main_lead.x = px(lx);
                    }
                } else {
                    ws_x = axis(-ws_x, (start_x, end_x), head_w, ws_w, content_w);
                }

                if let Some(sy) = split_y {
                    if !panes.active_pane.1 {
                        panes.shift.y = px(axis(-f32::from(panes.shift.y), (start_y, end_y), head_h, sy, content_h));
                    } else {
                        let (wy, ly) = main_axis(ws_y + f32::from(panes.main_lead.y), (start_y, end_y), sy, head_h, ws_h, content_h);
                        ws_y = wy;
                        panes.main_lead.y = px(ly);
                    }
                } else {
                    ws_y = axis(-ws_y, (start_y, end_y), head_h, ws_h, content_h);
                }
            }
            let slide = self.current_slide;
            self.wb_mut().panels.splits.insert(slide, panes);
            self.workspace_scroll.set_offset(point(px(ws_x), px(ws_y)));
            return;
        }

        let x = if range.last_column < frozen_columns {
            f32::from(offset.x)
        } else {
            let head = split_x.unwrap_or(head_w);
            axis(-f32::from(offset.x), (start_x, end_x), head, f32::from(ws.size.width), f32::from(content.size.width))
        };
        let y = if range.last_row < frozen_rows {
            f32::from(offset.y)
        } else {
            let head = split_y.unwrap_or(head_h);
            axis(-f32::from(offset.y), (start_y, end_y), head, f32::from(ws.size.height), f32::from(content.size.height))
        };
        self.workspace_scroll.set_offset(point(px(x), px(y)));
    }

    /// Maximum height (in pixels) the cell-edit box for `row` may grow to: from the cell's top down to the bottom of
    /// the sheet view (the top of the sheet tab bar), at least the cell's own height (`ey1 - ey0`).
    pub(crate) fn max_edit_box_height_px(&self, row: u32, ey0: f32, ey1: f32, px_per_point: f32) -> f32 {
        let cell_h = (ey1 - ey0).max(1.0);
        let (Some(content), Some(ws), Some((_, sheet))) = (self.workspace_content(), self.workspace_bounds, self.current_sheet()) else {
            return (409.5 * px_per_point).max(cell_h);
        };
        let (_, frozen_rows) = sheet.frozen.unwrap_or((0, 0));
        let shift_y = if row < frozen_rows {
            f32::from(self.pane_shift().y)
        } else {
            f32::from(self.workspace_scroll.offset().y)
        };
        let cell_top_in_ws = f32::from(content.slide_origin.y) + ey0 + shift_y;
        (f32::from(ws.size.height) - cell_top_in_ws - 2.0).max(cell_h)
    }

    /// Scroll horizontally so a span of `width_px` starting at `first_column`'s left edge is in view: if it fits
    /// within the viewport width (past the row header / frozen columns), scroll right minimally so its right edge is
    /// visible; if it is wider than the viewport width, align `first_column`'s left edge with the row header's right edge.
    pub(crate) fn scroll_span_horizontally_into_view(&self, first_column: u32, width_px: f32) {
        if self.page_layout_shown() {
            return;
        }
        let (Some(content), Some(ws), Some((shape, sheet))) =
            (self.workspace_content(), self.workspace_bounds, self.current_sheet())
        else {
            return;
        };
        let (frozen_columns, _) = sheet.frozen.unwrap_or((0, 0));
        if first_column < frozen_columns {
            return;
        }
        let scale = content.scale;
        let xs = sheet.column_offsets();
        let col_x = if (first_column as usize) < xs.len() {
            xs[first_column as usize]
        } else {
            let last = xs.last().copied().unwrap_or(0);
            last + (first_column as usize - xs.len().saturating_sub(1)) as i64 * sheet.default_column
        };
        let start = f32::from(content.slide_origin.x) + (shape.frame.x + col_x) as f32 * scale;
        let end = start + width_px;
        let (split_x, _) = self.frozen_split(content).unwrap_or((None, None));
        let head = split_x.unwrap_or(if sheet.headings { row_header_width(sheet) } else { 0.0 });
        let view = f32::from(ws.size.width);
        let right_limit = (view - SCROLLBAR_ROOM).max(head);
        let avail_w = (right_limit - head).max(0.0);
        let offset = self.workspace_scroll.offset();
        let mut visible = -f32::from(offset.x);
        if width_px > avail_w {
            visible = start - head;
        } else if start - head < visible {
            visible = start - head;
        } else if end > visible + right_limit {
            visible = end - right_limit;
        }
        self.workspace_scroll.set_offset(point(px(-visible.max(0.0)), offset.y));
    }

    /// After auto-fitting columns `first..=last`, scroll horizontally so the fitted span is in view (or aligned at
    /// the row header's right edge when wider than the viewport).
    pub(crate) fn scroll_columns_horizontally_into_view(&self, first: u32, last: u32) {
        let (Some(content), Some((_, sheet))) = (self.workspace_content(), self.current_sheet()) else { return };
        let xs = sheet.column_offsets();
        let at_x = |i: u32| {
            if (i as usize) < xs.len() {
                xs[i as usize]
            } else {
                let end = xs.last().copied().unwrap_or(0);
                end + (i as usize - xs.len().saturating_sub(1)) as i64 * sheet.default_column
            }
        };
        let width_px = (at_x(last + 1) - at_x(first)).max(0) as f32 * content.scale;
        self.scroll_span_horizontally_into_view(first, width_px);
    }
}

/// The name box and the ジャンプ dialog, where a cell reference is typed to
/// select it, as in Excel.
pub(crate) struct CellJump {
    /// The name box, at the left of the formula bar.
    pub(crate) name_box: Entity<InputState>,
    /// What the name box was last set to show (the selection), so it is set
    /// again only when that changes.
    name_box_shown: String,
    /// The press under way put the keys in the name box: its release
    /// selects the reference, to be typed over, as in Excel (after the
    /// press has put the caret where it was).
    select_on_release: bool,
    /// The name box dropdown menu is open.
    /// The 参照先 field of the ジャンプ dialog.
    goto_input: Entity<InputState>,
    /// The ジャンプ dialog is open.
    pub(crate) goto_open: bool,
    /// Why the dialog's reference was not taken, shown in it.
    goto_error: Option<SharedString>,
    /// A jump was made before the workspace was laid out: its range is to
    /// be scrolled into view once it is.
    pub(crate) pending_scroll: bool,
    /// History of jump references (cells, ranges, names).
    pub(crate) goto_history: Vec<String>,
    /// The name field over a sheet tab being renamed (a double-click on
    /// the tab, or シートの名前を変更).
    pub(crate) tab_rename: Entity<InputState>,
    /// The sheet being renamed.
    pub(crate) renaming: Option<SlideId>,
    _subscriptions: [Subscription; 3],
}

impl CellJump {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let name_box = cx.new(|cx| InputState::new(window, cx));
        let goto_input = cx.new(|cx| InputState::new(window, cx));
        let name_box_subscription = cx.subscribe_in(&name_box, window, |this, input, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } => {
                let text = input.read(cx).value().to_string();
                if this.go_to_reference(&text, window, cx) {
                    this.focus_handle.focus(window, cx);
                } else if super::names::valid_name(text.trim()) && this.name_selection(text.trim(), cx) {
                    // A new name for the selected range, as in Excel.
                    this.focus_handle.focus(window, cx);
                } else {
                    // As after Excel's error: the reference, selected, to be put right.
                    this.status = Some(crate::shell::i18n::format("_xlsx.invalid-reference-text", &[("text", &text.trim())]).into());
                    input.update(cx, |input, cx| input.select_all(window, cx));
                    cx.notify();
                }
            }
            _ => {}
        });
        let goto_subscription = cx.subscribe_in(&goto_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.confirm_goto(window, cx);
            }
        });
        let tab_rename = cx.new(|cx| InputState::new(window, cx));
        let rename_subscription = cx.subscribe_in(&tab_rename, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } | InputEvent::Blur => this.confirm_sheet_rename(window, cx),
            _ => {}
        });
        Self {
            name_box,
            name_box_shown: String::new(),
            select_on_release: false,
            goto_input,
            goto_open: false,
            goto_error: None,
            pending_scroll: false,
            goto_history: Vec::new(),
            tab_rename,
            renaming: None,
            _subscriptions: [name_box_subscription, goto_subscription, rename_subscription],
        }
    }
}

/// A place typed in the name box or the ジャンプ dialog: cells or a name, on
/// the sheet named or (`None`) the current one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Reference {
    pub(crate) sheet: Option<String>,
    pub(crate) target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// A cell or a range, 0-based; whole columns or rows reach [`OPEN_END`].
    Cells(CellRange),
    /// A defined name, such as `Print_Area`.
    Name(String),
}

/// The far end of whole columns or rows (`A:C`, `3:5`), before it is cut
/// to the sheet's grid.
pub(crate) const OPEN_END: u32 = u32::MAX;
/// Excel's limits: columns A to XFD, rows 1 to 1,048,576.
pub(crate) const MAX_COLUMNS: u32 = 16_384;
pub(crate) const MAX_ROWS: u32 = 1_048_576;

/// Read a reference as Excel's name box does: `B12`, `$B$12`, `b12`,
/// `A1:C5`, `A:C`, `3:5`, `Sheet2!A1`, `'Sheet name'!A1` (a `'` in the
/// name doubled) or a name. `None` if it is none of these.
pub(crate) fn parse_reference(text: &str) -> Option<Reference> {
    let text = text.trim();
    let (sheet, rest) = if let Some(quoted) = text.strip_prefix('\'') {
        // Up to the closing quote; `''` stands for one.
        let mut name = String::new();
        let mut chars = quoted.char_indices().peekable();
        let rest = loop {
            match chars.next()? {
                (_, '\'') if chars.peek().is_some_and(|(_, c)| *c == '\'') => {
                    chars.next();
                    name.push('\'');
                }
                (i, '\'') => break &quoted[i + 1..],
                (_, c) => name.push(c),
            }
        };
        (Some(name), rest.strip_prefix('!')?)
    } else if let Some((name, rest)) = text.rsplit_once('!') {
        (Some(name.to_string()), rest)
    } else {
        (None, text)
    };
    if sheet.as_deref().is_some_and(str::is_empty) {
        return None;
    }
    let target = match parse_cells(rest) {
        Some(range) => Target::Cells(range),
        None if is_name(rest) => Target::Name(rest.to_string()),
        None => return None,
    };
    Some(Reference { sheet, target })
}

/// One end of a range: a cell (`B12`), a whole column (`B`) or a whole row
/// (`12`), 0-based.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RangeEnd {
    Cell(u32, u32),
    Column(u32),
    Row(u32),
}

/// `B12`, `$B$12`, `b12`, `B` or `12`, within Excel's limits.
fn parse_range_end(text: &str) -> Option<RangeEnd> {
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(text.len());
    let (letters, digits) = text.split_at(split);
    let digits = if letters.is_empty() { digits } else { digits.strip_prefix('$').unwrap_or(digits) };
    if !digits.bytes().all(|b| b.is_ascii_digit()) || letters.len() > 3 {
        return None;
    }
    let column = letters.bytes().fold(0, |n, b| n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1));
    if column > MAX_COLUMNS {
        return None;
    }
    let row = match digits {
        "" => 0,
        _ => digits.parse::<u32>().ok().filter(|r| (1..=MAX_ROWS).contains(r))?,
    };
    match (column, row) {
        (0, 0) => None,
        (0, row) => Some(RangeEnd::Row(row - 1)),
        (column, 0) => Some(RangeEnd::Column(column - 1)),
        (column, row) => Some(RangeEnd::Cell(column - 1, row - 1)),
    }
}

/// A cell or a range of cells, columns or rows.
fn parse_cells(text: &str) -> Option<CellRange> {
    let mut ends = text.split(':');
    let first = parse_range_end(ends.next()?)?;
    let second = ends.next().map(parse_range_end);
    if ends.next().is_some() {
        return None;
    }
    let range = |c0: u32, r0: u32, c1: u32, r1: u32| CellRange {
        first_row: r0.min(r1),
        first_column: c0.min(c1),
        last_row: r0.max(r1),
        last_column: c0.max(c1),
    };
    match (first, second) {
        (RangeEnd::Cell(c, r), None) => Some(range(c, r, c, r)),
        (RangeEnd::Cell(c0, r0), Some(Some(RangeEnd::Cell(c1, r1)))) => Some(range(c0, r0, c1, r1)),
        (RangeEnd::Column(c0), Some(Some(RangeEnd::Column(c1)))) => Some(range(c0, 0, c1, OPEN_END)),
        (RangeEnd::Row(r0), Some(Some(RangeEnd::Row(r1)))) => Some(range(0, r0, OPEN_END, r1)),
        _ => None,
    }
}

/// Whether `text` can be a defined name: a letter, `_` or `\` and then
/// letters, digits, `_` and `.`.
pub(crate) fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// The print area's name, as the name box takes it (Excel keeps it as
/// `_xlnm.Print_Area`).
fn is_print_area(name: &str) -> bool {
    name.eq_ignore_ascii_case("Print_Area") || name.eq_ignore_ascii_case("_xlnm.Print_Area")
}

/// A range as the name box shows it: `B12`, or `A1:C5`.
fn range_name(range: CellRange) -> String {
    let first = cell_name(range.first_column, range.first_row);
    if range.first_column == range.last_column && range.first_row == range.last_row {
        first
    } else {
        format!("{first}:{}", cell_name(range.last_column, range.last_row))
    }
}

/// Where Ctrl+arrow goes along a row or column, from `at` a step `d` (±1)
/// at a time over `sizes` (0 for a hidden row or column), as Excel does:
/// from a filled cell with a filled one next to it, to the last filled one
/// before an empty one; otherwise to the next filled one, or the edge.
pub(crate) fn data_edge(at: u32, d: i32, sizes: &[i64], filled: impl Fn(u32) -> bool) -> u32 {
    let next = |i: u32| {
        let mut i = i as i64 + d as i64;
        while (0..sizes.len() as i64).contains(&i) && sizes[i as usize] == 0 {
            i += d as i64;
        }
        (0..sizes.len() as i64).contains(&i).then_some(i as u32)
    };
    let Some(mut n) = next(at) else { return at };
    if filled(at) && filled(n) {
        while let Some(m) = next(n).filter(|m| filled(*m)) {
            n = m;
        }
    } else {
        while !filled(n) {
            match next(n) {
                Some(m) => n = m,
                None => break,
            }
        }
    }
    n
}

/// A move of Excel's keys: to the edge of the data region (Ctrl+arrow),
/// column A (Home), the first cell (Ctrl+Home) or the last in use (Ctrl+End).
#[derive(Clone, Copy)]
enum Jump {
    Edge(i32, i32),
    RowStart,
    First,
    Last,
}

impl SlideEditor {
    /// The bindings of the keys that move about the cells and open ジャンプ.
    pub(crate) fn cell_jump_actions(d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &GoTo, window, cx| this.open_goto(window, cx)))
            .on_action(cx.listener(|this, _: &InsertTable, window, cx| this.open_table_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &JumpCellUp, _, cx| this.jump(Jump::Edge(0, -1), false, cx)))
            .on_action(cx.listener(|this, _: &JumpCellDown, _, cx| this.jump(Jump::Edge(0, 1), false, cx)))
            .on_action(cx.listener(|this, _: &JumpCellLeft, _, cx| this.jump(Jump::Edge(-1, 0), false, cx)))
            .on_action(cx.listener(|this, _: &JumpCellRight, _, cx| this.jump(Jump::Edge(1, 0), false, cx)))
            .on_action(cx.listener(|this, _: &ExtendJumpUp, _, cx| this.jump(Jump::Edge(0, -1), true, cx)))
            .on_action(cx.listener(|this, _: &ExtendJumpDown, _, cx| this.jump(Jump::Edge(0, 1), true, cx)))
            .on_action(cx.listener(|this, _: &ExtendJumpLeft, _, cx| this.jump(Jump::Edge(-1, 0), true, cx)))
            .on_action(cx.listener(|this, _: &ExtendJumpRight, _, cx| this.jump(Jump::Edge(1, 0), true, cx)))
            .on_action(cx.listener(|this, _: &SelectRowStart, _, cx| this.jump(Jump::RowStart, false, cx)))
            .on_action(cx.listener(|this, _: &SelectRowEnd, _, cx|{if !this.formula_edit_command("end",Modifiers::default(),cx){this.jump(Jump::Edge(1,0),false,cx)}}))
            .on_action(cx.listener(|this, _: &ExtendRowEnd, _, cx|{if !this.formula_edit_command("end",Modifiers{shift:true,..Default::default()},cx){this.jump(Jump::Edge(1,0),true,cx)}}))
            .on_action(cx.listener(|this, _: &ExtendRowStart, _, cx| this.jump(Jump::RowStart, true, cx)))
            .on_action(cx.listener(|this, _: &SelectFirstCell, _, cx| this.jump(Jump::First, false, cx)))
            .on_action(cx.listener(|this, _: &ExtendFirstCell, _, cx| this.jump(Jump::First, true, cx)))
            .on_action(cx.listener(|this, _: &SelectLastCell, _, cx| this.jump(Jump::Last, false, cx)))
            .on_action(cx.listener(|this, _: &ExtendLastCell, _, cx| this.jump(Jump::Last, true, cx)))
            .on_action(cx.listener(|this, _: &EditCell, window, cx| {
                if this.cells_take_typing(window, cx) {
                    if this.cell_edit().is_none() {
                        this.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx);
                    } else {
                        this.toggle_edit_mode(cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &CellBackspace, window, cx| {
                if this.cells_take_typing(window, cx) {
                    this.cell_delete(false, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &CellBold, _, cx| {
                let on = !this.current_style_bold();
                this.format_cells(ibis_doc_model::StyleChange::Bold(on), cx)
            }))
            .on_action(cx.listener(|this, _: &CellItalic, _, cx| {
                let on = !this.current_style_italic();
                this.format_cells(ibis_doc_model::StyleChange::Italic(on), cx)
            }))
            .on_action(cx.listener(|this, _: &CellUnderline, _, cx| {
                let on = !this.current_style_underline();
                this.format_cells(ibis_doc_model::StyleChange::Underline(on), cx)
            }))
            // While typing into a cell, copy, cut and paste work on the text being typed.
            .on_action(cx.listener(|this, _: &CellCopy, _, cx| if this.cell_edit().is_some() { this.edit_copy(false, cx) } else { this.copy_cells(false, cx) }))
            .on_action(cx.listener(|this, _: &CellCut, _, cx| if this.cell_edit().is_some() { this.edit_copy(true, cx) } else { this.copy_cells(true, cx) }))
            .on_action(cx.listener(|this, _: &CellPaste, _, cx| if this.cell_edit().is_some() { this.edit_paste(cx) } else { this.paste_cells(false, cx) }))
            .on_action(cx.listener(|this, _: &CellPastePlain, _, cx| if this.cell_edit().is_some() { this.edit_paste(cx) } else { this.paste_cells(true, cx) }))
            .on_action(cx.listener(|this, _: &CellPasteSpecialDialog, _, cx| this.open_paste_special_dialog(cx)))
            .on_action(cx.listener(|this, _: &ClearCellFormats, _, cx| this.clear_cell_formats(cx)))
            .on_action(cx.listener(|this, _: &CellTab, window, cx| this.cell_tab(window, cx)))
            .on_action(cx.listener(|this, _: &SelectAllCells, _, cx| this.select_all_cells(cx)))
            .on_action(cx.listener(|this, _: &InsertLink, window, cx| this.open_link_panel(window, cx)))
            .on_action(cx.listener(|this, _: &CycleReference, _, cx| {
                if this.cell_edit().is_some() { this.cycle_reference(cx); }
                else { this.repeat_sheet_edit(cx); }
            }))
            .on_action(cx.listener(|this, _: &CellEnter, window, cx| this.cell_enter(window, cx)))
            .on_action(cx.listener(|this, _: &CellFillSelection, _, cx| this.fill_selection_with_entry(cx)))
            .on_action(cx.listener(|this, _: &FillDown, _, cx| {
                // With shapes selected, ⌘D duplicates them (as in Excel).
                if this.selected_shapes().is_empty() {
                    this.fill_selection_edge(crate::xlsx::fill::FillDirection::Down, cx);
                } else {
                    this.duplicate_selected(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenCellFormat, window, cx| this.open_custom_format(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleFilter, _, cx| this.toggle_auto_filter(cx)))
            .on_action(cx.listener(|this, _: &FillRight, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Right, cx)))
            .on_action(cx.listener(|this, _: &FillUp, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Up, cx)))
            .on_action(cx.listener(|this, _: &FillLeft, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Left, cx)))
            .on_action(cx.listener(|this, _: &InsertDate, _, cx| this.insert_now_stamp(true, cx)))
            .on_action(cx.listener(|this, _: &InsertTime, _, cx| this.insert_now_stamp(false, cx)))
            .on_action(cx.listener(|this, _: &SelectColumns, _, cx| this.select_selection_lines(Axis::Columns, cx)))
            .on_action(cx.listener(|this, _: &SelectRows, _, cx| this.select_selection_lines(Axis::Rows, cx)))
            .on_action(cx.listener(|this, _: &CellLineBreak, _, cx| this.cell_line_break(cx)))
            .on_action(cx.listener(|this, _: &PickFromList, window, cx| this.pick_from_drop_down_list(window, cx)))
            .on_action(cx.listener(|this, _: &FlashFill, _, cx| this.flash_fill_column(cx)))
            .on_action(cx.listener(|this, _: &HideRows, _, cx| this.hide_lines(Axis::Rows, true, cx)))
            .on_action(cx.listener(|this, _: &HideColumns, _, cx| this.hide_lines(Axis::Columns, true, cx)))
            .on_action(cx.listener(|this, _: &UnhideRows, _, cx| this.hide_lines(Axis::Rows, false, cx)))
            .on_action(cx.listener(|this, _: &UnhideColumns, _, cx| this.hide_lines(Axis::Columns, false, cx)))
            .on_action(cx.listener(|this, _: &CalculateNow, _, cx| {
                this.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::CalculateNow)], cx);
            }))
            .on_action(cx.listener(|this, _: &CalculateSheet, _, cx| {
                let slide = this.current_slide;
                this.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::CalculateSheet { slide })], cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleShowFormulas, _, cx| this.toggle_show_formulas(cx)))
            .on_action(cx.listener(|this, _: &ReapplyFilter, _, cx| this.reapply_auto_filter(cx)))
            .on_action(cx.listener(|this, _: &EditNote, window, cx| this.open_note_panel(window, cx)))
            .on_action(cx.listener(|this, _: &InsertFunction, window, cx| this.insert_function_action(window, cx)))
            .on_action(cx.listener(|this, _: &PreviousComment, _, cx| this.select_next_comment(false, cx)))
            .on_action(cx.listener(|this, _: &NextComment, _, cx| this.select_next_comment(true, cx)))
            .on_action(cx.listener(|this, _: &ToggleShowAllNotes, _, cx| this.toggle_show_all_notes(cx)))
            .on_action(cx.listener(|this, _: &SheetInsert, _, cx| this.smart_insert(cx)))
            .on_action(cx.listener(|this, _: &SheetDelete, _, cx| this.smart_delete(cx)))
            .on_action(cx.listener(|this, _: &CellDelete, window, cx| {
                if this.cells_take_typing(window, cx) {
                    this.cell_delete(true, cx);
                } else {
                    cx.propagate();
                }
            }))
    }

    /// Select the cell a key such as Ctrl+→ goes to, or (`extend`) stretch
    /// the selected range's far corner there.
    fn jump(&mut self, jump: Jump, extend: bool, cx: &mut Context<Self>) {
        let key=match jump{Jump::First=>Some(("home",true)),Jump::Last=>Some(("end",true)),Jump::RowStart=>Some(("home",false)),Jump::Edge(dx,0) if extend&&dx!=0=>Some((if dx<0{"left"}else{"right"},true)),_=>None};
        if let Some((key,control))=key{if self.formula_edit_command(key,Modifiers{control,shift:extend,..Default::default()},cx){return;}}
        if let Jump::Edge(dx,dy)=jump {if self.formula_reference_key(dx,dy,extend,true,cx){return;}}
        if self.editing.is_some() {
            return;
        }
        let (Some((_, sheet)), Some(range), Some((column, row))) = (self.current_sheet(), self.selected_range(), self.current_cell()) else {
            return;
        };
        let last = |sizes: &[i64]| sizes.len().saturating_sub(1) as u32;
        let (last_column, last_row) = (last(&sheet.columns), last(&sheet.rows));
        // From the far corner when stretching (as `extend_cells`), else from
        // the selected cell's edge on the side it moves to.
        let merged = sheet.merge_at(row, column).unwrap_or(CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
        let from = if extend {
            let far_column = if column > range.first_column { range.first_column } else { range.last_column };
            let far_row = if row > range.first_row { range.first_row } else { range.last_row };
            (far_column, far_row)
        } else {
            (column, row)
        };
        let filled = |column: u32, row: u32| {
            // A merged range's cells are filled if its top-left one is.
            let (column, row) = sheet.merge_at(row, column).map_or((column, row), |m| (m.first_column, m.first_row));
            sheet.cell(row, column).is_some_and(|cell| !cell.text.is_empty())
        };
        let to = match jump {
            Jump::Edge(dx, 0) => {
                let at = match (extend, dx > 0) {
                    (true, _) => from.0,
                    (false, true) => merged.last_column,
                    (false, false) => merged.first_column,
                };
                (data_edge(at, dx, &sheet.columns, |c| filled(c, from.1)), from.1)
            }
            Jump::Edge(_, dy) => {
                let at = match (extend, dy > 0) {
                    (true, _) => from.1,
                    (false, true) => merged.last_row,
                    (false, false) => merged.first_row,
                };
                (from.0, data_edge(at, dy, &sheet.rows, |r| filled(from.0, r)))
            }
            Jump::RowStart => (0, from.1),
            Jump::First => {
                let (columns, rows) = sheet.frozen.unwrap_or((0, 0));
                (columns.min(last_column), rows.min(last_row))
            }
            Jump::Last => {
                let used = sheet.cells.iter().fold((0, 0), |(c, r), cell| (c.max(cell.column), r.max(cell.row)));
                (used.0.min(last_column), used.1.min(last_row))
            }
        };
        if extend {
            let was_kept = self.selected_ranges().len() > 1;
            if to == (column, row) {
                let slide = self.current_slide;
                self.wb_mut().cell_ends.remove(&slide);
            } else {
                let slide = self.current_slide;
                self.wb_mut().cell_ends.insert(slide, to);
            }
            self.extra_ranges_follow(was_kept);
        } else {
            self.set_selection(Vec::new());
            self.set_cell(to.0, to.1);
        }
        self.scroll_cell_into_view();
        cx.notify();
    }

    /// What the name box shows: a matching defined name if one refers to the
    /// selected cell/range, otherwise the selected cell or range; nothing
    /// while a shape is selected.
    fn name_box_text(&self) -> String {
        let (None, Some((_, sheet)), Some((column, row)), Some(range)) =
            (self.selection, self.current_sheet(), self.current_cell(), self.selected_range())
        else {
            return String::new();
        };
        if let Some(name) = self.defined_name_for_range(self.current_slide, range) {
            return name;
        }
        let cell = sheet.merge_at(row, column).unwrap_or(CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
        if range == cell { cell_name(column, row) } else { range_name(range) }
    }

    /// Defined name matching `(slide, range)` (sheet-local name first, then workbook-global).
    pub(crate) fn defined_name_for_range(&self, slide: SlideId, range: CellRange) -> Option<String> {
        let workbook = self.presentation().workbook.as_ref()?;
        let index = workbook.sheets.iter().position(|s| s.slide == Some(slide));
        let matches_range = |n: &ibis_doc_model::DefinedName| -> bool {
            if n.hidden || n.name.starts_with("_xlnm.") || n.name.is_empty() {
                return false;
            }
            let Some(target) = parse_reference(n.formula.trim().trim_start_matches('=')) else {
                return false;
            };
            if matches!(target.target, Target::Name(_)) {
                return false;
            }
            self.resolve_reference(&target) == Some((slide, range))
        };
        workbook
            .names
            .iter()
            .find(|n| n.sheet.is_some() && n.sheet == index && matches_range(n))
            .or_else(|| workbook.names.iter().find(|n| n.sheet.is_none() && matches_range(n)))
            .map(|n| n.name.clone())
    }

    /// Visible defined names for the Name Box dropdown, sorted case-insensitively.
    pub(crate) fn name_box_defined_names(&self) -> Vec<String> {
        let Some(workbook) = self.presentation().workbook.as_ref() else {
            return Vec::new();
        };
        let index = workbook.sheets.iter().position(|s| s.slide == Some(self.current_slide));
        let mut names: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for n in workbook
            .names
            .iter()
            .filter(|n| n.sheet == index)
            .chain(workbook.names.iter().filter(|n| n.sheet.is_none()))
            .chain(workbook.names.iter().filter(|n| n.sheet.is_some() && n.sheet != index))
        {
            if !n.hidden && !n.name.starts_with("_xlnm.") && !n.name.is_empty() {
                if seen.insert(n.name.to_lowercase()) {
                    names.push(n.name.clone());
                }
            }
        }
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        names
    }

    /// Show the selection in the name box, unless something is being typed in it.
    fn sync_name_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.name_box_text();
        let typing = self.wb().jump.name_box.read(cx).focus_handle(cx).is_focused(window);
        if !typing && text != self.wb().jump.name_box_shown {
            self.wb().jump.name_box.update(cx, |input, cx| input.set_value(text.clone(), window, cx));
            self.wb_mut().jump.name_box_shown = text;
        }
    }

    /// The sheet and cells a reference points to: on the sheet named (by
    /// its tab's name, in any case) or the current one, cut to its grid.
    pub(crate) fn resolve_reference(&self, reference: &Reference) -> Option<(SlideId, CellRange)> {
        let slides = &self.presentation().slides;
        let slide = match &reference.sheet {
            Some(name) => {
                let name = name.to_lowercase();
                slides.iter().enumerate().find(|(index, slide)| {
                    let tab = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", index + 1));
                    tab.to_lowercase() == name
                })?.1
            }
            None => self.slide(),
        };
        let (_, sheet) = sheet_of(slide)?;
        let range = match &reference.target {
            Target::Cells(range) => *range,
            Target::Name(name) if is_print_area(name) => sheet.print_area?,
            Target::Name(name) => {
                // A defined name: the range it stands for (the sheet's own name first).
                let index = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide.id)));
                let names = &self.presentation().workbook.as_ref()?.names;
                let defined = names.iter().find(|n| n.name.eq_ignore_ascii_case(name) && n.sheet.is_some() && n.sheet == index).or_else(|| names.iter().find(|n| n.name.eq_ignore_ascii_case(name) && n.sheet.is_none()))?;
                let target = parse_reference(defined.formula.trim().trim_start_matches('='))?;
                if matches!(target.target, Target::Name(_)) {
                    return None;
                }
                return self.resolve_reference(&target);
            }
        };
        let last_column = MAX_SHEET_COLUMNS.saturating_sub(1);
        let last_row = MAX_SHEET_ROWS.saturating_sub(1);
        Some((
            slide.id,
            CellRange {
                first_row: range.first_row.min(last_row),
                first_column: range.first_column.min(last_column),
                last_row: range.last_row.min(last_row),
                last_column: range.last_column.min(last_column),
            },
        ))
    }

    /// Select what `text` refers to, as the name box and ジャンプ do: the
    /// sheet shown if it names one, the range selected with its top-left
    /// cell as the selected one, and scrolled to the view's top left if it
    /// is out of view. False (nothing changed) if it refers to nothing.
    pub(crate) fn go_to_reference(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((slide, range)) = parse_reference(text).and_then(|r| self.resolve_reference(&r)) else {
            return false;
        };
        self.commit_edit(window, cx);
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        self.set_selection(Vec::new());
        self.set_cell(range.first_column, range.first_row);
        if (range.last_column, range.last_row) != (range.first_column, range.first_row) {
            self.wb_mut().cell_ends.insert(slide, (range.last_column, range.last_row));
        }
        if self.workspace_bounds.is_some() {
            self.scroll_range_into_view(true);
        } else {
            self.wb_mut().jump.pending_scroll = true;
        }
        self.status = None;
        cx.notify();
        true
    }

    /// Ctrl+G or F5 (or ⌘G on Mac): open the ジャンプ dialog.
    pub(crate) fn open_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_workbook() || self.editing.is_some() {
            return cx.propagate();
        }
        self.wb_mut().jump.goto_open = true;
        self.wb_mut().jump.goto_error = None;
        let initial_val = self.wb().jump.goto_history.first().cloned().unwrap_or_default();
        self.wb().jump.goto_input.update(cx, |input, cx| {
            input.set_value(&initial_val, window, cx);
            input.select_all(window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Add a jump reference to recent history (up to 30 items, duplicates moved to top).
    pub(crate) fn add_goto_history(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let jump = &mut self.wb_mut().jump;
        jump.goto_history.retain(|h| !h.eq_ignore_ascii_case(text));
        jump.goto_history.insert(0, text.to_string());
        if jump.goto_history.len() > 30 {
            jump.goto_history.truncate(30);
        }
    }

    /// Select an item from the destination list into the input box.
    pub(crate) fn select_goto_item(&mut self, item: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.wb().jump.goto_input.update(cx, |input, cx| {
            input.set_value(item, window, cx);
            input.select_all(window, cx);
            input.focus(window, cx);
        });
        self.wb_mut().jump.goto_error = None;
        cx.notify();
    }

    /// Double click an item: select and immediately confirm jump.
    pub(crate) fn double_click_goto_item(&mut self, item: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.select_goto_item(item, window, cx);
        self.confirm_goto(window, cx);
    }

    /// Special... button: close goto dialog and open Go To Special sidebar panel.
    pub(crate) fn open_goto_special_from_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_goto(window, cx);
        self.open_goto_special_panel(cx);
    }

    /// Collect defined names and history for the Go To dialog.
    pub(crate) fn goto_destinations(&self) -> Vec<(String, bool)> {
        let mut list = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1. Defined names from workbook (visible, non-system)
        if let Some(workbook) = self.presentation().workbook.as_ref() {
            let mut names: Vec<_> = workbook
                .names
                .iter()
                .filter(|n| !n.hidden && !n.name.starts_with("_xlnm.") && !n.name.is_empty())
                .map(|n| n.name.clone())
                .collect();
            names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            for name in names {
                if seen.insert(name.to_lowercase()) {
                    list.push((name, true));
                }
            }
        }

        // 2. Recent history
        for history in &self.wb().jump.goto_history {
            if seen.insert(history.to_lowercase()) {
                list.push((history.clone(), false));
            }
        }

        list
    }

    /// OK in the ジャンプ dialog: jump, or say the reference is wrong and
    /// keep the dialog open.
    pub(crate) fn confirm_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.wb().jump.goto_input.read(cx).value().trim().to_string();
        if self.go_to_reference(&text, window, cx) {
            self.add_goto_history(&text);
            self.close_goto(window, cx);
        } else {
            self.wb_mut().jump.goto_error = Some(crate::shell::i18n::text("_xlsx.invalid-reference").into());
            self.wb().jump.goto_input.update(cx, |input, cx| input.select_all(window, cx));
            cx.notify();
        }
    }

    pub(crate) fn close_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().jump.goto_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Esc in the name box or the ジャンプ dialog: leave it, the selection
    /// as it was. False if neither has the keys.
    pub(crate) fn cancel_cell_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.cancel_pptx_drag(cx) {
            return true;
        }
        if self.drag.take().is_some() {
            cx.notify();
            return true;
        }
        if self.editing.as_mut().and_then(|e| e.drag_anchor.take()).is_some() {
            if let Some(edit) = self.editing.as_mut() {
                edit.edit.caret = edit.edit.anchor;
            }
            cx.notify();
            return true;
        }
        if !self.is_workbook() {
            return false;
        }
        if self.cancel_sheet_drag(cx) {
            return true;
        }
        if self.completion_dismiss(cx) {
            return true;
        }
        if self.cancel_cell_edit(cx) {
            return true;
        }
        if let Some(state) = self.sheet_state_mut() {
            if state.panels.cells_dialog.is_some() {
                state.panels.cells_dialog = None;
                cx.notify();
                return true;
            }
        }
        if self.wb().jump.goto_open {
            self.close_goto(window, cx);
            return true;
        }
        if self.wb().jump.name_box.read(cx).focus_handle(cx).is_focused(window) {
            // Shown again from the selection.
            self.wb_mut().jump.name_box_shown = String::new();
            self.focus_handle.focus(window, cx);
            cx.notify();
            return true;
        }
        false
    }

    /// Cancel any active mouse drag in the worksheet (cell selection, fill handle, move, header resize, etc.).
    pub(crate) fn cancel_sheet_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(edit) = self.cell_edit_mut() {
            if let Some(anchor) = edit.dragging.take() {
                edit.dragging_in_bar = false;
                edit.anchor = None;
                edit.caret = anchor;
                cx.notify();
                return true;
            }
        }
        let slide = self.current_slide;
        let mut restore_ranges = None;
        let mut cancelled = false;
        if let Some(state) = self.sheet_state_mut() {
            if let Some(src) = state.fill_source.take() {
                state.fill_copy = false;
                if (src.last_column, src.last_row) == (src.first_column, src.first_row) {
                    state.cell_ends.remove(&slide);
                } else {
                    state.cell_ends.insert(slide, (src.last_column, src.last_row));
                }
                cancelled = true;
            }
            if state.cell_move.take().is_some() {
                cancelled = true;
            }
            if state.header_drag.take().is_some() {
                cancelled = true;
            }
            if state.header_select.take().is_some() {
                cancelled = true;
            }
            if state.split_drag.take().is_some() {
                cancelled = true;
            }
            if state.timeline_drag.take().is_some() {
                cancelled = true;
            }
            if state.control_drag.take().is_some() {
                cancelled = true;
            }
            if state.ruler_margin_drag.take().is_some() {
                cancelled = true;
            }
            if state.tab_drag.take().is_some() {
                cancelled = true;
            }
            if let Some(drag) = state.deselect_drag.take() {
                restore_ranges = Some(drag.base_ranges);
                cancelled = true;
            }
            if state.cell_drag {
                state.cell_drag = false;
                state.cell_ends.remove(&slide);
                cancelled = true;
            }
        }
        if let Some(ranges) = restore_ranges {
            self.set_selected_ranges(&ranges);
        }
        if cancelled {
            cx.notify();
        }
        cancelled
    }

    /// The ジャンプ dialog, under the name box: destinations list, 参照先 with Special, OK and キャンセル.
    fn render_goto(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let destinations = self.goto_destinations();
        let current_val = self.wb().jump.goto_input.read(cx).value().trim().to_string();

        v_flex()
            .id("goto")
            .absolute()
            .top(px(34.))
            .left(px(8.))
            .w(px(340.))
            .p_3()
            .gap_2()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_goto(window, cx)))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child(crate::shell::i18n::text("_xlsx.go-to"))),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(crate::shell::i18n::text("xlsx.goto-destination")))
                    .child(
                        v_flex()
                            .id("goto-destinations-list")
                            .h(px(130.))
                            .overflow_y_scroll()
                            .border_1()
                            .border_color(theme.border)
                            .rounded_sm()
                            .bg(theme.background)
                            .p_1()
                            .gap_0p5()
                            .children(if destinations.is_empty() {
                                vec![div().p_2().text_xs().text_color(theme.muted_foreground).child(crate::shell::i18n::text("xlsx.goto-no-destinations")).into_any_element()]
                            } else {
                                destinations.into_iter().enumerate().map(|(idx, (dest, is_named))| {
                                    let is_selected = current_val.eq_ignore_ascii_case(&dest);
                                    let d_clone = dest.clone();
                                    let d_clone2 = dest.clone();
                                    h_flex()
                                        .id(ElementId::NamedInteger("goto-item".into(), idx as u64))
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .items_center()
                                        .justify_between()
                                        .when(is_selected, |s| s.bg(theme.primary.opacity(0.15)))
                                        .hover(|h| h.bg(theme.muted))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select_goto_item(&d_clone, window, cx);
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                                            if ev.button == MouseButton::Left && ev.click_count >= 2 {
                                                this.double_click_goto_item(&d_clone2, window, cx);
                                            }
                                        }))
                                        .child(div().text_xs().child(dest))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(if is_named { crate::shell::i18n::text("xlsx.goto-type-name") } else { crate::shell::i18n::text("xlsx.goto-type-history") })
                                        )
                                        .into_any_element()
                                }).collect()
                            })
                    )
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(crate::shell::i18n::text("_xlsx.reference")))
                    .child(Input::new(&self.wb().jump.goto_input).small()),
            )
            .when_some(self.wb().jump.goto_error.clone(), |d, error| d.child(div().text_xs().text_color(theme.danger).child(error)))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pt_1()
                    .child(
                        Button::new("goto-special")
                            .small()
                            .label(crate::shell::i18n::text("xlsx.goto-special-button"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_goto_special_from_goto(window, cx)))
                    )
                    .child(
                        crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                            .action(
                                crate::shell::chrome::ActionDefinition::new("goto-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                                    .label("OK")
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_goto(window, cx))),
                            )
                            .action(
                                crate::shell::chrome::ActionDefinition::new("goto-cancel", "", crate::shell::chrome::ActionButtonKind::Normal)
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, window, cx| this.close_goto(window, cx))),
                            ))),
                    )
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{OPEN_END, Reference, Target, column_letters, data_edge, parse_reference, range_name};
    use ibis_doc_model::CellRange;

    fn cells(first_column: u32, first_row: u32, last_column: u32, last_row: u32) -> Target {
        Target::Cells(CellRange { first_row, first_column, last_row, last_column })
    }

    fn here(target: Target) -> Option<Reference> {
        Some(Reference { sheet: None, target })
    }

    #[test]
    fn references_are_read_as_excel_does() {
        assert_eq!(parse_reference("B12"), here(cells(1, 11, 1, 11)));
        assert_eq!(parse_reference("$B$12"), here(cells(1, 11, 1, 11)));
        assert_eq!(parse_reference("b$12"), here(cells(1, 11, 1, 11)));
        assert_eq!(parse_reference(" a201 "), here(cells(0, 200, 0, 200)));
        assert_eq!(parse_reference("A1:C5"), here(cells(0, 0, 2, 4)));
        // Corners in either order.
        assert_eq!(parse_reference("C5:A1"), here(cells(0, 0, 2, 4)));
        assert_eq!(parse_reference("XFD1048576"), here(cells(16_383, 1_048_575, 16_383, 1_048_575)));
        // Whole columns and rows.
        assert_eq!(parse_reference("B:D"), here(cells(1, 0, 3, OPEN_END)));
        assert_eq!(parse_reference("$3:$5"), here(cells(0, 2, OPEN_END, 4)));
    }

    #[test]
    fn references_may_name_a_sheet() {
        let on = |sheet: &str, target| Some(Reference { sheet: Some(sheet.into()), target });
        assert_eq!(parse_reference("Sheet2!A1"), on("Sheet2", cells(0, 0, 0, 0)));
        assert_eq!(parse_reference("'Sheet name'!$A$1:B2"), on("Sheet name", cells(0, 0, 1, 1)));
        assert_eq!(parse_reference("'売上 (2024)'!C3"), on("売上 (2024)", cells(2, 2, 2, 2)));
        assert_eq!(parse_reference("'It''s'!A1"), on("It's", cells(0, 0, 0, 0)));
        assert_eq!(parse_reference("集計!Print_Area"), on("集計", Target::Name("Print_Area".into())));
    }

    #[test]
    fn names_are_kept_and_the_rest_refused() {
        assert_eq!(parse_reference("Print_Area"), here(Target::Name("Print_Area".into())));
        for wrong in ["", "A1:B", "A:3", "A1:B2:C3", "!A1", "'Sheet!A1", "'Sheet'A1", "1A", "B 12", "$", "A1-"] {
            assert_eq!(parse_reference(wrong), None, "{wrong}");
        }
        // Past Excel's limits: no cells (only a name, which the workbook lacks).
        for past in ["A0", "XFE1", "A1048577"] {
            assert_eq!(parse_reference(past), here(Target::Name(past.into())), "{past}");
        }
    }

    #[test]
    fn ranges_are_named_as_in_the_name_box() {
        assert_eq!(range_name(CellRange { first_row: 11, first_column: 1, last_row: 11, last_column: 1 }), "B12");
        assert_eq!(range_name(CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 2 }), "A1:C5");
    }

    #[test]
    fn control_arrow_stops_at_the_data_region_edge() {
        // Rows 0-2 filled, 3-4 empty, 5-6 filled, 7 hidden, 8 filled, 9 empty.
        let sizes = [1, 1, 1, 1, 1, 1, 1, 0, 1, 1];
        let filled = |i: u32| [0, 1, 2, 5, 6, 8].contains(&i);
        // In a block: to its end.
        assert_eq!(data_edge(0, 1, &sizes, filled), 2);
        // At its end: to the next block.
        assert_eq!(data_edge(2, 1, &sizes, filled), 5);
        // Past the hidden row, as if it were not there.
        assert_eq!(data_edge(5, 1, &sizes, filled), 8);
        // From an empty cell, with nothing filled after it: to the edge.
        assert_eq!(data_edge(8, 1, &sizes, filled), 9);
        assert_eq!(data_edge(9, 1, &sizes, filled), 9);
        assert_eq!(data_edge(4, -1, &sizes, filled), 2);
        assert_eq!(data_edge(2, -1, &sizes, filled), 0);
    }

    #[test]
    fn column_letters_follow_excel() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }

    #[test]
    fn row_header_width_expands_for_large_row_counts() {
        assert_eq!(super::row_header_width_for_rows(0), 44.0);
        assert_eq!(super::row_header_width_for_rows(10), 44.0);
        assert_eq!(super::row_header_width_for_rows(9999), 44.0);
        assert_eq!(super::row_header_width_for_rows(10000), 52.0); // 5 digits
        assert_eq!(super::row_header_width_for_rows(100000), 60.0); // 6 digits
        assert_eq!(super::row_header_width_for_rows(1048576), 68.0); // 7 digits (Excel max rows)
    }

    #[test]
    fn spill_outline_and_ghost_formula_normal_and_blocked() {
        use ibis_doc_model::{
            CellEdit, CellInput, CellValue, Command, Document, EmuRect, EmuSize, Presentation, Shape, ShapeKind, Sheet,
            SheetCommand, Slide, Workbook, WorkbookSheet,
        };
        use std::sync::Arc;

        let mut p = Presentation::empty(EmuSize::WIDESCREEN);
        let slide_id = p.new_slide_id();
        let mut slide = Slide::new(slide_id);
        slide.name = Some("Sheet1".into());
        let shape_id = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 6], rows: vec![100; 6], ..Sheet::default() };
        slide.shapes.push(Shape {
            id: shape_id,
            frame: EmuRect::new(0, 0, 1000, 1000),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Sheet(Arc::new(sheet)),
            floating: None,
        });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![WorkbookSheet { name: "Sheet1".into(), state: Default::default(), slide: Some(slide_id), hidden: None, other: false }],
            ..Default::default()
        }));
        let mut doc = Document::new(p);

        // 1. Put =SEQUENCE(3,3,1,1) in A1 (0, 0)
        doc.execute(vec![Command::Sheet(SheetCommand::SetCells {
            slide: slide_id,
            edits: vec![CellEdit { row: 0, column: 0, input: CellInput::Formula("SEQUENCE(3,3,1,1)".into()) }],
        })])
        .unwrap();

        let expected_range = CellRange { first_row: 0, first_column: 0, last_row: 2, last_column: 2 };
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(doc.presentation().slide(slide_id).unwrap()).unwrap();

        // Anchor cell (0, 0) has its own formula (not a ghost formula), and spilled cells (0..=2, 0..=2) have ghost formula
        assert_eq!(super::spilled_ghost_formula_on(sheet, 0, 0), None);
        for r in 0..=2 {
            for c in 0..=2 {
                if (r, c) != (0, 0) {
                    assert_eq!(
                        super::spilled_ghost_formula_on(sheet, r, c).as_deref(),
                        Some("=SEQUENCE(3,3,1,1)"),
                        "spilled cell ({r}, {c}) should show ghost formula"
                    );
                }
                // Selecting any cell in the 3x3 spill range shows the unblocked solid outline (is_blocked = false)
                let sel = CellRange { first_row: r, first_column: c, last_row: r, last_column: c };
                assert_eq!(
                    super::active_spill_outlines_on(doc.presentation(), slide_id, sheet, Some((c, r)), &[sel]),
                    vec![(expected_range, false)]
                );
            }
        }
        // Selecting D4 (3, 3) outside the spill range shows no spill outline
        let outside = CellRange { first_row: 3, first_column: 3, last_row: 3, last_column: 3 };
        assert!(super::active_spill_outlines_on(doc.presentation(), slide_id, sheet, Some((3, 3)), &[outside]).is_empty());

        // 2. Block the spill by putting "blocker" in B2 (1, 1) -> A1 becomes #SPILL!
        doc.execute(vec![Command::Sheet(SheetCommand::SetCells {
            slide: slide_id,
            edits: vec![CellEdit { row: 1, column: 1, input: CellInput::Value(CellValue::Text("blocker".into())) }],
        })])
        .unwrap();

        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(doc.presentation().slide(slide_id).unwrap()).unwrap();
        assert_eq!(sheet.cell(0, 0).unwrap().value, CellValue::Error(ibis_doc_model::CellError::Spill));
        // When blocked, B2 is no longer a spilled ghost cell
        assert_eq!(super::spilled_ghost_formula_on(sheet, 1, 1), None);
        assert_eq!(super::spilled_ghost_formula_on(sheet, 0, 1), None);
        // Selecting the #SPILL! cell A1 (0, 0) shows the intended 3x3 spill range with is_blocked = true (dashed border)
        let a1_sel = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
        assert_eq!(
            super::active_spill_outlines_on(doc.presentation(), slide_id, sheet, Some((0, 0)), &[a1_sel]),
            vec![(expected_range, true)]
        );
    }
}
