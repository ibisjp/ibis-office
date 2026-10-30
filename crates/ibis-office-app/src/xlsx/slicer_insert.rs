// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > スライサー…: slicers added for the columns of the table (or the
//! fields of the pivot table) the selected cell is in. Each one is a
//! slicer shape beside the table, with its own cache name (`Slicer_<field>`,
//! also a defined name as Excel makes it); the file gets its parts when saved
//! (`save/xlsx_slicer.rs`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::slicer::{Slicer, SlicerSource};
use ibis_doc_model::{Command, EmuRect, Shape, ShapeKind, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// What the slicers are for: a table or a pivot table, and its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Table { table: String },
    Pivot { pivot: String },
}

pub(crate) struct SlicerInsertPanel {
    pub(crate) source: Option<Source>,
    pub(crate) fields: Vec<String>,
    pub(crate) checked: Vec<bool>,
    /// Where the slicers go: the table's top right corner (sheet EMU).
    pub(crate) origin: (i64, i64),
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// A slicer's size when Excel makes one: 144 × 206 pt.
const WIDTH: i64 = 1_828_800;
const HEIGHT: i64 = 2_619_372;
const GAP: i64 = 190_500;

impl SlideEditor {
    pub(crate) fn open_slicer_insert_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let mut panel = SlicerInsertPanel { source: None, fields: Vec::new(), checked: Vec::new(), origin: (0, 0), message: String::new(), ui: Default::default() };
        if let (Some(index), Some((shape, sheet))) = (self.table_at_selection(), self.current_sheet()) {
            let t = &sheet.tables[index];
            panel.source = Some(Source::Table { table: t.name.clone() });
            panel.fields = t.columns.clone();
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            panel.origin = (shape.frame.x + xs.get(t.range.last_column as usize + 1).copied().unwrap_or(0) + GAP, shape.frame.y + ys.get(t.range.first_row as usize).copied().unwrap_or(0));
        } else if let (Some(index), Some((shape, sheet))) = (self.pivot_at_selection(), self.current_sheet()) {
            let p = &sheet.pivots[index];
            panel.source = Some(Source::Pivot { pivot: p.name.clone() });
            // The fields with items to choose from (a number field has none).
            panel.fields = p.fields.iter().filter(|f| f.items.iter().any(|it| it.kind.is_none() && it.x.is_some())).map(|f| f.name.clone()).collect();
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            panel.origin = (shape.frame.x + xs.get(p.location.last_column as usize + 1).copied().unwrap_or(0) + GAP, shape.frame.y + ys.get(p.location.first_row as usize).copied().unwrap_or(0));
        } else {
            panel.message = text("xlsx.slicer-insert.select-table").to_string();
        }
        panel.checked = vec![false; panel.fields.len()];
        let state = self.wb_mut();
        state.close_settings();
        state.panels.slicer_insert = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// OK: a slicer for each field checked.
    pub(crate) fn apply_slicer_insert_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.slicer_insert.as_ref()) else { return };
        let Some(source) = p.source.clone() else { return };
        let chosen: Vec<String> = p.fields.iter().zip(&p.checked).filter(|(_, c)| **c).map(|(f, _)| f.clone()).collect();
        if chosen.is_empty() {
            if let Some(p) = self.wb_mut().panels.slicer_insert.as_mut() {
                p.message = text("xlsx.slicer-insert.choose").to_string();
            }
            return cx.notify();
        }
        let origin = p.origin;
        let slide = self.current_slide;
        let existing_names: Vec<String> = self.presentation().slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|s| match &s.kind { ShapeKind::Slicer(x) => Some(x.name.clone()), _ => None }).collect();
        let defined: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.names.iter().map(|n| n.name.to_lowercase()).collect()).unwrap_or_default();
        let prefix = text("xlsx.slicer-insert.cache-prefix").to_string();
        let mut used_names = existing_names;
        let mut used_caches = defined;
        let mut commands = Vec::new();
        let mut index = self.slide().shapes.len();
        for (k, field) in chosen.iter().enumerate() {
            // The slicer's name: the field's, made different from the others.
            let name = (1..).map(|n| if n == 1 { field.clone() } else { format!("{field} {n}") }).find(|n| !used_names.iter().any(|u| u.eq_ignore_ascii_case(n))).unwrap_or_else(|| field.clone());
            used_names.push(name.clone());
            let base: String = format!("{prefix}{}", field.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>());
            let cache = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}{n}") }).find(|c| !used_caches.contains(&c.to_lowercase())).unwrap_or_else(|| base.clone());
            used_caches.push(cache.to_lowercase());
            let source = match &source {
                Source::Table { table } => SlicerSource::Table { table: table.clone(), field: field.clone() },
                Source::Pivot { pivot } => SlicerSource::Pivot { field: field.clone(), pivots: vec![pivot.clone()] },
            };
            let slicer = Slicer { name, caption: field.clone(), cache: cache.clone(), style: String::new(), columns: 1, row_height: 0, show_caption: true, descending: false, source, items: Vec::new() };
            let id = self.document.ids().new_shape_id();
            let frame = EmuRect::new(origin.0 + k as i64 * (WIDTH + GAP), origin.1, WIDTH, HEIGHT);
            let shape = Shape::new(id, frame, ShapeKind::Slicer(Arc::new(slicer)));
            commands.push(Command::InsertShape { slide, index, shape });
            index += 1;
            // The name Excel makes for the cache.
            commands.push(Command::Sheet(SheetCommand::SetName { name: cache, sheet: None, formula: Some("#N/A".into()) }));
        }
        if let Err(err) = self.document.execute(commands) {
            if let Some(p) = self.wb_mut().panels.slicer_insert.as_mut() {
                p.message = format("edit.refused", &[("error", &err)]);
            }
            return cx.notify();
        }
        self.wb_mut().panels.slicer_insert = None;
        self.sync_shape_formulas(cx);
        cx.notify();
    }

    pub(crate) fn slicer_insert_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.slicer_insert.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();
        if !p.fields.is_empty() {
            body = body.child(div().text_sm().child(text("xlsx.slicer-insert.choose-fields")));
        }
        for (i, field) in p.fields.iter().enumerate() {
            body = body.child(mark(
                &format!("field-{i}"),
                super::toggle::toggle_row(
                    SharedString::from(format!("slicer-field-{i}")),
                    p.checked.get(i).copied().unwrap_or(false),
                    SharedString::from(field.clone()),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.slicer_insert.as_mut()
                            && let Some(c) = p.checked.get_mut(i)
                        {
                            *c = !*c;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
        }
        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("slicer-insert-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_slicer_insert_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.slicer-insert.title"), body.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [include_str!("../../i18n/ja/xlsx.tsv"), include_str!("../../i18n/en/xlsx.tsv")] {
            for id in ["xlsx.slicer-insert", "xlsx.slicer-insert.title", "xlsx.slicer-insert.select-table", "xlsx.slicer-insert.choose", "xlsx.slicer-insert.choose-fields", "xlsx.slicer-insert.cache-prefix"] {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(id)), "{id} is missing");
            }
        }
    }
}
