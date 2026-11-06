// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The view of a window: where it is scrolled, how far it is zoomed and what is selected, taken before a
//! file is read again (読み込み直す after another app changed it, `shell/file_watch.rs`) and put back after.
//!
//! What is kept, by screen:
//! - every screen: the scroll of the main content (x, y) and the zoom (a fit-to-window zoom stays a fit);
//! - the workbook (表計算画面): the shown sheet, and the selected cell and range of every sheet. The app keeps
//!   no scroll of a sheet it is not showing (a sheet opens at its top-left corner), so only the shown sheet's
//!   scroll is kept. A sheet is found by its name; if it is gone, the one at the same position (or the last);
//! - the document (文書画面): the caret and the other end of the selection, if the paragraph is still there and
//!   the offset is within it;
//! - the presentation (プレゼンテーション画面): the shown slide (the one at the same position, or the last if
//!   there are fewer slides) and the selected shapes that are still on it.
//!
//! Scrolling is put back as far as the new content allows (`clamp_workspace_scroll`).

use gpui_kit::*;
use ibis_doc_model::{FlowPos, ShapeId};

use crate::shell::editor::SlideEditor;

/// The view of one worksheet's selection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SheetCursor {
    pub name: Option<String>,
    pub cell: (u32, u32),
    pub end: Option<(u32, u32)>,
}

/// What only one screen has.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FormatView {
    Document { caret: Option<FlowPos>, anchor: Option<FlowPos> },
    Workbook { sheet_name: Option<String>, sheet_index: usize, cursors: Vec<SheetCursor> },
    Presentation { slide_index: usize, selection: Vec<ShapeId> },
}

/// A window's view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewState {
    pub scroll: Point<Pixels>,
    pub zoom: Option<f32>,
    pub format: FormatView,
}

impl SlideEditor {
    /// The view now.
    pub(crate) fn save_view_state(&self) -> ViewState {
        let presentation = self.presentation();
        let format = if self.is_workbook() {
            let sheet_index = presentation.slide_index(self.current_slide).unwrap_or(0);
            let state = self.sheet_state();
            let cursors = presentation
                .slides
                .iter()
                .filter_map(|slide| {
                    let state = state?;
                    let cell = state.cells.get(&slide.id).copied()?;
                    Some(SheetCursor { name: slide.name.clone(), cell, end: state.cell_ends.get(&slide.id).copied() })
                })
                .collect();
            FormatView::Workbook { sheet_name: self.slide().name.clone(), sheet_index, cursors }
        } else if self.is_document() {
            let state = self.doc_state();
            FormatView::Document { caret: state.and_then(|s| s.caret), anchor: state.and_then(|s| s.anchor) }
        } else {
            FormatView::Presentation {
                slide_index: presentation.slide_index(self.current_slide).unwrap_or(0),
                selection: self.selected_ids(),
            }
        };
        ViewState { scroll: self.workspace_scroll.offset(), zoom: self.zoom, format }
    }

    /// Put `view` back on what was read (the sheet, slide, selection, then the zoom and the scroll).
    pub(crate) fn restore_view_state(&mut self, view: &ViewState, window: &mut Window, cx: &mut Context<Self>) {
        match &view.format {
            FormatView::Workbook { sheet_name, sheet_index, cursors } if self.is_workbook() => {
                let slides: Vec<(ibis_doc_model::SlideId, Option<String>)> =
                    self.presentation().slides.iter().map(|s| (s.id, s.name.clone())).collect();
                if slides.is_empty() {
                    return;
                }
                let by_name = sheet_name.as_ref().and_then(|n| slides.iter().position(|(_, name)| name.as_ref() == Some(n)));
                let target = by_name.unwrap_or((*sheet_index).min(slides.len() - 1));
                // The cursors of the sheets still there; a sheet that is gone hands its cursor to the one shown instead.
                let shown_cursor = cursors.iter().find(|c| c.name.is_some() && c.name == *sheet_name).or_else(|| cursors.iter().find(|c| c.name.is_none() && sheet_name.is_none()));
                for (id, name) in &slides {
                    let Some(cursor) = cursors.iter().find(|c| c.name.is_some() && c.name == *name) else { continue };
                    self.put_cursor(*id, cursor);
                }
                if by_name.is_none()
                    && let Some(cursor) = shown_cursor
                {
                    self.put_cursor(slides[target].0, cursor);
                }
                self.select_slide(slides[target].0, window, cx);
            }
            FormatView::Presentation { slide_index, selection } if self.is_presentation() => {
                let slides = &self.presentation().slides;
                if slides.is_empty() {
                    return;
                }
                let id = slides[(*slide_index).min(slides.len() - 1)].id;
                self.select_slide(id, window, cx);
                let on_slide: Vec<ShapeId> = self.slide().shapes.iter().map(|s| s.id).collect();
                self.set_selection(selection.iter().copied().filter(|id| on_slide.contains(id)).collect());
            }
            FormatView::Document { caret, anchor } if self.is_document() => {
                let valid = |pos: &FlowPos| {
                    self.flow()
                        .and_then(|doc| ibis_doc_model::flow_command::paragraph(doc, pos.para))
                        .is_some_and(|p| pos.offset <= p.len())
                };
                if let Some(caret) = caret.filter(valid) {
                    match anchor.filter(valid) {
                        Some(anchor) => {
                            self.set_caret(anchor, false, cx);
                            self.set_caret(caret, true, cx);
                        }
                        None => self.set_caret(caret, false, cx),
                    }
                }
            }
            _ => {}
        }
        // A worksheet opens at its own zoom (`show_page`): the zoom the window had is put back over it.
        self.zoom = view.zoom;
        self.workspace_scroll.set_offset(view.scroll);
        self.clamp_workspace_scroll();
        cx.notify();
    }

    fn put_cursor(&mut self, slide: ibis_doc_model::SlideId, cursor: &SheetCursor) {
        let Some(state) = self.sheet_state_mut() else { return };
        state.cells.insert(slide, cursor.cell);
        match cursor.end {
            Some(end) => state.cell_ends.insert(slide, end),
            None => state.cell_ends.remove(&slide),
        };
    }
}
