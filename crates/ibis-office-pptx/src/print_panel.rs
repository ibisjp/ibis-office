// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (ファイル > 印刷… の前の側面パネル): which slides (すべて・
//! 現在のスライド・範囲指定), as slides or as handouts (1・2・3・4・6・9
//! 枚/ページ), in colour, grey or black and white, with or without a frame.
use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

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

/// One entry of the print layout dropdown.
#[derive(Clone, Copy, PartialEq)]
enum PrintLayout { Slides, Notes, Outline, Handout(u8) }
const PRINT_LAYOUTS: [PrintLayout; 9] = [PrintLayout::Slides, PrintLayout::Notes, PrintLayout::Outline, PrintLayout::Handout(1), PrintLayout::Handout(2), PrintLayout::Handout(3), PrintLayout::Handout(4), PrintLayout::Handout(6), PrintLayout::Handout(9)];
/// The list of slides is indented by one full-width character at the 14 px of the radio labels.
const CUSTOM_INDENT: f32 = 14.;
/// The layout dropdown's button and list items are 2.5 times the usual height, each with a 4:3 picture of the page (2026-11-04 user).
const LAYOUT_ROW_SCALE: f32 = 2.5;
const LAYOUT_WIDTH_SCALE: f32 = 3.5;
/// The picture's height (a row less 4 px of padding above and below); its width is 4:3.
const THUMB_HEIGHT: f32 = crate::shell::chrome::BUTTON_HEIGHT * LAYOUT_ROW_SCALE - 8.;
const THUMB_PAD: f32 = 6.;
const THUMB_GAP: f32 = 3.;

impl PrintLayout {
    fn of(settings: &PrintSettings) -> Self {
        if settings.notes_pages { Self::Notes } else if settings.outline { Self::Outline } else if let Some(n) = settings.handout { Self::Handout(n) } else { Self::Slides }
    }
    fn label(self) -> SharedString {
        match self {
            Self::Slides => crate::shell::i18n::text("pptx.print-panel.full-page-slides"),
            Self::Notes => crate::shell::i18n::text("pptx.print-panel.notes-pages-1-slide-its"),
            Self::Outline => crate::shell::i18n::text("pptx.print-panel.outline"),
            Self::Handout(n) => crate::shell::i18n::format("pptx.print-panel.handouts-n-slides-per-page", &[("n", &n)]).into(),
        }
    }
    fn apply(self, s: &mut PrintSettings) {
        s.handout = None; s.notes_pages = false; s.outline = false;
        match self { Self::Slides => {} Self::Notes => s.notes_pages = true, Self::Outline => s.outline = true, Self::Handout(n) => s.handout = Some(n) }
    }
    /// Slides across and down of the picture of a handout page (and whether the text lines are at the right of them).
    fn grid(n: u8) -> (usize, usize, bool) {
        match n { 1 => (1, 1, false), 2 => (1, 2, false), 3 => (1, 3, true), 4 => (2, 2, false), 6 => (2, 3, false), _ => (3, 3, false) }
    }
    /// A 4:3 picture of the page: the slides (blue) and the text lines (grey) as the layout prints them.
    fn thumbnail(self) -> AnyElement {
        let (w, h) = (THUMB_HEIGHT * 4. / 3., THUMB_HEIGHT);
        let (inner_w, inner_h) = (w - THUMB_PAD * 2. - 2., h - THUMB_PAD * 2. - 2.);
        let slide = |w: f32, h: f32| div().w(px(w)).h(px(h)).flex_none().bg(rgb(0x8ab4d8)).border_1().border_color(rgb(0x5f6368));
        let lines = |count: usize, w: f32| v_flex().gap(px(THUMB_GAP)).children((0..count).map(move |i| div().h(px(2.)).w(px(if i + 1 == count { w * 0.6 } else { w })).bg(rgb(0xbdc1c6))));
        let page = match self {
            Self::Slides => slide(inner_w, inner_h).into_any_element(),
            Self::Notes => {
                let sh = inner_h * 0.55;
                v_flex().items_center().gap(px(THUMB_GAP * 2.)).child(slide(sh * 4. / 3., sh)).child(lines(3, inner_w)).into_any_element()
            }
            Self::Outline => lines(6, inner_w).into_any_element(),
            Self::Handout(n) => {
                let (cols, rows, text) = Self::grid(n);
                let area_w = if text { inner_w * 0.5 } else { inner_w };
                let cw = (area_w - THUMB_GAP * (cols as f32 - 1.)) / cols as f32;
                let ch = (inner_h - THUMB_GAP * (rows as f32 - 1.)) / rows as f32;
                let grid = h_flex().gap(px(THUMB_GAP)).children((0..cols).map(|_| v_flex().gap(px(THUMB_GAP)).children((0..rows).map(|_| slide(cw, ch)))));
                if text {
                    h_flex().gap(px(THUMB_GAP * 2.)).items_center().child(grid).child(v_flex().gap(px(THUMB_GAP * 3.)).children((0..rows).map(|_| lines(2, inner_w * 0.4)))).into_any_element()
                } else { grid.into_any_element() }
            }
        };
        div().w(px(w)).h(px(h)).flex_none().flex().items_center().justify_center().bg(rgb(0xffffff)).border_1().border_color(rgb(0xdadce0)).child(page).into_any_element()
    }
}

