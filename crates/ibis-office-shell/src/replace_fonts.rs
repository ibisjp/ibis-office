// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換: the panel shared by the document, spreadsheet and presentation screens. It lists the fonts the file uses and
//! replaces one of them with another in the whole file (the document's text runs, the cells' formats, the presentation's text).
//! The fonts a file substitutes or embeds (shown for a presentation) and the panel's context menu are at the end.

use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Presentation, TextBody, Command, ShapeKind};

use crate::chrome::{MenuEntry, MenuItem};
use crate::editor::SlideEditor;
use crate::shape_state::ContextPopup;
use crate::palette::rgb;
use crate::format::with_format;

pub struct ReplaceFontsState {
    pub target: Entity<InputState>,
    pub replacement: Entity<InputState>,
    pub used_fonts: Vec<String>,
    pub(crate) message: Option<SharedString>,
}

pub fn same_font(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl SlideEditor {
    /// The fonts the open file uses: where a file's fonts are is the screen's to say (`FormatEditor::used_fonts`; the text of
    /// the shapes unless the screen says otherwise).
    pub fn used_fonts(&self) -> Vec<String> {
        with_format!(self, used_fonts(self))
    }

    /// `target` replaced by `replacement` in the whole file; how many places (runs, cells, text bodies) changed.
    pub(crate) fn replace_fonts_in_file(&mut self, target: &str, replacement: &str, cx: &mut Context<Self>) -> usize {
        with_format!(self, replace_fonts(self, target, replacement, cx))
    }

    /// `target` replaced by `replacement` in the text of the shapes; how many text bodies changed.
    pub(crate) fn replace_fonts_in_shapes(&mut self, target: &str, replacement: &str, cx: &mut Context<Self>) -> usize {
        let commands = replace_presentation_fonts_commands(self.presentation(), target, replacement);
        let count = commands.len();
        if count > 0 {
            self.run(commands, cx);
        }
        count
    }

    pub fn open_replace_fonts_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None; self.shape_edit.insets_panel = None;
        self.shape_edit.crop_panel = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None; self.shape_edit.find = None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::ReplaceFonts));
            self.shape_edit.chart_panel = None;
            self.shape_edit.theme_panel = None;
        }

        let used_fonts = self.used_fonts();
        let default_target = used_fonts.first().cloned().unwrap_or_default();
        let default_replacement = if default_target == "Arial" { "Calibri".to_string() } else { "Arial".to_string() };

        let target = cx.new(|cx| InputState::new(window, cx).default_value(default_target));
        let replacement = cx.new(|cx| InputState::new(window, cx).default_value(default_replacement));
        self.replace_fonts = Some(ReplaceFontsState { target, replacement, used_fonts, message: None });
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_replace_fonts_panel(&mut self, cx: &mut Context<Self>) {
        self.replace_fonts = None;
        cx.notify();
    }

    /// 置換 of the panel.
    pub fn replace_presentation_fonts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.replace_fonts.as_ref() else { return };
        let target = state.target.read(cx).value().to_string();
        let replacement = state.replacement.read(cx).value().to_string();

        let count = self.replace_fonts_in_file(&target, &replacement, cx);

        let new_used = self.used_fonts();
        if let Some(s) = self.replace_fonts.as_mut() {
            s.used_fonts = new_used;
            s.message = Some(if count > 0 {
                crate::i18n::format("pptx.replace-fonts.replaced-fonts-n-location-s", &[("n", &count)]).into()
            } else {
                crate::i18n::text("pptx.replace-fonts.font-replace-was-not-found").into()
            });
        }
        cx.notify();
    }

    pub fn replace_fonts_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::chrome::SidebarPanel> {
        let state = self.replace_fonts.as_ref()?;
        let target = state.target.clone();
        let replacement = state.replacement.clone();
        let used_fonts = state.used_fonts.clone();
        let message = state.message.clone();
        // Substitute fonts and embedding are the presentation's (its file keeps them); the other screens only replace.
        let presentation = self.format_info().font_substitutes;

        let mut body = v_flex().id("replace-fonts-panel").gap_2().px(px(18.)).py(px(8.));
        if presentation {
            body = body.on_mouse_down(
                MouseButton::Right,
                cx.listener(|e, event: &MouseDownEvent, w, cx| {
                    e.open_replace_fonts_context_menu(None, event.position, w, cx);
                }),
            );
        }

        // Fonts the file uses that this PC lacks (a note, not a button).
        let missing: Vec<String> = used_fonts.iter().filter(|f| !crate::render::shape_view::is_presentation_font_available(f, self.presentation())).cloned().collect();
        if !missing.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0xd93025)).child(crate::i18n::text("pptx.replace-fonts.missing-fonts-title")));
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(missing.join(", ")));
        }

        // Replace from: only the fonts the file uses (as Office's 置換前). Choosing only changes the field; 置換 does the work.
        body = body.child(crate::chrome::document_font_dropdown_row(
            crate::i18n::text("pptx.replace-fonts.with"), "panel-font-replace-target", &target, &used_fonts, false, cx, |_, _, _, _| {}));

        // Replace with: the fonts the file uses, and all of them.
        body = body.child(crate::chrome::document_font_dropdown_row(
            crate::i18n::text("pptx.replace-fonts.replace"), "panel-font-replace-destination", &replacement, &used_fonts, true, cx, |_, _, _, _| {}));

        if presentation {
            let subs = self.presentation().font_substitutions.clone();
            if !subs.is_empty() {
                body = body.child(div().text_sm().text_color(rgb(0x1a73e8)).child(crate::i18n::text("pptx.replace-fonts.substitutions-title")));
                for (tgt, rep) in &subs {
                    let tgt_clone = tgt.clone();
                    let tgt_ctx = tgt.clone();
                    body = body.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                                    cx.stop_propagation();
                                    e.open_replace_fonts_context_menu(Some(tgt_ctx.clone()), event.position, w, cx);
                                }),
                            )
                            .child(div().text_sm().child(format!("{tgt} → {rep}")))
                            .child(
                                crate::chrome::destructive_button(ElementId::Name(format!("pptx-remove-sub-{tgt}").into()), cx)
                                    .small()
                                    .label("✕")
                                    .on_click(cx.listener(move |e, _, _, cx| {
                                        e.set_font_substitution(&tgt_clone, None, cx);
                                    })),
                            ),
                    );
                }
            }

            let embedded_typefaces: Vec<String> = self.presentation().fonts.iter().map(|f| f.typeface.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            if !embedded_typefaces.is_empty() {
                body = body.child(div().text_sm().text_color(rgb(0x188038)).child(crate::i18n::text("pptx.replace-fonts.embedded-fonts-title")));
                for tf in &embedded_typefaces {
                    let tf_clone = tf.clone();
                    let tf_ctx = tf.clone();
                    body = body.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                                    cx.stop_propagation();
                                    e.open_replace_fonts_context_menu(Some(tf_ctx.clone()), event.position, w, cx);
                                }),
                            )
                            .child(div().text_sm().child(tf.clone()))
                            .child(
                                crate::chrome::destructive_button(ElementId::Name(format!("pptx-remove-embed-{tf}").into()), cx)
                                    .small()
                                    .label("✕")
                                    .on_click(cx.listener(move |e, _, _, cx| {
                                        e.remove_embedded_font(&tf_clone, cx);
                                    })),
                            ),
                    );
                }
            }
        }

        if let Some(msg) = message {
            body = body.child(div().text_sm().text_color(rgb(0x1a73e8)).child(msg));
        }

        use crate::chrome::{ActionButtonKind, ActionDefinition};
        let mut panel_actions = crate::chrome::action_bar().action(
            ActionDefinition::new("replace-fonts-apply", crate::i18n::text("_shell.replace"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.replace_presentation_fonts(w, cx))),
        );
        if presentation {
            panel_actions = panel_actions
                .action(ActionDefinition::new("pptx-replace-fonts-substitute", crate::i18n::text("pptx.replace-fonts.substitute"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.set_presentation_font_substitution_action(w, cx))))
                .action(ActionDefinition::new("pptx-replace-fonts-embed", crate::i18n::text("pptx.replace-fonts.embed-button"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.embed_presentation_font_action(w, cx))));
        }
        let panel_actions = panel_actions.action(crate::chrome::dialog_cancel_definition(cx));
        Some(crate::chrome::SidebarPanel::with_footer(crate::i18n::text("_pptx.replace-fonts"), body.into_any_element(), panel_actions))
    }
}

