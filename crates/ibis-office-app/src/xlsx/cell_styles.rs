// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > セルのスタイル: Excel's built-in cell styles (良い, 悪い,
//! 見出し, アクセント, 桁区切り, …; `ibis_pptx::preset_cell_styles`, from
//! ECMA-376's data with the workbook's theme) put on the selected cells as
//! their formats, in one step.
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellStyle, Command, SheetCommand};

/// A menu item's id for each style (`xlsx.cell-style.good`), made once.
fn item_id(key: &'static str) -> &'static str {
    thread_local! {
        static IDS: std::cell::RefCell<std::collections::HashMap<&'static str, &'static str>> = Default::default();
    }
    IDS.with(|ids| *ids.borrow_mut().entry(key).or_insert_with(|| Box::leak(format!("xlsx.cell-style.{key}").into_boxed_str())))
}

fn custom_item_id(name: &str) -> &'static str {
    thread_local! {
        static IDS: std::cell::RefCell<std::collections::HashMap<String, &'static str>> = Default::default();
    }
    IDS.with(|ids| *ids.borrow_mut().entry(name.to_string()).or_insert_with(|| Box::leak(format!("xlsx.custom-cell-style.{name}").into_boxed_str())))
}

pub(crate) struct NewCellStylePanel {
    pub(crate) name: Entity<InputState>,
    pub(crate) include_number_format: bool,
    pub(crate) include_alignment: bool,
    pub(crate) include_font: bool,
    pub(crate) include_border: bool,
    pub(crate) include_fill: bool,
    pub(crate) include_protection: bool,
    pub(crate) base_format: CellStyle,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// The built-in cell styles with this workbook's theme.
    pub(crate) fn cell_styles(&self) -> Option<std::rc::Rc<Vec<ibis_pptx::PresetCellStyle>>> {
        let state = self.sheet_state()?;
        let mut cached = state.panels.cell_styles.borrow_mut();
        if cached.is_none() {
            let styles = if let Some(w) = self.presentation().workbook.as_ref() {
                if !w.theme_colors.is_empty() {
                    ibis_pptx::preset_cell_styles_with_theme(&w.theme_colors, w.theme_fonts.as_ref())
                } else {
                    ibis_pptx::preset_cell_styles_of(self.source.as_deref())
                }
            } else {
                ibis_pptx::preset_cell_styles_of(self.source.as_deref())
            };
            *cached = Some(std::rc::Rc::new(styles));
        }
        cached.clone()
    }

