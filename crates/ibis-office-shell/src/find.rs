// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pptx text search and replacement, including table cells and run boundaries.
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Command, Presentation, ShapeId, ShapeKind, SlideId, TextBody};
use crate::shape_state::ContextPopup;
use crate::chrome::{MenuEntry, MenuItem, SidebarPanel};
use crate::editor::SlideEditor;
use crate::format::with_format;

actions!(
    pptx,
    [
        OpenFind,
        /// 次を検索・前を検索（macOS の ⌘G・⇧⌘G）.
        FindNext,
        FindPrevious,
    ]
);

/// Key context of the find panel (the keys work with its fields focused).
pub const FIND_PANEL_CONTEXT: &str = "PresentationFindPanel";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub(crate) slide: SlideId,
    pub(crate) shape: ShapeId,
    pub(crate) cell: Option<(usize, usize)>,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub struct FindState {
    pub replace_mode: bool,
    pub query: Entity<InputState>,
    pub replacement: Entity<InputState>,
    pub current: Option<usize>,
    pub match_case: bool,
    pub whole_word: bool,
    _subscription: Subscription,
}

fn text_ranges(text: &str, query: &str, match_case: bool, whole_word: bool) -> Vec<(usize, usize)> {
    let chars: Vec<_> = text.chars().collect();
    let needle: Vec<_> = query.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() { return Vec::new(); }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut ranges = Vec::new();
    let mut at = 0;
    while at + needle.len() <= chars.len() {
        let end = at + needle.len();
        let equal = chars[at..end].iter().zip(&needle).all(|(&a, &b)| if match_case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) });
        let boundary = !whole_word || ((!at.checked_sub(1).is_some_and(|i| word(chars[i]))) && (end == chars.len() || !word(chars[end])));
        if equal && boundary { ranges.push((at, end)); at = end; } else { at += 1; }
    }
    ranges
}

fn matches(presentation: &Presentation, query: &str, match_case: bool, whole_word: bool) -> Vec<Match> {
    if query.is_empty() { return Vec::new(); }
    let mut found = Vec::new();
    let mut body_matches = |slide, shape, cell, body: &TextBody| {
        let text = body.text();
        for (start, end) in text_ranges(&text, query, match_case, whole_word) {
            // Generated fields (date/slide number) are not replaceable plain text.
            let mut pos = 0;
            let mut field = false;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let next = pos + r.text.chars().count();
                    field |= r.style.field.is_some() && pos < end && next > start;
                    pos = next;
                }
                pos += 1;
            }
            if !field { found.push(Match { slide, shape, cell, start, end }); }
        }
    };
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() { body_matches(slide.id, shape.id, None, body); }
            if let ShapeKind::Table(table) = &shape.kind {
                for (row, r) in table.rows.iter().enumerate() {
                    for (column, cell) in r.cells.iter().enumerate().filter(|(_, c)| !c.merged) {
                        body_matches(slide.id, shape.id, Some((row, column)), &cell.text);
                    }
                }
            }
        }
    }
    found
}

pub(crate) fn replacement(hit: Match, text: &str) -> Command {
    let command = Command::ReplaceText { slide: hit.slide, shape: hit.shape, at: hit.start, remove: hit.end - hit.start, insert: text.into() };
    match hit.cell { Some((row, column)) => Command::InCell { row, column, command: Box::new(command) }, None => command }
}