/// プレゼンテーション全スライドで使用されているフォントファミリー名を重複なく昇順で収集する。
pub fn presentation_used_fonts(presentation: &Presentation) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    let mut collect_from_body = |body: &TextBody| {
        if let Some(font) = &body.style.font {
            let trimmed = font.trim();
            if !trimmed.is_empty() {
                fonts.insert(trimmed.to_string());
            }
        }
        if let Some(font) = &body.style.fallback_font {
            let trimmed = font.trim();
            if !trimmed.is_empty() {
                fonts.insert(trimmed.to_string());
            }
        }
        for p in &body.paragraphs {
            if let Some(font) = &p.end_style.font {
                let trimmed = font.trim();
                if !trimmed.is_empty() {
                    fonts.insert(trimmed.to_string());
                }
            }
            if let Some(font) = &p.end_style.fallback_font {
                let trimmed = font.trim();
                if !trimmed.is_empty() {
                    fonts.insert(trimmed.to_string());
                }
            }
            for r in &p.runs {
                if let Some(font) = &r.style.font {
                    let trimmed = font.trim();
                    if !trimmed.is_empty() {
                        fonts.insert(trimmed.to_string());
                    }
                }
                if let Some(font) = &r.style.fallback_font {
                    let trimmed = font.trim();
                    if !trimmed.is_empty() {
                        fonts.insert(trimmed.to_string());
                    }
                }
            }
        }
    };

    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() {
                collect_from_body(body);
            }
            if let ShapeKind::Table(table) = &shape.kind {
                for r in &table.rows {
                    for cell in &r.cells {
                        collect_from_body(&cell.text);
                    }
                }
            }
        }
    }

    fonts.into_iter().collect()
}

