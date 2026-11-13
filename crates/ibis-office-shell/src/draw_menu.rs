// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The commands of the Draw button's menu (menu bar redesign, stage 8; `tasks/doc/menu-redesign/implementation-spec.md`
//! section 3). The button is in the title row of the three screens and its menu is one definition (`menu_layout::DRAW_MENU`);
//! what a screen cannot do is grey with its reason at the right of the row (`MenuItem::hint`): the pens and the converting work
//! on the sheet only so far (the screen says what they do, `FormatEditor::draw_command`), the drawing canvas is for the
//! document screen, and what needs ink to be selected says so.
//! The commands are in the command table, so the menu search finds them (「描画 ▸ ペン」).

use crate::chrome::{MenuEntry, MenuItem};
use crate::commands::CommandRegistry;
use crate::editor::SlideEditor;
use crate::i18n;
use crate::format::with_format;
use crate::ink::PenKind;

/// A command of the Draw menu that a screen does in its own way (`FormatEditor::draw_command`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawCommand {
    /// Choose a pen (the sheet: its Ink panel opens with the pen chosen).
    Pen(PenKind),
    /// Convert the ink selected to a shape.
    ToShape,
}

impl SlideEditor {
    /// The Draw menu's commands, as menu entries (the layout puts them in order).
    pub(crate) fn draw_menu_commands(&self, reg: &CommandRegistry) -> Vec<MenuEntry> {
        let kind = self.kind();
        let state = self.context_state();
        let not_yet = |item: MenuItem| item.disabled(true).hint(i18n::text("draw.reason.not-yet"));
        // A pen: what choosing it does is the screen's (the sheet opens its Ink panel with the pen chosen). The other screens
        // choose their pen in the panel.
        let pen = |id: &'static str, icon: &'static str, which: PenKind| -> MenuEntry {
            let mut item = MenuItem::tr(id).icon(icon);
            match with_format!(self, draw_command(self, DrawCommand::Pen(which))) {
                Some(handler) => {
                    item.handler = Some(handler);
                    item.into()
                }
                None => not_yet(item).into(),
            }
        };
        let panel = crate::sidebars::of(self.kind()).iter().find(|d| d.id == "sidebar.ink").map(|d| d.menu_item(self, &state, reg));
        let mut out: Vec<MenuEntry> = vec![
            pen("draw.select", "arrow_selector_tool", PenKind::Select),
            not_yet(MenuItem::tr("draw.lasso").icon("gesture")).into(),
            pen("draw.pen.black", "edit", PenKind::Pen),
            pen("draw.pen.pencil", "edit", PenKind::Pencil),
            pen("draw.pen.highlighter", "ink_highlighter", PenKind::Highlighter),
            pen("draw.eraser", "ink_eraser", PenKind::Eraser),
            not_yet(MenuItem::tr("draw.ruler").icon("straighten")).into(),
            not_yet(MenuItem::tr("draw.to-math").icon("function")).into(),
            not_yet(MenuItem::tr("draw.replay").icon("replay")).into(),
        ];
        // The first pen's id is also the group's name; the group is in the layout (`draw.pen`).
        // Converting ink to a shape (the sheet): only with ink selected.
        let mut to_shape = MenuItem::tr("draw.to-shape").icon("shapes");
        out.push(match with_format!(self, draw_command(self, DrawCommand::ToShape)) {
            Some(handler) if state.ink_selected => {
                to_shape.handler = Some(handler);
                to_shape.into()
            }
            Some(_) => to_shape.disabled(true).hint(i18n::text("draw.reason.ink")).into(),
            None => not_yet(to_shape).into(),
        });
        // The drawing canvas is the document screen's.
        let canvas = MenuItem::tr("draw.canvas").icon("draw");
        out.push(if crate::format::info(kind).drawing_canvas { not_yet(canvas).into() } else { canvas.disabled(true).hint(i18n::text("draw.reason.document-only")).into() });
        // Hide the ink (the sheet has it as 表示 ▸ インクを非表示; the command is in the sheet's menus only).
        let hide = MenuItem::tr("draw.hide-ink").icon("visibility_off");
        out.push(match reg.entry("xlsx.hide-ink") {
            Some(MenuEntry::Item(base)) => {
                let mut item = hide;
                item.checked = base.checked;
                item.disabled = base.disabled;
                item.handler = base.handler.clone();
                item.into()
            }
            _ => not_yet(hide).into(),
        });
        // The pen group's add row opens the Ink panel, where the pens are kept.
        let mut add = MenuItem::tr("draw.pen.add").icon("add");
        // The panel itself.
        let mut open_panel = MenuItem::tr("draw.ink-panel").icon("draw");
        if let Some(panel) = panel {
            open_panel.handler = panel.handler.clone();
            add.handler = panel.handler;
        }
        out.push(add.into());
        out.push(open_panel.into());
        out
    }
}
