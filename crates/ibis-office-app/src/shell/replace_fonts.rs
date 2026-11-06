// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換: the panel shared by the document, spreadsheet and presentation screens. It lists the fonts the file uses and
//! replaces one of them with another in the whole file (the document's text runs, the cells' formats, the presentation's text).
//! The presentation's own extras (substitute font, embedding) stay in `pptx/replace_fonts.rs`.

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, FlowCommand, FlowPos, FlowRunKind, RunChange, ShapeKind, SheetCommand, StyleChange};

use crate::pptx::replace_fonts::{presentation_used_fonts, replace_presentation_fonts_commands};
use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;

pub(crate) struct ReplaceFontsState {
    pub(crate) target: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    pub(crate) used_fonts: Vec<String>,
    pub(crate) message: Option<SharedString>,
}

fn same_font(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The typefaces the document's text runs are drawn with (Latin and East Asian), in order, without repeats.
pub(crate) fn document_used_fonts(doc: &ibis_doc_model::FlowDocument) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    for p in ibis_doc_model::all_paragraphs(doc) {
        for r in &p.runs {
            if !matches!(r.kind, FlowRunKind::Text(_)) {
                continue;
            }
            for f in [&r.style.font, &r.style.east_asian_font].into_iter().flatten() {
                let f = f.trim();
                if !f.is_empty() {
                    fonts.insert(f.to_string());
                }
            }
        }
    }
    fonts.into_iter().collect()
}

/// The commands that set `replacement` on the text runs of `doc` drawn in `target`, and how many runs they change.
pub(crate) fn document_replace_commands(doc: &ibis_doc_model::FlowDocument, target: &str, replacement: &str) -> (Vec<FlowCommand>, usize) {
    let mut commands = Vec::new();
    let mut count = 0;
    if target.trim().is_empty() || replacement.trim().is_empty() || same_font(target, replacement) {
        return (commands, count);
    }
    for p in ibis_doc_model::all_paragraphs(doc) {
        let mut offset = 0;
        for r in &p.runs {
            let len = r.len();
            if matches!(r.kind, FlowRunKind::Text(_)) && len > 0 {
                let (from, to) = (FlowPos::new(p.id, offset), FlowPos::new(p.id, offset + len));
                let mut hit = false;
                if r.style.font.as_deref().is_some_and(|f| same_font(f, target)) {
                    commands.push(FlowCommand::SetRunFormat { from, to, change: RunChange::Font(replacement.trim().to_string()) });
                    hit = true;
                }
                if r.style.east_asian_font.as_deref().is_some_and(|f| same_font(f, target)) {
                    commands.push(FlowCommand::SetRunFormat { from, to, change: RunChange::EastAsianFont(replacement.trim().to_string()) });
                    hit = true;
                }
                count += usize::from(hit);
            }
            offset += len;
        }
    }
    (commands, count)
}

/// The typefaces the cells' formats of the workbook use (a cell on the workbook's default font names none).
pub(crate) fn workbook_used_fonts(presentation: &ibis_doc_model::Presentation) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let ShapeKind::Sheet(sheet) = &shape.kind {
                for cell in sheet.cells.iter() {
                    if let Some(f) = sheet.styles.get(cell.style as usize).and_then(|s| s.font.as_deref()) {
                        let f = f.trim();
                        if !f.is_empty() {
                            fonts.insert(f.to_string());
                        }
                    }
                }
            }
        }
    }
    fonts.into_iter().collect()
}

/// The commands that set `replacement` as the font of the cells formatted in `target` (adjacent cells of a row in one range),
/// and how many cells they change.
pub(crate) fn workbook_replace_commands(presentation: &ibis_doc_model::Presentation, target: &str, replacement: &str) -> (Vec<Command>, usize) {
    let mut commands = Vec::new();
    let mut count = 0;
    if target.trim().is_empty() || replacement.trim().is_empty() || same_font(target, replacement) {
        return (commands, count);
    }
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            let ShapeKind::Sheet(sheet) = &shape.kind else { continue };
            let mut ranges: Vec<CellRange> = Vec::new();
            for cell in sheet.cells.iter() {
                if !sheet.styles.get(cell.style as usize).and_then(|s| s.font.as_deref()).is_some_and(|f| same_font(f, target)) {
                    continue;
                }
                count += 1;
                match ranges.last_mut() {
                    Some(last) if last.first_row == cell.row && last.last_row == cell.row && last.last_column + 1 == cell.column => last.last_column = cell.column,
                    _ => ranges.push(CellRange { first_row: cell.row, first_column: cell.column, last_row: cell.row, last_column: cell.column }),
                }
            }
            if !ranges.is_empty() {
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide: slide.id, ranges, change: StyleChange::Font(Some(replacement.trim().to_string())) }));
            }
        }
    }
    (commands, count)
}

impl SlideEditor {
    /// The fonts the open file uses.
    pub(crate) fn used_fonts(&self) -> Vec<String> {
        if self.is_document() {
            self.flow().map(|d| document_used_fonts(d)).unwrap_or_default()
        } else if self.is_workbook() {
            workbook_used_fonts(self.presentation())
        } else {
            presentation_used_fonts(self.presentation())
        }
    }