/// システムにインストールされておらず、プレゼンテーション内にも埋め込まれていない不足フォントを検出する。
pub(crate) fn presentation_missing_fonts(presentation: &Presentation) -> Vec<String> {
    presentation_used_fonts(presentation)
        .into_iter()
        .filter(|f| !crate::render::shape_view::is_presentation_font_available(f, presentation))
        .collect()
}

/// 単一の `TextBody` 内で対象フォントを置換し、変更があれば新しい `TextBody` を返す。
pub(crate) fn replace_fonts_in_body(body: &TextBody, target: &str, replacement: &str) -> Option<TextBody> {
    let target = target.trim();
    let replacement = replacement.trim();
    if target.is_empty() || replacement.is_empty() || target.eq_ignore_ascii_case(replacement) {
        return None;
    }

    let mut changed = false;
    let mut new_body = body.clone();
    let font_matches = |f: &str| f.trim().eq_ignore_ascii_case(target);

    if let Some(font) = &new_body.style.font {
        if font_matches(font) {
            new_body.style.font = Some(replacement.to_string());
            changed = true;
        }
    }
    if let Some(font) = &new_body.style.fallback_font {
        if font_matches(font) {
            new_body.style.fallback_font = Some(replacement.to_string());
            changed = true;
        }
    }

    for p in &mut new_body.paragraphs {
        if let Some(font) = &p.end_style.font {
            if font_matches(font) {
                p.end_style.font = Some(replacement.to_string());
                changed = true;
            }
        }
        if let Some(font) = &p.end_style.fallback_font {
            if font_matches(font) {
                p.end_style.fallback_font = Some(replacement.to_string());
                changed = true;
            }
        }
        for r in &mut p.runs {
            if let Some(font) = &r.style.font {
                if font_matches(font) {
                    r.style.font = Some(replacement.to_string());
                    changed = true;
                }
            }
            if let Some(font) = &r.style.fallback_font {
                if font_matches(font) {
                    r.style.fallback_font = Some(replacement.to_string());
                    changed = true;
                }
            }
        }
    }

    if changed {
        Some(new_body)
    } else {
        None
    }
}

