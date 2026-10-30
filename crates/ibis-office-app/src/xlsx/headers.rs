// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The row and column headers as Excel works them: a click selects the
//! whole column or row (Shift extends), a drag on the line between two
//! headers sizes the one before it (to the selected ones, if it is among
//! them; to nothing, it hides it), a double-click there fits the column to
//! its widest content, and the right button opens the header's menu
//! (insert, delete, hide, show, clear formats).

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, EMU_PER_POINT, Emu, SheetCommand, sheet_command::Axis};

use gpui_kit::component::menu::PopupMenu;

use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
use crate::shell::editor::SlideEditor;

/// How near the line between two headers (in pixels) a press takes it.
const GRIP: f32 = 4.0;

/// Excel's maximum row height (409 pt).
pub(crate) const MAX_ROW_HEIGHT_PT: f32 = 409.0;
/// Excel's maximum column width (255 characters; in points by the workbook's digit: `max_column_width_pt`).
pub(crate) const MAX_COLUMN_WIDTH_CHARS: f32 = 255.0;

/// Where the headers were last painted, in window pixels: (index, start,
/// end) of each column and row shown, and px per EMU.
#[derive(Clone, Debug, Default)]
pub(crate) struct HeaderGeometry {
    pub(crate) columns: Vec<(u32, f32, f32)>,
    pub(crate) rows: Vec<(u32, f32, f32)>,
    pub(crate) scale: f32,
    /// The top-left corner of the headers (the workspace's).
    pub(crate) origin: (f32, f32),
    pub(crate) row_header_width: f32,
}

impl HeaderGeometry {
    pub(crate) fn head_w(&self) -> f32 {
        if self.row_header_width > 0.0 {
            self.row_header_width
        } else {
            super::sheet_ui::ROW_HEADER_WIDTH
        }
    }
}

pub(crate) type SharedGeometry = Rc<RefCell<HeaderGeometry>>;

/// A header's line being dragged.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeaderDrag {
    pub(crate) axis: Axis,
    pub(crate) index: u32,
    /// Where the column or row starts, and where the pointer is, in window pixels.
    pub(crate) start: f32,
    pub(crate) current: f32,
    pub(crate) moved: bool,
}

/// The header's menu, open.
pub(crate) struct HeaderPopup {
    pub(crate) menu: Entity<PopupMenu>,
    pub(crate) position: Point<Pixels>,
    _dismiss: Subscription,
}

/// What a press on a header is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HeaderHit {
    /// The line after column or row `index` (starting at `start`).
    Line { index: u32, start: f32 },
    /// Column or row `index` itself.
    Header(u32),
}

pub(crate) fn hit(spans: &[(u32, f32, f32)], at: f32) -> Option<HeaderHit> {
    // The line first. Hidden columns' lines are all where the one before
    // them ends: left of it, the visible column is sized; right of it, the
    // last hidden one is dragged out, as in Excel.
    let near: Vec<&(u32, f32, f32)> = spans.iter().filter(|(_, _, end)| (at - end).abs() <= GRIP).collect();
    if let Some(&&(_, _, end)) = near.first() {
        let pick = if at < end { near.iter().filter(|(_, s, e)| e > s).last().or(near.last()) } else { near.last() };
        if let Some(&&(index, start, _)) = pick {
            return Some(HeaderHit::Line { index, start });
        }
    }
    spans.iter().find(|(_, start, end)| (*start..*end).contains(&at)).map(|&(index, _, _)| HeaderHit::Header(index))
}

impl SlideEditor {
    /// The columns (or rows) selected whole, if the selection is that.
    pub(crate) fn selected_lines(&self, axis: Axis) -> Option<(u32, u32)> {
        let (_, sheet) = self.current_sheet()?;
        let r = self.selected_range()?;
        match axis {
            Axis::Columns => (r.first_row == 0 && r.last_row as usize + 1 >= sheet.rows.len()).then_some((r.first_column, r.last_column)),
            Axis::Rows => (r.first_column == 0 && r.last_column as usize + 1 >= sheet.columns.len()).then_some((r.first_row, r.last_row)),
        }
    }

    fn header_geometry(&self) -> HeaderGeometry {
        self.sheet_state().map(|s| s.header_geometry.borrow().clone()).unwrap_or_default()
    }

