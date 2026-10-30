// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 情報 (file.info): Document properties panel for Excel workbooks.
//! Provides view and edit for Core Properties (title, creator, subject,
//! keywords, description/comments, category, revision, dates) and App
//! Properties (company, manager) as well as workbook summary statistics.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// Document properties panel for Excel workbook in the sidebar.
pub(crate) struct DocPropertiesPanel {
    pub(crate) title: Entity<InputState>,
    pub(crate) creator: Entity<InputState>,
    pub(crate) subject: Entity<InputState>,
    pub(crate) keywords: Entity<InputState>,
    pub(crate) description: Entity<InputState>,
    pub(crate) category: Entity<InputState>,
    pub(crate) manager: Entity<InputState>,
    pub(crate) company: Entity<InputState>,
    pub(crate) custom_properties: Vec<ibis_doc_model::CustomProperty>,
    pub(crate) new_custom_name: Entity<InputState>,
    pub(crate) new_custom_val: Entity<InputState>,
    pub(crate) created: String,
    pub(crate) modified: String,
    pub(crate) last_printed: String,
    pub(crate) revision: String,
    pub(crate) sheet_count: usize,
    pub(crate) cell_count: usize,
    pub(crate) defined_names_count: usize,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Open the Document Properties sidebar panel for the workbook.
    pub(crate) fn open_sheet_doc_properties_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (props, sheet_count, cell_count, defined_names_count) = {
            let p = self.presentation();
            let Some(wb) = p.workbook.as_deref() else { return };
            let props = wb.properties.clone();
            let sheet_count = wb.sheets.len();
            let mut cell_count = 0;
            for s in &p.slides {
                for shape in &s.shapes {
                    if let ibis_doc_model::ShapeKind::Sheet(sheet) = &shape.kind {
                        cell_count += sheet.cells.len();
                    }
                }
            }
            let defined_names_count = wb.names.len();
            (props, sheet_count, cell_count, defined_names_count)
        };

        let title = cx.new(|cx| InputState::new(window, cx).default_value(props.title.clone().unwrap_or_default()));
        let creator = cx.new(|cx| InputState::new(window, cx).default_value(props.creator.clone().unwrap_or_default()));
        let subject = cx.new(|cx| InputState::new(window, cx).default_value(props.subject.clone().unwrap_or_default()));
        let keywords = cx.new(|cx| InputState::new(window, cx).default_value(props.keywords.clone().unwrap_or_default()));
        let description = cx.new(|cx| InputState::new(window, cx).default_value(props.description.clone().unwrap_or_default()));
        let category = cx.new(|cx| InputState::new(window, cx).default_value(props.category.clone().unwrap_or_default()));
        let manager = cx.new(|cx| InputState::new(window, cx).default_value(props.manager.clone().unwrap_or_default()));
        let company = cx.new(|cx| InputState::new(window, cx).default_value(props.company.clone().unwrap_or_default()));
        let created = props.created.clone().unwrap_or_default();
        let modified = props.modified.clone().unwrap_or_default();
        let last_printed = props.last_printed.clone().unwrap_or_default();
        let revision = props.revision.clone().unwrap_or_default();

        let new_custom_name = cx.new(|cx| InputState::new(window, cx));
        let new_custom_val = cx.new(|cx| InputState::new(window, cx));
        let custom_properties = props.custom.clone();

        let panel = DocPropertiesPanel {
            title,
            creator,
            subject,
            keywords,
            description,
            category,
            manager,
            company,
            custom_properties,
            new_custom_name,
            new_custom_val,
            created,
            modified,
            last_printed,
            revision,
            sheet_count,
            cell_count,
            defined_names_count,
            message: String::new(),
            ui: Default::default(),
        };

