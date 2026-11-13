// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! メモ (Excel's notes, the comments of older versions), written in place as a text box.
//!
//! - 挿入 > 新しいメモ (or 編集 for a cell with one): a square yellow box beside the cell, typed into at once. A new
//!   note starts with `<the user's name>:` and a line break, the caret on the second line. The box has the eight handles
//!   of a text box (resizing), and its border moves it; the text takes the size, colour, bold, italic and underline the
//!   toolbar gives (the edit is a `CellEdit` with a `NoteEdit`, so the keys, the input method and the formats of the cell
//!   being typed into work in it).
//! - The pointer on the selected cell that has a note for a second shows the note; it stays a second after the pointer
//!   leaves the cell, and as long as the pointer is on it; a click on it edits it.
//! - The box's top left corner is joined to the cell's top right corner by a grey line.
//!
//! Threaded comments are another feature (threaded_comments.rs).
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CellRun, CellStyle, CellUnderline, Command, EmuRect, NoteRect, ResizeHandle, SheetCommand, SheetComment};

use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;

use super::cell_edit::{CellEdit, EditMode};
use crate::ext::*;

/// The pointer on the selected cell for this long shows its note (the user's rule 2026-11-04).
pub(crate) const HOVER_MS: u64 = 1000;
/// How long a note shown stays after the pointer left the cell (or the note).
pub(crate) const GRACE_MS: u64 = 1000;
/// The font size of a note's text, in hundredths of a point (Excel's 9 pt).
pub(crate) const FONT_SIZE: u32 = 900;
/// The typeface of a note's text (the Japanese Excel's default for notes, MS PGothic).
const FONT: &str = "MS PGothic"; // i18n-ok（フォント名）
/// A note's fill (the `fillcolor` Excel writes for a note) and the border and the grey of the line to the cell.
const FILL: u32 = 0xFFFFE1;
const BORDER: u32 = 0x000000;
const CONNECTOR: u32 = 0x808080;
/// The least a note's box is made by dragging a handle (EMU: 24 pt x 16 pt).
const MIN_SIDE: i64 = 16 * ibis_doc_model::EMU_PER_POINT;
/// How far outside the box's border the strip that moves it reaches (pixels).
const MOVE_STRIP: f32 = 3.0;

/// A note being typed into (the `CellEdit`'s `note`).
#[derive(Clone, Debug)]
pub(crate) struct NoteEdit {
    /// The box, relative to the cell's top right corner.
    pub(crate) rect: NoteRect,
    pub(crate) author: String,
    /// The note as it was (none for a new one).
    pub(crate) original: Option<SheetComment>,
    /// A drag of the box going on.
    pub(crate) drag: Option<NoteDrag>,
}

/// A drag of the box: its handle (none: the border, moving it), where the pointer was pressed and the box then.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NoteDrag {
    pub(crate) handle: Option<ResizeHandle>,
    pub(crate) from: Point<Pixels>,
    pub(crate) rect: NoteRect,
}

/// The hover card of a note: what the pointer did and the timers (dropped when replaced, which cancels them; they run
/// on the executor's clock, which a scenario advances).
#[derive(Default)]
pub(crate) struct NoteCard {
    /// The cell whose note shows.
    pub(crate) shown: Option<(u32, u32)>,
    /// The cell (of a note) the pointer's last move was in.
    over: Option<(u32, u32)>,
    /// The pointer is on the card.
    on_card: bool,
    /// Where the card was drawn (for the scenarios).
    pub(crate) bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    show_timer: Option<((u32, u32), Task<()>)>,
    grace_timer: Option<Task<()>>,
}

#[ibis_office_macros::editor_ext(Notes)]
impl SlideEditor {
    /// The note of a cell (a threaded comment's placeholder note is not one).
    pub(crate) fn note_of(&self, column: u32, row: u32) -> Option<&SheetComment> {
        let (_, sheet) = self.current_sheet()?;
        sheet.comment_at(row, column).filter(|_| !sheet.threads.iter().any(|t| (t.column, t.row) == (column, row)))
    }

    /// The note being typed into, if one is.
    pub(crate) fn note_edit(&self) -> Option<&NoteEdit> {
        self.cell_edit().filter(|e| e.slide == self.current_slide).and_then(|e| e.note.as_ref())
    }

