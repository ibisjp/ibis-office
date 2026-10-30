// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフ テンプレート (.crtx) の保存・読込・適用・管理 (XLSX-CHEDIT-021).
//!
//! Office Chart Template (`.crtx`) 形式によるグラフ書式・種別の保存と適用。
//! 既存グラフの系列データ（値・項目・参照）を維持したまま、テンプレートの
//! グラフ種別、グループ化、配色・スタイル、軸設定、凡例等を一括適用する。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChartTemplateMode {
    Save,
    Apply,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChartTemplateInfo {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

pub(crate) struct ChartTemplatePanel {
    #[allow(dead_code)]
    pub(crate) chart_shape: ShapeId,
    pub(crate) mode: ChartTemplateMode,
    pub(crate) template_name: Entity<InputState>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) templates: Vec<ChartTemplateInfo>,
    pub(crate) error: Option<SharedString>,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// Returns the default directory where user chart templates are stored.
pub(crate) fn default_chart_templates_dir() -> PathBuf {
    let base = crate::shell::app_data::base();
    base.unwrap_or_else(|| PathBuf::from(".")).join("ibisOffice").join("ChartTemplates")
}

/// Lists all saved `.crtx` chart templates from the templates directory.
pub(crate) fn list_saved_chart_templates() -> Vec<ChartTemplateInfo> {
    let dir = default_chart_templates_dir();
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("crtx")) {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Template").to_string();
                list.push(ChartTemplateInfo { name, path });
            }
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

impl SlideEditor {
    /// Opens the "Chart Template" (グラフ テンプレート) sidebar panel for the selected chart.
    pub(crate) fn open_chart_template_panel(&mut self, mode: ChartTemplateMode, window: &mut Window, cx: &mut Context<Self>) {
        let chart_shape = if let Some((id, _)) = self.sheet_chart() {
            id
        } else if let Some(id) = self.slide().shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Chart(_) => Some(s.id),
            _ => None,
        }) {
            self.selection = Some(id);
            id
        } else {
            self.status = Some(text("_xlsx.select-a-chart").into());
            cx.notify();
            return;
        };

        self.commit_cell_edit(cx);
        self.wb_mut().close_settings();

        let default_name = "CustomChart1".to_string();
        let template_name = cx.new(|cx| InputState::new(window, cx).default_value(default_name));
        let templates = list_saved_chart_templates();
        let selected_index = if templates.is_empty() { None } else { Some(0) };

        self.wb_mut().panels.chart_template = Some(ChartTemplatePanel {
            chart_shape,
            mode,
            template_name,
            selected_index,
            templates,
            error: None,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });

        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_chart_template_mode(&mut self, mode: ChartTemplateMode, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
            p.mode = mode;
            p.error = None;
            p.templates = list_saved_chart_templates();
            if p.selected_index.is_none() && !p.templates.is_empty() {
                p.selected_index = Some(0);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_selected_chart_template(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
            p.selected_index = Some(index);
            p.error = None;
        }
        cx.notify();
    }

    /// Saves the selected chart as a `.crtx` template file in the user's template directory.
    pub(crate) fn save_selected_chart_template(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.chart_template.as_ref() else { return };
        let name = p.template_name.read(cx).text().to_string().trim().to_string();

        if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(text("fix.chart-template.invalid-template-name").into());
            }
            cx.notify();
            return;
        }

        let Some((_, chart)) = self.sheet_chart() else {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(text("_xlsx.select-a-chart").into());
            }
            cx.notify();
            return;
        };

        let crtx_bytes = match ibis_pptx::chart_to_crtx(chart) {
            Ok(bytes) => bytes,
            Err(e) => {
                if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                    p.error = Some(e.to_string().into());
                }
                cx.notify();
                return;
            }
        };

        let dir = default_chart_templates_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(crate::shell::i18n::format("_xlsx.could-not-create-the-folder-e", &[("e", &e)]).into());
            }
            cx.notify();
            return;
        }

        let target_path = dir.join(format!("{name}.crtx"));
        if let Err(e) = std::fs::write(&target_path, &crtx_bytes) {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(crate::shell::i18n::format("_xlsx.could-not-save-the-template-e", &[("e", &e)]).into());
            }
            cx.notify();
            return;
        }

        self.status = Some(crate::shell::i18n::format("_xlsx.template-name-was-saved", &[("name", &name)]).into());
        // Refresh templates and switch to Apply mode or close
        if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
            p.templates = list_saved_chart_templates();
            p.mode = ChartTemplateMode::Apply;
            p.selected_index = p.templates.iter().position(|t| t.name == name).or(Some(0));
            p.error = None;
        }
        cx.notify();
    }

    /// Applies the currently selected saved template onto the active sheet's chart.
    pub(crate) fn apply_selected_chart_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.chart_template.as_ref() else { return };
        let Some(idx) = p.selected_index else {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(text("fix.chart-template.please-select-template-apply").into());
            }
            cx.notify();
            return;
        };
        let Some(tmpl_info) = p.templates.get(idx).cloned() else { return };

        let data = match std::fs::read(&tmpl_info.path) {
            Ok(d) => d,
            Err(e) => {
                if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                    p.error = Some(crate::shell::i18n::format("_xlsx.could-not-load-the-template-e", &[("e", &e)]).into());
                }
                cx.notify();
                return;
            }
        };

        if let Err(err) = self.apply_chart_template_bytes(&data, window, cx) {
            if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
                p.error = Some(err.into());
            }
            cx.notify();
            return;
        }

        self.status = Some(crate::shell::i18n::format("_xlsx.applied-template-name", &[("name", &tmpl_info.name)]).into());
        self.wb_mut().panels.chart_template = None;
        self.sidebar_open = true;
        cx.notify();
    }

    /// Deletes the currently selected template file.
    pub(crate) fn delete_selected_chart_template(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.chart_template.as_ref() else { return };
        let Some(idx) = p.selected_index else { return };
        let Some(tmpl_info) = p.templates.get(idx).cloned() else { return };

        let _ = std::fs::remove_file(&tmpl_info.path);
        if let Some(p) = self.wb_mut().panels.chart_template.as_mut() {
            p.templates = list_saved_chart_templates();
            p.selected_index = if p.templates.is_empty() {
                None
            } else {
                Some(idx.min(p.templates.len() - 1))
            };
            p.error = None;
        }
        self.status = Some(crate::shell::i18n::format("_xlsx.deleted-template-name", &[("name", &tmpl_info.name)]).into());
        cx.notify();
    }

    /// Reads `.crtx` bytes, parses the chart, and applies it to the active chart with Undo/Redo.
    pub(crate) fn apply_chart_template_bytes(&mut self, bytes: &[u8], window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let template = ibis_pptx::read_crtx(bytes).map_err(|e| e.to_string())?;
        if self.sheet_chart().is_none() {
            return Err(crate::shell::i18n::text("_xlsx.no-chart-is-selected").to_string());
        }

        self.change_sheet_chart(|target| {
            ibis_pptx::apply_chart_template(target, &template);
        }, cx);

        // Re-open chart panel to reflect updated properties if open
        if self.sheet_chart().is_some() {
            self.open_sheet_chart_panel(window, cx);
        }

        Ok(())
    }

    /// Prompts user to pick a `.crtx` file and applies it to the selected chart.
    pub(crate) fn prompt_apply_chart_template_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("fix.chart-template.select-chart-template").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(bytes) = std::fs::read(&path) else { return };
                this.update_in(cx, |this, window, cx| {
                    if let Err(err) = this.apply_chart_template_bytes(&bytes, window, cx) {
                        this.status = Some(err.into());
                    } else {
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Template");
                        this.status = Some(crate::shell::i18n::format("_xlsx.applied-template-name", &[("name", &name)]).into());
                        this.wb_mut().panels.chart_template = None;
                        this.sidebar_open = true;
                    }
                    cx.notify();
                }).ok();
            })
            .detach();
        }
    }

    /// Renders the sidebar panel for グラフ テンプレート.
    pub(crate) fn chart_template_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let mut panel_fixed_actions = crate::shell::chrome::action_bar();
        let p = self.sheet_state()?.panels.chart_template.as_ref()?;
        let mode = p.mode;
        let selected_idx = p.selected_index;
        let templates_count = p.templates.len();
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

        let mode_tabs=crate::shell::chrome::tab_control_with_mode([text("fix.chart-template.apply-template"),text("fix.chart-template.save-template")],usize::from(mode==ChartTemplateMode::Save),"chart-template-tabs",crate::shell::chrome::TabMode::Intrinsic,|e,index,cx|e.set_chart_template_mode(if index==0{ChartTemplateMode::Apply}else{ChartTemplateMode::Save},cx),cx);

        let mut content = v_flex().gap_3().px(px(16.)).py(px(8.)).p_0().child(mode_tabs);

        match mode {
            ChartTemplateMode::Save => {
                let name_input = crate::shell::chrome::text_field(&p.template_name).full_width();
                content = content
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(text("fix.chart-template.template-name")))
                    .child(mark("chart-template-name-input", name_input.into_any_element()));

                let save_btn = crate::shell::chrome::ActionDefinition::new("chart-template-save-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(text("_shell.save"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_selected_chart_template(cx);
                    }));

                let cancel_btn = crate::shell::chrome::ActionDefinition::new("chart-template-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("_shell.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.chart_template = None;
                        this.sidebar_open = true;
                        cx.notify();
                    }));

                panel_fixed_actions = panel_fixed_actions.actions((crate::shell::chrome::action_bar()
                        .action(mark_action("chart-template-save", save_btn))
                        .action(mark_action("chart-template-cancel", cancel_btn))).into_definitions());
            }
            ChartTemplateMode::Apply => {
                content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(text("fix.chart-template.saved-templates")));

                if p.templates.is_empty() {
                    content = content.child(div().text_sm().text_color(cx.theme().muted).child(text("fix.chart-template.no-saved-templates-found")));
                } else {
                    let mut list = v_flex().gap_1();
                    for (i, tmpl) in p.templates.iter().enumerate() {
                        let is_sel = selected_idx == Some(i);
                        let item_btn = crate::shell::chrome::radio_button(("chart-template-item", i),is_sel,false,tmpl.name.clone(),cx.listener(move |this, _, _, cx| {
                                this.set_selected_chart_template(i, cx);
                            }));
                        list = list.child(mark(&format!("chart-template-item-{i}"), item_btn.into_any_element()));
                    }
                    content = content.child(list);
                }

                let apply_btn = crate::shell::chrome::ActionDefinition::new("chart-template-apply-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(text("_shell.apply"))
                    .disabled(selected_idx.is_none() || templates_count == 0)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_selected_chart_template(window, cx);
                    }));

                let delete_btn = crate::shell::chrome::ActionDefinition::new("chart-template-delete-btn", "", crate::shell::chrome::ActionButtonKind::Destructive)
                    .label(text("_shell.delete"))
                    .disabled(selected_idx.is_none() || templates_count == 0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.delete_selected_chart_template(cx);
                    }));

                let from_file_btn = crate::shell::chrome::ActionDefinition::new("chart-template-from-file-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("fix.chart-template.apply-file"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prompt_apply_chart_template_file(window, cx);
                    }));

                let cancel_btn = crate::shell::chrome::ActionDefinition::new("chart-template-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("_shell.cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.chart_template = None;
                        this.sidebar_open = true;
                        cx.notify();
                    }));

                panel_fixed_actions = panel_fixed_actions.actions((crate::shell::chrome::action_bar()
                            .action(mark_action("chart-template-apply", apply_btn))
                            .action(mark_action("chart-template-delete", delete_btn))
                            .action(mark_action("chart-template-cancel", cancel_btn))).into_definitions()).actions((crate::shell::chrome::action_bar()
                            .action(mark_action("chart-template-from-file", from_file_btn))).into_definitions());
            }
        }

        if let Some(err) = &p.error {
            content = content.child(
                div()
                    .p(px(6.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(err.clone()),
            );
        }

        Some(SidebarPanel::with_footer(text("fix.chart-template.chart-template"), content.into_any_element(), panel_fixed_actions))
    }
}