    /// Put the style `key` on the selected cells (and the ranges selected
    /// with them).
    pub(crate) fn apply_cell_style(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::FormatCells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(style) = self.cell_styles().and_then(|s| s.iter().find(|s| s.key == key).cloned()) else { return };
        let (slide, ranges) = (self.current_slide, self.selected_ranges());
        if ranges.is_empty() {
            return;
        }
        // 標準 is the workbook's own Normal style (its font and size).
        let changes = if key == "normal" {
            use ibis_doc_model::{Edge, StyleChange as C};
            let d = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
            let mut changes = vec![C::NumberFormat("General".into()), C::Font(d.font.clone()), C::FontSize(d.font_size), C::Bold(d.bold), C::Italic(d.italic), C::UnderlineKind(d.underline), C::Strike(d.strike), C::Color(d.color), C::Fill(None)];
            changes.extend([Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].map(|e| C::Border(e, None)));
            changes
        } else {
            style.changes
        };
        let commands: Vec<Command> = changes.into_iter().map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: ranges.clone(), change })).collect();
        if let Err(err) = self.document.execute(commands) {
            self.status = Some(crate::shell::i18n::format("_xlsx.this-action-is-unavailable-err", &[("err", &err)]).into());
        }
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
        }
        cx.notify();
    }

    /// Apply a user-defined custom cell style to the selected cells.
    pub(crate) fn apply_custom_cell_style(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::FormatCells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let custom_style = self.presentation().workbook.as_ref().and_then(|wb| {
            wb.custom_cell_styles.iter().find(|s| s.name == name).cloned()
        });
        let Some(custom_style) = custom_style else { return };
        let (slide, ranges) = (self.current_slide, self.selected_ranges());
        if ranges.is_empty() {
            return;
        }
        let changes = custom_style.to_style_changes();
        let commands: Vec<Command> = changes
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: ranges.clone(), change }))
            .collect();
        if let Err(err) = self.document.execute(commands) {
            self.status = Some(crate::shell::i18n::format("_xlsx.this-action-is-unavailable-err", &[("err", &err)]).into());
        }
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
        }
        cx.notify();
    }

    /// Open the "新しいセル スタイル" panel.
    pub(crate) fn open_new_cell_style_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let base_format = self.current_cell()
            .and_then(|(c, r)| self.current_sheet().and_then(|(_, s)| s.cell(r, c).map(|x| s.styles[x.style as usize].clone())))
            .unwrap_or_else(|| self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default());

        let existing_names: Vec<String> = self.presentation().workbook.as_ref()
            .map(|wb| wb.custom_cell_styles.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        let mut num = 1;
        let mut default_name = crate::shell::i18n::format("_xlsx.style-num", &[("num", &num)]);
        while existing_names.iter().any(|n| n == &default_name) {
            num += 1;
            default_name = crate::shell::i18n::format("_xlsx.style-num", &[("num", &num)]);
        }

        let input_state = cx.new(|cx| InputState::new(window, cx));
        let dn = default_name.clone();
        input_state.update(cx, |i, cx| i.set_value(dn, window, cx));

        let panel = NewCellStylePanel {
            name: input_state,
            include_number_format: true,
            include_alignment: true,
            include_font: true,
            include_border: true,
            include_fill: true,
            include_protection: true,
            base_format,
            message: String::new(),
            ui: Default::default(),
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
        s.panels.new_cell_style = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Apply and create the custom cell style from the panel.
    pub(crate) fn apply_new_cell_style_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.new_cell_style.as_ref()) else { return };
        let name = p.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            if let Some(p) = self.wb_mut().panels.new_cell_style.as_mut() {
                p.message = text("_xlsx.enter-a-style-name").to_string();
            }
            cx.notify();
            return;
        }

        if !p.include_number_format && !p.include_alignment && !p.include_font && !p.include_border && !p.include_fill && !p.include_protection {
            if let Some(p) = self.wb_mut().panels.new_cell_style.as_mut() {
                p.message = text("_xlsx.select-at-least-one-format").to_string();
            }
            cx.notify();
            return;
        }

        let style = ibis_doc_model::CustomCellStyle {
            name,
            format: p.base_format.clone(),
            include_number_format: p.include_number_format,
            include_alignment: p.include_alignment,
            include_font: p.include_font,
            include_border: p.include_border,
            include_fill: p.include_fill,
            include_protection: p.include_protection,
        };

        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddCustomCellStyle { style })]) {
            self.status = Some(crate::shell::i18n::format("_xlsx.this-action-is-unavailable-err", &[("err", &err)]).into());
        }
        self.wb_mut().panels.new_cell_style = None;
        cx.notify();
    }

    /// Sidebar panel for creating a new cell style.
    pub(crate) fn new_cell_style_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        let p = self.sheet_state()?.panels.new_cell_style.as_ref()?;
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

        let base = &p.base_format;
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.style-name")));
        body = body.child(mark("style-name", crate::shell::chrome::text_field(&p.name).full_width().into_any_element()));

        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(red()).child(SharedString::from(p.message.clone())));
        }

        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.style-includes")));

        let check_btn = |id: &'static str, btn_id: &'static str, checked: bool, label_text: String| {
            mark(
                id,
                crate::shell::chrome::checkbox_button(btn_id, label_text, checked, false, cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.new_cell_style.as_mut() {
                            match id {
                                "include-number-format" => p.include_number_format = !p.include_number_format,
                                "include-alignment" => p.include_alignment = !p.include_alignment,
                                "include-font" => p.include_font = !p.include_font,
                                "include-border" => p.include_border = !p.include_border,
                                "include-fill" => p.include_fill = !p.include_fill,
                                "include-protection" => p.include_protection = !p.include_protection,
                                _ => {}
                            }
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };

        let num_desc = if base.number_format.is_empty() || base.number_format == "General" { crate::shell::i18n::text("_xlsx.general").to_string() } else { base.number_format.clone() };
        body = body.child(check_btn("include-number-format", "btn-num", p.include_number_format, crate::shell::i18n::format("_xlsx.number-format-num-desc", &[("num_desc", &num_desc)])));

        let mut align_desc = String::new();
        match base.align {
            ibis_doc_model::CellAlign::Left => align_desc.push_str(&crate::shell::i18n::text("docx.page-number.bottom-left")),
            ibis_doc_model::CellAlign::Center => align_desc.push_str(&crate::shell::i18n::text("_shell.center-2")),
            ibis_doc_model::CellAlign::Right => align_desc.push_str(&crate::shell::i18n::text("docx.page-number.bottom-right")),
            _ => align_desc.push_str(&crate::shell::i18n::text("_xlsx.general")),
        }
        match base.vertical {
            ibis_doc_model::CellVerticalAlign::Top => align_desc.push_str(&crate::shell::i18n::text("_xlsx.top-2-2")),
            ibis_doc_model::CellVerticalAlign::Center => align_desc.push_str(&crate::shell::i18n::text("_xlsx.middle-2")),
            _ => align_desc.push_str(&crate::shell::i18n::text("_xlsx.bottom-2-2")),
        }
        if base.wrap { align_desc.push_str(&crate::shell::i18n::text("_xlsx.wrap-text-2")); }
        body = body.child(check_btn("include-alignment", "btn-align", p.include_alignment, crate::shell::i18n::format("_xlsx.alignment-align-desc", &[("align_desc", &align_desc)])));

        let font_name = base.font.clone().unwrap_or_else(|| crate::shell::i18n::text("_xlsx.default-font").to_string());
        let mut font_desc = format!("{} {}pt", font_name, base.font_size as f64 / 100.0);
        if base.bold { font_desc.push_str(&crate::shell::i18n::text("_xlsx.bold")); }
        if base.italic { font_desc.push_str(&crate::shell::i18n::text("_xlsx.italic")); }
        body = body.child(check_btn("include-font", "btn-font", p.include_font, crate::shell::i18n::format("_xlsx.font-font-desc", &[("font_desc", &font_desc)])));

        let border_desc = if base.left.is_some() || base.right.is_some() || base.top.is_some() || base.bottom.is_some() {
            crate::shell::i18n::text("_xlsx.with-borders")
        } else {
            crate::shell::i18n::text("_shell.no-borders")
        };
        body = body.child(check_btn("include-border", "btn-border", p.include_border, crate::shell::i18n::format("_xlsx.border-border-desc", &[("border_desc", &border_desc)])));

        let fill_desc = match base.fill {
            Some(c) => format!("#{:06X}", c.0 & 0xFFFFFF),
            None => crate::shell::i18n::text("_shell.no-fill").to_string(),
        };
        body = body.child(check_btn("include-fill", "btn-fill", p.include_fill, crate::shell::i18n::format("_xlsx.fill-fill-desc", &[("fill_desc", &fill_desc)])));

        let protect_desc = if base.unlocked { crate::shell::i18n::text("_xlsx.unlocked") } else { crate::shell::i18n::text("_xlsx.locked") };
        body = body.child(check_btn("include-protection", "btn-protect", p.include_protection, crate::shell::i18n::format("_xlsx.protection-protect-desc", &[("protect_desc", &protect_desc)])));

        let btn_row = crate::shell::chrome::action_bar()
            .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("cell-style-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_new_cell_style_panel(cx)))))
            .action(mark_action("close", crate::shell::chrome::ActionDefinition::new("cell-style-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| {
                this.wb_mut().panels.new_cell_style = None;
                cx.notify();
            }))));

        let panel_terminal_actions = btn_row;
        Some(crate::shell::chrome::SidebarPanel::with_footer(text("_xlsx.new-cell-style"), body.into_any_element(), panel_terminal_actions))
    }

    /// The セルのスタイル submenu: custom styles + built-in styles by Excel's groups + "新しいセル スタイル…".
    pub(crate) fn cell_style_menu(&self) -> Option<MenuEntry> {
        let styles = self.cell_styles()?;
        let mut items: Vec<MenuEntry> = Vec::new();

        // Custom styles section if any
        if let Some(wb) = self.presentation().workbook.as_ref() {
            if !wb.custom_cell_styles.is_empty() {
                for cs in &wb.custom_cell_styles {
                    let name = cs.name.clone();
                    let key = custom_item_id(&name);
                    items.push(MenuItem::new(key, crate::shell::i18n::format("_xlsx.name-custom", &[("name", &name)])).on(move |this, _, cx| this.apply_custom_cell_style(&name, cx)).into());
                }
                items.push(MenuEntry::Separator);
            }
        }

        let mut group = "";
        for style in styles.iter() {
            if style.group != group {
                if !group.is_empty() {
                    items.push(MenuEntry::Separator);
                }
                group = style.group;
            }
            let key = style.key;
            items.push(MenuItem::new(item_id(key), style.name.clone()).on(move |this, _, cx| this.apply_cell_style(key, cx)).into());
        }

        items.push(MenuEntry::Separator);
        items.push(MenuItem::tr("xlsx.new-cell-style").on(|this, window, cx| this.open_new_cell_style_panel(window, cx)).into());

        Some(Submenu::tr("xlsx.cell-styles", "format_paint", items).into())
    }
}
