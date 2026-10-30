// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > タイムライン… / コンテキスト：ピボットテーブルの分析 > タイムラインの挿入:
//! Timelines added for the date fields of the pivot table (or table) the selected
//! cell is in. Each one is a timeline shape beside the table.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::timeline::Timeline;
use ibis_doc_model::{CellValue, Command, EmuRect, Shape, ShapeKind, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// What the timelines are for: a table or a pivot table, and its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Table { table: String },
    Pivot { pivot: String },
}

pub(crate) struct TimelineInsertPanel {
    pub(crate) source: Option<Source>,
    pub(crate) fields: Vec<String>,
    pub(crate) checked: Vec<bool>,
    /// Calculated bounds (start serial, end serial) per field, if available.
    pub(crate) bounds: Vec<(i64, i64)>,
    /// Where the timelines go: the table's top right corner (sheet EMU).
    pub(crate) origin: (i64, i64),
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// A timeline's size when Excel makes one: 264 × 108 pt.
const WIDTH: i64 = 3_352_800;
const HEIGHT: i64 = 1_371_600;
const GAP: i64 = 190_500;

impl SlideEditor {
    pub(crate) fn open_timeline_insert_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let mut panel = TimelineInsertPanel {
            source: None,
            fields: Vec::new(),
            checked: Vec::new(),
            bounds: Vec::new(),
            origin: (0, 0),
            message: String::new(),
            ui: Default::default(),
        };
        if let (Some(index), Some((shape, sheet))) = (self.pivot_at_selection(), self.current_sheet()) {
            let p = &sheet.pivots[index];
            panel.source = Some(Source::Pivot { pivot: p.name.clone() });
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            panel.origin = (
                shape.frame.x + xs.get(p.location.last_column as usize + 1).copied().unwrap_or(0) + GAP,
                shape.frame.y + ys.get(p.location.first_row as usize).copied().unwrap_or(0),
            );
            for f in &p.fields {
                if f.name.is_empty() {
                    continue;
                }
                let mut min_serial = i64::MAX;
                let mut max_serial = i64::MIN;
                for v in &f.shared {
                    if let CellValue::Number(n) = v
                        && *n >= 1.0
                        && *n <= 100_000.0
                    {
                        let s = *n as i64;
                        min_serial = min_serial.min(s);
                        max_serial = max_serial.max(s + 1);
                    }
                }
                let b = if min_serial < max_serial {
                    (min_serial, max_serial)
                } else {
                    (45_658, 46_023)
                };
                panel.fields.push(f.name.clone());
                panel.bounds.push(b);
            }
        } else if let (Some(index), Some((shape, sheet))) = (self.table_at_selection(), self.current_sheet()) {
            let t = &sheet.tables[index];
            panel.source = Some(Source::Table { table: t.name.clone() });
            panel.fields = t.columns.clone();
            panel.bounds = vec![(45_658, 46_023); panel.fields.len()];
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            panel.origin = (
                shape.frame.x + xs.get(t.range.last_column as usize + 1).copied().unwrap_or(0) + GAP,
                shape.frame.y + ys.get(t.range.first_row as usize).copied().unwrap_or(0),
            );
        } else {
            panel.message = text("xlsx.timeline-insert.select-pivot").to_string();
        }
        panel.checked = vec![false; panel.fields.len()];
        let state = self.wb_mut();
        state.close_settings();
        state.panels.timeline_insert = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// OK: a timeline for each field checked.
    pub(crate) fn apply_timeline_insert_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.timeline_insert.as_ref()) else { return };
        let Some(source) = p.source.clone() else { return };
        let chosen: Vec<(String, (i64, i64))> = p
            .fields
            .iter()
            .zip(&p.checked)
            .zip(&p.bounds)
            .filter(|((_, c), _)| **c)
            .map(|((f, _), b)| (f.clone(), *b))
            .collect();
        if chosen.is_empty() {
            if let Some(p) = self.wb_mut().panels.timeline_insert.as_mut() {
                p.message = text("xlsx.timeline-insert.choose").to_string();
            }
            return cx.notify();
        }
        let origin = p.origin;
        let slide = self.current_slide;
        let existing_names: Vec<String> = self
            .presentation()
            .slides
            .iter()
            .flat_map(|s| s.shapes.iter())
            .filter_map(|s| match &s.kind {
                ShapeKind::Timeline(x) => Some(x.name.clone()),
                _ => None,
            })
            .collect();
        let defined: Vec<String> = self
            .presentation()
            .workbook
            .as_ref()
            .map(|w| w.names.iter().map(|n| n.name.to_lowercase()).collect())
            .unwrap_or_default();
        let prefix = text("xlsx.timeline-insert.cache-prefix").to_string();
        let mut used_names = existing_names;
        let mut used_caches = defined;
        let mut commands = Vec::new();
        let mut index = self.slide().shapes.len();
        for (k, (field, bounds)) in chosen.iter().enumerate() {
            let name = (1..)
                .map(|n| if n == 1 { field.clone() } else { format!("{field} {n}") })
                .find(|n| !used_names.iter().any(|u| u.eq_ignore_ascii_case(n)))
                .unwrap_or_else(|| field.clone());
            used_names.push(name.clone());
            let base: String = format!(
                "{prefix}{}",
                field
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
                    .collect::<String>()
            );
            let cache = (1..)
                .map(|n| if n == 1 { base.clone() } else { format!("{base}{n}") })
                .find(|c| !used_caches.contains(&c.to_lowercase()))
                .unwrap_or_else(|| base.clone());
            used_caches.push(cache.to_lowercase());
            let pivots = match &source {
                Source::Pivot { pivot } => vec![pivot.clone()],
                Source::Table { .. } => Vec::new(),
            };
            let timeline = Timeline {
                name,
                caption: field.clone(),
                cache: cache.clone(),
                source: field.clone(),
                pivots,
                level: 2,
                selection_level: 2,
                scroll: bounds.0,
                bounds: *bounds,
                selection: None,
                show_header: true,
                show_selection_label: true,
                show_time_level: true,
                show_scroll_bar: true,
                style: "TimeSlicerStyleLight1".into(),
                part: String::new(),
                cache_part: String::new(),
            };
            let id = self.document.ids().new_shape_id();
            let frame = EmuRect::new(origin.0, origin.1 + k as i64 * (HEIGHT + GAP), WIDTH, HEIGHT);
            let shape = Shape::new(id, frame, ShapeKind::Timeline(Arc::new(timeline)));
            commands.push(Command::InsertShape { slide, index, shape });
            index += 1;
            commands.push(Command::Sheet(SheetCommand::SetName {
                name: cache,
                sheet: None,
                formula: Some("#N/A".into()),
            }));
        }
        if let Err(err) = self.document.execute(commands) {
            if let Some(p) = self.wb_mut().panels.timeline_insert.as_mut() {
                p.message = format("edit.refused", &[("error", &err)]);
            }
            return cx.notify();
        }
        self.wb_mut().panels.timeline_insert = None;
        self.sync_shape_formulas(cx);
        cx.notify();
    }

    pub(crate) fn timeline_insert_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.timeline_insert.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
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
            body = body.child(div().text_sm().child(text("xlsx.timeline-insert.choose-fields")));
        }
        for (i, field) in p.fields.iter().enumerate() {
            body = body.child(mark(
                &format!("field-{i}"),
                super::toggle::toggle_row(
                    SharedString::from(format!("timeline-field-{i}")),
                    p.checked.get(i).copied().unwrap_or(false),
                    SharedString::from(field.clone()),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.timeline_insert.as_mut()
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
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("timeline-insert-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_timeline_insert_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.timeline-insert.title"), body.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [
            include_str!("../../i18n/ja/xlsx.tsv"),
            include_str!("../../i18n/en/xlsx.tsv"),
        ] {
            for id in [
                "xlsx.timeline-insert",
                "xlsx.timeline-insert.title",
                "xlsx.timeline-insert.select-pivot",
                "xlsx.timeline-insert.choose",
                "xlsx.timeline-insert.choose-fields",
                "xlsx.timeline-insert.cache-prefix",
                "xlsx.pivot-insert-timeline",
            ] {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(id)), "{id} is missing");
            }
        }
    }
}
