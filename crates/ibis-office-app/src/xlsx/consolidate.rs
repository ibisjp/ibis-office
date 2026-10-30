// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 統合…: consolidate data from several ranges/sheets
//! into destination cells (`SheetCommand::Consolidate`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, ConsolidateFunction, DataConsolidate, SheetCommand, SlideId};

use super::dedupe_edit::column_name;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct ConsolidatePanel {
    pub(crate) function: ConsolidateFunction,
    pub(crate) reference_input: Entity<InputState>,
    pub(crate) references: Vec<String>,
    pub(crate) selected_ref: Option<usize>,
    pub(crate) top_labels: bool,
    pub(crate) left_labels: bool,
    pub(crate) link: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    pub(crate) slide: SlideId,
}

impl SlideEditor {
    pub(crate) fn current_sheet_qualified_range_string(&self) -> Option<String> {
        let r = self.selected_range()?;
        let sname = self
            .presentation()
            .workbook
            .as_ref()
            .and_then(|w| w.sheets.iter().find(|s| s.slide == Some(self.current_slide)).map(|s| s.name.clone()))
            .unwrap_or_else(|| "Sheet1".to_string());
        let qname = ibis_calc::formula::quote_sheet(&sname);
        let c1 = column_name(r.first_column);
        let c2 = column_name(r.last_column);
        Some(format!("{}!${}${}:${}${}", qname, c1, r.first_row + 1, c2, r.last_row + 1))
    }

    pub(crate) fn normalize_consolidate_ref(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        if let Some((slide_id, r)) = super::sheet_ui::parse_reference(trimmed).and_then(|ref_| self.resolve_reference(&ref_)) {
            let sname = self
                .presentation()
                .workbook
                .as_ref()
                .and_then(|w| w.sheets.iter().find(|s| s.slide == Some(slide_id)).map(|s| s.name.clone()))
                .unwrap_or_else(|| "Sheet1".to_string());
            let qname = ibis_calc::formula::quote_sheet(&sname);
            let c1 = column_name(r.first_column);
            let c2 = column_name(r.last_column);
            return format!("{}!${}${}:${}${}", qname, c1, r.first_row + 1, c2, r.last_row + 1);
        }
        trimmed.to_string()
    }