    /// The pixels of a point on the screen.
    fn note_px_per_point(&self) -> Option<f32> {
        Some(ibis_doc_model::EMU_PER_POINT as f32 * self.slide_layout()?.scale)
    }

    /// The typeface a note's text is laid out in.
    pub(crate) fn note_font(&self) -> String {
        self.note_style().font.unwrap_or_default()
    }

    /// The style a note's text is laid out in: the workbook's, at the note's size, from the top left, wrapped.
    fn note_style(&self) -> CellStyle {
        let mut style = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        style.font_size = FONT_SIZE;
        // Excel's note font (a Gothic): a PC without it gets the Gothic the font substitution picks.
        style.font = Some(FONT.into());
        style.vertical = ibis_doc_model::CellVerticalAlign::Top;
        style.vertical_set = true;
        style.align = ibis_doc_model::CellAlign::Left;
        style.wrap = true;
        style.fill = None;
        style.indent = 0;
        style.rotation = 0;
        style.shrink = false;
        style.number_format = "General".into();
        style
    }

    /// The text of a note laid out in a box `width` x `height` pixels, the lines' places from the box's corner.
    fn note_layout(&self, cx: &App, text: &str, runs: Option<Arc<Vec<CellRun>>>, width: f32, height: f32, px_per_point: f32) -> Option<super::sheet_view::EditLayout> {
        let (_, sheet) = self.current_sheet()?;
        let style = self.note_style();
        let ts = WindowTextSystem::new(cx.text_system().clone());
        // A cell far from every cell there is: its own style is the one given.
        let far = u32::MAX - 1;
        Some(super::sheet_view::edit_layout(&ts, sheet, Some(&style), &self.resources, far, far, text, runs, (0.0, 0.0), (width, height), px_per_point, width))
    }

    /// The box of a note on the screen: its top left corner and its size (pixels), the cell's top right corner it is joined to.
    pub(crate) fn note_screen_box(&self, column: u32, row: u32, rect: NoteRect) -> Option<(Point<Pixels>, Size<Pixels>, Point<Pixels>)> {
        let layout = self.slide_layout()?;
        let tr = self.cell_top_right(column, row)?;
        let origin = point(tr.x + layout.to_px(rect.dx), tr.y + layout.to_px(rect.dy));
        Some((origin, size(layout.to_px(rect.width), layout.to_px(rect.height)), tr))
    }

    /// The box a note without one gets: beside its cell.
    fn default_note_rect(&self, row: u32) -> NoteRect {
        let top = self.current_sheet().and_then(|(_, s)| s.row_offsets().get(row as usize).copied()).unwrap_or(0);
        NoteRect::default_for(top)
    }

    /// 新しいメモ / メモの編集: the selected cell's note typed into, in its box (a new note holds `name:` and a line break,
    /// the caret on the second line).
    pub(crate) fn begin_note_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_read_only() {
            return;
        }
        self.commit_cell_edit(cx);
        if self.cell_edit().is_some() {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        // A cell with a thread has its own panel.
        if self.current_sheet().is_some_and(|(_, s)| s.threads.iter().any(|t| (t.column, t.row) == (column, row))) {
            return;
        }
        let original = self.note_of(column, row).cloned();
        let author = original.as_ref().map(|c| c.author.clone()).filter(|a| !a.is_empty()).unwrap_or_else(crate::shell::user_name::user_name);
        let part = |text: &str, bold: bool| CellRun { text: text.to_string(), size: Some(FONT_SIZE), bold: Some(bold), italic: Some(false), underline: Some(CellUnderline::None), strike: Some(false), color: Some(ibis_doc_model::Rgb::BLACK), font: None };
        let (text, runs) = match &original {
            Some(c) if !c.runs.is_empty() && c.runs.iter().map(|r| r.text.as_str()).collect::<String>() == c.text => (c.text.clone(), c.runs.clone()),
            Some(c) => (c.text.clone(), vec![part(&c.text, false)]),
            None => {
                let head = format!("{author}:");
                (format!("{head}\n"), vec![part(&head, true), part("\n", false)])
            }
        };
        let rect = original.as_ref().and_then(|c| c.rect).unwrap_or_else(|| self.default_note_rect(row));
        let slide = self.current_slide;
        let caret = text.encode_utf16().count();
        self.set_selection(Vec::new());
        if let Some(state) = self.sheet_state_mut() {
            state.note_card = NoteCard::default();
            state.comment_hover = None;
            state.validation_warning = None;
            state.cell_edit = Some(CellEdit {
                slide,
                column,
                row,
                text,
                caret,
                marked: None,
                marked_clauses: Vec::new(),
                mode: EditMode::Edit,
                anchor: None,
                runs: Some(runs),
                pointed: None,
                reference_drag: false,
                rect: Default::default(),
                pixels_per_point: Default::default(),
                layout: Default::default(),
                dragging: None,
                dragging_in_bar: false,
                pending: None,
                scroll_y: Default::default(),
                followed_caret: Default::default(),
                fitted_height: Default::default(),
                line_nav: Default::default(),
                note: Some(NoteEdit { rect, author, original, drag: None }),
            });
        }
        self.focus_handle.focus(window, cx);
        self.seed_note_geometry(cx);
        cx.notify();
    }