impl SlideEditor {
    pub fn open_pptx_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_pptx_find_with(true, window, cx);
    }

    /// ⌘G / ⇧⌘G: the next / previous match. With the panel closed it opens, with the text last typed there.
    pub fn pptx_find_shortcut(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.uses_shape_edit() && self.shape_edit.find.is_none() {
            let query = self.last_find_query.clone();
            self.open_pptx_find_with(false, window, cx);
            if query.is_empty() { return; }
            if let Some(input) = self.shape_edit.find.as_ref().map(|f| f.query.clone()) {
                input.update(cx, |q, cx| q.set_value(query, window, cx));
            }
        }
        self.pptx_find_next(forward, window, cx);
    }

    /// The text selected in the text being edited (one line), for the find field.
    fn pptx_selected_line(&self) -> Option<String> {
        let edit = self.text_editing()?;
        let (a, b) = (edit.anchor.min(edit.caret), edit.anchor.max(edit.caret));
        if a == b { return None; }
        let body = self.edited_body()?;
        let text = body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n");
        let picked: String = text.chars().skip(a).take(b - a).collect();
        (!picked.is_empty() && !picked.contains('\n')).then_some(picked)
    }

    /// `selection_to_query`: the selected text goes into the field, as in PowerPoint's ⌘F.
    fn open_pptx_find_with(&mut self, selection_to_query: bool, window: &mut Window, cx: &mut Context<Self>) {
        let prefill = if selection_to_query { self.pptx_selected_line() } else { None };
        self.end_text_edit(window, cx);
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None; self.shape_edit.insets_panel = None;
        self.shape_edit.crop_panel = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None;
        if self.uses_shape_edit() { with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Find)); self.shape_edit.chart_panel = None; }
        if self.uses_shape_edit() && self.shape_edit.find.is_none() {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("_shell.find-what")));
            let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("_shell.replace-with")));
            let subscription = cx.subscribe_in(&query, window, |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    // The field also says "changed" when it takes the keys back after a match was shown: the same text.
                    let text = input.read(cx).value().to_string();
                    if text == this.last_find_query { return; }
                    this.last_find_query = text;
                    if let Some(f) = this.shape_edit.find.as_mut() { f.current = None; }
                    cx.notify();
                }
                // Enter is the Next button, Shift+Enter the Previous one.
                InputEvent::PressEnter { shift, .. } => {
                    this.pptx_find_next(!*shift, window, cx);
                    // Showing the match edits its text; the field keeps the keys for the next Enter.
                    input.update(cx, |q, cx| q.focus(window, cx));
                }
                _ => {}
            });
            if self.uses_shape_edit() { self.shape_edit.find = Some(FindState { replace_mode: false, query, replacement, current: None, match_case: true, whole_word: false, _subscription: subscription }); }
        }
        self.sidebar_open = true;
        if let Some(query) = self.shape_edit.find.as_ref().map(|f| f.query.clone()) {
            if let Some(text) = prefill {
                query.update(cx, |q, cx| q.set_value(text.clone(), window, cx));
                self.last_find_query = text;
                if let Some(f) = self.shape_edit.find.as_mut() { f.current = None; }
            }
            query.update(cx, |q, cx| { q.focus(window, cx); q.select_all(window, cx); });
        }
        cx.notify();
    }

    pub fn toggle_pptx_find_option(&mut self, case: bool, cx: &mut Context<Self>) {
        if let Some(f) = self.shape_edit.find.as_mut() {
            if case { f.match_case = !f.match_case; } else { f.whole_word = !f.whole_word; }
            f.current = None;
        }
        cx.notify();
    }

    pub fn pptx_find_matches(&self, cx: &App) -> Vec<Match> {
        self.shape_edit.find.as_ref().map(|f| matches(self.presentation(), f.query.read(cx).value().as_ref(), f.match_case, f.whole_word)).unwrap_or_default()
    }

    pub(crate) fn pptx_find_goto(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let hits = self.pptx_find_matches(cx);
        let Some(&hit) = hits.get(index) else { return };
        self.select_slide(hit.slide, window, cx);
        match hit.cell { Some(cell) => self.begin_table_cell_edit(hit.shape, cell, window, cx), None => self.begin_text_edit(hit.shape, false, false, window, cx) }
        if let Some(edit) = self.shape_edit.edit.as_mut() { edit.anchor = hit.start; edit.caret = hit.end; }
        if self.uses_shape_edit() {
            if let Some(f) = self.shape_edit.find.as_mut() { f.current = Some(index); }
        }
        cx.notify();
    }

    pub fn pptx_find_next(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let hits = self.pptx_find_matches(cx);
        if hits.is_empty() { return; }
        let current = self.shape_edit.find.as_ref().and_then(|f| f.current);
        let index = match current { Some(i) if forward => (i + 1) % hits.len(), Some(i) => (i + hits.len() - 1) % hits.len(), None if forward => 0, None => hits.len() - 1 };
        self.pptx_find_goto(index, window, cx);
    }

    pub(crate) fn pptx_replace_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let hits = self.pptx_find_matches(cx);
        let Some(&hit) = hits.get(index) else { return };
        let Some(f) = self.shape_edit.find.as_ref() else { return };
        let text = f.replacement.read(cx).value().to_string();
        self.run(vec![replacement(hit, &text)], cx);
        if let Some(f) = self.shape_edit.find.as_mut() { f.current = None; }
        cx.notify();
    }

    pub fn pptx_replace(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let hits = self.pptx_find_matches(cx);
        let Some(f) = self.shape_edit.find.as_ref() else { return };
        let text = f.replacement.read(cx).value().to_string();
        let commands = if all {
            // Work backwards within every body: earlier offsets stay valid.
            hits.into_iter().rev().map(|hit| replacement(hit, &text)).collect()
        } else {
            hits.get(f.current.unwrap_or(0)).copied().map(|hit| vec![replacement(hit, &text)]).unwrap_or_default()
        };
        self.run(commands, cx);
        if let Some(f) = self.shape_edit.find.as_mut() { f.current = None; }
        cx.notify();
    }

    /// Context menu entries for the Find & Replace panel or a specific match row.
    pub fn find_panel_context_menu_entries(&self, hit_index: Option<usize>, cx: &App) -> Vec<MenuEntry> {
        let hits = self.pptx_find_matches(cx);
        let has_hits = !hits.is_empty();
        let (match_case, whole_word) = self.shape_edit.find.as_ref().map_or((true, false), |f| (f.match_case, f.whole_word));
        let mut entries = Vec::new();
        if let Some(idx) = hit_index.filter(|&i| i < hits.len()) {
            entries.push(
                MenuItem::tr("_shell.view")
                    .icon("visibility")
                    .on(move |e, w, cx| e.pptx_find_goto(idx, w, cx))
                    .into(),
            );
            entries.push(
                MenuItem::tr("_shell.replace")
                    .icon("find_replace")
                    .on(move |e, w, cx| e.pptx_replace_at(idx, w, cx))
                    .into(),
            );
            entries.push(MenuEntry::Separator);
        }
        entries.push(
            MenuItem::tr("_shell.next")
                .icon("arrow_downward")
                .disabled(!has_hits)
                .on(|e, w, cx| e.pptx_find_next(true, w, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("_shell.previous")
                .icon("arrow_upward")
                .disabled(!has_hits)
                .on(|e, w, cx| e.pptx_find_next(false, w, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("_shell.replace-all")
                .icon("done_all")
                .disabled(!has_hits)
                .on(|e, w, cx| e.pptx_replace(true, w, cx))
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("pptx.find.match-case")
                .checked(match_case)
                .on(|e, _, cx| e.toggle_pptx_find_option(true, cx))
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.find.whole-words")
                .checked(whole_word)
                .on(|e, _, cx| e.toggle_pptx_find_option(false, cx))
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| {
                    if e.uses_shape_edit() {
                        e.shape_edit.find = None;
                    }
                    cx.notify();
                })
                .into(),
        );
        entries
    }

    pub(crate) fn open_find_panel_context_menu(&mut self, hit_index: Option<usize>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.find_panel_context_menu_entries(hit_index, cx));
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

    pub fn pptx_find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let f = self.shape_edit.find.as_ref()?;
        let (query, text, current) = (f.query.clone(), f.replacement.clone(), f.current);
        let (match_case, whole_word) = (f.match_case, f.whole_word);
        let replace_mode = f.replace_mode;
        let hits = self.pptx_find_matches(cx);
        let n = hits.len();
        let status = current.filter(|&i| i < n).map_or_else(|| crate::i18n::format("docx.find.n-results", &[("n", &n)]), |i| crate::i18n::format("_pptx.n-of-total", &[("n", &(i + 1)), ("total", &n)]));
        let theme = cx.theme();
        let mut match_list = v_flex().gap_1();
        for (idx, &hit) in hits.iter().enumerate().take(25) {
            let slide_num = self.presentation().slides.iter().position(|s| s.id == hit.slide).map(|i| i + 1).unwrap_or(1);
            let snippet = self
                .presentation()
                .slide(hit.slide)
                .and_then(|s| s.shape(hit.shape))
                .and_then(|shape| match (hit.cell, &shape.kind) {
                    (Some((r, c)), ShapeKind::Table(t)) => t.rows.get(r).and_then(|row| row.cells.get(c)).map(|cell| &cell.text),
                    _ => shape.text_body(),
                })
                .map(|body| {
                    let chars: Vec<char> = body.text().chars().collect();
                    let start = hit.start.saturating_sub(10);
                    let end = (hit.end + 10).min(chars.len());
                    chars[start..end].iter().collect::<String>().replace('\n', " ")
                })
                .unwrap_or_default();
            let selected = current == Some(idx);
            let row = h_flex()
                .id(("pptx-find-match", idx))
                .gap_2()
                .items_center()
                .px(px(6.))
                .py(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_color(if selected { Hsla::from(rgb(0x2f6db5)) } else { theme.border })
                .bg(if selected { Hsla::from(rgb(0x2f6db5)).opacity(0.12) } else { theme.title_bar_border.opacity(0.08) })
                .cursor_pointer()
                .on_click(cx.listener(move |e, _, w, cx| e.pptx_find_goto(idx, w, cx)))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        e.open_find_panel_context_menu(Some(idx), event.position, w, cx);
                    }),
                )
                .child(div().text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(theme.muted_foreground).child(SharedString::from(format!("#{slide_num}"))))
                .child(div().flex_1().text_sm().text_color(theme.foreground).truncate().child(SharedString::from(snippet)));
            match_list = match_list.child(row);
        }
        let body = v_flex()
            .id("pptx-find-panel")
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|e, event: &MouseDownEvent, w, cx| {
                    e.open_find_panel_context_menu(None, event.position, w, cx);
                }),
            )
            .child(crate::chrome::radio_group().layout(crate::chrome::RadioGroupLayout::Flow)
                .child(crate::chrome::radio_button("pptx-find-mode-search", !replace_mode, false, crate::i18n::text("_shell.find"), cx.listener(|this, _, _, cx| { if let Some(f) = this.shape_edit.find.as_mut() { f.replace_mode = false; } cx.notify(); })))
                .child(crate::chrome::radio_button("pptx-find-mode-replace", replace_mode, false, crate::i18n::text("_shell.replace"), cx.listener(|this, _, _, cx| { if let Some(f) = this.shape_edit.find.as_mut() { f.replace_mode = true; } cx.notify(); }))))
            .child(crate::chrome::text_field(&query).full_width().select_on_focus())
            .child(crate::chrome::checkbox_button("pptx-find-case", crate::i18n::text("_shell.match-case"), match_case, false, cx.listener(|e, _, _, cx| e.toggle_pptx_find_option(true, cx))))
            .child(crate::chrome::checkbox_button("pptx-find-word", crate::i18n::text("pptx.chrome.find-whole-words-only"), whole_word, false, cx.listener(|e, _, _, cx| e.toggle_pptx_find_option(false, cx))))
            .when(replace_mode, |d| d.child(crate::chrome::text_field(&text).full_width().select_on_focus()))
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::chrome::find_nav_button("pptx-find-previous", crate::chrome::FindNav::Previous).on_click(cx.listener(|e, _, w, cx| e.pptx_find_next(false, w, cx))))
                .child(crate::chrome::find_nav_button("pptx-find-next", crate::chrome::FindNav::Next).on_click(cx.listener(|e, _, w, cx| e.pptx_find_next(true, w, cx))))
                .when(replace_mode, |d| d
                    .child(crate::chrome::action_button("pptx-replace-one").small().label(crate::i18n::text("_shell.replace")).on_click(cx.listener(|e, _, w, cx| e.pptx_replace(false, w, cx))))
                    .child(crate::chrome::action_button("pptx-replace-all").small().label(crate::i18n::text("_shell.replace-all")).on_click(cx.listener(|e, _, w, cx| e.pptx_replace(true, w, cx))))))
            .child(div().text_sm().child(status))
            .children((!hits.is_empty()).then_some(match_list));
        // The keys of the presentation's find also work with the focus in the panel's fields.
        let body = body
            .key_context(FIND_PANEL_CONTEXT)
            .on_action(cx.listener(|this, _: &OpenFind, window, cx| this.open_pptx_find(window, cx)))
            .on_action(cx.listener(|this, _: &FindNext, window, cx| this.pptx_find_shortcut(true, window, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, window, cx| this.pptx_find_shortcut(false, window, cx)));
        Some(SidebarPanel { title: crate::i18n::text("_shell.find-and-replace").into(), body: body.into_any_element() })
    }
}
