// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 詳細設定… (フィルターオプションの設定): filter rows of a list
//! range according to criteria in a criteria range, either in-place or copied
//! to an extract range, optionally skipping duplicate records (`SheetCommand::AdvancedFilter`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SlideId};

use super::dedupe_edit::parse_range;
use super::sheet_ui::parse_reference;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct AdvancedFilterPanel {
    pub(crate) copy_to_other: bool,
    pub(crate) list_range: Entity<InputState>,
    pub(crate) criteria_range: Entity<InputState>,
    pub(crate) copy_to_range: Entity<InputState>,
    pub(crate) unique: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    pub(crate) slide: SlideId,
}

fn format_cell_range_a1(r: CellRange) -> String {
    if r.first_row == r.last_row && r.first_column == r.last_column {
        ibis_doc_model::cell_name(r.first_column, r.first_row)
    } else {
        format!(
            "{}:{}",
            ibis_doc_model::cell_name(r.first_column, r.first_row),
            ibis_doc_model::cell_name(r.last_column, r.last_row)
        )
    }
}

impl SlideEditor {
    pub(crate) fn resolve_advanced_filter_range(&self, raw: &str) -> Option<CellRange> {
        let t = raw.trim().strip_prefix('=').unwrap_or(raw.trim()).trim();
        if t.is_empty() {
            return None;
        }
        if let Some(ref_obj) = parse_reference(t)
            && let Some((slide, range)) = self.resolve_reference(&ref_obj)
            && slide == self.current_slide
        {
            return Some(range);
        }
        parse_range(t)
    }

    pub(crate) fn set_advanced_filter_range_from_selection(&mut self, field: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        let val = format_cell_range_a1(r);
        let input = self.wb().panels.advanced_filter.as_ref().and_then(|p| match field {
            "list" => Some(p.list_range.clone()),
            "criteria" => Some(p.criteria_range.clone()),
            "copy_to" => Some(p.copy_to_range.clone()),
            _ => None,
        });
        if let Some(input) = input {
            input.update(cx, |i, cx| i.set_value(val, window, cx));
            cx.notify();
        }
    }

    pub(crate) fn open_advanced_filter_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let list = self
            .sort_selection()
            .map(|(r, _, _)| r)
            .or_else(|| self.selected_range())
            .unwrap_or(CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 });
        let list_str = format_cell_range_a1(list);
        let criteria_str = self.resolve_advanced_filter_range("Criteria").map(format_cell_range_a1).unwrap_or_default();
        let copy_to_str = self.resolve_advanced_filter_range("Extract").map(format_cell_range_a1).unwrap_or_default();

        let input = |val: &str, window: &mut Window, cx: &mut Context<Self>| {
            let s = cx.new(|cx| InputState::new(window, cx));
            s.update(cx, |i, cx| i.set_value(val.to_string(), window, cx));
            s
        };

        let panel = AdvancedFilterPanel {
            copy_to_other: false,
            list_range: input(&list_str, window, cx),
            criteria_range: input(&criteria_str, window, cx),
            copy_to_range: input(&copy_to_str, window, cx),
            unique: false,
            message: String::new(),
            ui: Default::default(),
            slide: self.current_slide,
        };

        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.advanced_filter = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn advanced_filter_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.advanced_filter.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    pub(crate) fn apply_advanced_filter_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.advanced_filter.as_ref()) else { return };
        if p.slide != self.current_slide {
            return self.advanced_filter_message(crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
        }

        let list_str = p.list_range.read(cx).value().to_string();
        let criteria_str = p.criteria_range.read(cx).value().to_string();
        let copy_to_str = p.copy_to_range.read(cx).value().to_string();
        let copy_to_other = p.copy_to_other;
        let unique = p.unique;

        let Some(list_range) = self.resolve_advanced_filter_range(&list_str) else {
            return self.advanced_filter_message(text("_xlsx.enter-the-list-range-correctly-for-example-a1-c1").into(), cx);
        };
        let Some(criteria_range) = self.resolve_advanced_filter_range(&criteria_str) else {
            return self.advanced_filter_message(text("_xlsx.enter-a-valid-criteria-range-for-example-e1-f2").into(), cx);
        };
        let copy_to = if copy_to_other {
            match self.resolve_advanced_filter_range(&copy_to_str) {
                Some(r) => Some(r),
                None => return self.advanced_filter_message(text("_xlsx.enter-a-valid-extract-range-for-example-h1").into(), cx),
            }
        } else {
            None
        };

        let slide = self.current_slide;
        match self.document.execute(vec![Command::Sheet(SheetCommand::AdvancedFilter {
            slide,
            list_range,
            criteria_range,
            copy_to,
            unique,
        })]) {
            Ok(_) => {
                self.advanced_filter_message(text("_xlsx.the-filter-has-been-applied").into(), cx);
            }
            Err(e) => {
                self.advanced_filter_message(crate::shell::i18n::format("_xlsx.error-error", &[("error", &format!("{e:?}"))]), cx);
            }
        }
    }

    pub(crate) fn advanced_filter_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.advanced_filter.as_ref()?;
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

        let copy_to_other = p.copy_to_other;
        let unique = p.unique;

        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        // Action selector (Radio buttons: in-place vs copy-to)
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.copy-to")));
        body = body.child(mark(
            "action-inplace",
            crate::shell::chrome::radio_button("adv-action-inplace", !copy_to_other, false,
                crate::shell::i18n::format("_xlsx.mark-filter-in-place", &[("mark", &"")]).trim().to_string(), cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.advanced_filter.as_mut() {
                        p.copy_to_other = false;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));
        body = body.child(mark(
            "action-copy",
            crate::shell::chrome::radio_button("adv-action-copy", copy_to_other, false,
                crate::shell::i18n::format("_xlsx.mark-copy-to-another-location", &[("mark", &"")]).trim().to_string(), cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.advanced_filter.as_mut() {
                        p.copy_to_other = true;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));

        // Inputs
        body = body.child(
            h_flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child(text("_xlsx.list-range"))),
        );
        body = body.child(mark("list-input", super::range_field::local_range_field(&p.list_range,"advanced_filter-p-list_range",false,cx).into_any_element()));

        body = body.child(
            h_flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child(text("_xlsx.criteria-range"))),
        );
        body = body.child(mark("criteria-input", super::range_field::local_range_field(&p.criteria_range,"advanced_filter-p-criteria_range",false,cx).into_any_element()));

        if copy_to_other {
            body = body.child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(text("_xlsx.copy-to"))),
            );
            body = body.child(mark("copy-to-input", super::range_field::local_range_field(&p.copy_to_range,"advanced_filter-p-copy_to_range",false,cx).into_any_element()));
        }

        // Unique records checkbox
        body = body.child(mark(
            "unique",
            crate::shell::chrome::checkbox_button("adv-unique", crate::shell::i18n::format("_xlsx.mark-unique-records-only", &[("mark", &"")]).trim().to_string(), unique, false, cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.advanced_filter.as_mut() {
                        p.unique = !p.unique;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));

        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("adv-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_advanced_filter_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.advanced"), body.into_any_element(), panel_terminal_actions))
    }
}
