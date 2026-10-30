// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workbook's right pane (todo-fix #79): no tabs; it shows the attributes of what is selected,
//! the cells' format (セルの書式設定), the chart's (グラフ エディター) or the shape's or text
//! box's (サイズと位置・塗りつぶし・枠線). データの入力規則 is opened from the Data menu and stays
//! until what is selected changes to another kind.
use gpui_kit::*;

use crate::shell::editor::SlideEditor;

/// What the right pane is for: the selection's kind (a chart or a shape by its id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RightKey {
    Cells,
    Chart(ibis_doc_model::ShapeId),
    Shape(ibis_doc_model::ShapeId),
}

impl SlideEditor {
    /// Explicit settings commands replace the previous settings session. Keeping this
    /// reset here prevents the sidebar render chain from choosing a stale panel first.
    pub(crate) fn begin_sheet_settings(&mut self) {
        self.document.end_repeat_sheet_group();
        self.wb_mut().close_settings();
    }

    /// The right pane's button / 表示 > サイドバー: show or hide the pane. Shown, it is for what is
    /// selected (the render's `sync_right_pane`).
    pub(crate) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        if let Some(s) = self.sheet_state_mut() {
            s.right_key = None;
        }
        cx.notify();
        window.refresh();
    }

    /// What the right pane is for now.
    fn right_key(&self) -> RightKey {
        if let Some((id, _)) = self.sheet_chart() {
            RightKey::Chart(id)
        } else if let Some(id) = self.selected_sheet_shape() {
            RightKey::Shape(id)
        } else {
            RightKey::Cells
        }
    }

    /// The pane follows the selection: with nothing of the attribute panels open it shows the
    /// selection's, and when the selection goes to another kind (or another chart, shape) the pane
    /// goes with it. `open` is whether another panel (the name manager, a search, ...) is
    /// already on show, which stays while the selection stays.
    pub(crate) fn sync_right_pane(&mut self, shown: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.sheet_range_pick_active() { return false; }
        if !self.sidebar_open || !self.is_workbook() {
            return false;
        }
        // The query editor is not a panel of the chain (it fills the window): the pane leaves it be (it was closed here,
        // as a panel no attribute pane stands for: xlsx::query001_editor).
        if self.sheet_state().is_some_and(|s| s.panels.query_editor.is_some()) {
            return false;
        }
        let key = self.right_key();
        let before = self.sheet_state().and_then(|s| s.right_key);
        // Only the attribute panels follow the selection; another panel (the name manager, a search,
        // the selection pane, ...) stays until it is closed.
        let attribute_panel = self.sheet_state().is_some_and(|s| s.custom_format.is_some() || s.chart_edit.is_some() || s.validation.is_some() || s.panels.frame.is_some());
        if shown && (!attribute_panel || before.is_none() || before == Some(key)) {
            if let Some(s) = self.sheet_state_mut() {
                s.right_key = Some(key);
            }
            self.refresh_shape_pane(key, window, cx);
            return false;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.right_key = Some(key);
            s.custom_format = None;
            s.validation = None;
            s.chart_edit = None;
            s.names = None;
            s.find = None;
            s.link = None;
            s.page_setup = false;
            s.panels.frame = None;
        }
        match key {
            RightKey::Cells => self.open_custom_format_unfocused(window, cx),
            RightKey::Chart(_) => self.open_sheet_chart_panel(window, cx),
            RightKey::Shape(_) => self.open_sheet_frame_panel(window, cx),
        }
        true
    }

    /// A shape's panel shows its place and size as they are now (after a drag, an undo).
    fn refresh_shape_pane(&mut self, key: RightKey, window: &mut Window, cx: &mut Context<Self>) {
        if let RightKey::Shape(id) = key {
            self.refresh_sheet_frame_panel(id, window, cx);
        }
    }
}

impl SlideEditor {
    /// Repeat the last semantic operation at the current selection, preserving
    /// the original style values rather than toggling the destination's values.
    pub(crate) fn repeat_sheet_edit(&mut self,cx:&mut Context<Self>){
        use ibis_doc_model::{Command,SheetCommand};
        use ibis_doc_model::sheet_command::Axis;
        let Some(range)=self.selected_range() else{return;};
        let slide=self.current_slide;
        let mut repeated=Vec::new();
        for edit in self.document.repeat_sheet_commands(){
            let next=match edit {
                SheetCommand::SetFormat{change,..}=>SheetCommand::SetFormat{slide,ranges:self.selected_ranges(),change:change.clone()},
                SheetCommand::ClearFormats{..}=>SheetCommand::ClearFormats{slide,ranges:self.selected_ranges()},
                SheetCommand::Insert{axis,count,..}=>SheetCommand::Insert{slide,axis:*axis,at:if *axis==Axis::Rows{range.first_row}else{range.first_column},count:*count},
                SheetCommand::Delete{axis,count,..}=>SheetCommand::Delete{slide,axis:*axis,at:if *axis==Axis::Rows{range.first_row}else{range.first_column},count:*count},
                SheetCommand::ShiftCells{down,insert,..}=>SheetCommand::ShiftCells{slide,range,down:*down,insert:*insert},
                _=>continue,
            };
            repeated.push(Command::Sheet(next));
        }
        if !repeated.is_empty(){self.document.end_repeat_sheet_group();self.run_sheet(repeated,cx);}
    }
}


impl super::State {
    /// All settings entry points share this reset, including selection attributes
    /// stored outside EditPanels. A newly requested dialog must never be hidden
    /// behind the previous panel in the render chain.
    pub(crate) fn close_settings(&mut self) {
        self.panels.close();
        self.custom_format=None;
        self.chart_edit=None;
        self.validation=None;
        self.names=None;
        self.find=None;
        self.link=None;
        self.page_setup=false;
        self.header_footer_edit=None;
        self.conditional_edit=None;
        self.sort_edit=None;
        self.right_key=None;
    }
}