/// The print layout (slides, notes, outline, handouts of 1 to 9 slides): a dropdown whose button and items show a 4:3 picture of the page and its name.
fn layout_dropdown(settings: &PrintSettings, cx: &mut Context<SlideEditor>) -> impl IntoElement {
    let current = PrintLayout::of(settings);
    let choices = PRINT_LAYOUTS.into_iter().map(|layout| crate::shell::chrome::preview_dropdown::PreviewChoice {
        label: layout.label(),
        content: std::rc::Rc::new(move || h_flex().w_full().items_center().gap(px(10.)).child(layout.thumbnail()).child(div().flex_1().min_w_0().truncate().text_size(px(13.)).child(layout.label())).into_any_element()),
        choose: std::rc::Rc::new(move |e: &mut SlideEditor, _, cx| e.edit_print_panel(|s| layout.apply(s), cx)),
    }).collect();
    let selected = PRINT_LAYOUTS.iter().position(|l| *l == current).unwrap_or(0);
    let height = crate::shell::chrome::BUTTON_HEIGHT * LAYOUT_ROW_SCALE;
    let size = crate::shell::chrome::preview_dropdown::PreviewSize { width: crate::shell::chrome::FIELD_WIDTH * LAYOUT_WIDTH_SCALE, height, row_height: height };
    crate::shell::chrome::preview_dropdown::preview_dropdown_sized("pptx-print-layout", choices, selected, size, cx)
}

pub struct PrintPanel {
    pub(crate) settings: PrintSettings,
    custom: Entity<InputState>,
    error: Option<SharedString>,
}

#[ibis_office_macros::editor_ext(PrintPanelExt)]
impl SlideEditor {
    /// The print settings now.
    pub(crate) fn print_settings(&self) -> PrintSettings {
        self.pptx().map(|s| s.print_settings.clone()).unwrap_or_default()
    }

    pub fn open_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let settings = self.print_settings();
        let custom = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.print-panel.slides-placeholder")).default_value(settings.custom.clone()));
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

    pub fn close_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.print_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn print_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.print_panel.as_ref()?;
        let (settings, custom, error) = (p.settings.clone(), p.custom.clone(), p.error.clone());
        let heading = |label: &str| div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text(label));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body
            .child(heading("pptx.print-panel.slides-print"))
            .child(crate::shell::chrome::radio_button("pptx-print-all", settings.range == PrintRange::All, false, crate::shell::i18n::text("pptx.slideshow.see-all-slides"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::All, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-current", settings.range == PrintRange::Current, false, crate::shell::i18n::text("pptx.print-panel.current-slide"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::Current, cx))))
            .child(crate::shell::chrome::radio_button("pptx-print-custom", settings.range == PrintRange::Custom, false, crate::shell::i18n::text("pptx.print-panel.custom-range"), cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.range = PrintRange::Custom, cx))))
            // The list of slides is indented by one full-width character (the label of the radio above it) and is as wide as the dialog.
            .child(div().w_full().pl(px(CUSTOM_INDENT)).child(crate::shell::chrome::text_field(&custom).full_width()))
            .child(heading("_shell.print-layout"))
            .child(crate::shell::chrome::checkbox_button("pptx-print-landscape", crate::shell::i18n::text("pptx.print-panel.landscape-handouts"), settings.landscape, false, cx.listener(|e, _, _, cx| e.edit_print_panel(|s| s.landscape = !s.landscape, cx))))
            .child(layout_dropdown(&settings, cx));
        // 色: the label and the three choices on one line when they fit the pane (two lines when they do not).
        let colors = crate::shell::chrome::segment_control_row(crate::shell::i18n::text("docx.common.color"), [(PrintColorKind::Color, "pptx.print-panel.color", "pptx-print-color"), (PrintColorKind::Gray, "pptx.print-panel.grayscale", "pptx-print-gray"), (PrintColorKind::BlackWhite, "pptx.print-panel.pure-black-white", "pptx-print-bw")].into_iter().map(|(value, label, id)| {
            crate::shell::chrome::segment_control::SegmentItem::label(id, crate::shell::i18n::text(label), settings.color == value).on_click(cx.listener(move |e, _, _, cx| e.edit_print_panel(|s| s.color = value, cx)))
        }));
        body = body
            .child(colors)
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
