// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workbook's part of a window's view (`shell/view_state.rs`), taken before the file is read again and put
//! back after: the shown sheet, and the selected cell and range of every sheet. The app keeps no scroll of a
//! sheet it is not showing (a sheet opens at its top-left corner), so only the shown sheet's scroll is kept (by
//! the shell). A sheet is found by its name; if it is gone, the one at the same position (or the last).

use gpui_kit::*;
use ibis_doc_model::SlideId;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The view of one worksheet's selection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SheetCursor {
    pub name: Option<String>,
    pub cell: (u32, u32),
    pub end: Option<(u32, u32)>,
}

/// What a workbook's window keeps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeptView {
    pub sheet_name: Option<String>,
    pub sheet_index: usize,
    pub cursors: Vec<SheetCursor>,
}

#[ibis_office_macros::editor_ext(ViewState)]
impl SlideEditor {
    /// The shown sheet and every sheet's cursor now.
    pub(crate) fn keep_sheet_view(&self) -> KeptView {
        let presentation = self.presentation();
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
        KeptView { sheet_name: self.slide().name.clone(), sheet_index, cursors }
    }

    /// Put `view` back on the workbook that was read.
    pub(crate) fn put_sheet_view_back(&mut self, view: &KeptView, window: &mut Window, cx: &mut Context<Self>) {
        let KeptView { sheet_name, sheet_index, cursors } = view;
        let slides: Vec<(SlideId, Option<String>)> = self.presentation().slides.iter().map(|s| (s.id, s.name.clone())).collect();
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

    fn put_cursor(&mut self, slide: SlideId, cursor: &SheetCursor) {
        let Some(state) = self.sheet_state_mut() else { return };
        state.cells.insert(slide, cursor.cell);
        match cursor.end {
            Some(end) => state.cell_ends.insert(slide, end),
            None => state.cell_ends.remove(&slide),
        };
    }
}