        if let Some(s) = self.sheet_state_mut() {
            s.panels.properties = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Document Properties sidebar panel.
    pub(crate) fn close_sheet_doc_properties_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.properties = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Add a custom property from the input fields.
    pub(crate) fn add_custom_property(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (name, val) = {
            let Some(p) = self.sheet_state().and_then(|s| s.panels.properties.as_ref()) else { return };
            (
                p.new_custom_name.read(cx).text().to_string().trim().to_string(),
                p.new_custom_val.read(cx).text().to_string().trim().to_string(),
            )
        };
        if name.is_empty() {
            return;
        }
        let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.properties.as_mut()) else { return };
        if let Some(existing) = p.custom_properties.iter_mut().find(|cp| cp.name == name) {
            existing.value = ibis_doc_model::CustomPropertyValue::Text(val);
        } else {
            p.custom_properties.push(ibis_doc_model::CustomProperty {
                name,
                value: ibis_doc_model::CustomPropertyValue::Text(val),
            });
        }
        p.new_custom_name.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        p.new_custom_val.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    /// Remove a custom property by index.
    pub(crate) fn remove_custom_property(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.properties.as_mut()) else { return };
        if index < p.custom_properties.len() {
            p.custom_properties.remove(index);
            cx.notify();
        }
    }