    /// ⌃Space / ⇧Space: the columns (rows) the selection covers, whole. While
    /// typing, ⇧Space is a space.
    pub(crate) fn select_selection_lines(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if self.cell_edit().is_some() {
            if axis == Axis::Rows {
                self.cell_replace(None, " ", None, cx);
            }
            return;
        }
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Columns => (r.first_column, r.last_column),
            Axis::Rows => (r.first_row, r.last_row),
        };
        self.select_lines(axis, first, last, cx);
    }

    /// Select whole columns or rows, from `first` to `last` (the anchor at `first`).
    pub(crate) fn select_lines(&mut self, axis: Axis, first: u32, last: u32, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (rows, columns) = (sheet.rows.len().max(1) as u32 - 1, sheet.columns.len().max(1) as u32 - 1);
        let (anchor, end) = match axis {
            Axis::Columns => ((first, 0), (last, rows)),
            Axis::Rows => ((0, first), (columns, last)),
        };
        let slide = self.current_slide;
        self.selection = None;
        let state = self.wb_mut();
        state.cells.insert(slide, anchor);
        state.cell_ends.insert(slide, end);
        cx.notify();
    }

    /// Press on the top-left corner above the row headers and left of the
    /// column headers (the Select All button): selects every cell of the sheet.
    pub(crate) fn on_header_corner_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        self.commit_cell_edit(cx);
        self.select_all_cells(cx);
    }

    /// Press on a header strip.
    pub(crate) fn on_header_down(&mut self, axis: Axis, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        let geometry = self.header_geometry();
        let (spans, at) = match axis {
            Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
            Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
        };
        match hit(spans, at) {
            Some(HeaderHit::Line { index, start }) => {
                if event.click_count >= 2 {
                    match axis {
                        Axis::Columns => return self.fit_columns(index, window, cx),
                        Axis::Rows => return self.fit_rows(index, window, cx),
                    }
                }
                self.commit_cell_edit(cx);
                let scale = geometry.scale.max(1e-4);
                let max_px = match axis {
                    Axis::Rows => MAX_ROW_HEIGHT_PT * EMU_PER_POINT as f32 * scale,
                    Axis::Columns => max_column_width_pt(self.default_font_size_pt()) * EMU_PER_POINT as f32 * scale,
                };
                self.wb_mut().header_drag = Some(HeaderDrag { axis, index, start, current: at.clamp(start, start + max_px), moved: false });
                cx.notify();
            }
            Some(HeaderHit::Header(index)) => {
                let anchor = self.selected_range().map(|r| match axis {
                    Axis::Columns => r.first_column.min(r.last_column),
                    Axis::Rows => r.first_row.min(r.last_row),
                });
                let anchor_cell = self.current_cell().map(|(c, r)| if axis == Axis::Columns { c } else { r });
                match (event.modifiers.shift, anchor_cell.or(anchor)) {
                    (true, Some(a)) => self.select_lines(axis, a, index, cx),
                    // ⌘: if already selected, deselect it; otherwise add to selection.
                    _ if event.modifiers.platform => {
                        let line_range = match axis {
                            Axis::Columns => CellRange {
                                first_column: index,
                                last_column: index,
                                first_row: 0,
                                last_row: super::sheet_ui::MAX_ROWS - 1,
                            },
                            Axis::Rows => CellRange {
                                first_column: 0,
                                last_column: super::sheet_ui::MAX_COLUMNS - 1,
                                first_row: index,
                                last_row: index,
                            },
                        };
                        let current = self.selected_ranges();
                        let intersects = current.iter().any(|r| match axis {
                            Axis::Columns => (r.first_column..=r.last_column).contains(&index),
                            Axis::Rows => (r.first_row..=r.last_row).contains(&index),
                        });
                        if intersects {
                            let remaining = super::goto_special::subtract_ranges(&current, line_range);
                            if !remaining.is_empty() {
                                self.set_selected_ranges(&remaining);
                                cx.notify();
                                return;
                            }
                        }
                        let kept = self.selected_ranges();
                        self.select_lines(axis, index, index, cx);
                        self.keep_with_selection(kept);
                    }
                    _ => self.select_lines(axis, index, index, cx),
                }
                self.wb_mut().header_select = Some(axis);
            }
            None => {}
        }
    }

    /// The right button on a header: the column or row pressed is selected
    /// unless it is already (the menu then acts on the selection).
    pub(crate) fn on_header_right_down(&mut self, axis: Axis, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let geometry = self.header_geometry();
        let (spans, at) = match axis {
            Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
            Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
        };
        let index = match hit(spans, at) {
            Some(HeaderHit::Header(i)) | Some(HeaderHit::Line { index: i, .. }) => i,
            None => return,
        };
        // Within whole columns (rows) selected: the menu is for them.
        let within = self.selected_lines(axis).is_some_and(|(a, b)| (a..=b).contains(&index));
        if !within {
            self.select_lines(axis, index, index, cx);
        }
        self.open_header_menu(axis, event.position, window, cx);
    }

    /// Open the header's menu at `position`.
    pub(crate) fn open_header_menu(&mut self, axis: Axis, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.header_menu(axis);
        self.open_popup_menu(entries, position, window, cx);
    }

    /// Open a pop-up menu of `entries` at `position` (the header's, a
    /// cell's, a sheet tab's).
    pub(crate) fn open_popup_menu(&mut self, entries: Vec<MenuEntry>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = std::rc::Rc::new(entries);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(240.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.sheet_state_mut() {
                state.header_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().header_popup = Some(HeaderPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// The pointer moves (over the workspace): a line dragged follows it, a
    /// header selection stretches.
    pub(crate) fn on_header_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let default_size = self.default_font_size_pt();
        if event.pressed_button.is_none() {
            let cell = self.cell_under(event.position);
            self.show_comment_card(cell, cx);
        }
        let geometry = self.header_geometry();
        let Some(state) = self.sheet_state_mut() else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            state.header_drag = None;
            state.header_select = None;
            // On a header's line: the resize pointer.
            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
            let (ox, oy) = geometry.origin;
            let (head_w, head_h) = (geometry.head_w(), super::sheet_ui::COLUMN_HEADER_HEIGHT);
            let hover = if (oy..oy + head_h).contains(&y) && x >= ox + head_w {
                matches!(hit(&geometry.columns, x), Some(HeaderHit::Line { .. })).then_some(Axis::Columns)
            } else if (ox..ox + head_w).contains(&x) && y >= oy + head_h {
                matches!(hit(&geometry.rows, y), Some(HeaderHit::Line { .. })).then_some(Axis::Rows)
            } else {
                None
            };
            if state.header_hover != hover {
                state.header_hover = hover;
                cx.notify();
            }
            return;
        }
        if let Some(drag) = state.header_drag.as_mut() {
            let scale = geometry.scale.max(1e-4);
            let max_px = match drag.axis {
                Axis::Rows => MAX_ROW_HEIGHT_PT * EMU_PER_POINT as f32 * scale,
                Axis::Columns => max_column_width_pt(default_size) * EMU_PER_POINT as f32 * scale,
            };
            let at = match drag.axis {
                Axis::Columns => f32::from(event.position.x),
                Axis::Rows => f32::from(event.position.y),
            };
            drag.current = at.clamp(drag.start, drag.start + max_px);
            drag.moved = true;
            return cx.notify();
        }
        if let Some(axis) = state.header_select {
            let geometry = self.header_geometry();
            let (spans, at) = match axis {
                Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
                Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
            };
            let index = spans.iter().find(|(_, s, e)| (*s..*e).contains(&at)).map(|s| s.0);
            if let (Some(index), Some((c, r))) = (index, self.current_cell()) {
                let was_kept = self.selected_ranges().len() > 1;
                self.select_lines(axis, if axis == Axis::Columns { c } else { r }, index, cx);
                self.extra_ranges_follow(was_kept);
            }
        }
    }

    /// The button comes up: a dragged line sets the size.
    pub(crate) fn on_header_up(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.sheet_state_mut() else { return };
        state.header_select = None;
        let Some(drag) = state.header_drag.take() else { return };
        if !drag.moved {
            return;
        }
        let scale = self.header_geometry().scale;
        if scale <= 0.0 {
            return cx.notify();
        }
        // Less than a couple of pixels hides it, as in Excel.
        let px_size = (drag.current - drag.start).max(0.0);
        let max_emu = match drag.axis {
            Axis::Rows => (MAX_ROW_HEIGHT_PT * EMU_PER_POINT as f32).round() as Emu,
            Axis::Columns => (max_column_width_pt(self.default_font_size_pt()) * EMU_PER_POINT as f32).round() as Emu,
        };
        let size: Emu = if px_size < 2.0 {
            0
        } else {
            (((px_size / scale) as Emu / (EMU_PER_POINT / 4)) * (EMU_PER_POINT / 4)).min(max_emu)
        };
        // The selected columns or rows too, if the one dragged is among them.
        let (first, last) = self.selected_lines(drag.axis).filter(|(a, b)| (*a..=*b).contains(&drag.index)).unwrap_or((drag.index, drag.index));
        let slide = self.current_slide;
        let started = crate::debug_timing::start();
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: drag.axis, first, last, size })]) {
            log::warn!("sizing rejected: {err}");
        }
        crate::debug_timing::report("resize header: execute", started);
        cx.notify();
    }

    /// Fit column `index` (and the selected ones, if it is among them) to
    /// the widest text in it, as a double-click on its right line does.
    pub(crate) fn fit_columns(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (first, last) = self.selected_lines(Axis::Columns).filter(|(a, b)| (*a..=*b).contains(&index)).unwrap_or((index, index));
        let ts = window.text_system().clone();
        let mut commands = Vec::new();
        let slide = self.current_slide;
        let started = crate::debug_timing::start();
        // One pass over the cells for all the columns; each format's font found once (a column is a million cells of a few formats).
        let mut widest = vec![0.0f32; (last - first + 1) as usize];
        let mut measures: Vec<Option<(super::sheet_view::GlyphWidths, f32, f32, f32)>> = Vec::new();
        measures.resize_with(sheet.styles.len(), || None);
        for cell in sheet.cells.iter() {
            if cell.column < first || cell.column > last || cell.text.is_empty() {
                continue;
            }
            // A merged cell does not count, as in Excel.
            if sheet.merge_at(cell.row, cell.column).is_some() {
                continue;
            }
            let style = &sheet.styles[cell.style as usize];
            let slot = &mut measures[cell.style as usize];
            let (glyphs, excel_digit, drawn_digit, italic) = slot.get_or_insert_with(|| self.excel_measure(style, &ts));
            // The widest of its lines (a line break starts another: issue #17, a cell of 54 lines was fitted to all of them end to end).
            let text_width = cell.text.split('\n').map(|line| glyphs.width(line.trim_end_matches('\r'))).fold(0.0, f32::max);
            let width = text_width * *excel_digit / *drawn_digit + *italic + style.indent as f32 * 3.0 * *excel_digit;
            let w = &mut widest[(cell.column - first) as usize];
            *w = w.max(width);
        }
        for (i, widest) in widest.into_iter().enumerate() {
            if widest <= 0.0 {
                continue;
            }
            let column = first + i as u32;
            // No wider than Excel lets a column be (255 characters).
            let size = fitted_width(widest).min((max_column_width_pt(self.default_font_size_pt()) * EMU_PER_POINT as f32) as Emu);
            // Fitted, the column is the sheet's to widen again (not a width set by hand).
            commands.push(Command::Sheet(SheetCommand::SetColumnWidthAuto { slide, first: column, last: column, size }));
        }
        crate::debug_timing::report("fit columns: measure", started);
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("fitting rejected: {err}");
        }
        self.scroll_columns_horizontally_into_view(first, last);
        cx.notify();
    }

    /// Fit row `index` (and the selected ones, if it is among them) to
    /// their contents, as a double-click on its bottom line does.
    pub(crate) fn fit_rows(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        let (first, last) = self.selected_lines(Axis::Rows).filter(|(a, b)| (*a..=*b).contains(&index)).unwrap_or((index, index));
        let ts = window.text_system().clone();
        let commands = self.fit_rows_commands(first, last, &ts);
        if !commands.is_empty()
            && let Err(err) = self.document.execute(commands)
        {
            log::warn!("fitting rows rejected: {err}");
        }
        cx.notify();
    }

    /// What `excel_text_width` takes from a format, found once: its glyph widths, Excel's digit, the digit as drawn here, and the italic's extra.
    pub(crate) fn excel_measure<'a>(&self, style: &ibis_doc_model::CellStyle, ts: &'a TextSystem) -> (super::sheet_view::GlyphWidths<'a>, f32, f32, f32) {
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let digit = (default_size * 0.55).round().max(1.0);
        let size = style.font_size as f32 / 100.0;
        let excel_digit = digit * size / default_size;
        let mut glyphs = super::sheet_view::GlyphWidths::new(style, &self.resources, ts);
        let drawn_digit = glyphs.width("0").max(0.1);
        let italic = if style.italic { 2.0 * size / 12.0 } else { 0.0 };
        (glyphs, excel_digit, drawn_digit, italic)
    }

    /// How wide Excel draws a cell's text, in points. Excel measures in its
    /// own font, whose digit is the workbook's digit width (0.55 em, rounded,
    /// at the default size): the width as drawn here, scaled so a digit is
    /// that; its slanted (synthesized) italic needs about 2 pt more (tried in Excel).
    pub(crate) fn excel_text_width(&self, text: &str, style: &ibis_doc_model::CellStyle, ts: &TextSystem) -> f32 {
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let digit = (default_size * 0.55).round().max(1.0);
        let size = style.font_size as f32 / 100.0;
        let excel_digit = digit * size / default_size;
        let drawn_digit = super::sheet_view::text_width_pt("0", style, &self.resources, ts).max(0.1);
        let width = text.split('\n').map(|line| super::sheet_view::text_width_pt(line.trim_end_matches('\r'), style, &self.resources, ts)).fold(0.0, f32::max) * excel_digit / drawn_digit;
        let italic = if style.italic { 2.0 * size / 12.0 } else { 0.0 };
        width + italic + style.indent as f32 * 3.0 * excel_digit
    }

    /// The size of the workbook's default font, in points (its digit sets how wide a character of column width is).
    pub(crate) fn default_font_size_pt(&self) -> f32 {
        self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0)
    }

    /// The header's menu (the right button), for the columns or rows selected.
    pub(crate) fn header_menu(&self, axis: Axis) -> Vec<MenuEntry> {
        let menus = self.menus();
        let (unit, before, after) = match axis {
            Axis::Columns => (crate::shell::i18n::text("_xlsx.column"), crate::shell::i18n::text("_shell.insert-column-left"), crate::shell::i18n::text("_shell.insert-column-right")),
            Axis::Rows => (crate::shell::i18n::text("_xlsx.row"), crate::shell::i18n::text("_shell.insert-row-above"), crate::shell::i18n::text("_shell.insert-row-below")),
        };
        let (cut, copy, paste): (&'static str, &'static str, &'static str) = ("xlsx.header-cut", "xlsx.header-copy", "xlsx.header-paste");
        let id = |c: &'static str, r: &'static str| if axis == Axis::Columns { c } else { r };
        let mut out: Vec<MenuEntry> = vec![
            MenuItem::new(cut, "_xlsx.cut").icon("content_cut").shortcut("⌘X").on(|this, _, cx| this.copy_cells(true, cx)).into(),
            MenuItem::new(copy, "_shell.copy").icon("content_copy").shortcut("⌘C").on(|this, _, cx| this.copy_cells(false, cx)).into(),
            MenuItem::tr("xlsx.copy-as-picture").icon("image").on(|this, _, cx| this.copy_as_picture(cx)).into(),
            MenuItem::new(paste, "_xlsx.paste").icon("content_paste").shortcut("⌘V").on(|this, _, cx| this.paste_cells(false, cx)).into(),
        ];
        out.extend(menus.find_entry("xlsx.paste-special"));
        out.extend([
            MenuEntry::Separator,
            MenuItem::new(id("xlsx.header-insert-left", "xlsx.header-insert-above"), before).icon("add").shortcut("⌘+").on(move |this, _, cx| this.insert_lines(axis, false, cx)).into(),
            MenuItem::new(id("xlsx.header-insert-right", "xlsx.header-insert-below"), after).icon("add").on(move |this, _, cx| this.insert_lines(axis, true, cx)).into(),
            MenuItem::new(id("xlsx.header-delete-columns", "xlsx.header-delete-rows"), crate::shell::i18n::format("_xlsx.delete-unit", &[("unit", &unit)])).icon("delete").shortcut("⌘-").on(move |this, _, cx| this.delete_lines(axis, cx)).into(),
            MenuItem::tr("xlsx.header-clear").on(|this, _, cx| this.clear_selected_cells(cx)).into(),
            MenuItem::tr("xlsx.header-clear-formats").icon("format_clear").shortcut("⌘\\").on(|this, _, cx| this.clear_cell_formats(cx)).into(),
            MenuEntry::Separator,
            MenuItem::new(id("xlsx.header-format-columns", "xlsx.header-format-rows"), "_xlsx.format-cells")
                .icon("tune")
                .on(|this, window, cx| this.open_custom_format(window, cx))
                .into(),
            MenuItem::new(id("xlsx.header-column-width", "xlsx.header-row-height"), if axis == Axis::Columns { crate::shell::i18n::text("_xlsx.column-width") } else { crate::shell::i18n::text("_xlsx.row-height") })
                .on(move |this, window, cx| match axis {
                    Axis::Columns => this.open_column_width_panel(window, cx),
                    Axis::Rows => this.open_row_height_panel(window, cx),
                })
                .into(),
            MenuItem::new(id("xlsx.header-fit-columns", "xlsx.header-fit-rows"), if axis == Axis::Columns { crate::shell::i18n::text("_xlsx.autofit-column-width") } else { crate::shell::i18n::text("_xlsx.autofit-row-height") })
                .on(move |this, window, cx| this.fit_selected(axis, window, cx))
                .into(),
            MenuItem::new(id("xlsx.header-hide-columns", "xlsx.header-hide-rows"), crate::shell::i18n::format("_xlsx.hide-unit", &[("unit", &unit)])).icon("visibility_off").shortcut(if axis == Axis::Columns { "⌃0" } else { "⌃9" }).on(move |this, _, cx| this.hide_lines(axis, true, cx)).into(),
            MenuItem::new(id("xlsx.header-unhide-columns", "xlsx.header-unhide-rows"), crate::shell::i18n::format("_xlsx.unhide-unit", &[("unit", &unit)])).icon("visibility").on(move |this, _, cx| this.hide_lines(axis, false, cx)).into(),
            MenuEntry::Separator,
            MenuItem::new(id("xlsx.header-group-columns", "xlsx.header-group-rows"), "xlsx.group")
                .on(move |this, _, cx| this.group_lines_on_axis(axis, true, cx))
                .into(),
            MenuItem::new(id("xlsx.header-ungroup-columns", "xlsx.header-ungroup-rows"), "xlsx.ungroup")
                .on(move |this, _, cx| this.group_lines_on_axis(axis, false, cx))
                .into(),
        ]);
        if axis == Axis::Rows {
            let has_break = self
                .current_cell()
                .zip(self.current_sheet())
                .is_some_and(|((_, r), (_, s))| r > 0 && s.print.row_breaks.contains(&r));
            let row0 = self.current_cell().is_none_or(|(_, r)| r == 0);
            out.push(MenuEntry::Separator);
            out.push(
                MenuItem::new("xlsx.header-page-break", "_xlsx.page-break")
                    .checked(has_break)
                    .disabled(row0)
                    .on(|this, _, cx| this.toggle_row_break(cx))
                    .into(),
            );
        }
        out
    }

    /// The commands fitting rows `first`–`last` to their contents, as
    /// Excel's 行の高さの自動調整: the sheet's default height for a row of
    /// one line in the default size; a wrapped cell's lines (its text broken
    /// at the column's width) or a larger font make it taller.
    pub(crate) fn fit_rows_commands(&self, first: u32, last: u32, ts: &TextSystem) -> Vec<Command> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let base = if sheet.default_row > 0 { sheet.default_row as f32 / EMU_PER_POINT as f32 } else { 18.0 };
        let slide = self.current_slide;
        let mut out = Vec::new();
        for row in first..=last {
            let mut height = base;
            for cell in sheet.cells.iter().filter(|c| c.row == row && !c.text.is_empty()) {
                // In Excel, merged cells spanning multiple rows or columns do not participate in row auto-fitting.
                if sheet.merge_at(cell.row, cell.column).is_some_and(|m| m.first_row != m.last_row || m.first_column != m.last_column) {
                    continue;
                }
                let style = &sheet.styles[cell.style as usize];
                // A part of the text in a larger size sets the line.
                let size = cell.runs.iter().flat_map(|r| r.iter()).filter_map(|r| r.size).max().map_or(style.font_size, |s| s.max(style.font_size)) as f32 / 100.0;
                let line = base * size / default_size;
                let lines = if style.wrap {
                    let room = sheet.columns.get(cell.column as usize).copied().unwrap_or(0) as f32 / EMU_PER_POINT as f32 - 5.0;
                    // Break where the text is wider than the room (spaces first for Latin text).
                    let mut lines = 0;
                    for paragraph in cell.text.split('\n') {
                        let mut current = String::new();
                        lines += 1;
                        for ch in paragraph.chars() {
                            current.push(ch);
                            if self.excel_text_width(&current, style, ts) > room && current.chars().count() > 1 {
                                lines += 1;
                                current = ch.to_string();
                            }
                        }
                    }
                    lines.max(1)
                } else {
                    cell.text.split('\n').count().max(1)
                };
                // Turned text needs the height of its turned line (not measured
                // against Excel: its width by the angle's sine, its line by the
                // cosine); vertical text a character's height each.
                let turned = match style.rotation {
                    0 => 0.0,
                    255 => line * cell.text.chars().count() as f32,
                    r => {
                        let a = (if r <= 90 { r as f32 } else { r as f32 - 90.0 }).to_radians();
                        self.excel_text_width(&cell.text, style, ts) * a.sin() + line * a.cos() + 4.0
                    }
                };
                height = height.max(line * lines as f32).max(turned);
            }
            let size = ((height.min(409.5) * 4.0).round() / 4.0 * EMU_PER_POINT as f32) as Emu;
            let current_h = sheet.rows.get(row as usize).copied().filter(|&h| h > 0).unwrap_or(sheet.default_row);
            if current_h != size && size > 0 {
                out.push(Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first: row, last: row, size }));
            }
        }
        out
    }

    /// Fit the selected columns, or give the selected rows the sheet's default height.
    pub(crate) fn fit_selected(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        match axis {
            Axis::Columns => match self.selected_lines(Axis::Columns) {
                Some((a, _)) => self.fit_columns(a, window, cx),
                None => {
                    for c in r.first_column..=r.last_column {
                        self.fit_columns(c, window, cx);
                    }
                }
            },
            Axis::Rows => {
                let ts = window.text_system().clone();
                let commands = self.fit_rows_commands(r.first_row, r.last_row, &ts);
                if !commands.is_empty()
                    && let Err(err) = self.document.execute(commands)
                {
                    log::warn!("sizing rejected: {err}");
                }
                cx.notify();
            }
        }
    }
}

