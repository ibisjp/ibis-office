// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (ファイル > 印刷… の前の側面パネル): which slides (すべて・
//! 現在のスライド・範囲指定), as slides or as handouts (1・2・3・4・6・9
//! 枚/ページ), in colour, grey or black and white, with or without a frame.
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Which slides are printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PrintRange {
    #[default]
    All,
    Current,
    /// The slides listed (`1,3,5-8`).
    Custom,
}

/// The colour of the print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PrintColorKind {
    #[default]
    Color,
    Gray,
    BlackWhite,
}

/// The settings a print is made with (kept while the file is open).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PrintSettings {
    pub(crate) range: PrintRange,
    /// The list of slides, when `range` is `Custom`.
    pub(crate) custom: String,
    /// Slides to a handout page; `None` prints the slides themselves.
    pub(crate) handout: Option<u8>,
    /// Each slide above its notes, one to a page (ノート ページ).
    pub(crate) notes_pages: bool,
    /// The slides' titles and bodies as text (アウトライン).
    pub(crate) outline: bool,
    pub(crate) color: PrintColorKind,
    pub(crate) frame: bool,
    /// Handouts on a landscape page (配布資料の向き).
    pub(crate) landscape: bool,
}

impl PrintSettings {
    /// Whether the print is not the plain vector PDF of every slide (so it is
    /// made of the slides' pictures).
    pub(crate) fn needs_pictures(&self) -> bool {
        self.handout.is_some() || self.notes_pages || self.outline || self.color != PrintColorKind::Color || self.frame
    }

    /// The program's arguments for these settings, `current` the (1-based)
    /// number of the slide shown.
    pub(crate) fn arguments(&self, current: usize, marks: &ibis_doc_model::NotesMarks) -> Vec<String> {
        let mut args = Vec::new();
        match self.range {
            PrintRange::All => {}
            PrintRange::Current => args.extend(["--slides".to_string(), current.to_string()]),
            PrintRange::Custom => args.extend(["--slides".to_string(), self.custom.clone()]),
        }
        match self.color {
            PrintColorKind::Color => {}
            PrintColorKind::Gray => args.extend(["--print-color".to_string(), "gray".to_string()]),
            PrintColorKind::BlackWhite => args.extend(["--print-color".to_string(), "bw".to_string()]),
        }
        if self.notes_pages {
            args.push("--notes-pages".to_string());
        }
        if self.outline {
            args.push("--outline-pages".to_string());
        }
        if self.handout.is_some() || self.notes_pages || self.outline {
            if let Some(n) = self.handout {
                args.extend(["--handout".to_string(), n.to_string()]);
                if self.landscape {
                    args.push("--handout-landscape".to_string());
                }
            }
            // The header, date, footer and page number of the notes and handouts.
            if let Some(text) = marks.header.as_ref().filter(|t| !t.is_empty()) {
                args.extend(["--handout-header".to_string(), text.clone()]);
            }
            if let Some(ibis_doc_model::NotesDate::Auto(text) | ibis_doc_model::NotesDate::Fixed(text)) = marks.date.as_ref().filter(|d| matches!(d, ibis_doc_model::NotesDate::Auto(t) | ibis_doc_model::NotesDate::Fixed(t) if !t.is_empty())) {
                args.extend(["--handout-date".to_string(), text.clone()]);
            }
            if let Some(text) = marks.footer.as_ref().filter(|t| !t.is_empty()) {
                args.extend(["--handout-footer".to_string(), text.clone()]);
            }
            if marks.number {
                args.push("--handout-number".to_string());
            }
        }
        if self.frame {
            args.push("--print-frame".to_string());
        }
        args
    }
}

/// A slide list such as `1,3,5-8` (numbers from 1, a range going up),
/// as the program takes it; `None` if it is not one or names a slide past `total`.
pub(crate) fn valid_slide_list(text: &str, total: usize) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if text.is_empty() {
        return None;
    }
    for part in text.split(',') {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?),
            None => {
                let n = part.parse::<usize>().ok()?;
                (n, n)
            }
        };
        if a == 0 || b < a || b > total {
            return None;
        }
    }
    Some(text)
}

