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
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_pptx::master_page::{self, LevelStyle, Page, StyleKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

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
    /// The name of the page shown, as typed (名前の変更).
    name: Entity<InputState>,
    /// The pictures of the pages in the grid of the panel (made when first shown, made again when the file changed).
    thumbs: Vec<Option<std::rc::Rc<PageThumb>>>,
    /// A fingerprint of `base` the thumbnails were made from.
    thumbs_of: u64,
    /// The grid's keyboard focus (arrows move the page shown).
    grid_focus: FocusHandle,
}

/// A page of the master view drawn small: the page as a slide of its own file, with that file's resources.
struct PageThumb {
    slide: ibis_doc_model::Slide,
    size: ibis_doc_model::EmuSize,
    resources: crate::render::shape_view::Resources,
}

/// A layout's picture is this much of a master's (the layouts stand under their master, smaller, as in PowerPoint).
const LAYOUT_THUMB_RATIO: f32 = 0.6;
/// The width of the number before a master's picture.
const MASTER_NUMBER_W: f32 = 16.;
/// How far a layout is set in from its master's picture.
const LAYOUT_INDENT: f32 = 24.;

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// A page's picture: the slide drawn at `width`, as the slide thumbnails are.
fn page_picture(slide: &ibis_doc_model::Slide, size: ibis_doc_model::EmuSize, res: &crate::render::shape_view::Resources, width: f32) -> Div {
    let scale = width / size.width.max(1) as f32;
    let height = size.height as f32 * scale;
    div().relative().w(px(width)).h(px(height)).flex_none().overflow_hidden().bg(gpui_kit::white())
        .child(crate::render::shape_view::slide_backdrop(slide, scale, res))
        .children(slide.shown_shapes().map(|shape| crate::render::shape_view::shape_element(shape, shape.frame, scale, res, None)))
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

impl MasterView {
    /// The master view open, to change it, out of the editor's `format` field (see `State::of_mut`).
    fn of_mut(format: &mut dyn crate::shell::format::FormatState) -> Option<&mut MasterView> {
        super::State::of_mut(format)?.master_view.as_mut()
    }
}

#[ibis_office_macros::editor_ext(MasterViewExt)]
impl SlideEditor {
    /// The master view, while it is open (the presentation's state has it).
    fn master_view(&self) -> Option<&MasterView> {
        self.pptx()?.master_view.as_ref()
    }

    /// `view` is the master view open. A page shown is a document loaded, and a load makes the presentation's state anew: the
    /// view is taken out before the load and put back here after it.
    fn set_master_view(&mut self, view: MasterView) {
        if let Some(state) = self.pptx_mut() {
            state.master_view = Some(view);
        }
    }

    /// The name of the page shown (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_name(&self) -> String {
        self.master_view().map(|v| v.pages[v.current].name.clone()).unwrap_or_default()
    }

    /// The page shown, and how many pages of the grid have a picture of their own (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub fn master_view_grid(&self) -> Option<(usize, usize)> {
        self.master_view().map(|v| (v.current, v.thumbs.iter().flatten().count()))
    }

    /// How many pages have a picture made (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_thumb_count(&self) -> usize {
        self.master_view().map_or(0, |v| v.thumbs.iter().flatten().count())
    }

    /// Which picture each page has (the address of the shared picture; 0 for none), to tell a picture made again (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_thumb_ids(&self) -> Vec<usize> {
        self.master_view().map_or_else(Vec::new, |v| v.thumbs.iter().map(|t| t.as_ref().map_or(0, |t| std::rc::Rc::as_ptr(t) as usize)).collect())
    }

    /// The grid's keyboard focus (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub fn focus_master_grid(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = self.master_view() { v.grid_focus.focus(window, cx); }
    }

    /// The name typed for the page shown (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn set_master_page_name_input(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.master_view().map(|v| v.name.clone()) else { return };
        input.update(cx, |i, cx| i.set_value(name.to_string(), window, cx));
    }

    /// The names of the pages (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn master_view_page_names(&self) -> Vec<String> {
        self.master_view().map(|v| v.pages.iter().map(|p| p.name.clone()).collect()).unwrap_or_default()
    }

    #[cfg(feature = "scenarios")]
    pub fn master_view_page_count(&self) -> usize {
        self.master_view().map_or(0, |v| v.pages.len())
    }

    /// The inputs of the text style filled as typed (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn set_master_style_inputs(&mut self, size: &str, color: &str, font: &str, bold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = MasterView::of_mut(&mut *self.format) else { return };
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

    /// The arrows, Home and End on the list of pages move the page shown. True when the key is taken.
    pub(crate) fn master_grid_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(view) = self.master_view().filter(|v| v.grid_focus.is_focused(window)) else { return false };
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt || key.modifiers.shift { return false; }
        let (current, count) = (view.current, view.pages.len());
        let target = match key.key.as_str() {
            "left" | "up" => current.checked_sub(1),
            "right" | "down" => (current + 1 < count).then_some(current + 1),
            "home" => Some(0),
            "end" => count.checked_sub(1),
            _ => return false,
        };
        if let Some(target) = target {
            self.switch_master_page(target, window, cx);
            // Showing a page loads the file again, which takes the focus: the grid keeps it for the next key.
            if let Some(v) = self.master_view() { v.grid_focus.focus(window, cx); }
        }
        true
    }

    /// The list of pages' keyboard focus, while the master view is open.
    pub(crate) fn master_view_focus(&self) -> Option<FocusHandle> {
        self.master_view().map(|v| v.grid_focus.clone())
    }

    pub fn is_master_view(&self) -> bool {
        self.master_view().is_some()
    }

    /// 表示 > スライド マスター: the master view, at the layout the slide shown follows.
    pub fn open_master_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_master_view() || self.is_page_view() || !self.has_document || !self.is_presentation() || self.slideshow().is_some() {
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
            name: cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.name"))),
            thumbs: Vec::new(),
            thumbs_of: 0,
            grid_focus: cx.focus_handle(),
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
        // The pictures of the grid are made from the file: a changed file makes them anew.
        let timing = crate::clock::debug_timing::start();
        let print = fingerprint(&view.base);
        crate::clock::debug_timing::report("master switch: fingerprint", timing);
        if view.thumbs_of != print || view.thumbs.len() != view.pages.len() {
            view.thumbs = vec![None; view.pages.len()];
            view.thumbs_of = print;
        }
        let timing = crate::clock::debug_timing::start();
        let opened = master_page::build(&view.base, &part).map_err(|e| e.to_string()).and_then(|temp| ibis_pptx::open(temp).map_err(|e| e.to_string()));
        crate::clock::debug_timing::report("master switch: build and open the page", timing);
        match opened {
            Ok(opened) => {
                let timing = crate::clock::debug_timing::start();
                view.current = index;
                let name = view.file_name.clone().unwrap_or_default();
                let flags = self.view_aids();
                self.load(opened.presentation, name, window, cx);
                self.source = Some(std::sync::Arc::new(opened.source));
                self.set_master_view(view);
                self.restore_view_aids(flags);
                crate::clock::debug_timing::report("master switch: load", timing);
                let (name_input, page_name) = self.master_view().map(|v| (v.name.clone(), v.pages[index].name.clone())).unzip();
                if let (Some(input), Some(page_name)) = (name_input, page_name) {
                    input.update(cx, |i, cx| i.set_value(page_name, window, cx));
                }
                // The page has guides of its own (the file's): shown.
                if !self.slide().guides.is_empty() && self.pptx().is_some() {
                    self.shape_edit.guides = true;
                }
                cx.notify();
            }
            Err(err) => {
                self.set_master_view(view);
                self.set_status(err, cx);
            }
        }
    }

    /// グリッド線・ガイド・スナップ: what is on, kept over the pages of the master view (a page is a file of its own).
    fn view_aids(&self) -> Option<(bool, bool, bool, bool)> {
        self.pptx().map(|_| (self.shape_edit.gridlines, self.shape_edit.guides, self.shape_edit.snap_to_grid, self.shape_edit.snap_to_shape))
    }

    fn restore_view_aids(&mut self, aids: Option<(bool, bool, bool, bool)>) {
        if let (Some((gridlines, guides, snap_to_grid, snap_to_shape)), Some(_)) = (aids, self.pptx_mut()) {
            self.shape_edit.gridlines = gridlines;
            self.shape_edit.guides = guides;
            self.shape_edit.snap_to_grid = snap_to_grid;
            self.shape_edit.snap_to_shape = snap_to_shape;
        }
    }

    /// The inputs of the text style shown the values the master has for the level chosen.
    fn fill_style_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = MasterView::of_mut(&mut *self.format) else { return };
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
        if let Some(view) = MasterView::of_mut(&mut *self.format) {
            view.style.kind = kind;
            view.style.level = level;
        }
        self.fill_style_inputs(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_master_style_bold(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(view) = MasterView::of_mut(&mut *self.format) {
            view.style.bold = on;
        }
        cx.notify();
    }

    /// 適用: the look the inputs state is that of the style and level chosen, on the slides that follow the master.
    pub(crate) fn apply_master_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.pptx_mut().and_then(|s| s.master_view.take()) else { return };
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
                self.set_master_view(view);
                self.set_status(err, cx);
            }
        }
    }

    /// プレースホルダーの挿入: a placeholder of `kind` put on the page shown (the layout or the master), in the middle.
    pub(crate) fn insert_master_placeholder(&mut self, kind: master_page::PlaceholderKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.pptx_mut().and_then(|s| s.master_view.take()) else { return };
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
                self.set_master_view(view);
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

    /// The file of the view changed by `op` (given the file with the page shown as edited, and the page): the pages
    /// listed again, `op`'s page (or the same one) shown.
    fn edit_master_file(&mut self, window: &mut Window, cx: &mut Context<Self>, op: impl FnOnce(&[u8], &Page) -> Result<(Vec<u8>, Option<String>), String>) {
        let Some(mut view) = self.pptx_mut().and_then(|s| s.master_view.take()) else { return };
        self.end_text_edit(window, cx);
        let page = view.pages[view.current].clone();
        let result = self.merge_master_page(&view).and_then(|merged| {
            let (bytes, show) = op(&merged, &page)?;
            let pages = master_page::pages(&bytes).map_err(|e| e.to_string())?;
            Ok((bytes, pages, show))
        });
        match result {
            Ok((bytes, pages, show)) => {
                let same = view.current.min(pages.len().saturating_sub(1));
                let current = show.or(Some(page.part.clone())).and_then(|part| pages.iter().position(|p| p.part == part)).unwrap_or(same);
                view.base = bytes;
                view.pages = pages;
                self.show_master_page(view, current, window, cx);
                self.fill_style_inputs(window, cx);
            }
            Err(err) => {
                self.set_master_view(view);
                self.set_status(err, cx);
            }
        }
    }

    /// 名前の変更: the page shown named as typed.
    pub(crate) fn rename_master_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.master_view().map(|v| v.name.read(cx).value().trim().to_string()) else { return };
        if name.is_empty() {
            return self.set_status(crate::shell::i18n::text("pptx.section-panel.enter-name").to_string(), cx);
        }
        self.edit_master_file(window, cx, move |bytes, page| Ok((master_page::rename_page(bytes, &page.part, &name).map_err(|e| e.to_string())?, None)));
    }

    /// レイアウトの複製: the layout shown copied (named "<name> のコピー"), the copy shown.
    pub(crate) fn duplicate_master_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.master_page_is_layout() {
            return;
        }
        let existing: Vec<String> = self.master_view().map(|v| v.pages.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        self.edit_master_file(window, cx, move |bytes, page| {
            let base = crate::shell::i18n::format("pptx.layout-panel.copy-name", &[("name", &page.name)]);
            let name = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") }).find(|n| !existing.contains(n)).unwrap_or(base);
            let (bytes, copy) = master_page::duplicate_layout(bytes, &page.part, &name).map_err(|e| e.to_string())?;
            Ok((bytes, Some(copy)))
        });
    }

    /// レイアウトの削除: the layout shown taken away (not one a slide follows).
    pub(crate) fn delete_master_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.master_page_is_layout() {
            return;
        }
        self.edit_master_file(window, cx, |bytes, page| {
            if master_page::layout_in_use(bytes, &page.part).map_err(|e| e.to_string())? {
                return Err(crate::shell::i18n::text("pptx.master-view.layout-in-use").to_string());
            }
            Ok((master_page::delete_layout(bytes, &page.part).map_err(|e| e.to_string())?, Some(page.of.clone())))
        });
    }

    /// Another page of the master view: what was made on this one kept.
    pub(crate) fn switch_master_page(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut view) = self.pptx_mut().and_then(|s| s.master_view.take()) else { return };
        if index == view.current || index >= view.pages.len() {
            self.set_master_view(view);
            return;
        }
        self.end_text_edit(window, cx);
        // A page nothing was done on (no step to undo) is what the file has: the file stays as it is, so the pictures of
        // the list, made from it, stay too (they are made again only when a page was edited).
        let timing = crate::clock::debug_timing::start();
        let merged = if self.document.can_undo() { self.merge_master_page(&view).map(Some) } else { Ok(None) };
        crate::clock::debug_timing::report("master switch: merge the page into the file", timing);
        match merged {
            Ok(Some(bytes)) => view.base = bytes,
            Ok(None) => {}
            Err(err) => {
                self.set_master_view(view);
                return self.set_status(err, cx);
            }
        }
        self.show_master_page(view, index, window, cx);
    }

    /// マスター表示を閉じる: the file with what was made, read again, the slides as they were.
    pub fn close_master_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_master_view(false, window, cx);
    }

    /// 閉じる, with the slide that was shown made to follow the layout of the page shown (レイアウトの適用).
    pub(crate) fn apply_master_page_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_master_view(true, window, cx);
    }

    /// Whether the page shown is a layout (one a slide can follow).
    pub(crate) fn master_page_is_layout(&self) -> bool {
        self.master_view().is_some_and(|v| !v.pages[v.current].master)
    }

    fn finish_master_view(&mut self, apply: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.pptx_mut().and_then(|s| s.master_view.take()) else { return };
        // The layout to follow: its name, and which of that name it is (by the order of the pages).
        let wanted = (apply && !view.pages[view.current].master).then(|| {
            let page = &view.pages[view.current];
            (page.name.clone(), view.pages[..view.current].iter().filter(|p| !p.master && p.name == page.name).count())
        });
        self.end_text_edit(window, cx);
        let merged = self.merge_master_page(&view).and_then(|bytes| ibis_pptx::open(bytes).map_err(|e| e.to_string()));
        let (opened, view) = match merged {
            Ok(opened) => (opened, view),
            Err(err) => {
                self.set_master_view(view);
                return self.set_status(crate::shell::i18n::format("pptx.master-view.master-could-not-saved-error", &[("error", &err)]), cx);
            }
        };
        let name = view.file_name.clone().unwrap_or_default();
        let flags = self.view_aids();
        self.load(opened.presentation, name, window, cx);
        self.restore_view_aids(flags);
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
        if let Some((name, ordinal)) = wanted
            && let Some(layout) = self.presentation().layouts.iter().filter(|l| l.name == name).nth(ordinal).map(|l| l.id)
        {
            self.apply_layout(layout, window, cx);
        }
        self.set_status(crate::shell::i18n::text("pptx.master-view.slide-master-view-closed-undo").to_string(), cx);
        cx.notify();
    }

    /// Context menu entries for page `index` in Slide Master View (PPTX-MASTER-001..005).
    pub(crate) fn master_page_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some(view) = self.master_view() else { return Vec::new() };
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
            MenuItem::tr("pptx.master-view.apply-to-slide").icon("check").disabled(page.master).on(move |e, w, cx| {
                e.switch_master_page(index, w, cx);
                e.apply_master_page_and_close(w, cx);
            }).into(),
            MenuItem::tr("pptx.master-view.duplicate-layout").icon("content_copy").disabled(page.master).on(move |e, w, cx| {
                e.switch_master_page(index, w, cx);
                e.duplicate_master_layout(w, cx);
            }).into(),
            MenuItem::tr("pptx.master-view.delete-layout").icon("delete").disabled(page.master).on(move |e, w, cx| {
                e.switch_master_page(index, w, cx);
                e.delete_master_layout(w, cx);
            }).into(),
            Submenu::tr("pptx.background", "wallpaper", crate::chrome::background_menu(self)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.master-view.close-master-view")
                .icon("close")
                .on(|e, w, cx| e.close_master_view(w, cx))
                .into(),
        ]
    }

    /// Open context menu for page `index` in Slide Master View at pointer `position`.
    pub(crate) fn open_master_page_context_menu(&mut self, index: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.master_view().is_none_or(|v| index >= v.pages.len()) {
            return;
        }
        self.switch_master_page(index, window, cx);
        let entries = self.master_page_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    /// The pictures of the pages not made yet (the page shown is drawn from the live document): for the list on the left.
    pub(crate) fn fill_master_thumbs(&mut self, cx: &mut Context<Self>) {
        let timing = crate::clock::debug_timing::start();
        let mut made = 0;
        if let Some(view) = MasterView::of_mut(&mut *self.format) {
            for i in 0..view.pages.len() {
                if i != view.current && view.thumbs.get(i).is_some_and(Option::is_none) {
                    let opened = master_page::build(&view.base, &view.pages[i].part).map_err(|e| e.to_string()).and_then(|temp| ibis_pptx::open(temp).map_err(|e| e.to_string()));
                    if let Ok(opened) = opened && let Some(slide) = opened.presentation.slides.first().cloned() {
                        let size = opened.presentation.size_of(&slide);
                        let resources = crate::render::shape_view::Resources::new(&opened.presentation, cx);
                        view.thumbs[i] = Some(std::rc::Rc::new(PageThumb { slide, size, resources }));
                        made += 1;
                    }
                }
            }
        }
        if made > 0 {
            crate::clock::debug_timing::report(&format!("master list: {made} pictures made"), timing);
        }
    }

    /// The pages of the file in the list on the left (instead of the slides): each slide master as a picture with its
    /// number, its layouts under it, indented and smaller. The page shown is framed; arrows move it; a right click opens
    /// the page's menu.
    pub(crate) fn render_master_page_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(view) = self.master_view() else { return Vec::new() };
        let t = crate::shell::i18n::text;
        let master_w = self.thumbnail_width();
        let layout_w = master_w * LAYOUT_THUMB_RATIO;
        let mut master_no = 0usize;
        let mut items: Vec<AnyElement> = Vec::new();
        for (i, page) in view.pages.iter().enumerate() {
            let name = if page.name.is_empty() { t("pptx.chrome.no-name").to_string() } else { page.name.clone() };
            let label: SharedString = if page.master { crate::shell::i18n::format("pptx.master-view.slide-master-name", &[("name", &name)]).into() } else { name.into() };
            let selected = i == view.current;
            let width = if page.master { master_w } else { layout_w };
            let inner = width - 6.;
            let picture = if selected {
                page_picture(self.slide(), self.presentation().size_of(self.slide()), &self.resources, inner)
            } else if let Some(Some(thumb)) = view.thumbs.get(i) {
                page_picture(&thumb.slide, thumb.size, &thumb.resources, inner)
            } else {
                div().w(px(inner)).h(px(inner * 9. / 16.)).flex_none().bg(gpui_kit::white())
            };
            let number = page.master.then(|| {
                master_no += 1;
                div().w(px(MASTER_NUMBER_W)).flex_none().text_size(px(12.)).text_right().child(SharedString::from(master_no.to_string()))
            });
            let tip = label.clone();
            let frame = if selected { crate::shell::chrome::PRIMARY_BUTTON_BG } else { 0xc8c8c8 };
            let card = div().id(ElementId::Name(format!("pptx-master-page-{i}").into()))
                .w(px(width)).flex_none().p(px(2.)).rounded(px(3.)).cursor_pointer().bg(rgb(0xffffff))
                .border(px(if selected { 2. } else { 1. })).border_color(rgb(frame))
                .hover(|d| d.border_color(rgb(crate::shell::chrome::PRIMARY_BUTTON_BG)))
                .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
                .aria_label(label.clone())
                .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                    cx.stop_propagation();
                    e.open_master_page_context_menu(i, event.position, w, cx);
                }))
                .on_click(cx.listener(move |e, _, w, cx| {
                    if let Some(v) = e.master_view() { v.grid_focus.focus(w, cx); }
                    e.switch_master_page(i, w, cx);
                    if let Some(v) = e.master_view() { v.grid_focus.focus(w, cx); }
                }))
                .child(picture)
                // The name of the page under its picture (a long name is cut; the tooltip has all of it).
                .child(div().w_full().mt(px(2.)).text_size(px(11.)).text_center().truncate().child(label));
            // A layout is set in by the width of the master's number, so that it stands under its master's picture.
            let row = h_flex().items_start().gap_2().px_2().py(px(3.))
                .when(!page.master, |d| d.pl(px(8. + MASTER_NUMBER_W + LAYOUT_INDENT)))
                .children(number)
                .child(card);
            items.push(row.into_any_element());
        }
        items
    }

    pub fn master_view_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let view = self.master_view()?;
        let t = crate::shell::i18n::text;
        let mut body = v_flex().gap_1().px(px(12.)).py(px(8.)).p_0();
        // 名前の変更・レイアウトの複製と削除: of the page shown.
        let is_layout = !view.pages[view.current].master;
        body = body
            .child(div().mt_2().text_sm().font_weight(FontWeight::BOLD).child(t("_shell.name")))
            .child(crate::shell::chrome::text_field(&view.name).full_width())
            .child(
                h_flex().gap_1().flex_wrap()
                    .child(crate::shell::chrome::action_button("pptx-master-rename").small().label(t("pptx.master-view.rename")).on_click(cx.listener(|e, _, w, cx| e.rename_master_page(w, cx))))
                    .child(crate::shell::chrome::action_button("pptx-master-duplicate-layout").small().label(t("pptx.master-view.duplicate-layout")).disabled(!is_layout).on_click(cx.listener(|e, _, w, cx| e.duplicate_master_layout(w, cx))))
                    .child(crate::shell::chrome::action_button("pptx-master-delete-layout").small().label(t("pptx.master-view.delete-layout")).disabled(!is_layout).on_click(cx.listener(|e, _, w, cx| e.delete_master_layout(w, cx)))),
            );
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
            .child(crate::shell::chrome::text_field(&style.size).full_width().unit("pt", px(20.)).spinner("pptx-master-style-size", crate::shell::chrome::SpinnerSpec::new(1.0).steps(FONT_SIZE_STEPS).range(1.0, 4000.0), cx, |_, _, _, _| {}))
            .child(Checkbox::new("pptx-master-style-bold").label(t("_shell.bold")).checked(bold).on_click(cx.listener(|e, on: &bool, _, cx| e.toggle_master_style_bold(*on, cx))))
            .child(crate::shell::chrome::panel_color_field("master-color",&style.color,cx))
            .child(crate::shell::chrome::panel_font_dropdown("panel-font-master",&style.font,cx))
            .child(crate::shell::chrome::primary_button("pptx-master-style-apply").small().label(t("pptx.master-view.apply-style")).on_click(cx.listener(|e, _, w, cx| e.apply_master_style(w, cx))));
        let panel_terminal_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-master-apply", t("pptx.master-view.apply-to-slide"), crate::shell::chrome::ActionButtonKind::Primary).disabled(!is_layout).on_click(cx.listener(|e, _, w, cx| e.apply_master_page_and_close(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-master-close", t("pptx.master-view.close-master-view"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_master_view(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.master-view.slide-master"), body.into_any_element(), panel_terminal_actions))
    }
}

/// The sizes of PowerPoint's font size list, which the ▲▼ of the text style's size step through (applied with 文字の書式を適用).
const FONT_SIZE_STEPS: &[f32] = &[8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0, 72.0, 80.0, 88.0, 96.0];