/// The width a column fitted to text `text_pt` wide gets: Excel shows a
/// number from text + 4.2 pt (found by trying widths in Excel; `###`
/// below) and leaves a little more when it fits a column. In quarter points.
/// The widest a column can be in Excel, in points (255 characters, matching `row_col_size.rs`).
pub(crate) fn max_column_width_pt(_default_size_pt: f32) -> f32 {
    MAX_COLUMN_WIDTH_CHARS * 6.4
}

pub(crate) fn fitted_width(text_pt: f32) -> Emu {
    let pt = ((text_pt + 5.0) * 4.0).ceil() / 4.0;
    (pt * EMU_PER_POINT as f32) as Emu
}

#[cfg(test)]
mod tests {
    use super::{HeaderHit, hit};

    #[test]
    fn hits() {
        let spans = [(0, 0.0, 50.0), (1, 50.0, 50.0), (2, 50.0, 120.0)];
        assert_eq!(hit(&spans, 20.0), Some(HeaderHit::Header(0)));
        // Where 0 ends (and the hidden 1): left of the line sizes 0, right
        // of it drags out 1.
        assert_eq!(hit(&spans, 49.0), Some(HeaderHit::Line { index: 0, start: 0.0 }));
        assert_eq!(hit(&spans, 51.0), Some(HeaderHit::Line { index: 1, start: 50.0 }));
        assert_eq!(hit(&spans, 118.0), Some(HeaderHit::Line { index: 2, start: 50.0 }));
        assert_eq!(hit(&spans, 80.0), Some(HeaderHit::Header(2)));
        assert_eq!(hit(&spans, 200.0), None);
    }

    #[test]
    fn row_height_and_col_width_limits() {
        use ibis_doc_model::EMU_PER_POINT;
        assert_eq!(super::MAX_ROW_HEIGHT_PT, 409.0);
        assert_eq!(super::MAX_COLUMN_WIDTH_CHARS, 255.0);
        let max_row_emu = (super::MAX_ROW_HEIGHT_PT * EMU_PER_POINT as f32).round() as i64;
        assert_eq!(max_row_emu, 409 * 12700);
    }
}