    pub(crate) fn add_consolidate_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ref_str) = self.current_sheet_qualified_range_string() else { return };
        if let Some(p) = self.wb_mut().panels.consolidate.as_mut() {
            p.reference_input.update(cx, |i, cx| i.set_value(ref_str.clone(), window, cx));
            if !p.references.contains(&ref_str) {
                p.references.push(ref_str);
            }
        }
        cx.notify();
    }

    pub(crate) fn clear_all_consolidate_references(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.consolidate.as_mut() {
            p.references.clear();
            p.selected_ref = None;
        }
        cx.notify();
    }

    pub(crate) fn open_consolidate_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let existing = self.current_sheet().and_then(|(_, s)| s.data_consolidate.clone());
        let (func, top, left, link, refs) = match existing {
            Some(dc) => (dc.function, dc.top_labels, dc.left_labels, dc.link, dc.data_refs),
            None => (ConsolidateFunction::Sum, false, false, false, Vec::new()),
        };

        let initial_ref = self.current_sheet_qualified_range_string().unwrap_or_default();

        let ref_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(initial_ref, window, cx);
            s
        });

        let panel = ConsolidatePanel {
            function: func,
            reference_input: ref_input,
            references: refs,
            selected_ref: None,
            top_labels: top,
            left_labels: left,
            link,
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
        state.panels.consolidate = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn consolidate_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.consolidate.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    pub(crate) fn apply_consolidate_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.consolidate.as_ref()) else { return };
        if p.slide != self.current_slide {
            return self.consolidate_message(text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
        }

        let mut refs: Vec<String> = p.references.iter().map(|r| self.normalize_consolidate_ref(r)).collect();
        let input_val = self.normalize_consolidate_ref(&p.reference_input.read(cx).value());
        if !input_val.is_empty() && !refs.contains(&input_val) {
            refs.push(input_val);
        }

        if refs.is_empty() {
            return self.consolidate_message(text("_xlsx.please-add-a-source-range").into(), cx);
        }

        let dest = self
            .selected_range()
            .map(|r| (r.first_row, r.first_column))
            .or_else(|| self.current_cell().map(|(c, r)| (r, c)))
            .unwrap_or((0, 0));

        let consolidate = DataConsolidate {
            function: p.function,
            left_labels: p.left_labels,
            top_labels: p.top_labels,
            link: p.link,
            data_refs: refs,
        };

        let slide = self.current_slide;
        match self.document.execute(vec![Command::Sheet(SheetCommand::Consolidate {
            slide,
            destination: dest,
            consolidate,
        })]) {
            Ok(_) => {
                self.consolidate_message(text("_xlsx.consolidation-complete").into(), cx);
            }
            Err(e) => {
                self.consolidate_message(crate::shell::i18n::format("_xlsx.error-e", &[("e", &format!("{e:?}"))]), cx);
            }
        }
    }

    pub(crate) fn consolidate_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.consolidate.as_ref()?;
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

        // 1. 集計方法 (Function)
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.function")));
        let choices = ConsolidateFunction::ALL.iter().map(|&f| {
            crate::shell::chrome::MenuItem::new("consolidate-function",text(f.label())).checked(p.function==f).on(move|this,_,cx|{
                if let Some(p)=this.wb_mut().panels.consolidate.as_mut(){p.function=f;}
                cx.notify();
            }).into()
        }).collect();
        body=body.child(mark("function",div().w(crate::shell::chrome::FIELD_WIDTH).child(crate::shell::chrome::Dropdown::from_entries("consolidate-fn",text(p.function.label()),choices,true,0,&cx.weak_entity()).into_any_element()).into_any_element()));

        // 2. 参照範囲 (Reference input)
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.references")));
        body = body.child(mark("reference-input", super::range_field::local_range_field(&p.reference_input,"consolidate-p-reference_input",false,cx).into_any_element()));

        // Add / Add Selection / Delete / Clear All buttons
        let sel_ref = p.selected_ref;
        let has_refs = !p.references.is_empty();
        let mut btn_row = crate::shell::chrome::action_bar();
        btn_row = btn_row.action(mark_action("add-ref", crate::shell::chrome::ActionDefinition::new("consolidate-add", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("fix.common.add"))
                .on_click(cx.listener(|this, _, _, cx| {
                    let raw = this
                        .sheet_state()
                        .and_then(|s| s.panels.consolidate.as_ref())
                        .map(|p| p.reference_input.read(cx).value().to_string())
                        .unwrap_or_default();
                    let val = this.normalize_consolidate_ref(&raw);
                    if let Some(p) = this.wb_mut().panels.consolidate.as_mut()
                        && !val.is_empty()
                        && !p.references.contains(&val)
                    {
                        p.references.push(val);
                    }
                    cx.notify();
                }))));
        btn_row = btn_row.action(mark_action("add-selection", crate::shell::chrome::ActionDefinition::new("consolidate-add-selection", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(text("xlsx.consolidate-add-selection"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.add_consolidate_selection(window, cx);
                }))));
        btn_row = btn_row.action(mark_action("del-ref", crate::shell::chrome::ActionDefinition::new("consolidate-del", "", crate::shell::chrome::ActionButtonKind::Destructive)
                .disabled(sel_ref.is_none())
                .label(text("_shell.delete"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.consolidate.as_mut() {
                        if let Some(idx) = p.selected_ref {
                            if idx < p.references.len() {
                                p.references.remove(idx);
                                p.selected_ref = None;
                            }
                        }
                    }
                    cx.notify();
                }))));
        btn_row = btn_row.action(mark_action("clear-all-refs", crate::shell::chrome::ActionDefinition::new("consolidate-clear-all", "", crate::shell::chrome::ActionButtonKind::Destructive)
                .disabled(!has_refs)
                .label(text("xlsx.consolidate-clear-all"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_all_consolidate_references(cx);
                }))));
        body = body.child(btn_row);

        // 3. 統合元範囲 (All references list)
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.all-references")));
        let mut list_box = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().min_h(px(80.));
        if p.references.is_empty() {
            list_box = list_box.child(div().text_sm().text_color(cx.theme().muted).child(text("_xlsx.no-ranges-have-been-added")));
        } else {
            for (i, r_str) in p.references.iter().enumerate() {
                let is_selected = Some(i) == sel_ref;
                let r_str_clone = r_str.clone();
                let item_el = div()
                    .px(px(6.))
                    .py(px(3.))
                    .rounded_sm()
                    .text_sm()
                    .bg(if is_selected { cx.theme().accent.opacity(0.2) } else { hsla(0.0, 0.0, 0.0, 0.0) })
                    .child(SharedString::from(r_str.clone()));
                list_box = list_box.child(mark(
                    &format!("ref-item-{i}"),
                    Button::new(format!("ref-btn-{i}"))
                        .small()
                        .ghost()
                        .child(item_el)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(p) = this.wb_mut().panels.consolidate.as_mut() {
                                p.selected_ref = Some(i);
                                p.reference_input.update(cx, |inp, cx| inp.set_value(r_str_clone.clone(), window, cx));
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
        }
        body = body.child(list_box);

        // 4. 統合の基準 (Use labels in)
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.use-labels-in")));
        let top_labels = p.top_labels;
        body = body.child(mark(
            "top-labels",
            crate::shell::chrome::checkbox_button("consolidate-top", crate::shell::i18n::format("_xlsx.mark-top-row", &[("mark", &"")]).trim().to_string(), top_labels, false, cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.consolidate.as_mut() {
                        p.top_labels = !p.top_labels;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));
        let left_labels = p.left_labels;
        body = body.child(mark(
            "left-labels",
            crate::shell::chrome::checkbox_button("consolidate-left", crate::shell::i18n::format("_xlsx.mark-left-column", &[("mark", &"")]).trim().to_string(), left_labels, false, cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.consolidate.as_mut() {
                        p.left_labels = !p.left_labels;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));

        // 5. 連動 (Link)
        let link = p.link;
        body = body.child(mark(
            "link",
            crate::shell::chrome::checkbox_button("consolidate-link", crate::shell::i18n::format("_xlsx.mark-create-links-to-source-data", &[("mark", &"")]).trim().to_string(), link, false, cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.consolidate.as_mut() {
                        p.link = !p.link;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));

        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }

        // 6. Action buttons (OK / Close)

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("consolidate-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_consolidate_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.consolidate"), body.into_any_element(), panel_terminal_actions))
    }
}
