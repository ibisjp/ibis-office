// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換 (書式 > フォントの置換…): replace a font across the entire
//! presentation (all slides, text boxes, shapes and table cells) with another font.

use ibis_doc_model::{Command, Presentation, ShapeKind, TextBody};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;
use crate::pptx::SlidePopup;
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;

/// プレゼンテーション全スライドで使用されているフォントファミリー名を重複なく昇順で収集する。
pub(crate) fn presentation_used_fonts(presentation: &Presentation) -> Vec<String> {
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
pub(crate) fn replace_presentation_fonts_commands(presentation: &Presentation, target: &str, replacement: &str) -> Vec<Command> {
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

pub(crate) fn create_embedded_font_data(typeface: &str) -> Vec<u8> {
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
    /// プレゼンテーション内のフォントを一括置換する（直接呼び出し用）
    pub(crate) fn replace_fonts_in_presentation(&mut self, target: &str, replacement: &str, cx: &mut Context<Self>) -> usize {
        let commands = replace_presentation_fonts_commands(self.presentation(), target, replacement);
        let count = commands.len();
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        count
    }

    pub(crate) fn missing_fonts(&self) -> Vec<String> {
        presentation_missing_fonts(self.presentation())
    }

    pub(crate) fn font_substitutions(&self) -> &std::collections::BTreeMap<String, String> {
        &self.presentation().font_substitutions
    }

    pub(crate) fn set_font_substitution(&mut self, target: &str, substitute: Option<&str>, cx: &mut Context<Self>) {
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

    pub(crate) fn embedded_fonts(&self) -> &[ibis_doc_model::EmbeddedFont] {
        &self.presentation().fonts
    }

    pub(crate) fn embed_font(&mut self, typeface: &str, cx: &mut Context<Self>) -> bool {
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

    pub(crate) fn remove_embedded_font(&mut self, typeface: &str, cx: &mut Context<Self>) -> bool {
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
            s.message = Some(crate::shell::i18n::format("pptx.replace-fonts.substitute-success", &[("target", &target), ("substitute", &replacement)]).into());
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
            s.message = Some(crate::shell::i18n::format("pptx.replace-fonts.embed-success", &[("name", &target)]).into());
        }
        cx.notify();
    }

    pub(crate) fn select_replace_font_target(&mut self, font_name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.replace_fonts.as_ref().map(|st| st.target.clone()) {
            target.update(cx, |t, cx| t.set_value(font_name, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn replace_fonts_context_menu_entries(&self, font_name: Option<&str>, cx: &App) -> Vec<MenuEntry> {
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
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }
}