pub(crate) struct PrintPanel {
    settings: PrintSettings,
    custom: Entity<InputState>,
    error: Option<SharedString>,
}

impl SlideEditor {
    /// The print settings now.
    pub(crate) fn print_settings(&self) -> PrintSettings {
        self.pptx().map(|s| s.print_settings.clone()).unwrap_or_default()
    }

    pub(crate) fn open_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let settings = self.print_settings();
        let custom = cx.new(|cx| InputState::new(window, cx).default_value(settings.custom.clone()));
        let focus = custom.clone();
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.layout_panel = None;
            s.show_panel = None;
            s.print_panel = Some(PrintPanel { settings, custom, error: None });
        }
        self.sidebar_open = true;
        focus.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn edit_print_panel(&mut self, change: impl FnOnce(&mut PrintSettings), cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.print_panel.as_mut()) {
            change(&mut p.settings);
            p.error = None;
        }
        cx.notify();
    }

    /// The panel's settings kept; `false` (an error shown) if the list of slides is wrong.
    pub(crate) fn keep_print_settings(&mut self, cx: &mut Context<Self>) -> bool {
        let total = self.presentation().slides.len();
        let Some((mut settings, text)) = self.pptx().and_then(|s| s.print_panel.as_ref()).map(|p| (p.settings.clone(), p.custom.read(cx).value().to_string())) else { return false };
        if settings.range == PrintRange::Custom {
            match valid_slide_list(&text, total) {
                Some(list) => settings.custom = list,
                None => {
                    if let Some(p) = self.pptx_mut().and_then(|s| s.print_panel.as_mut()) {
                        p.error = Some(crate::shell::i18n::format("pptx.print-panel.enter-slides-like-1-3", &[("n", &total)]).into());
                    }
                    cx.notify();
                    return false;
                }
            }
        }
        if let Some(s) = self.pptx_mut() {
            s.print_settings = settings;
        }
        true
    }

    /// 印刷: the settings kept, the panel closed, and the print made.
    pub(crate) fn apply_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.keep_print_settings(cx) {
            return;
        }
        self.close_print_panel(window, cx);
        self.print_now(window, cx);
    }

    pub(crate) fn close_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.print_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn print_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.print_panel.as_ref()?;
        let (settings, custom, error) = (p.settings.clone(), p.custom.clone(), p.error.clone());
        let heading = |label: &str| div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text(label));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body
            .child(heading("pptx.print-panel.slides-print"))
            .child(crate::shell::chrome::radio_button("pptx-print-all", settings.range == PrintRange::All, false, crate::shell::i18n::text("pptx.slideshow.see-all-slides"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::All, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-current", settings.range == PrintRange::Current, false, crate::shell::i18n::text("pptx.print-panel.current-slide"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::Current, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-custom", settings.range == PrintRange::Custom, false, crate::shell::i18n::text("pptx.print-panel.custom-range"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::Custom, cx))))
            .child(div().w(px(200.)).child(crate::shell::chrome::text_field(&custom).full_width()))
            .child(heading("_shell.print-layout"))
            .child(crate::shell::chrome::radio_button("pptx-print-slides", settings.handout.is_none() && !settings.notes_pages && !settings.outline, false, crate::shell::i18n::text("pptx.print-panel.full-page-slides"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| { s.handout = None; s.notes_pages = false; s.outline = false }, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-notes", settings.notes_pages, false, crate::shell::i18n::text("pptx.print-panel.notes-pages-1-slide-its"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| { s.handout = None; s.notes_pages = true; s.outline = false }, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-outline", settings.outline, false, crate::shell::i18n::text("pptx.print-panel.outline"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| { s.handout = None; s.notes_pages = false; s.outline = true }, cx))));
        for n in [1u8, 2, 3, 4, 6, 9] {
            let label = crate::shell::i18n::format("pptx.print-panel.handouts-n-slides-per-page", &[("n", &n)]);
            body = body.child(crate::shell::chrome::radio_button(ElementId::Name(format!("pptx-print-handout-{n}").into()), settings.handout == Some(n), false, label, cx.listener(move |e, _, _, cx| e.edit_print_panel(|s| { s.handout = Some(n); s.notes_pages = false; s.outline = false }, cx))));
        }
        body = body
            .child(Checkbox::new("pptx-print-landscape").label(crate::shell::i18n::text("pptx.print-panel.landscape-handouts")).checked(settings.landscape).on_click(cx.listener(|e, on: &bool, _, cx| {
                let on = *on;
                e.edit_print_panel(|s| s.landscape = on, cx)
            })))
            .child(heading("docx.common.color"))
            .child(crate::shell::chrome::radio_button("pptx-print-color", settings.color == PrintColorKind::Color, false, crate::shell::i18n::text("pptx.print-panel.color"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.color = PrintColorKind::Color, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-gray", settings.color == PrintColorKind::Gray, false, crate::shell::i18n::text("pptx.print-panel.grayscale"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.color = PrintColorKind::Gray, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-bw", settings.color == PrintColorKind::BlackWhite, false, crate::shell::i18n::text("pptx.print-panel.pure-black-white"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.color = PrintColorKind::BlackWhite, cx))))
            .child(crate::shell::chrome::checkbox_button("pptx-print-frame", crate::shell::i18n::text("pptx.print-panel.frame-slides"), settings.frame, false, cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.frame = !s.frame, cx))));
        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("pptx-print-apply", crate::shell::i18n::text("pptx.print-panel.print"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_print_panel(w, cx))))
                .action(crate::shell::chrome::ActionDefinition::new("pptx-print-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_print_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.print-panel.print"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_slide_list, PrintColorKind, PrintRange, PrintSettings};

    #[test]
    fn slide_lists() {
        assert_eq!(valid_slide_list(" 1, 3,5-8 ", 10).as_deref(), Some("1,3,5-8"));
        assert_eq!(valid_slide_list("2", 2).as_deref(), Some("2"));
        for bad in ["", "0", "3-2", "x", "1,,2", "11", "1-11"] {
            assert_eq!(valid_slide_list(bad, 10), None, "{bad:?}");
        }
    }

    #[test]
    fn arguments() {
        let none = PrintSettings::default();
        assert!(none.arguments(4, &Default::default()).is_empty() && !none.needs_pictures());
        let all = PrintSettings { range: PrintRange::Current, custom: String::new(), handout: Some(6), notes_pages: false, outline: false, color: PrintColorKind::Gray, frame: true, landscape: false };
        assert_eq!(all.arguments(4, &Default::default()), ["--slides", "4", "--print-color", "gray", "--handout", "6", "--print-frame"]);
        assert!(all.needs_pictures());
        // Handouts on their side with what the notes pages carry.
        let marks = ibis_doc_model::NotesMarks { header: Some("社外秘".into()), date: Some(ibis_doc_model::NotesDate::Fixed("2026/10/11".into())), footer: Some("ibis".into()), number: true };
        let wide = PrintSettings { handout: Some(4), landscape: true, ..Default::default() };
        assert_eq!(wide.arguments(1, &marks), ["--handout", "4", "--handout-landscape", "--handout-header", "社外秘", "--handout-date", "2026/10/11", "--handout-footer", "ibis", "--handout-number"]);
        // Notes pages carry them too.
        let notes = PrintSettings { notes_pages: true, ..Default::default() };
        assert_eq!(notes.arguments(1, &marks), ["--notes-pages", "--handout-header", "社外秘", "--handout-date", "2026/10/11", "--handout-footer", "ibis", "--handout-number"]);
        assert!(notes.needs_pictures());
        // The outline (the slides' text) too.
        let outline = PrintSettings { outline: true, ..Default::default() };
        assert_eq!(outline.arguments(1, &marks), ["--outline-pages", "--handout-header", "社外秘", "--handout-date", "2026/10/11", "--handout-footer", "ibis", "--handout-number"]);
        assert!(outline.needs_pictures());
        // Not for the slides themselves.
        assert!(PrintSettings::default().arguments(1, &marks).is_empty());
        let range = PrintSettings { range: PrintRange::Custom, custom: "1,3-4".into(), ..Default::default() };
        assert_eq!(range.arguments(1, &Default::default()), ["--slides", "1,3-4"]);
        assert!(!range.needs_pictures());
    }
}
