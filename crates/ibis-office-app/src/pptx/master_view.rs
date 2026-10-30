// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド マスター表示 (表示 > スライド マスター): a slide master, or one of its
//! layouts, edited as a slide (`ibis_pptx::master_page`): the shapes can be
//! moved, resized, changed, added and taken away as on any slide. What
//! is made here goes into the master (or the layout) when the page is left for
//! another or the view is closed; the file is then read again from what
//! the program holds (not from the disk: what was not saved is kept), and the
//! history of undoing starts over.
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_pptx::master_page::{self, LevelStyle, Page, StyleKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The master view, while it is open.
pub(crate) struct MasterView {
    /// The file as it was when a page was made (and after each page left).
    base: Vec<u8>,
    pages: Vec<Page>,
    /// The page shown.
    current: usize,
    /// What the file was, to be again when the view closes.
    #[cfg(not(target_family = "wasm"))]
    path: Option<std::path::PathBuf>,
    file_name: Option<SharedString>,
    #[cfg(not(target_family = "wasm"))]
    password: Option<String>,
    slide_index: usize,
    /// The text style being edited (文字の書式).
    style: StyleEdit,
}

/// The inputs of the text style of one level of one master style.
struct StyleEdit {
    kind: StyleKind,
    level: u8,
    size: Entity<InputState>,
    bold: bool,
    color: Entity<InputState>,
    font: Entity<InputState>,
}

impl SlideEditor {
    /// The name of the page shown (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_name(&self) -> String {
        self.master_view.as_ref().map(|v| v.pages[v.current].name.clone()).unwrap_or_default()
    }

    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_page_count(&self) -> usize {
        self.master_view.as_ref().map_or(0, |v| v.pages.len())
    }

    /// The inputs of the text style filled as typed (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn set_master_style_inputs(&mut self, size: &str, color: &str, font: &str, bold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.master_view.as_mut() else { return };
        view.style.bold = bold;
        let (a, b, c) = (view.style.size.clone(), view.style.color.clone(), view.style.font.clone());
        a.update(cx, |i, cx| i.set_value(size.to_string(), window, cx));
        b.update(cx, |i, cx| i.set_value(color.to_string(), window, cx));
        c.update(cx, |i, cx| i.set_value(font.to_string(), window, cx));
    }

    /// The font size of the first title-like shape of slide `index` (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn title_size_of_slide(&self, index: usize) -> Option<u32> {
        self.presentation().slides.get(index).and_then(|s| s.shapes.first()).and_then(|sh| sh.text_body()).and_then(|b| b.paragraphs.first()).and_then(|p| p.runs.first()).and_then(|r| r.style.font_size)
    }

    pub(crate) fn is_master_view(&self) -> bool {
        self.master_view.is_some()
    }

    /// 表示 > スライド マスター: the master view, at the layout the slide shown follows.
    pub(crate) fn open_master_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_master_view() || self.is_page_view() || !self.has_document || !self.is_presentation() || self.slideshow.is_some() {
            return;
        }
        self.end_text_edit(window, cx);
        let Some(source) = self.source.clone() else {
            return self.set_status(crate::shell::i18n::text("pptx.master-view.master-document-cannot-edited-yet").to_string(), cx);
        };
        let bytes = match source.save(self.presentation(), ibis_pptx::SaveOptions::default()) {
            Ok(bytes) => bytes,
            Err(err) => return self.set_status(crate::shell::i18n::format("file.save-failed", &[("error", &err)]), cx),
        };
        let pages = match master_page::pages(&bytes) {
            Ok(pages) if !pages.is_empty() => pages,
            Ok(_) => return self.set_status(crate::shell::i18n::text("pptx.master-view.there-no-slide-master").to_string(), cx),
            Err(err) => return self.set_status(err.to_string(), cx),
        };
        // The layout the slide follows (by its name), else the master.
        let layout_name = self.slide().layout.and_then(|l| self.presentation().layout(l)).map(|l| l.name.clone());
        let current = layout_name.and_then(|name| pages.iter().position(|p| !p.master && p.name == name)).unwrap_or(0);
        let slide_index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let view = MasterView {
            base: bytes,
            pages,
            current,
            #[cfg(not(target_family = "wasm"))]
            path: self.path.clone(),
            file_name: self.file_name.clone(),
            #[cfg(not(target_family = "wasm"))]
            password: self.protection_password.clone(),
            slide_index,
            style: StyleEdit {
                kind: StyleKind::Title,
                level: 1,
                size: cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.master-view.size-pt"))),
                bold: false,
                color: cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.master-view.color-rrggbb"))),
                font: cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.font"))),
            },
        };
        self.show_master_page(view, current, window, cx);
        self.fill_style_inputs(window, cx);
        self.set_status(crate::shell::i18n::text("pptx.master-view.slide-master-view-undo-history").to_string(), cx);
    }

    /// Page `index` of `view` made the document.
    fn show_master_page(&mut self, mut view: MasterView, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let part = view.pages[index].part.clone();
        let opened = master_page::build(&view.base, &part).map_err(|e| e.to_string()).and_then(|temp| ibis_pptx::open(temp).map_err(|e| e.to_string()));
        match opened {
            Ok(opened) => {
                view.current = index;
                let name = view.file_name.clone().unwrap_or_default();
                self.load(opened.presentation, name, window, cx);
                self.source = Some(std::sync::Arc::new(opened.source));
                self.master_view = Some(view);
                cx.notify();
            }
            Err(err) => {
                self.master_view = Some(view);
                self.set_status(err, cx);
            }
        }
    }

    /// The inputs of the text style shown the values the master has for the level chosen.
    fn fill_style_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.master_view.as_mut() else { return };
        let master = view.pages[view.current].of.clone();
        let current = master_page::text_styles(&view.base, &master).unwrap_or_default().into_iter().find(|(k, l, _)| *k == view.style.kind && *l == view.style.level).map(|(_, _, s)| s).unwrap_or_default();
        view.style.bold = current.bold.unwrap_or(false);
        let (size, color, font) = (view.style.size.clone(), view.style.color.clone(), view.style.font.clone());
        size.update(cx, |i, cx| i.set_value(current.size.map(|s| format!("{}", s as f32 / 100.0)).unwrap_or_default(), window, cx));
        color.update(cx, |i, cx| i.set_value(current.color.map(|c| format!("{c:06X}")).unwrap_or_default(), window, cx));
        font.update(cx, |i, cx| i.set_value(current.font.clone().unwrap_or_default(), window, cx));
    }

    /// 文字の書式: the style and level chosen.
    pub(crate) fn choose_master_style(&mut self, kind: StyleKind, level: u8, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.master_view.as_mut() {
            view.style.kind = kind;
            view.style.level = level;
        }
        self.fill_style_inputs(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_master_style_bold(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(view) = self.master_view.as_mut() {
            view.style.bold = on;
        }
        cx.notify();
    }

    /// 適用: the look the inputs state is that of the style and level chosen, on the slides that follow the master.
    pub(crate) fn apply_master_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.master_view.take() else { return };
        self.end_text_edit(window, cx);
        let size = view.style.size.read(cx).value().trim().to_string();
        let color = view.style.color.read(cx).value().trim().trim_start_matches('#').to_string();
        let font = view.style.font.read(cx).value().trim().to_string();
        let change = LevelStyle {
            size: size.parse::<f32>().ok().filter(|s| *s > 0.0).map(|s| (s * 100.0).round() as u32),
            bold: Some(view.style.bold),
            color: (color.len() == 6).then(|| u32::from_str_radix(&color, 16).ok()).flatten(),
            font: (!font.is_empty()).then_some(font),
        };
        let (kind, level, master) = (view.style.kind, view.style.level, view.pages[view.current].of.clone());
        let result = self.merge_master_page(&view).and_then(|bytes| master_page::set_text_style(&bytes, &master, kind, level, &change).map_err(|e| e.to_string()));
        match result {
            Ok(bytes) => {
                view.base = bytes;
                let current = view.current;
                self.show_master_page(view, current, window, cx);
                self.fill_style_inputs(window, cx);
            }
            Err(err) => {
                self.master_view = Some(view);
                self.set_status(err, cx);
            }
        }
    }

    /// プレースホルダーの挿入: a placeholder of `kind` put on the page shown (the layout or the master), in the middle.
    pub(crate) fn insert_master_placeholder(&mut self, kind: master_page::PlaceholderKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.master_view.take() else { return };
        self.end_text_edit(window, cx);
        let part = view.pages[view.current].part.clone();
        let result = self.merge_master_page(&view).and_then(|bytes| master_page::add_placeholder(&bytes, &part, kind).map_err(|e| e.to_string()));
        match result {
            Ok(bytes) => {
                view.base = bytes;
                let current = view.current;
                self.show_master_page(view, current, window, cx);
            }
            Err(err) => {
                self.master_view = Some(view);
                self.set_status(err, cx);
            }
        }
    }

    /// The page shown, as edited, put into the file the view holds.
    fn merge_master_page(&mut self, view: &MasterView) -> Result<Vec<u8>, String> {
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_pptx.can-t-save").to_string())?;
        let edited = source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;
        master_page::apply(&view.base, &edited, &view.pages[view.current].part).map_err(|e| e.to_string())
    }

    /// Another page of the master view: what was made on this one kept.
    pub(crate) fn switch_master_page(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.master_view.take() else { return };
        if index == view.current || index >= view.pages.len() {
            self.master_view = Some(view);
            return;
        }
        self.end_text_edit(window, cx);
        match self.merge_master_page(&view) {
            Ok(bytes) => view.base = bytes,
            Err(err) => {
                self.master_view = Some(view);
                return self.set_status(err, cx);
            }
        }
        self.show_master_page(view, index, window, cx);
    }

    /// マスター表示を閉じる: the file with what was made, read again, the slides as they were.
    pub(crate) fn close_master_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.master_view.take() else { return };
        self.end_text_edit(window, cx);
        let merged = self.merge_master_page(&view).and_then(|bytes| ibis_pptx::open(bytes).map_err(|e| e.to_string()));
        let (opened, view) = match merged {
            Ok(opened) => (opened, view),
            Err(err) => {
                self.master_view = Some(view);
                return self.set_status(crate::shell::i18n::format("pptx.master-view.master-could-not-saved-error", &[("error", &err)]), cx);
            }
        };
        let name = view.file_name.clone().unwrap_or_default();
        self.load(opened.presentation, name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        self.file_name = view.file_name.clone();
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = view.path.clone();
            self.protection_password = view.password.clone();
        }
        if let Some(id) = self.presentation().slides.get(view.slide_index).map(|s| s.id) {
            self.select_slide(id, window, cx);
        }
        self.set_status(crate::shell::i18n::text("pptx.master-view.slide-master-view-closed-undo").to_string(), cx);
        cx.notify();
    }

    /// Context menu entries for page `index` in Slide Master View (PPTX-MASTER-001..005).
    pub(crate) fn master_page_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some(view) = self.master_view.as_ref() else { return Vec::new() };
        let Some(page) = view.pages.get(index) else { return Vec::new() };
        let t = crate::shell::i18n::text;
        let name = if page.name.is_empty() { t("pptx.chrome.no-name").to_string() } else { page.name.clone() };
        let page_title: SharedString = if page.master {
            crate::shell::i18n::format("pptx.master-view.slide-master-name", &[("name", &name)]).into()
        } else {
            name.into()
        };

        let placeholder_items: Vec<MenuEntry> = [
            (master_page::PlaceholderKind::Title, "pptx.master-view.placeholder.title", "title"),
            (master_page::PlaceholderKind::Text, "pptx.master-view.placeholder.text", "notes"),
            (master_page::PlaceholderKind::Content, "pptx.master-view.placeholder.content", "dashboard"),
            (master_page::PlaceholderKind::Picture, "pptx.master-view.placeholder.picture", "image"),
            (master_page::PlaceholderKind::Chart, "pptx.master-view.placeholder.chart", "insert_chart"),
            (master_page::PlaceholderKind::Table, "pptx.master-view.placeholder.table", "table"),
            (master_page::PlaceholderKind::Media, "pptx.master-view.placeholder.media", "movie"),
        ]
        .into_iter()
        .map(|(kind, label, icon)| {
            MenuItem::tr(label)
                .icon(icon)
                .on(move |e, w, cx| {
                    e.switch_master_page(index, w, cx);
                    e.insert_master_placeholder(kind, w, cx);
                })
                .into()
        })
        .collect();

        vec![
            MenuItem::new("pptx.master-view.select-page", page_title)
                .icon("view_quilt")
                .checked(view.current == index)
                .on(move |e, w, cx| e.switch_master_page(index, w, cx))
                .into(),
            MenuEntry::Separator,
            Submenu::tr("pptx.master-view.placeholder.insert", "add_box", placeholder_items).into(),
            Submenu::tr("pptx.background", "wallpaper", crate::pptx::chrome::background_menu(self)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.master-view.close-master-view")
                .icon("close")
                .on(|e, w, cx| e.close_master_view(w, cx))
                .into(),
        ]
    }

    /// Open context menu for page `index` in Slide Master View at pointer `position`.
    pub(crate) fn open_master_page_context_menu(&mut self, index: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.master_view.as_ref().is_none_or(|v| index >= v.pages.len()) {
            return;
        }
        self.switch_master_page(index, window, cx);
        let entries = self.master_page_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    pub(crate) fn master_view_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let view = self.master_view.as_ref()?;
        let t = crate::shell::i18n::text;
        let mut body = v_flex().gap_1().px(px(12.)).py(px(8.)).p_0();
        for (i, page) in view.pages.iter().enumerate() {
            let name = if page.name.is_empty() { t("pptx.chrome.no-name").to_string() } else { page.name.clone() };
            let label = if page.master { crate::shell::i18n::format("pptx.master-view.slide-master-name", &[("name", &name)]) } else { name.into() };
            let row = div()
                .id(ElementId::Name(format!("pptx-master-page-row-{i}").into()))
                .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                    cx.stop_propagation();
                    e.open_master_page_context_menu(i, event.position, w, cx);
                }))
                .child(crate::shell::chrome::radio_button(ElementId::Name(format!("pptx-master-page-{i}").into()), i == view.current, false, label, cx.listener(move |e, _, w, cx| e.switch_master_page(i, w, cx))));
            body = body.child(row);
        }
        // プレースホルダーの挿入: onto the page shown.
        body = body.child(div().mt_2().text_sm().font_weight(FontWeight::BOLD).child(t("pptx.master-view.placeholder.insert")));
        let mut kinds = h_flex().gap_1().flex_wrap();
        for (kind, label) in [
            (master_page::PlaceholderKind::Title, "pptx.master-view.placeholder.title"),
            (master_page::PlaceholderKind::Text, "pptx.master-view.placeholder.text"),
            (master_page::PlaceholderKind::Content, "pptx.master-view.placeholder.content"),
            (master_page::PlaceholderKind::Picture, "pptx.master-view.placeholder.picture"),
            (master_page::PlaceholderKind::Chart, "pptx.master-view.placeholder.chart"),
            (master_page::PlaceholderKind::Table, "pptx.master-view.placeholder.table"),
            (master_page::PlaceholderKind::Media, "pptx.master-view.placeholder.media"),
        ] {
            kinds = kinds.child(crate::shell::chrome::action_button(ElementId::Name(format!("pptx-master-placeholder-{label}").into())).small().label(t(label)).on_click(cx.listener(move |e, _, w, cx| e.insert_master_placeholder(kind, w, cx))));
        }
        body = body.child(kinds);
        // 文字の書式: the text styles of the master (the title's, the body's) by level.
        let style = &view.style;
        let (kind, level, bold) = (style.kind, style.level, style.bold);
        body = body.child(div().mt_2().text_sm().font_weight(FontWeight::BOLD).child(t("pptx.master-view.text-styles")));
        body = body.child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
            .child(crate::shell::chrome::radio_button("pptx-master-style-title", kind == StyleKind::Title, false, crate::shell::i18n::text("_shell.title"), cx.listener(move |e, _, w, cx| e.choose_master_style(StyleKind::Title, level, w, cx))))
            .child(crate::shell::chrome::radio_button("pptx-master-style-body", kind == StyleKind::Body, false, crate::shell::i18n::text("pptx.master-view.body"), cx.listener(move |e, _, w, cx| e.choose_master_style(StyleKind::Body, level, w, cx))))
            .child(crate::shell::chrome::radio_button("pptx-master-style-other", kind == StyleKind::Other, false, crate::shell::i18n::text("_shell.more"), cx.listener(move |e, _, w, cx| e.choose_master_style(StyleKind::Other, level, w, cx)))));
        let mut levels = h_flex().gap_1().flex_wrap();
        for n in 1..=9u8 {
            levels = levels.child(crate::shell::chrome::action_button(ElementId::Name(format!("pptx-master-level-{n}").into())).small().label(if n == level { format!("[{n}]") } else { n.to_string() }).on_click(cx.listener(move |e, _, w, cx| e.choose_master_style(kind, n, w, cx))));
        }
        body = body
            .child(div().text_sm().child(t("pptx.master-view.level")))
            .child(levels)
            .child(crate::shell::chrome::text_field(&style.size).full_width().spinner("pptx-master-style-size", crate::shell::chrome::SpinnerSpec::new(1.0).steps(FONT_SIZE_STEPS).range(1.0, 4000.0), cx, |_, _, _, _| {}))
            .child(Checkbox::new("pptx-master-style-bold").label(t("_shell.bold")).checked(bold).on_click(cx.listener(|e, on: &bool, _, cx| e.toggle_master_style_bold(*on, cx))))
            .child(crate::shell::chrome::panel_color_field("master-color",&style.color,cx))
            .child(crate::shell::chrome::panel_font_dropdown("panel-font-master",&style.font,cx))
            .child(crate::shell::chrome::primary_button("pptx-master-style-apply").small().label(t("pptx.master-view.apply-style")).on_click(cx.listener(|e, _, w, cx| e.apply_master_style(w, cx))));
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-master-close", t("pptx.master-view.close-master-view"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_master_view(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.master-view.slide-master"), body.into_any_element(), panel_terminal_actions))
    }
}

/// The sizes of PowerPoint's font size list, which the ▲▼ of the text style's size step through (applied with 文字の書式を適用).
const FONT_SIZE_STEPS: &[f32] = &[8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0, 72.0, 80.0, 88.0, 96.0];