    /// `target` replaced by `replacement` in the whole file; how many places (runs, cells, text bodies) changed.
    pub(crate) fn replace_fonts_in_file(&mut self, target: &str, replacement: &str, cx: &mut Context<Self>) -> usize {
        if self.is_document() {
            let Some((commands, count)) = self.flow().map(|d| document_replace_commands(d, target, replacement)) else { return 0 };
            if count > 0 {
                self.run_flow(commands, cx);
            }
            count
        } else if self.is_workbook() {
            let (commands, count) = workbook_replace_commands(self.presentation(), target, replacement);
            if count > 0 {
                self.run(commands, cx);
            }
            count
        } else {
            let commands = replace_presentation_fonts_commands(self.presentation(), target, replacement);
            let count = commands.len();
            if count > 0 {
                self.run(commands, cx);
            }
            count
        }
    }

    pub(crate) fn open_replace_fonts_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        if let Some(state) = self.pptx_mut() {
            state.section_panel = None;
            state.gradient_panel = None;
            state.shadow_panel = None;
            state.glow_panel = None;
            state.paragraph_panel = None;
            state.timing_panel = None;
            state.crop_panel = None;
            state.size_panel = None;
            state.frame_panel = None;
            state.comments = None;
            state.chart_panel = None;
            state.header_footer = None;
            state.insets_panel = None;
            state.find = None;
            state.theme_panel = None;
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
    pub(crate) fn replace_presentation_fonts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.replace_fonts.as_ref() else { return };
        let target = state.target.read(cx).value().to_string();
        let replacement = state.replacement.read(cx).value().to_string();

        let count = self.replace_fonts_in_file(&target, &replacement, cx);

        let new_used = self.used_fonts();
        if let Some(s) = self.replace_fonts.as_mut() {
            s.used_fonts = new_used;
            s.message = Some(if count > 0 {
                crate::shell::i18n::format("pptx.replace-fonts.replaced-fonts-n-location-s", &[("n", &count)]).into()
            } else {
                crate::shell::i18n::text("pptx.replace-fonts.font-replace-was-not-found").into()
            });
        }
        cx.notify();
    }

    pub(crate) fn replace_fonts_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        let state = self.replace_fonts.as_ref()?;
        let target = state.target.clone();
        let replacement = state.replacement.clone();
        let used_fonts = state.used_fonts.clone();
        let message = state.message.clone();
        // Substitute fonts and embedding are the presentation's (its file keeps them); the other screens only replace.
        let presentation = !self.is_document() && !self.is_workbook();

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
            body = body.child(div().text_sm().text_color(rgb(0xd93025)).child(crate::shell::i18n::text("pptx.replace-fonts.missing-fonts-title")));
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(missing.join(", ")));
        }

        // Replace from: only the fonts the file uses (as Office's 置換前). Choosing only changes the field; 置換 does the work.
        body = body.child(crate::shell::chrome::document_font_dropdown_row(
            crate::shell::i18n::text("pptx.replace-fonts.with"), "panel-font-replace-target", &target, &used_fonts, false, cx, |_, _, _, _| {}));

        // Replace with: the fonts the file uses, and all of them.
        body = body.child(crate::shell::chrome::document_font_dropdown_row(
            crate::shell::i18n::text("pptx.replace-fonts.replace"), "panel-font-replace-destination", &replacement, &used_fonts, true, cx, |_, _, _, _| {}));

        if presentation {
            let subs = self.presentation().font_substitutions.clone();
            if !subs.is_empty() {
                body = body.child(div().text_sm().text_color(rgb(0x1a73e8)).child(crate::shell::i18n::text("pptx.replace-fonts.substitutions-title")));
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
                                crate::shell::chrome::destructive_button(ElementId::Name(format!("pptx-remove-sub-{tgt}").into()), cx)
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
                body = body.child(div().text_sm().text_color(rgb(0x188038)).child(crate::shell::i18n::text("pptx.replace-fonts.embedded-fonts-title")));
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
                                crate::shell::chrome::destructive_button(ElementId::Name(format!("pptx-remove-embed-{tf}").into()), cx)
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

        use crate::shell::chrome::{ActionButtonKind, ActionDefinition};
        let mut panel_actions = crate::shell::chrome::action_bar().action(
            ActionDefinition::new("replace-fonts-apply", crate::shell::i18n::text("_shell.replace"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.replace_presentation_fonts(w, cx))),
        );
        if presentation {
            panel_actions = panel_actions
                .action(ActionDefinition::new("pptx-replace-fonts-substitute", crate::shell::i18n::text("pptx.replace-fonts.substitute"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.set_presentation_font_substitution_action(w, cx))))
                .action(ActionDefinition::new("pptx-replace-fonts-embed", crate::shell::i18n::text("pptx.replace-fonts.embed-button"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.embed_presentation_font_action(w, cx))));
        }
        let panel_actions = panel_actions.action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(crate::shell::chrome::SidebarPanel::with_footer(crate::shell::i18n::text("_pptx.replace-fonts"), body.into_any_element(), panel_actions))
    }
}