/// プレゼンテーション全体で指定したフォントを置換する `Command` 一覧を生成する。
pub fn replace_presentation_fonts_commands(presentation: &Presentation, target: &str, replacement: &str) -> Vec<Command> {
    let mut commands = Vec::new();
    let target = target.trim();
    let replacement = replacement.trim();
    if target.is_empty() || replacement.is_empty() || target.eq_ignore_ascii_case(replacement) {
        return commands;
    }

    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() {
                if let Some(new_body) = replace_fonts_in_body(body, target, replacement) {
                    commands.push(Command::SetTextBody {
                        slide: slide.id,
                        shape: shape.id,
                        body: new_body,
                    });
                }
            }
            if let ShapeKind::Table(table) = &shape.kind {
                for (row, r) in table.rows.iter().enumerate() {
                    for (column, cell) in r.cells.iter().enumerate().filter(|(_, c)| !c.merged) {
                        if let Some(new_body) = replace_fonts_in_body(&cell.text, target, replacement) {
                            commands.push(Command::InCell {
                                row,
                                column,
                                command: Box::new(Command::SetTextBody {
                                    slide: slide.id,
                                    shape: shape.id,
                                    body: new_body,
                                }),
                            });
                        }
                    }
                }
            }
        }
    }

    commands
}

// ----- The fonts the file substitutes and embeds; the panel's context menu -----

pub fn create_embedded_font_data(typeface: &str) -> Vec<u8> {
    let name_bytes = typeface.as_bytes();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x00\x01\x00\x00");
    bytes.extend_from_slice(&(1u16).to_be_bytes());
    bytes.extend_from_slice(&(16u16).to_be_bytes());
    bytes.extend_from_slice(&(3u16).to_be_bytes());
    bytes.extend_from_slice(&(0u16).to_be_bytes());
    bytes.extend_from_slice(b"name");
    bytes.extend_from_slice(&(0u32).to_be_bytes());
    bytes.extend_from_slice(&(28u32).to_be_bytes());
    bytes.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
    bytes.extend_from_slice(name_bytes);
    bytes
}

impl SlideEditor {

    pub fn missing_fonts(&self) -> Vec<String> {
        presentation_missing_fonts(self.presentation())
    }

    pub fn font_substitutions(&self) -> &std::collections::BTreeMap<String, String> {
        &self.presentation().font_substitutions
    }

    pub fn set_font_substitution(&mut self, target: &str, substitute: Option<&str>, cx: &mut Context<Self>) {
        let target = target.trim();
        if target.is_empty() {
            return;
        }
        let mut subs = self.presentation().font_substitutions.clone();
        if let Some(sub) = substitute.map(str::trim).filter(|s| !s.is_empty()) {
            subs.insert(target.to_string(), sub.to_string());
        } else {
            subs.remove(target);
        }
        self.run(vec![Command::SetFontSubstitutions { substitutions: subs }], cx);
    }

    pub fn embedded_fonts(&self) -> &[ibis_doc_model::EmbeddedFont] {
        &self.presentation().fonts
    }

    pub fn embed_font(&mut self, typeface: &str, cx: &mut Context<Self>) -> bool {
        let typeface = typeface.trim();
        if typeface.is_empty() {
            return false;
        }
        if self.presentation().fonts.iter().any(|f| f.typeface.eq_ignore_ascii_case(typeface)) {
            return true;
        }
        let bytes = create_embedded_font_data(typeface);

        let mut fonts = self.presentation().fonts.clone();
        fonts.push(ibis_doc_model::EmbeddedFont {
            typeface: typeface.to_string(),
            bold: false,
            italic: false,
            data: bytes.into(),
        });
        self.run(vec![Command::SetEmbeddedFonts { fonts }], cx);
        true
    }

    pub fn remove_embedded_font(&mut self, typeface: &str, cx: &mut Context<Self>) -> bool {
        let typeface = typeface.trim();
        let before_len = self.presentation().fonts.len();
        let mut fonts = self.presentation().fonts.clone();
        fonts.retain(|f| !f.typeface.eq_ignore_ascii_case(typeface));
        if fonts.len() != before_len {
            self.run(vec![Command::SetEmbeddedFonts { fonts }], cx);
            true
        } else {
            false
        }
    }

