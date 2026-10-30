// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > ユーザー設定のビュー…:
//! Custom Views management (`SheetCommand::AddCustomView`, `DeleteCustomView`, `ShowCustomView`).
//! Capturing and restoring: zoom, grid lines, headings, page break preview, frozen panes,
//! selection / active cell, print settings, hidden rows/columns and auto-filter.
//! Exclusion rule: disabled when workbook contains any table (XLSX-VIEW-016, XLSX-VIEW-017, XLSX-VIEW-018).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{Command, CustomSheetView, CustomWorkbookView, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CustomViewDialogMode {
    List,
    Add,
}

pub(crate) struct CustomViewsPanel {
    pub(crate) mode: CustomViewDialogMode,
    pub(crate) selected_index: Option<usize>,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) include_print_settings: bool,
    pub(crate) include_hidden: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// A new GUID for custom workbook / sheet view.
fn new_guid() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seed = (crate::clock::unix_millis() as u64) ^ COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mix = |mut x: u64| {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^ (x >> 33)
    };
    let (a, b) = (mix(seed), mix(seed ^ 0x5555_5555_5555_5555));
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xffff, a & 0xfff, 0x8000 | (b >> 48) & 0x3fff, b & 0xffff_ffff_ffff)
}

impl SlideEditor {
    pub(crate) fn toggle_custom_views_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.wb().panels.custom_views.is_some() {
            self.wb_mut().panels.custom_views = None;
            self.sidebar_open = false;
            cx.notify();
        } else {
            self.open_custom_views_panel(window, cx);
        }
    }

    pub(crate) fn open_custom_views_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let views_count = self.presentation().workbook.as_ref().map(|wb| wb.custom_views.len()).unwrap_or(0);
        let selected_index = if views_count > 0 { Some(0) } else { None };
        let message = if self.presentation().has_tables() {
            text("_xlsx.custom-views-can-t-be-used-because-the-workbook").to_string()
        } else {
            String::new()
        };
        let panel = CustomViewsPanel {
            mode: CustomViewDialogMode::List,
            selected_index,
            name_input,
            include_print_settings: true,
            include_hidden: true,
            message,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.custom_views = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn open_add_custom_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.presentation().has_tables() {
            if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                p.message = text("_xlsx.custom-views-can-t-be-used-because-the-workbook").to_string();
            }
            cx.notify();
            return;
        }
        if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
            p.mode = CustomViewDialogMode::Add;
            p.name_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.include_print_settings = true;
            p.include_hidden = true;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn apply_add_custom_view_ui(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.presentation().has_tables() {
            if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                p.message = text("_xlsx.custom-views-can-t-be-used-because-the-workbook").to_string();
            }
            cx.notify();
            return;
        }

        let (name, include_print_settings, include_hidden) = {
            let Some(p) = self.wb().panels.custom_views.as_ref() else { return };
            let name = p.name_input.read(cx).value().trim().to_string();
            (name, p.include_print_settings, p.include_hidden)
        };

        if name.is_empty() {
            if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                p.message = text("_xlsx.enter-a-name-for-the-view").to_string();
            }
            cx.notify();
            return;
        }

        if let Some(wb) = self.presentation().workbook.as_ref() {
            if wb.custom_views.iter().any(|v| v.name == name) {
                if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                    p.message = text("_xlsx.a-view-with-this-name-already-exists").to_string();
                }
                cx.notify();
                return;
            }
        }

        let guid = new_guid();
        let current_slide_id = self.current_slide;
        let active_sheet_index = self
            .presentation()
            .slides
            .iter()
            .position(|s| s.id == current_slide_id)
            .unwrap_or(0);

        let wb_view = CustomWorkbookView {
            name: name.clone(),
            guid: guid.clone(),
            include_print_settings,
            include_hidden,
            active_sheet_index,
        };

        let mut sheet_views = Vec::new();
        for slide in &self.presentation().slides {
            if let Some(sheet) = sheet_of_slide(slide) {
                let mut sv = CustomSheetView {
                    guid: guid.clone(),
                    name: name.clone(),
                    zoom: sheet.zoom,
                    grid_lines: sheet.grid_lines,
                    headings: sheet.headings,
                    show_ruler: sheet.show_ruler,
                    page_break_preview: sheet.page_break_preview,
                    frozen: sheet.frozen,
                    split: sheet.split,
                    active_cell: None,
                    selection_range: None,
                    print: if include_print_settings { Some(sheet.print.clone()) } else { None },
                    hidden_rows: if include_hidden { Some(sheet.hidden_rows.clone()) } else { None },
                    hidden_columns: if include_hidden { Some(sheet.hidden_columns.clone()) } else { None },
                    auto_filter: if include_hidden { Some(sheet.auto_filter.clone()) } else { None },
                    filtered_rows: if include_hidden { Some(sheet.filtered_rows.clone()) } else { None },
                    show_formulas: sheet.show_formulas,
                    show_zeros: sheet.show_zeros,
                };
                if slide.id == current_slide_id {
                    if let Some(pos) = self.wb().cells.get(&slide.id).copied() {
                        sv.active_cell = Some(pos);
                    }
                    if let Some(range) = self.selected_range() {
                        sv.selection_range = Some(range);
                    }
                }
                sheet_views.push((slide.id, sv));
            }
        }

        self.run_sheet(
            vec![Command::Sheet(SheetCommand::AddCustomView {
                view: wb_view,
                sheet_views,
            })],
            cx,
        );

        let views_count = self.presentation().workbook.as_ref().map(|wb| wb.custom_views.len()).unwrap_or(0);
        if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
            p.mode = CustomViewDialogMode::List;
            p.selected_index = if views_count > 0 { Some(views_count - 1) } else { None };
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn show_selected_custom_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.presentation().has_tables() {
            if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                p.message = text("_xlsx.custom-views-can-t-be-used-because-the-workbook").to_string();
            }
            cx.notify();
            return;
        }

        let view_name = {
            let Some(p) = self.wb().panels.custom_views.as_ref() else { return };
            let Some(sel_idx) = p.selected_index else { return };
            let Some(wb) = self.presentation().workbook.as_ref() else { return };
            let Some(v) = wb.custom_views.get(sel_idx) else { return };
            v.name.clone()
        };

        self.run_sheet(vec![Command::Sheet(SheetCommand::ShowCustomView { name: view_name.clone() })], cx);

        // Restore active sheet, active cell, and selection in SlideEditor state
        let target_info = self.presentation().workbook.as_ref().and_then(|wb| {
            let wb_view = wb.custom_views.iter().find(|v| v.name == view_name)?;
            let slide = self.presentation().slides.get(wb_view.active_sheet_index)?;
            let sheet = sheet_of_slide(slide)?;
            let sv = sheet.custom_views.iter().find(|v| v.guid == wb_view.guid)?;
            Some((slide.id, sv.active_cell, sv.selection_range))
        });

        if let Some((target_slide, active_cell, selection_range)) = target_info {
            self.select_slide(target_slide, window, cx);
            if let Some(pos) = active_cell {
                self.wb_mut().cells.insert(target_slide, pos);
            }
            if let Some(range) = selection_range {
                self.wb_mut().cell_ends.insert(target_slide, (range.last_column, range.last_row));
            }
        }
        cx.notify();
    }

    pub(crate) fn delete_selected_custom_view(&mut self, cx: &mut Context<Self>) {
        if self.presentation().has_tables() {
            if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
                p.message = text("_xlsx.custom-views-can-t-be-used-because-the-workbook").to_string();
            }
            cx.notify();
            return;
        }

        let view_name = {
            let Some(p) = self.wb().panels.custom_views.as_ref() else { return };
            let Some(sel_idx) = p.selected_index else { return };
            let Some(wb) = self.presentation().workbook.as_ref() else { return };
            let Some(v) = wb.custom_views.get(sel_idx) else { return };
            v.name.clone()
        };

        self.run_sheet(vec![Command::Sheet(SheetCommand::DeleteCustomView { name: view_name })], cx);

        let views_count = self.presentation().workbook.as_ref().map(|wb| wb.custom_views.len()).unwrap_or(0);
        if let Some(p) = self.wb_mut().panels.custom_views.as_mut() {
            p.selected_index = if views_count > 0 {
                p.selected_index.map(|i| i.min(views_count - 1))
            } else {
                None
            };
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn custom_views_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let mut panel_fixed_actions = crate::shell::chrome::action_bar();
        let p = self.wb().panels.custom_views.as_ref()?;
        let has_tables = self.presentation().has_tables();

        let ui = p.ui.clone();
        let mark = move |id: &str, el: AnyElement| {
            let id = id.to_string();
            let ui = ui.clone();
            div()
                .relative()
                .child(el)
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
            let id = id.to_string();
            let ui = p.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
        };

        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        if has_tables {
            body = body.child(
                div()
                    .p(px(8.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(text("_xlsx.custom-views-can-t-be-used-because-the-workbook")),
            );
        }

        match p.mode {
            CustomViewDialogMode::List => {
                let views = self.presentation().workbook.as_ref().map(|wb| wb.custom_views.clone()).unwrap_or_default();
                let sel_idx = p.selected_index;

                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.views")));

                let mut list_box = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().min_h(px(100.));
                if views.is_empty() {
                    list_box = list_box.child(div().text_sm().text_color(cx.theme().muted).child(text("_xlsx.no-views-have-been-added")));
                } else {
                    for (i, v) in views.iter().enumerate() {
                        let is_selected = Some(i) == sel_idx;
                        let item_el = div()
                            .id(SharedString::from(format!("cv-btn-{i}")))
                            .w_full()
                            .px(px(6.))
                            .py(px(4.))
                            .rounded_sm()
                            .text_sm()
                            .cursor_pointer()
                            .bg(if is_selected { cx.theme().accent.opacity(0.2) } else { hsla(0.0, 0.0, 0.0, 0.0) })
                            .hover(|s| s.bg(cx.theme().muted))
                            .child(SharedString::from(v.name.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.custom_views.as_mut() {
                                    p.selected_index = Some(i);
                                }
                                cx.notify();
                            }));
                        list_box = list_box.child(mark(
                            &format!("custom-view-item-{i}"),
                            item_el.into_any_element(),
                        ));
                    }
                }
                body = body.child(list_box);

                if let Some(v) = sel_idx.and_then(|i| views.get(i)) {
                    let mut details = v_flex().gap_1().p(px(6.)).rounded_sm().bg(cx.theme().muted.opacity(0.1));
                    if v.include_print_settings {
                        details = details.child(div().text_sm().child(text("_xlsx.include-print-settings")));
                    }
                    if v.include_hidden {
                        details = details.child(div().text_sm().child(text("_xlsx.include-hidden-rows-columns-and-filter-settings")));
                    }
                    body = body.child(details);
                }

                if !p.message.is_empty() {
                    body = body.child(div().text_sm().text_color(cx.theme().danger).child(SharedString::from(p.message.clone())));
                }

                let has_selection = sel_idx.is_some() && !views.is_empty() && !has_tables;
                let mut row1 = crate::shell::chrome::action_bar();
                row1 = row1.action(mark_action("custom-view-show", crate::shell::chrome::ActionDefinition::new("cv-show-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .disabled(!has_selection)
                        .label(text("_shell.view"))
                        .on_click(cx.listener(|this, _, window, cx| this.show_selected_custom_view(window, cx)))));
                row1 = row1.action(mark_action("custom-view-add", crate::shell::chrome::ActionDefinition::new("cv-add-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .disabled(has_tables)
                        .label(text("xlsx.solver.add"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_add_custom_view(window, cx)))));
                row1 = row1.action(mark_action("custom-view-del", crate::shell::chrome::ActionDefinition::new("cv-del-btn", "", crate::shell::chrome::ActionButtonKind::Destructive)
                        .disabled(!has_selection)
                        .label(text("_shell.delete"))
                        .on_click(cx.listener(|this, _, _, cx| this.delete_selected_custom_view(cx)))));
                panel_fixed_actions = panel_fixed_actions.actions((row1).into_definitions());
            }
            CustomViewDialogMode::Add => {
                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.add-view")));

                body = body.child(div().text_sm().child(text("_xlsx.name")));
                body = body.child(mark("custom-view-name", crate::shell::chrome::text_field(&p.name_input).full_width().into_any_element()));

                let inc_print = p.include_print_settings;
                let inc_hidden = p.include_hidden;

                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).pt(px(4.)).child(text("_xlsx.settings-to-include-in-view")));

                body = body.child(mark(
                    "custom-view-opt-print",
                    Checkbox::new("cv-opt-print")
                        .label(text("_xlsx.print-settings"))
                        .checked(inc_print)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if let Some(p) = this.wb_mut().panels.custom_views.as_mut() {
                                p.include_print_settings = *checked;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));

                body = body.child(mark(
                    "custom-view-opt-hidden",
                    Checkbox::new("cv-opt-hidden")
                        .label(text("_xlsx.hidden-rows-and-columns-and-filter-settings"))
                        .checked(inc_hidden)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if let Some(p) = this.wb_mut().panels.custom_views.as_mut() {
                                p.include_hidden = *checked;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));

                if !p.message.is_empty() {
                    body = body.child(div().text_sm().text_color(cx.theme().danger).child(SharedString::from(p.message.clone())));
                }

                let mut actions = crate::shell::chrome::action_bar();
                actions = actions.action(mark_action("custom-view-ok", crate::shell::chrome::ActionDefinition::new("cv-ok-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("OK"))
                        .on_click(cx.listener(|this, _, window, cx| this.apply_add_custom_view_ui(window, cx)))));
                actions = actions.action(mark_action("custom-view-cancel", crate::shell::chrome::ActionDefinition::new("cv-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.custom_views.as_mut() {
                                p.mode = CustomViewDialogMode::List;
                                p.message.clear();
                            }
                            cx.notify();
                        }))));
                panel_fixed_actions = panel_fixed_actions.actions((actions).into_definitions());
            }
        }

        Some(SidebarPanel::with_footer(text("_xlsx.custom-views"), body.into_any_element(), panel_fixed_actions))
    }
}
