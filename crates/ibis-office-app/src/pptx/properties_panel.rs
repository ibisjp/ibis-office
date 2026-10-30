// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 情報 (file.info): Document properties panel for PowerPoint presentations.
//! Provides view and edit for Core Properties (title, creator, subject,
//! keywords, description/comments, category, revision, dates) and App
//! Properties (company, manager) as well as presentation summary statistics.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::pptx::MediaQuality;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// Document properties panel for PowerPoint presentation in the sidebar.
pub(crate) struct PresentationDocPropertiesPanel {
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
    pub(crate) embed_fonts: bool,
    pub(crate) created: String,
    pub(crate) modified: String,
    pub(crate) last_printed: String,
    pub(crate) revision: String,
    pub(crate) slide_count: usize,
    pub(crate) shape_count: usize,
    pub(crate) notes_count: usize,
    pub(crate) section_count: usize,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Open the Document Properties sidebar panel for the presentation.
    pub(crate) fn open_presentation_properties_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (props, slide_count, shape_count, notes_count, section_count) = {
            let p = self.presentation();
            let props = p.properties.clone();
            let slide_count = p.slides.len();
            let shape_count = p.slides.iter().map(|s| s.shapes.len()).sum();
            let notes_count = p.slides.iter().filter(|s| !s.notes.is_empty()).count();
            let section_count = p.sections.len();
            (props, slide_count, shape_count, notes_count, section_count)
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
        let embed_fonts = !self.presentation().fonts.is_empty();

        let panel = PresentationDocPropertiesPanel {
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
            embed_fonts,
            created,
            modified,
            last_printed,
            revision,
            slide_count,
            shape_count,
            notes_count,
            section_count,
            message: String::new(),
            ui: Default::default(),
        };

        if let Some(s) = self.pptx_mut() {
            s.properties_panel = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Document Properties sidebar panel.
    pub(crate) fn close_presentation_properties_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.properties_panel = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Add a custom property from the input fields.
    pub(crate) fn add_presentation_custom_property(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (name, val) = {
            let Some(p) = self.pptx().and_then(|s| s.properties_panel.as_ref()) else { return };
            (
                p.new_custom_name.read(cx).text().to_string().trim().to_string(),
                p.new_custom_val.read(cx).text().to_string().trim().to_string(),
            )
        };
        if name.is_empty() {
            return;
        }
        let Some(p) = self.pptx_mut().and_then(|s| s.properties_panel.as_mut()) else { return };
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
    pub(crate) fn remove_presentation_custom_property(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.pptx_mut().and_then(|s| s.properties_panel.as_mut()) else { return };
        if index < p.custom_properties.len() {
            p.custom_properties.remove(index);
            cx.notify();
        }
    }

    /// Embed or clear embedded fonts for the entire presentation.
    pub(crate) fn set_embed_presentation_fonts(&mut self, embed: bool, cx: &mut Context<Self>) {
        if embed {
            let mut used = super::replace_fonts::presentation_used_fonts(self.presentation());
            if used.is_empty() {
                if let Some(tf) = &self.presentation().theme_fonts {
                    let mut add_font_set = |fs: &ibis_doc_model::FontSet| {
                        for opt in [&fs.latin, &fs.east_asian, &fs.complex] {
                            if let Some(f) = opt {
                                let trimmed = f.trim();
                                if !trimmed.is_empty() && !used.iter().any(|s| s.eq_ignore_ascii_case(trimmed)) {
                                    used.push(trimmed.to_string());
                                }
                            }
                        }
                    };
                    add_font_set(&tf.major);
                    add_font_set(&tf.minor);
                }
                if used.is_empty() {
                    used.push("Calibri".to_string());
                }
            }
            let mut fonts = self.presentation().fonts.clone();
            for typeface in used {
                if !fonts.iter().any(|f| f.typeface.eq_ignore_ascii_case(&typeface)) {
                    let bytes = super::replace_fonts::create_embedded_font_data(&typeface);
                    fonts.push(ibis_doc_model::EmbeddedFont {
                        typeface,
                        bold: false,
                        italic: false,
                        data: bytes.into(),
                    });
                }
            }
            if fonts != self.presentation().fonts {
                self.run(vec![ibis_doc_model::Command::SetEmbeddedFonts { fonts }], cx);
            }
        } else if !self.presentation().fonts.is_empty() {
            self.run(vec![ibis_doc_model::Command::SetEmbeddedFonts { fonts: Vec::new() }], cx);
        }
    }

    /// Toggle font embedding in the Document Properties panel.
    pub(crate) fn toggle_presentation_doc_properties_embed_fonts(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.properties_panel.as_mut()) {
            p.embed_fonts = enabled;
        }
        self.set_embed_presentation_fonts(enabled, cx);
        cx.notify();
    }

    /// Apply the entered document properties via `Command::SetDocProperties`.
    pub(crate) fn apply_presentation_doc_properties(&mut self, cx: &mut Context<Self>) {
        let (title, creator, subject, keywords, description, category, manager, company, custom, embed_fonts) = {
            let Some(p) = self.pptx().and_then(|s| s.properties_panel.as_ref()) else { return };
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
                p.embed_fonts,
            )
        };

        let opt = |s: String| if s.is_empty() { None } else { Some(s) };

        let mut new_props = self.presentation().properties.clone();
        new_props.title = opt(title);
        new_props.creator = opt(creator);
        new_props.subject = opt(subject);
        new_props.keywords = opt(keywords);
        new_props.description = opt(description);
        new_props.category = opt(category);
        new_props.manager = opt(manager);
        new_props.company = opt(company);
        new_props.custom = custom;

        let command = ibis_doc_model::Command::SetDocProperties { properties: new_props };
        self.run(vec![command], cx);

        let current_embedded = !self.presentation().fonts.is_empty();
        if embed_fonts != current_embedded {
            self.set_embed_presentation_fonts(embed_fonts, cx);
        }

        if let Some(p) = self.pptx_mut().and_then(|s| s.properties_panel.as_mut()) {
            p.message = text("docx.properties-panel.document-properties-updated").to_string();
        }
        cx.notify();
    }

    pub(crate) fn clear_presentation_personal_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.properties_panel.as_mut()) {
            p.creator.update(cx, |input, cx| input.set_value("", window, cx));
            p.manager.update(cx, |input, cx| input.set_value("", window, cx));
            p.company.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.apply_presentation_doc_properties(cx);
    }