    /// パネル上の「代替フォントに設定」ボタンから実行
    pub(crate) fn set_presentation_font_substitution_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.replace_fonts.as_ref() else { return };
        let target = state.target.read(cx).value().to_string();
        let replacement = state.replacement.read(cx).value().to_string();
        if target.trim().is_empty() || replacement.trim().is_empty() {
            return;
        }
        self.set_font_substitution(&target, Some(&replacement), cx);
        if let Some(s) = self.replace_fonts.as_mut() {
            s.message = Some(crate::i18n::format("pptx.replace-fonts.substitute-success", &[("target", &target), ("substitute", &replacement)]).into());
        }
        cx.notify();
    }

    /// パネル上の「フォントを埋め込む」ボタンから実行
    pub(crate) fn embed_presentation_font_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.replace_fonts.as_ref() else { return };
        let target = state.target.read(cx).value().to_string();
        if target.trim().is_empty() {
            return;
        }
        self.embed_font(&target, cx);
        if let Some(s) = self.replace_fonts.as_mut() {
            s.message = Some(crate::i18n::format("pptx.replace-fonts.embed-success", &[("name", &target)]).into());
        }
        cx.notify();
    }

    pub(crate) fn select_replace_font_target(&mut self, font_name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.replace_fonts.as_ref().map(|st| st.target.clone()) {
            target.update(cx, |t, cx| t.set_value(font_name, window, cx));
        }
        cx.notify();
    }

    pub fn replace_fonts_context_menu_entries(&self, font_name: Option<&str>, cx: &App) -> Vec<MenuEntry> {
        let (target_val, replacement_val) = self
            .replace_fonts
            .as_ref()
            .map(|st| (st.target.read(cx).value().trim().to_string(), st.replacement.read(cx).value().trim().to_string()))
            .unwrap_or_default();
        let can_replace = !target_val.is_empty() && !replacement_val.is_empty() && !target_val.eq_ignore_ascii_case(&replacement_val);
        let mut entries = Vec::new();
        if let Some(font) = font_name.map(str::trim).filter(|f| !f.is_empty()) {
            let f_sel = font.to_string();
            let f_rem_emb = font.to_string();
            let f_rem_sub = font.to_string();
            let is_embedded = self.presentation().fonts.iter().any(|ef| ef.typeface.eq_ignore_ascii_case(font));
            let has_sub = self.presentation().font_substitutions.contains_key(font);
            entries.push(
                MenuItem::tr("pptx.replace-fonts.with")
                    .icon("font_download")
                    .checked(target_val.eq_ignore_ascii_case(font))
                    .on(move |e, w, cx| e.select_replace_font_target(&f_sel, w, cx))
                    .into(),
            );
            if is_embedded || has_sub {
                entries.push(
                    MenuItem::tr("_shell.delete")
                        .icon("delete")
                        .on(move |e, _, cx| {
                            if is_embedded {
                                e.remove_embedded_font(&f_rem_emb, cx);
                            }
                            if has_sub {
                                e.set_font_substitution(&f_rem_sub, None, cx);
                            }
                        })
                        .into(),
                );
            }
            entries.push(MenuEntry::Separator);
        }
        entries.push(
            MenuItem::tr("_shell.replace")
                .icon("find_replace")
                .disabled(!can_replace)
                .on(|e, w, cx| e.replace_presentation_fonts(w, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.replace-fonts.substitute")
                .icon("swap_horiz")
                .disabled(!can_replace)
                .on(|e, w, cx| e.set_presentation_font_substitution_action(w, cx))
                .into(),
        );
        if let Some(font) = font_name.map(str::trim).filter(|f| !f.is_empty()) {
            let f_emb = font.to_string();
            let is_embedded = self.presentation().fonts.iter().any(|ef| ef.typeface.eq_ignore_ascii_case(font));
            entries.push(
                MenuItem::tr("pptx.replace-fonts.embed-button")
                    .icon("font_download")
                    .disabled(is_embedded)
                    .on(move |e, _, cx| {
                        e.embed_font(&f_emb, cx);
                    })
                    .into(),
            );
        } else {
            entries.push(
                MenuItem::tr("pptx.replace-fonts.embed-button")
                    .icon("font_download")
                    .disabled(target_val.is_empty())
                    .on(|e, w, cx| e.embed_presentation_font_action(w, cx))
                    .into(),
            );
        }
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_replace_fonts_panel(cx))
                .into(),
        );
        entries
    }

    pub(crate) fn open_replace_fonts_context_menu(&mut self, font_name: Option<String>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.replace_fonts_context_menu_entries(font_name.as_deref(), cx));
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.shape_edit.popup = None;
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.shape_edit.popup = Some(ContextPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }
}
