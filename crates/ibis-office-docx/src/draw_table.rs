// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 罫線を引いて表を作成 (DOCX-TABLE-002, Word's 挿入 > 表 > 罫線を引く): with the mode on, dragging a box on the page makes
//! a table of one cell as wide and as tall as the box, where the drag began; more cells are made of it by splitting.

use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowPos, RowHeight, TableEdit};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// A box being drawn: its page, where it began and where the mouse is (points of the page).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawBox {
    pub(crate) page: usize,
    pub(crate) start: (f32, f32),
    pub(crate) now: (f32, f32),
}

/// What the box being drawn becomes besides a table: a shape of a preset geometry, or a line (DOCX-IMAGE-011).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawKind {
    Shape(&'static str),
    Line { arrow: bool },
    /// An elbow (`bentConnector3`) or curved (`curvedConnector3`) connector, with an arrow at its end or not.
    Connector { curved: bool, arrow: bool },
}

/// The smallest box that makes a table (points).
const MIN_SIZE: (f32, f32) = (24.0, 12.0);

#[ibis_office_macros::editor_ext(DrawTable)]
impl SlideEditor {
    /// 挿入 > 罫線を引いて表を作成: the mode on or off.
    pub(crate) fn toggle_draw_table(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.draw_table_mode = !state.draw_table_mode;
            state.draw_box = None;
        }
        cx.notify();
    }

    /// 挿入 > 図形: the next drag on the page draws this shape (a click puts it at its usual size).
    pub(crate) fn arm_draw_shape(&mut self, kind: DrawKind, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.draw_shape = Some(kind);
            state.draw_table_mode = false;
            state.draw_box = None;
        }
        cx.notify();
    }

    /// The mouse went down on the page with the mode on. Whether it began a box.
    pub(crate) fn begin_draw_table(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if !self.doc_state().is_some_and(|s| s.draw_table_mode || s.draw_shape.is_some()) {
            return false;
        }
        if let Some(state) = self.doc_state_mut() {
            state.draw_box = Some(DrawBox { page, start: (x, y), now: (x, y) });
        }
        cx.notify();
        true
    }

    pub(crate) fn drag_draw_table(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if let Some(b) = self.doc_state_mut().and_then(|s| s.draw_box.as_mut()) {
            b.now = (x, y);
            cx.notify();
        }
    }

    /// The mouse went up: a box big enough becomes a table of one cell (one step of undo), the mode off.
    pub(crate) fn end_draw_table(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.doc_state_mut().and_then(|s| s.draw_box.take()) else { return };
        let (w, h) = ((b.now.0 - b.start.0).abs(), (b.now.1 - b.start.1).abs());
        if let Some(kind) = self.doc_state_mut().and_then(|s| s.draw_shape.take()) {
            self.finish_draw_shape(kind, b, cx);
            return;
        }
        if w < MIN_SIZE.0 || h < MIN_SIZE.1 {
            cx.notify();
            return;
        }
        let (x, y) = (b.start.0.min(b.now.0), b.start.1.min(b.now.1));
        let Some(at) = self.position_at(b.page, x, y) else { return };
        let Some(doc) = self.flow() else { return };
        let first = doc.new_id();
        let emu = |v: f32| (v * 12_700.0) as i64;
        let ok = self.run_flow(
            vec![FlowCommand::InsertTable { at, rows: 1, columns: 1, width: emu(w) }, FlowCommand::EditTable { table: first, edit: TableEdit::RowHeight { row: 0, height: RowHeight::AtLeast(emu(h)) } }],
            cx,
        );
        if let Some(state) = self.doc_state_mut() {
            state.draw_table_mode = false;
        }
        if ok && self.flow().is_some_and(|d| ibis_doc_model::flow_command::paragraph(d, first + 2).is_some()) {
            self.set_caret(FlowPos::new(first + 2, 0), false, cx);
        }
        cx.notify();
    }

    /// The mouse went up with a shape armed: dragged, the shape takes the box (placed from the top left of the page); a click
    /// puts it at its usual size at the clicked place. One step of undo; the shape is selected.
    fn finish_draw_shape(&mut self, kind: DrawKind, b: DrawBox, cx: &mut Context<Self>) {
        let Some(at) = self.position_at(b.page, b.start.0.min(b.now.0), b.start.1.min(b.now.1)) else { return };
        let (w, h) = ((b.now.0 - b.start.0).abs(), (b.now.1 - b.start.1).abs());
        let emu = |v: f32| (v * 12_700.0) as i64;
        if w < 4.0 && h < 4.0 {
            self.set_caret(at, false, cx);
            match kind {
                DrawKind::Shape(preset) => self.doc_insert_shape(preset, cx),
                DrawKind::Line { arrow } => self.doc_insert_connector(None, arrow, cx),
                DrawKind::Connector { curved, arrow } => self.doc_insert_connector(Some(if curved { "curvedConnector3" } else { "bentConnector3" }), arrow, cx),
            }
            return;
        }
        let place = |insert: FlowCommand, size: ibis_doc_model::EmuSize, x: i64, y: i64| vec![insert, FlowCommand::PlaceShapeOnPage { at, size, x, y }];
        let commands = match kind {
            DrawKind::Shape(preset) => {
                let size = ibis_doc_model::EmuSize::new(emu(w), emu(h));
                let Some(insert) = Self::shape_command(preset, at, size) else { return };
                place(insert, size, emu(b.start.0.min(b.now.0)), emu(b.start.1.min(b.now.1)))
            }
            DrawKind::Line { arrow } | DrawKind::Connector { arrow, .. } => {
                let geometry = match kind {
                    DrawKind::Connector { curved, .. } => ibis_pptx::preset_geometry(if curved { "curvedConnector3" } else { "bentConnector3" }),
                    _ => None,
                };
                // The ends meet the connection sites of shapes near them (8 points), and the connector is glued to those.
                let radius = emu(8.0);
                let (mut start, mut end) = ((emu(b.start.0), emu(b.start.1)), (emu(b.now.0), emu(b.now.1)));
                let mut glue = ibis_doc_model::Glue::default();
                if let Some(doc) = self.flow() {
                    if let Some((id, idx, p)) = ibis_doc_model::connector::nearest_site(doc, start.0, start.1, radius) {
                        glue.start = Some((id, idx));
                        start = p;
                    }
                    if let Some((id, idx, p)) = ibis_doc_model::connector::nearest_site(doc, end.0, end.1, radius) {
                        glue.end = Some((id, idx));
                        end = p;
                    }
                }
                let (frame, flip_h, flip_v) = ibis_doc_model::connector::frame_between(start, end);
                let size = ibis_doc_model::EmuSize::new(frame.width, frame.height);
                let insert = Self::connector_command(geometry, arrow, at, size, flip_h, flip_v, glue);
                place(insert, size, frame.x, frame.y)
            }
        };
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(at.para, at.offset + 1), false, cx);
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = true;
            }
        }
        cx.notify();
    }

    /// The box while it is drawn, on page `index` (`k` pixels to the point).
    pub(crate) fn draw_table_overlay(&self, index: usize, k: f32) -> Option<AnyElement> {
        let b = self.doc_state()?.draw_box.filter(|b| b.page == index)?;
        let (x, y) = (b.start.0.min(b.now.0), b.start.1.min(b.now.1));
        let (w, h) = ((b.now.0 - b.start.0).abs(), (b.now.1 - b.start.1).abs());
        Some(div().absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).border_1().border_color(rgb(0x1a73e8)).bg(hsla(0.6, 0.8, 0.5, 0.08)).into_any_element())
    }
}