    /// Apply the entered document properties via `SheetCommand::SetDocProperties`.
    pub(crate) fn apply_sheet_doc_properties(&mut self, cx: &mut Context<Self>) {
        let (title, creator, subject, keywords, description, category, manager, company, custom) = {
            let Some(p) = self.sheet_state().and_then(|s| s.panels.properties.as_ref()) else { return };
            (
                p.title.read(cx).text().to_string().trim().to_string(),
                p.creator.read(cx).text().to_string().trim().to_string(),
                p.subject.read(cx).text().to_string().trim().to_string(),
                p.keywords.read(cx).text().to_string().trim().to_string(),
                p.description.read(cx).text().to_string().trim().to_string(),
                p.category.read(cx).text().to_string().trim().to_string(),
                p.manager.read(cx).text().to_string().trim().to_string(),
                p.company.read(cx).text().to_string().trim().to_string(),
                p.custom_properties.clone(),
            )
        };

        let opt = |s: String| if s.is_empty() { None } else { Some(s) };

        let mut new_props = self.presentation()
            .workbook.as_deref()
            .map(|w| w.properties.clone())
            .unwrap_or_default();

        new_props.title = opt(title);
        new_props.creator = opt(creator);
        new_props.subject = opt(subject);
        new_props.keywords = opt(keywords);
        new_props.description = opt(description);
        new_props.category = opt(category);
        new_props.manager = opt(manager);
        new_props.company = opt(company);
        new_props.custom = custom;

        let command = ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetDocProperties(new_props));
        self.execute(vec![command], cx);
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.properties.as_mut()) {
            p.message = text("docx.properties-panel.document-properties-updated").to_string();
        }
        cx.notify();
    }

    /// Render the Document Properties sidebar panel.
    pub(crate) fn sheet_doc_properties_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.properties.as_ref()?;
        let theme = cx.theme();
        let ui = p.ui.clone();

        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
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
            let ui = ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let stat_item = |label: &'static str, value: usize| {
            h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .child(div().text_sm().text_color(theme.muted_foreground).child(text(label)))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(crate::shell::ribbon::thousands(value)))
        };

        let meta_item = |label: &'static str, val: &str| {
            let val_str: SharedString = if val.is_empty() { "-".into() } else { val.to_string().into() };
            h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .child(div().text_sm().text_color(theme.muted_foreground).child(text(label)))
                .child(div().text_sm().text_color(theme.foreground).child(val_str))
        };

        let field_input = |id: &'static str, label: &'static str, entity: &Entity<InputState>| {
            v_flex()
                .gap_1()
                .py(px(3.))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.muted_foreground).child(text(label)))
                .child(mark(id, crate::shell::chrome::text_field(entity).full_width().into_any_element()))
        };

        let mut body = v_flex()
            .gap_2()
            .px(px(16.))
            .py(px(8.)).p_0();

        // 統計・メタ情報セクション
        let stats_box = v_flex()
            .gap_1()
            .p(px(8.))
            .rounded(px(4.))
            .bg(theme.title_bar_border.opacity(0.15))
            .child(stat_item("_xlsx.sheet-count", p.sheet_count))
            .child(stat_item("_xlsx.data-cell-count", p.cell_count))
            .child(stat_item("_xlsx.defined-names-count", p.defined_names_count))
            .child(meta_item("pptx.properties-panel.created", &crate::clock::format_local_datetime(&p.created)))
            .child(meta_item("pptx.properties-panel.modified", &crate::clock::format_local_datetime(&p.modified)))
            .child(meta_item("docx.properties-panel.last-printed", &crate::clock::format_local_datetime(&p.last_printed)))
            .child(meta_item("docx.properties-panel.revision", &p.revision));

        body = body.child(stats_box);

        // プロパティ入力フィールド群
        body = body
            .child(field_input("title", "_shell.title", &p.title))
            .child(field_input("creator", "pptx.properties-panel.author", &p.creator))
            .child(field_input("subject", "pptx.properties-panel.subject", &p.subject))
            .child(field_input("keywords", "docx.properties-panel.keywords", &p.keywords))
            .child(field_input("description", "_shell.comment", &p.description))
            .child(field_input("company", "docx.properties-panel.company", &p.company))
            .child(field_input("manager", "docx.properties-panel.manager", &p.manager))
            .child(field_input("category", "docx.properties-panel.category", &p.category));

        // カスタムプロパティセクション
        let mut custom_section = v_flex().gap_1().py(px(4.));
        custom_section = custom_section.child(
            div().text_sm().font_weight(FontWeight::BOLD).text_color(theme.foreground).child(text("pptx.properties-panel.custom-properties"))
        );
        for (ix, prop) in p.custom_properties.iter().enumerate() {
            let val_str = match &prop.value {
                ibis_doc_model::CustomPropertyValue::Text(s) => s.clone(),
                ibis_doc_model::CustomPropertyValue::Number(s) => s.clone(),
                ibis_doc_model::CustomPropertyValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
                ibis_doc_model::CustomPropertyValue::DateTime(s) => crate::clock::format_local_datetime(s),
            };
            let prop_name = prop.name.clone();
            let row = h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .px(px(4.))
                .rounded(px(3.))
                .bg(theme.title_bar_border.opacity(0.1))
                .child(
                    h_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(SharedString::from(prop_name)))
                        .child(div().text_sm().text_color(theme.muted_foreground).child(": "))
                        .child(div().text_sm().text_color(theme.foreground).child(SharedString::from(val_str)))
                )
                .child(
                    mark(
                        &format!("remove_custom_{ix}"),
                        crate::shell::chrome::destructive_button(("remove_custom", ix),cx)
                            .small()
                            .label(text("×"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_custom_property(ix, cx);
                            }))
                            .into_any_element(),
                    )
                );
            custom_section = custom_section.child(row);
        }

        // カスタムプロパティ追加フォーム
        let add_row = h_flex()
            .gap_1()
            .items_center()
            .pt(px(2.))
            .child(div().flex_1().child(mark("new_custom_name", crate::shell::chrome::text_field(&p.new_custom_name).full_width().into_any_element())))
            .child(div().flex_1().child(mark("new_custom_val", crate::shell::chrome::text_field(&p.new_custom_val).full_width().into_any_element())))
            .child(
                mark(
                    "add_custom",
                    crate::shell::chrome::action_button("add_custom")
                        .secondary()
                        .small()
                        .label(text("fix.common.add"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_custom_property(window, cx);
                        }))
                        .into_any_element(),
                )
            );
        custom_section = custom_section.child(add_row);
        body = body.child(custom_section);

        if !p.message.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(p.message.clone()))
            );
        }

        // アクションボタン
        let panel_actions = crate::shell::chrome::action_bar()
                .action(
                    mark_action("apply_properties", crate::shell::chrome::ActionDefinition::new("apply_properties", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(text("_shell.apply"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_sheet_doc_properties(cx);
                            })))
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.document-properties"), body.into_any_element(), panel_actions))
    }
}
