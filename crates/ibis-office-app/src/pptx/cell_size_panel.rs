// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! セルのサイズ: the height of the rows and the width of the columns of the
//! cells selected in a table typed as numbers (cm), as PowerPoint's レイアウト >
//! セルのサイズ. The table grows or shrinks by what the rows and columns do.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The least a row or column is (as dragging a border leaves it), and the most.
const LEAST: i64 = 91_440;
const MOST: i64 = 570_240_000;

pub(crate) struct CellSizePanel {
    pub(crate) height: Entity<InputState>,
    pub(crate) width: Entity<InputState>,
    shape: ShapeId,
    /// The rows and columns of the cells selected, first to last.
    rows: (usize, usize),
    columns: (usize, usize),
    error: Option<SharedString>,
}

fn cm(emu: i64) -> String {
    let v = format!("{:.2}", emu as f64 / 360_000.);
    v.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `value` cm in EMU, from 0.25 cm to 1584 cm.
fn length(value: &str) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (emu.is_finite() && (LEAST as f64..=MOST as f64).contains(&emu)).then_some(emu as i64)
}

impl SlideEditor {
    /// The rows and columns of the table cells selected (the edited cell to the one Shift+clicked).
    fn selected_cell_span(&self) -> Option<(ShapeId, (usize, usize), (usize, usize))> {
        let e = self.text_editing()?;
        let at = e.cell?;
        let other = e.cell_anchor.unwrap_or(at);
        Some((e.shape, (at.0.min(other.0), at.0.max(other.0)), (at.1.min(other.1), at.1.max(other.1))))
    }

    pub(crate) fn can_set_cell_size(&self) -> bool {
        self.selected_cell_span().is_some_and(|(id, ..)| self.slide().shape(id).is_some_and(|s| matches!(s.kind, ShapeKind::Table(_))))
    }

    /// 行と列 > セルのサイズ…: the panel for the rows and columns of the cells selected.
    pub(crate) fn open_cell_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((shape, rows, columns)) = self.selected_cell_span() else { return };
        let Some(s) = self.slide().shape(shape).cloned() else { return };
        let ShapeKind::Table(table) = &s.kind else { return };
        let heights = table.row_heights(s.frame.height);
        let (Some(&h), Some(&w)) = (heights.get(rows.0), table.columns.get(columns.0)) else { return };
        self.end_text_edit(window, cx);
        let height = cx.new(|cx| InputState::new(window, cx).default_value(cm(h)));
        let width = cx.new(|cx| InputState::new(window, cx).default_value(cm(w)));
        let first = height.clone();
        if let Some(p) = self.pptx_mut() {
            p.timing_panel = None; p.section_panel = None; p.gradient_panel = None; p.shadow_panel = None; p.glow_panel = None; p.paragraph_panel = None; p.find = None; p.size_panel = None; p.insets_panel = None; p.crop_panel = None; p.comments = None; p.chart_panel = None; p.header_footer = None; p.frame_panel = None;
            p.cell_size_panel = Some(CellSizePanel { height, width, shape, rows, columns, error: None });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_cell_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.cell_size_panel.as_ref()) else { return };
        let (height, width) = (panel.height.read(cx).value().to_string(), panel.width.read(cx).value().to_string());
        let (id, rows, columns) = (panel.shape, panel.rows, panel.columns);
        let (Some(height), Some(width)) = (length(&height), length(&width)) else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.cell_size_panel.as_mut()) {
                p.error = Some(crate::shell::i18n::text("pptx.cell-size-panel.height-width-must-numbers-0"));
            }
            return cx.notify();
        };
        let Some(shape) = self.slide().shape(id).cloned() else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let mut table = (**table).clone();
        // The rows as drawn, those selected as typed; the table's size follows.
        let heights = table.row_heights(shape.frame.height);
        for (row, h) in table.rows.iter_mut().zip(&heights) {
            row.height = *h;
        }
        for row in table.rows.iter_mut().take(rows.1 + 1).skip(rows.0) {
            row.height = height;
        }
        for w in table.columns.iter_mut().take(columns.1 + 1).skip(columns.0) {
            *w = width;
        }
        let mut frame = shape.frame;
        frame.height = table.rows.iter().map(|r| r.height).sum();
        frame.width = table.columns.iter().sum();
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if ShapeKind::Table(Box::new(table.clone())) != shape.kind {
            commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) });
        }
        if frame != shape.frame {
            commands.push(Command::SetFrame { slide, shape: id, frame });
        }
        self.run(commands, cx);
        self.close_cell_size_panel(window, cx);
    }

    pub(crate) fn close_cell_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.cell_size_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn cell_size_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.cell_size_panel.as_ref()?;
        let (height, width, error) = (p.height.clone(), p.width.clone(), p.error.clone());
        let t = crate::shell::i18n::text;
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body.child(crate::shell::chrome::text_field_row(t("pptx.cell-size-panel.row-height-cm"), crate::shell::chrome::text_field(&height).unit("cm", px(24.)).spinner("pptx-cell-size-height", crate::shell::chrome::SpinnerSpec::new(0.1).min(0.25), cx, |_, _, _, _| {})));
        body = body.child(crate::shell::chrome::text_field_row(t("pptx.cell-size-panel.column-width-cm"), crate::shell::chrome::text_field(&width).unit("cm", px(24.)).spinner("pptx-cell-size-width", crate::shell::chrome::SpinnerSpec::new(0.1).min(0.25), cx, |_, _, _, _| {})));
        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("pptx-cell-size-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_cell_size_panel(w, cx))))
                .action(crate::shell::chrome::ActionDefinition::new("pptx-cell-size-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_cell_size_panel(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.cell-size-panel.cell-size"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{cm, length};

    #[test]
    fn sizes_are_read_in_cm() {
        assert_eq!(length("2.54"), Some(914_400));
        assert_eq!(length("0.1"), None);
        assert_eq!(length("1585"), None);
        assert_eq!(length("x"), None);
        assert_eq!(cm(360_000), "1");
    }
}