    /// Where the text of the note being typed into is and how it is laid out, known before it is drawn (the pointer's
    /// press that begins the edit places the caret in it).
    pub(crate) fn seed_note_geometry(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.cell_edit().cloned() else { return };
        let Some(note) = &e.note else { return };
        let (Some((origin, size, _)), Some(ppp)) = (self.note_screen_box(e.column, e.row, note.rect), self.note_px_per_point()) else { return };
        let (w, h) = (f32::from(size.width) - 2.0, f32::from(size.height) - 2.0);
        let runs = e.runs.clone().map(Arc::new);
        let Some(laid) = self.note_layout(cx, &e.text, runs, w, h, ppp) else { return };
        e.pixels_per_point.set(ppp);
        e.rect.set(Some(Bounds::new(origin + point(px(1.0), px(1.0)), gpui_kit::size(px(w), px(h)))));
        *e.layout.borrow_mut() = Some(Rc::new(laid));
    }

    /// Finish typing the note (Esc, a click elsewhere, another command): its text, formats and box are put on the cell
    /// as one step; a note with no text is taken off.
    pub(crate) fn finish_note_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.take_if(|e| e.note.is_some())) else { return };
        let Some(note) = edit.note.clone() else { return };
        let (slide, row, column) = (edit.slide, edit.row, edit.column);
        let runs = edit.runs.clone().filter(|r| r.iter().map(|x| x.text.as_str()).collect::<String>() == edit.text).unwrap_or_default();
        let comment = (!edit.text.trim().is_empty()).then(|| SheetComment { row, column, author: note.author.clone(), text: edit.text.clone(), runs, rect: Some(note.rect) });
        if comment != note.original {
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetComment { slide, row, column, comment })]) {
                self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            }
        }
        cx.notify();
    }

    /// メモの削除 / コメントとメモのクリア: the notes of the selected cells
    /// (and of the ranges selected with them) taken off, one step.
    pub(crate) fn delete_notes(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let commands: Vec<Command> = sheet
            .comments
            .iter()
            .filter(|c| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)))
            .map(|c| Command::Sheet(SheetCommand::SetComment { slide, row: c.row, column: c.column, comment: None }))
            .collect();
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        if let Some(s) = self.sheet_state_mut() {
            s.note_card = NoteCard::default();
        }
        cx.notify();
    }

    // ----- The card shown by the pointer ---------------------------------------------------------------------------

    /// The pointer moved with no button down: on the selected cell that has a note (a second shows it), on the card, or
    /// elsewhere (a second later the card goes).
    pub(crate) fn note_pointer_moved(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let editing = self.cell_edit().is_some();
        let current = self.current_cell();
        let over = (!editing).then(|| self.cell_under(position)).flatten().filter(|&c| Some(c) == current).filter(|&(column, row)| self.note_of(column, row).is_some());
        let Some(state) = self.sheet_state() else { return };
        let on_card = state.note_card.shown.is_some() && state.note_card.bounds.get().is_some_and(|b| b.dilate(px(1.)).contains(&position));
        let shown = state.note_card.shown;
        let card = &mut self.wb_mut().note_card;
        card.over = over;
        card.on_card = on_card;
        match over {
            Some(cell) => {
                card.grace_timer = None;
                if shown != Some(cell) && card.show_timer.as_ref().map(|t| t.0) != Some(cell) {
                    let timer = cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(std::time::Duration::from_millis(HOVER_MS)).await;
                        this.update(cx, |this, cx| {
                            // Still in the cell, a second on.
                            if let Some(s) = this.sheet_state_mut()
                                && s.note_card.over == Some(cell)
                            {
                                s.note_card.shown = Some(cell);
                                s.note_card.show_timer = None;
                                cx.notify();
                            }
                        })
                        .ok();
                    });
                    self.wb_mut().note_card.show_timer = Some((cell, timer));
                }
            }
            None => {
                card.show_timer = None;
                if on_card {
                    card.grace_timer = None;
                } else if shown.is_some() {
                    self.start_note_grace(cx);
                }
            }
        }
    }

    /// The pointer is on the card or left it.
    fn note_card_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let Some(card) = self.sheet_state_mut().map(|s| &mut s.note_card) else { return };
        card.on_card = hovered;
        if hovered {
            card.grace_timer = None;
        } else if card.shown.is_some() && card.over.is_none() {
            self.start_note_grace(cx);
        }
    }

    /// The card goes after the grace time, unless the pointer comes back to the cell or onto the card (which cancel it).
    fn start_note_grace(&mut self, cx: &mut Context<Self>) {
        if self.wb().note_card.grace_timer.is_some() {
            return;
        }
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(GRACE_MS)).await;
            this.update(cx, |this, cx| {
                if let Some(s) = this.sheet_state_mut() {
                    let bounds = s.note_card.bounds.clone();
                    s.note_card = NoteCard { bounds, ..Default::default() };
                }
                cx.notify();
            })
            .ok();
        });
        self.wb_mut().note_card.grace_timer = Some(timer);
    }

    /// Show the note of `cell` now (the next note's navigation): it stays as a shown one does.
    pub(crate) fn show_note_card(&mut self, cell: (u32, u32), cx: &mut Context<Self>) {
        if self.note_of(cell.0, cell.1).is_none() {
            return;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.note_card.shown = Some(cell);
            s.note_card.over = None;
        }
        self.start_note_grace(cx);
        cx.notify();
    }

    // ----- Dragging the box -------------------------------------------------------------------------------------------

    /// A press on a handle (`Some`) or on the border (none): the box follows the pointer until it is released.
    fn start_note_drag(&mut self, handle: Option<ResizeHandle>, from: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(note) = self.cell_edit_mut().and_then(|e| e.note.as_mut()) {
            note.drag = Some(NoteDrag { handle, from, rect: note.rect });
            cx.notify();
        }
    }

    /// The pointer moved with the button down: the box moves or is resized from where it was when pressed.
    fn drag_note_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        let Some(note) = self.cell_edit_mut().and_then(|e| e.note.as_mut()) else { return };
        let Some(drag) = note.drag else { return };
        let (dx, dy) = (layout.to_emu_len(position.x - drag.from.x), layout.to_emu_len(position.y - drag.from.y));
        let start = drag.rect;
        note.rect = match drag.handle {
            None => NoteRect { dx: start.dx + dx, dy: start.dy + dy, ..start },
            Some(handle) => {
                let r = EmuRect::new(start.dx, start.dy, start.width, start.height).resized(handle, dx, dy, MIN_SIDE);
                NoteRect { dx: r.x, dy: r.y, width: r.width, height: r.height }
            }
        };
        self.seed_note_geometry(cx);
        cx.notify();
    }

    fn end_note_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(note) = self.cell_edit_mut().and_then(|e| e.note.as_mut())
            && note.drag.take().is_some()
        {
            cx.notify();
        }
    }

    // ----- Drawing ---------------------------------------------------------------------------------------------------

    /// The notes on the sheet: the one typed into (with its handles), the one the pointer shows, and all of them when
    /// 常にメモを表示 is on.
    pub(crate) fn render_notes(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.wb().filter_panel.is_some() || self.page_layout_shown() {
            return Vec::new();
        }
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let mut out = Vec::new();
        let editing = self.note_edit().map(|n| (self.cell_edit().map_or((0, 0), |e| (e.column, e.row)), n));
        if let Some((cell, _)) = &editing {
            if let Some(card) = self.render_note_box(*cell, cx) {
                out.push(card);
            }
        }
        let editing_cell = editing.map(|(cell, _)| cell);
        let mut cells: Vec<(u32, u32)> = Vec::new();
        if self.sheet_state().is_some_and(|s| s.show_all_notes) {
            cells.extend(sheet.comments.iter().map(|n| (n.column, n.row)).filter(|&(c, r)| self.note_of(c, r).is_some()));
        }
        if let Some(shown) = self.sheet_state().and_then(|s| s.note_card.shown).filter(|&c| Some(c) == self.current_cell()) {
            if !cells.contains(&shown) {
                cells.push(shown);
            }
        }
        for cell in cells.into_iter().filter(|c| Some(*c) != editing_cell) {
            if self.cell_edit().is_some() {
                break;
            }
            if let Some(card) = self.render_note_box(cell, cx) {
                out.push(card);
            }
        }
        out
    }

    /// One note's box: yellow, square, its text in its fonts, joined to its cell by a grey line; typed into (caret,
    /// selection, the eight handles, the border that moves it), or shown, and a click on it edits it.
    fn render_note_box(&self, (column, row): (u32, u32), cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing = self.cell_edit().filter(|e| e.slide == self.current_slide && (e.column, e.row) == (column, row) && e.note.is_some());
        let (text, runs, rect) = match editing {
            Some(e) => (e.text.clone(), e.runs.clone().unwrap_or_default(), e.note.as_ref()?.rect),
            None => {
                let n = self.note_of(column, row)?;
                let runs = if n.runs.iter().map(|r| r.text.as_str()).collect::<String>() == n.text { n.runs.clone() } else { Vec::new() };
                (n.text.clone(), runs, n.rect.unwrap_or_else(|| self.default_note_rect(row)))
            }
        };
        let (origin, size, tr) = self.note_screen_box(column, row, rect)?;
        // A note of a cell scrolled out of the view is not drawn (the one typed into stays).
        if editing.is_none() && self.workspace_bounds.is_some_and(|b| !b.contains(&tr)) {
            return None;
        }
        let ppp = self.note_px_per_point()?;
        let (w, h) = (f32::from(size.width), f32::from(size.height));
        let layout = Rc::new(self.note_layout(cx, &text, (!runs.is_empty()).then(|| Arc::new(runs)), w - 2.0, h - 2.0, ppp)?);
        let theme = cx.theme().clone();
        let owner = cx.weak_entity();
        let selection = editing.and_then(|e| e.selection().map(|r| (super::cell_edit::byte_at(&e.text, r.start), super::cell_edit::byte_at(&e.text, r.end))));
        let caret = editing.map(|e| (e.caret_byte(), e.caret, e.displayed_line(&layout)));
        let edit_rect = editing.map(|e| (e.rect.clone(), e.layout.clone(), e.pixels_per_point.clone()));
        if let Some((rect_cell, layout_cell, ppp_cell)) = &edit_rect {
            ppp_cell.set(ppp);
            *layout_cell.borrow_mut() = Some(layout.clone());
            let _ = rect_cell;
        }
        let focus = self.focus_handle.clone();
        let painted = layout.clone();
        let rect_cell = edit_rect.as_ref().map(|r| r.0.clone());
        let bounds_cell = self.wb().note_card.bounds.clone();
        let is_editing = editing.is_some();
        let box_down = cx.listener(move |this, e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            if this.note_edit().is_none() {
                // A click on the card: it is edited, the caret where the pointer is.
                this.begin_note_edit(window, cx);
            }
            this.focus_handle.focus(window, cx);
            this.press_in_cell_text(e.position, e.click_count, e.modifiers.shift, cx);
        });
        let inner = div()
            .id(("note-text", row as usize * 100_000 + column as usize))
            .absolute()
            .size_full()
            .bg(rgb(FILL))
            .border_1()
            .border_color(rgb(BORDER))
            // Typed into, the text and the caret go on past the box (its background ends); shown, the box cuts the text.
            .when(!is_editing, |d| d.overflow_hidden())
            .cursor_text()
            .on_mouse_down(MouseButton::Left, box_down)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        let o = bounds.origin;
                        if let Some(r) = &rect_cell {
                            r.set(Some(bounds));
                        }
                        if let Some((b0, b1)) = selection {
                            for l in painted.lines.iter() {
                                let (a, b) = (b0.max(l.start), b1.min(l.end));
                                if a < b {
                                    let (xa, xb) = (l.x_at(a), l.x_at(b));
                                    window.paint_quad(fill(Bounds::new(point(o.x + px(xa), o.y + px(l.baseline - l.ascent)), gpui_kit::size(px(xb - xa), px(l.ascent + l.descent))), rgba(0x3390ff55)));
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
                        if let Some((_, caret_utf16, line)) = caret
                            && let Some(l) = painted.lines.get(line).filter(|_| focus.is_focused(window) && crate::shell::caret::visible(format!("note:{caret_utf16}:{}", painted.lines.len()), window, cx))
                        {
                            let b = caret.map_or(0, |c| c.0);
                            window.paint_quad(fill(Bounds::new(point(o.x + px(l.x_at(b)), o.y + px(l.baseline - l.ascent)), gpui_kit::size(px(1.0), px(l.ascent + l.descent))), rgb(0x000000)));
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        // The grey line from the box's top left corner to the cell's top right corner.
        let tr_offset = tr - origin;
        let connector = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let mut path = PathBuilder::stroke(px(1.0));
                path.move_to(bounds.origin);
                path.line_to(bounds.origin + tr_offset);
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(CONNECTOR));
                }
            },
        )
        .absolute()
        .size_full();
        let mut outer = div()
            .id(("note-box", row as usize * 100_000 + column as usize))
            .relative()
            .w(size.width)
            .h(size.height)
            .child(connector)
            .child(inner);
        if is_editing {
            // The eight handles of a text box, and the border that moves the box (the handles above it).
            let strips = [
                (px(0.), px(-MOVE_STRIP), size.width, px(MOVE_STRIP * 2.)),
                (px(0.), size.height - px(MOVE_STRIP), size.width, px(MOVE_STRIP * 2.)),
                (px(-MOVE_STRIP), px(0.), px(MOVE_STRIP * 2.), size.height),
                (size.width - px(MOVE_STRIP), px(0.), px(MOVE_STRIP * 2.), size.height),
            ];
            for (i, (x, y, sw, sh)) in strips.into_iter().enumerate() {
                outer = outer.child(
                    div()
                        .id(("note-move", i))
                        .absolute()
                        .left(x)
                        .top(y)
                        .w(sw)
                        .h(sh)
                        .cursor(CursorStyle::OpenHand)
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_note_drag(None, e.position, cx);
                        })),
                );
            }
            let points = ResizeHandle::ALL.map(|handle| {
                let (ax, ay) = handle.anchor();
                (handle, point(size.width * (ax as f32 / 2.0), size.height * (ay as f32 / 2.0)))
            });
            for (handle, element) in crate::shell::selection_handles::group("note-handles".into(), points, None, Default::default(), theme.primary, theme.background) {
                let Some(handle) = handle else { continue };
                outer = outer.child(element.on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.start_note_drag(Some(handle), e.position, cx);
                })));
            }
            // The pointer anywhere (also off the box): the drag follows it and ends where it is released.
            let (move_owner, up_owner) = (owner.clone(), owner.clone());
            outer = outer.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let owner = move_owner.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && e.pressed_button == Some(MouseButton::Left) {
                                owner.update(cx, |this, cx| this.drag_note_to(e.position, cx)).ok();
                            }
                        });
                        let owner = up_owner.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && e.button == MouseButton::Left {
                                owner.update(cx, |this, cx| this.end_note_drag(cx)).ok();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        } else {
            let hover_owner = owner.clone();
            outer = outer
                .on_hover(move |hovered, _, cx| {
                    hover_owner.update(cx, |this, cx| this.note_card_hovered(*hovered, cx)).ok();
                })
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| bounds_cell.set(Some(bounds))).absolute().size_full());
        }
        Some(deferred(anchored().position(origin).snap_to_window_with_margin(px(0.)).child(outer)).with_priority(1).into_any_element())
    }
}