    pub(crate) fn copy_presentation_custom_property(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.properties_panel.as_ref()) else { return };
        let Some(prop) = p.custom_properties.get(index) else { return };
        let val_str = match &prop.value {
            ibis_doc_model::CustomPropertyValue::Text(s) => s.clone(),
            ibis_doc_model::CustomPropertyValue::Number(s) => s.clone(),
            ibis_doc_model::CustomPropertyValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            ibis_doc_model::CustomPropertyValue::DateTime(s) => s.clone(),
        };
        let copied = format!("{}: {}", prop.name, val_str);
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
    }

    pub(crate) fn properties_panel_context_menu_entries(&self, custom_idx: Option<usize>) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some(p) = self.pptx().and_then(|s| s.properties_panel.as_ref()) else {
            return Vec::new();
        };
        let embed_fonts = p.embed_fonts;
        let has_custom = custom_idx.is_some_and(|i| i < p.custom_properties.len());

        let mut entries = vec![
            MenuItem::tr("_shell.apply")
                .icon("check")
                .on(|this, _, cx| this.apply_presentation_doc_properties(cx))
                .into(),
        ];
        if let Some(idx) = custom_idx.filter(|_| has_custom) {
            entries.push(
                MenuItem::tr("edit.copy")
                    .icon("content_copy")
                    .on(move |this, _, cx| this.copy_presentation_custom_property(idx, cx))
                    .into(),
            );
            entries.push(
                MenuItem::tr("edit.delete")
                    .icon("delete")
                    .on(move |this, _, cx| this.remove_presentation_custom_property(idx, cx))
                    .into(),
            );
        }
        entries.push(
            MenuItem::tr("fix.common.add")
                .icon("add")
                .on(|this, w, cx| this.add_presentation_custom_property(w, cx))
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("pptx.properties-panel.embed-fonts")
                .checked(embed_fonts)
                .on(move |this, _, cx| this.toggle_presentation_doc_properties_embed_fonts(!embed_fonts, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.inspect.remove-properties")
                .icon("delete_sweep")
                .on(|this, w, cx| this.clear_presentation_personal_properties(w, cx))
                .into(),
        );
        entries.push(
            Submenu::tr(
                "pptx.media.compress",
                "compress",
                vec![
                    MenuItem::tr("pptx.media.compress-1080p")
                        .on(|this, _, cx| {
                            this.compress_presentation_media(MediaQuality::FullHd, cx);
                        })
                        .into(),
                    MenuItem::tr("pptx.media.compress-720p")
                        .on(|this, _, cx| {
                            this.compress_presentation_media(MediaQuality::Hd, cx);
                        })
                        .into(),
                    MenuItem::tr("pptx.media.compress-480p")
                        .on(|this, _, cx| {
                            this.compress_presentation_media(MediaQuality::Standard, cx);
                        })
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::tr("pptx.media.optimize-compatibility")
                        .on(|this, _, cx| {
                            this.optimize_media_compatibility(cx);
                        })
                        .into(),
                ],
            )
            .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("pptx.inspect-document")
                .on(|this, w, cx| this.open_presentation_inspect_panel(w, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.check-accessibility")
                .on(|this, w, cx| this.open_pptx_accessibility_panel(w, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.compatibility-check")
                .on(|this, w, cx| {
                    this.close_presentation_properties_panel(cx);
                    this.open_pptx_compatibility_panel(w, cx);
                })
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, _, cx| this.close_presentation_properties_panel(cx))
                .into(),
        );
        entries
    }

    pub(crate) fn open_properties_panel_context_menu(
        &mut self,
        custom_idx: Option<usize>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_text_edit(window, cx);
        let items = self.properties_panel_context_menu_entries(custom_idx);
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.pptx_mut() {
                s.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.slide_popup = Some(super::SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    /// Render the Document Properties sidebar panel.
    pub(crate) fn presentation_properties_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.properties_panel.as_ref()?;
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
            .id("pptx-properties-panel")
            .gap_2()
            .px(px(16.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_properties_panel_context_menu(None, event.position, window, cx);
            }));

        // 統計・メタ情報セクション
        let stats_box = v_flex()
            .gap_1()
            .p(px(8.))
            .rounded(px(4.))
            .bg(theme.title_bar_border.opacity(0.15))
            .child(stat_item("pptx.properties-panel.slides", p.slide_count))
            .child(stat_item("pptx.properties-panel.shapes", p.shape_count))
            .child(stat_item("pptx.properties-panel.notes", p.notes_count))
            .child(stat_item("pptx.properties-panel.sections", p.section_count))
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
                ibis_doc_model::CustomPropertyValue::DateTime(s) => s.clone(),
            };
            let prop_name = prop.name.clone();
            let row = h_flex()
                .id(("pptx-custom-prop-row", ix))
                .justify_between()
                .items_center()
                .py(px(2.))
                .px(px(4.))
                .rounded(px(3.))
                .bg(theme.title_bar_border.opacity(0.1))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_properties_panel_context_menu(Some(ix), event.position, window, cx);
                }))
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
                                this.remove_presentation_custom_property(ix, cx);
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
                            this.add_presentation_custom_property(window, cx);
                        }))
                        .into_any_element(),
                )
            );
        custom_section = custom_section.child(add_row);
        body = body.child(custom_section);

        // フォントの埋め込み (PPTX-FILE / DocProperties)
        let font_embed_section = v_flex()
            .gap_1()
            .py(px(4.))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.foreground)
                    .child(text("pptx.properties-panel.fonts-section"))
            )
            .child(
                mark(
                    "embed_fonts",
                    Checkbox::new("pptx-doc-props-embed-fonts")
                        .label(text("pptx.properties-panel.embed-fonts"))
                        .checked(p.embed_fonts)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.toggle_presentation_doc_properties_embed_fonts(*on, cx);
                        }))
                        .into_any_element(),
                )
            );
        body = body.child(font_embed_section);

        // メディアの圧縮と互換性 (PPTX-VIDEO-017)
        let (media_count, media_bytes) = self.presentation_media_summary();
        if media_count > 0 {
            let media_mb = media_bytes as f64 / 1_048_576.0;
            let summary_text = format!("{} ({:.1} MB)", format("pptx.media.media-count", &[("count", &media_count)]), media_mb);
            let media_section = v_flex()
                .gap_1()
                .py(px(4.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.foreground)
                        .child(text("pptx.media.section-title")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(summary_text)),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(
                            crate::shell::chrome::action_button("compress-1080p")
                                .small()

                                .label(text("pptx.media.compress-1080p"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.compress_presentation_media(MediaQuality::FullHd, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::action_button("compress-720p")
                                .small()

                                .label(text("pptx.media.compress-720p"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.compress_presentation_media(MediaQuality::Hd, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::action_button("compress-480p")
                                .small()

                                .label(text("pptx.media.compress-480p"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.compress_presentation_media(MediaQuality::Standard, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::action_button("optimize-compatibility")
                                .small()

                                .label(text("pptx.media.optimize-compatibility"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.optimize_media_compatibility(cx);
                                })),
                        ),
                );
            body = body.child(media_section);
        }

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
                    mark_action("apply_properties", crate::shell::chrome::ActionDefinition::new("apply_properties", text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_presentation_doc_properties(cx);
                            })))
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("pptx.properties-panel.presentation-properties"), body.into_any_element(), panel_actions))
    }
}
