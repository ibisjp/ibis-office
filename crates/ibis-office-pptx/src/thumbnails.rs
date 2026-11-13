// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The slide list on the left: numbered thumbnails, click to open a slide.

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::render::shape_view::{shape_element, slide_backdrop};
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::ext::*;

/// A slide being dragged in the list, to be dropped at another place.
#[derive(Clone)]
pub(crate) struct DraggedSlide {
    pub(crate) slide: ibis_doc_model::SlideId,
    pub(crate) number: usize,
}

/// What follows the pointer while a slide is dragged: its number.
struct DraggedSlideView(usize);

impl Render for DraggedSlideView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(theme.primary)
            .text_color(theme.primary_foreground)
            .text_xs()
            .child(crate::shell::i18n::format("pptx.grid-view.slide-n", &[("n", &self.0)]))
    }
}

pub(crate) const PANE_WIDTH: f32 = 216.0;
const SECTION_HEADER_HEIGHT: f32 = 24.0;

/// Extract title and body bullet paragraphs from a slide for Outline View.
#[allow(dead_code)]
fn slide_outline_text(slide: &ibis_doc_model::Slide) -> (Option<String>, Vec<(usize, String)>) {
    let mut texts_with_y: Vec<(i64, Vec<(usize, String)>)> = Vec::new();

    for shape in &slide.shapes {
        if let Some(tb) = shape.text_body() {
            let mut paras = Vec::new();
            for p in &tb.paragraphs {
                let pt = p.text().trim().to_string();
                if !pt.is_empty() {
                    paras.push((p.level as usize, pt));
                }
            }
            if !paras.is_empty() {
                texts_with_y.push((shape.frame.y, paras));
            }
        }
    }

    texts_with_y.sort_by_key(|(y, _)| *y);

    let mut title = None;
    let mut body_paragraphs = Vec::new();

    for (_, paras) in texts_with_y {
        let mut paras_iter = paras.into_iter();
        if title.is_none() {
            if let Some((_, first_p)) = paras_iter.next() {
                title = Some(first_p);
            }
        }
        for (lvl, text) in paras_iter {
            body_paragraphs.push((lvl, text));
        }
    }

    (title, body_paragraphs)
}

#[ibis_office_macros::editor_ext(Thumbnails)]
impl SlideEditor {
    fn render_thumbnail_card(
        &self,
        slide: &ibis_doc_model::Slide,
        index: usize,
        curr_y: &mut f32,
        visible_top: f32,
        visible_bottom: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let size = self.presentation().size_of(slide);
        let thumb_w = self.thumbnail_width();
        let scale = (thumb_w / size.width.max(1) as f32).min(thumb_w / size.height.max(1) as f32);
        let (width, height) = (size.width as f32 * scale, size.height as f32 * scale);
        let item_h = height + 12.0;
        let item_top = *curr_y;
        let item_bottom = *curr_y + item_h;
        *curr_y += item_h;

        let id = slide.id;
        let current = id == self.current_slide;
        let chosen = current || self.pptx().is_some_and(|st| st.slide_selection.contains(&id));
        let in_view = item_bottom >= visible_top && item_top <= visible_bottom;
        let should_render = in_view || current;

        let ready = should_render && self.resources.preload.slide_ready(slide);
        let shapes: Vec<AnyElement> = if ready {
            self.resources.set_wrap(slide);
            slide
                .shown_shapes()
                .map(|shape| shape_element(shape, shape.frame, scale, &self.resources, None))
                .collect()
        } else {
            Vec::new()
        };

        h_flex()
            .id(("slide-thumbnail", id.0))
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if let Some(s) = this.pptx_mut() {
                    s.section_selection = None;
                }
                this.list_focus.focus(window, cx);
                this.click_slide(id, &event.modifiers(), window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    // A slide of the selection keeps it (the menu acts on all of them).
                    if !this.selected_slide_ids().contains(&id) {
                        this.select_slide(id, window, cx);
                    }
                    this.open_slide_context_menu(id, event.position, window, cx);
                }),
            )
            .when(self.pages_are_slides(), |d| {
                d.on_drag(DraggedSlide { slide: id, number: index + 1 }, |dragged, _, _, cx| {
                    cx.new(|_| DraggedSlideView(dragged.number))
                })
                .drag_over::<DraggedSlide>(move |style, _, _, cx| {
                    style.border_color(cx.theme().primary).bg(cx.theme().accent)
                })
                .on_drop(cx.listener(move |this, dragged: &DraggedSlide, window, cx| {
                    // A slide of several selected: they all go.
                    let chosen = this.selected_slide_ids();
                    if chosen.len() > 1 && chosen.contains(&dragged.slide) {
                        return this.move_selected_slides_to(id, window, cx);
                    }
                    if dragged.slide != id {
                        this.run(
                            vec![ibis_doc_model::Command::MoveSlide { slide: dragged.slide, index }],
                            cx,
                        );
                        this.select_slide(dragged.slide, window, cx);
                    }
                }))
            })
            .child(
                v_flex()
                    .w(px(18.))
                    .items_end()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_right()
                            .text_color(if chosen { theme.foreground } else { theme.muted_foreground })
                            .when(slide.hidden, |d| d.line_through())
                            .child(SharedString::from((index + 1).to_string())),
                    )
                    .when(!slide.comments.is_empty(), |d| {
                        d.child(
                            Icon::new(Material("chat_bubble"))
                                .size(px(14.))
                                .text_color(rgb(0xf9ab00)),
                        )
                    }),
            )
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .border_2()
                    .border_color(if chosen { theme.primary } else { theme.border })
                    .rounded_sm()
                    .bg(gpui_kit::white())
                    .when(ready, |d| d.child(slide_backdrop(slide, scale, &self.resources)))
                    .children(shapes)
                    .when(slide.hidden, |d| d.opacity(0.45)),
            )
            .into_any_element()
    }

    fn render_thumbnail_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let accent = cx.theme().accent;
        let max_y = f32::from(self.list_scroll.max_offset().y);
        let scroll_y = (-f32::from(self.list_scroll.offset().y)).clamp(0.0, max_y.max(0.0));
        let view_h = self.workspace_bounds.map(|b| f32::from(b.size.height)).unwrap_or(720.0);
        let visible_top = scroll_y - 300.0;
        let visible_bottom = scroll_y + view_h + 300.0;
        let mut curr_y = 0.0f32;

        let has_sections = !self.presentation().sections.is_empty()
            && self.presentation().valid_sections(&self.presentation().sections);

        if has_sections {
            let mut rows = Vec::new();
            let mut slide_idx = 0;
            for (index, section) in self.presentation().sections.iter().enumerate() {
                let folded = self.pptx().is_some_and(|s| s.folded_sections.contains(&section.id));
                let toggle_id = section.id.clone();
                let select_id = section.id.clone();
                let menu_id = section.id.clone();
                rows.push(
                    h_flex()
                        .id(("pptx-section-header", index))
                        .gap_2()
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .when(self.current_section() == Some(index), move |d| d.bg(accent))
                        .on_click(cx.listener(move |this, _, w, cx| this.select_section(&select_id, w, cx)))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                                cx.stop_propagation();
                                this.open_section_context_menu(&menu_id, event.position, w, cx);
                            }),
                        )
                        .child(
                            div()
                                .id(("pptx-section-fold", index))
                                .child(if folded { "▸" } else { "▾" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_section_fold(&toggle_id, cx);
                                })),
                        )
                        .child(div().text_xs().child(format!("{} ({})", section.name, section.slides.len())))
                        .into_any_element(),
                );
                curr_y += SECTION_HEADER_HEIGHT;

                for &slide_id in &section.slides {
                    if let Some(slide) = self.presentation().slide(slide_id) {
                        if !folded {
                            rows.push(self.render_thumbnail_card(
                                slide,
                                slide_idx,
                                &mut curr_y,
                                visible_top,
                                visible_bottom,
                                cx,
                            ));
                        }
                    }
                    slide_idx += 1;
                }
            }
            rows
        } else {
            self.presentation()
                .slides
                .iter()
                .enumerate()
                .map(|(index, slide)| {
                    self.render_thumbnail_card(
                        slide,
                        index,
                        &mut curr_y,
                        visible_top,
                        visible_bottom,
                        cx,
                    )
                })
                .collect()
        }
    }

    /// Returns the row index in the thumbnail list corresponding to `slide_index`.
    /// When sections are present, section header rows shift the row indices,
    /// and folded sections hide slides.
    pub fn row_of_slide(&self, slide_index: usize) -> usize {
        let pres = self.presentation();
        let Some(target_slide) = pres.slides.get(slide_index) else {
            return slide_index;
        };
        let target_id = target_slide.id;
        if pres.sections.is_empty() || !pres.valid_sections(&pres.sections) {
            return slide_index;
        }
        let mut row = 0;
        for section in &pres.sections {
            let folded = self.pptx().is_some_and(|s| s.folded_sections.contains(&section.id));
            let header_row = row;
            row += 1;
            for &s_id in &section.slides {
                if s_id == target_id {
                    return if folded { header_row } else { row };
                }
                if !folded {
                    row += 1;
                }
            }
        }
        slide_index
    }

    /// The outline: each slide's title (with its number) and its body's paragraphs as lines; the line being edited shows
    /// its caret and selection (`pptx/outline_edit.rs`), a click puts the caret in a line.
    fn render_outline_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use crate::outline_edit::LineKind;
        let theme = cx.theme();
        let edit = self.outline_edit().cloned();
        let lines = self.outline_lines();
        let mut number = 0usize;
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let slide = self.presentation().slide(line.slide);
                let current = line.slide == self.current_slide;
                let title = line.kind == LineKind::Title;
                if title {
                    number += 1;
                }
                let hidden = slide.is_some_and(|s| s.hidden);
                let editing = edit.as_ref().filter(|e| e.line == i);
                let shown: SharedString = if title && line.text.is_empty() && editing.is_none() { crate::shell::i18n::text("pptx.outline.untitled") } else { line.text.clone().into() };
                let color = if title { theme.foreground } else { theme.muted_foreground };
                // What of the line is selected (the selection may run over several lines), and where its caret is.
                let length = line.text.chars().count();
                let span = edit.as_ref().and_then(|e| e.span());
                let selection: Option<std::ops::Range<usize>> = span.and_then(|((l1, o1), (l2, o2))| {
                    if i < l1 || i > l2 {
                        return None;
                    }
                    Some(if l1 == l2 { o1..o2 } else if i == l1 { o1..length } else if i == l2 { 0..o2 } else { 0..length })
                });
                let caret_here = editing.is_some() && span.is_none();
                let content: AnyElement = if editing.is_some() || selection.is_some() {
                    let chars: Vec<char> = line.text.chars().collect();
                    let r = selection.clone().unwrap_or_else(|| editing.map_or(0..0, |e| e.range()));
                    let cut = |a: usize, b: usize| -> String { chars[a.min(chars.len())..b.min(chars.len())].iter().collect() };
                    let mut selected = theme.primary;
                    selected.a = 0.25;
                    let marked = editing.and_then(|e| e.marked.clone()).unwrap_or_default();
                    let prefix = cut(0, r.start);
                    let sel_str = cut(r.start, r.end);
                    let suffix = cut(r.end, chars.len());
                    let full_text = if !marked.is_empty() {
                        format!("{prefix}{marked}{suffix}")
                    } else {
                        line.text.clone()
                    };
                    let sel_bytes = (!r.is_empty() && marked.is_empty()).then(|| prefix.len()..(prefix.len() + sel_str.len()));
                    // Each clause of the composing text underlined as the input method does (`render::ime_text`), as (style, bytes of the line).
                    let marked_underlines: Vec<(ibis_doc_model::UnderlineStyle, std::ops::Range<usize>)> = {
                        let at: Vec<usize> = marked.char_indices().map(|(i, _)| i).chain(std::iter::once(marked.len())).collect();
                        let clauses = editing.map(|e| e.marked_clauses.clone()).unwrap_or_default();
                        crate::render::ime_text::spans(&marked, &clauses).into_iter().map(|s| (s.style(), prefix.len() + at[s.range.start]..prefix.len() + at[s.range.end])).collect()
                    };
                    let caret_byte = (caret_here && r.is_empty() && marked.is_empty()).then_some(prefix.len());
                    let font_size = if title { 13.0 } else { 12.0 };
                    let fg = theme.foreground;
                    let overlay_text = full_text.clone();
                    h_flex()
                        .relative()
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(div().child(SharedString::from(full_text)))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    let shaped = if overlay_text.is_empty() {
                                        None
                                    } else {
                                        let mut f = window.text_style().font();
                                        if title {
                                            f.weight = FontWeight::BOLD;
                                        }
                                        let run = TextRun {
                                            len: overlay_text.len(),
                                            font: f,
                                            color: fg,
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        };
                                        Some(window.text_system().shape_line(SharedString::from(overlay_text.clone()), px(font_size), &[run], None))
                                    };
                                    let x_at = |b: usize| -> Pixels {
                                        shaped.as_ref().map_or(px(0.), |s| s.x_for_index(b.min(overlay_text.len())))
                                    };
                                    let h = px(14.).min(bounds.size.height);
                                    let top = bounds.origin.y + (bounds.size.height - h) * 0.5;
                                    if let Some(sr) = &sel_bytes {
                                        let x0 = x_at(sr.start);
                                        let x1 = x_at(sr.end);
                                        if x1 > x0 {
                                            window.paint_quad(fill(
                                                Bounds::new(point(bounds.origin.x + x0, top), gpui_kit::size(x1 - x0, h)),
                                                selected,
                                            ));
                                        }
                                    }
                                    for (style, mr) in &marked_underlines {
                                        let x0 = x_at(mr.start);
                                        let x1 = x_at(mr.end);
                                        if x1 > x0 {
                                            // The text is `font_size` pixels (a point is 4/3 pixel), centred in the line.
                                            let k = 4.0 / 3.0;
                                            let baseline = shaped.as_ref().map_or(f32::from(h), |s| f32::from((h - (s.ascent + s.descent)) * 0.5 + s.ascent));
                                            crate::render::ime_text::paint_span(window, point(bounds.origin.x + x0, top), k, *style, font_size / k, f32::from(x1 - x0), baseline, fg);
                                        }
                                    }
                                    if let Some(cb) = caret_byte {
                                        let cx = x_at(cb);
                                        window.paint_quad(fill(
                                            Bounds::new(point(bounds.origin.x + cx, top), gpui_kit::size(px(1.5), h)),
                                            fg,
                                        ));
                                    }
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                        .into_any_element()
                } else {
                    div().flex_1().min_w(px(0.)).overflow_hidden().whitespace_nowrap().child(shown).into_any_element()
                };
                let id = line.slide;
                let text = line.text.clone();
                let indent = if title { 0.0 } else { 20.0 + f32::from(line.level) * 14.0 };
                h_flex()
                    .id(("outline-line", i))
                    .w_full()
                    .px_2()
                    .py(px(if title { 4. } else { 1. }))
                    .mt(px(if title && i > 0 { 4. } else { 0. }))
                    .gap_1p5()
                    .items_start()
                    .cursor_text()
                    .text_size(px(if title { 13. } else { 12. }))
                    .when(title, |d| d.font_weight(FontWeight::BOLD))
                    .text_color(color)
                    .bg(if current && title { theme.accent } else { gpui_kit::transparent_black() })
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        // The character the click is at: by the text's own widths.
                        let size = window.text_style().font_size.to_pixels(window.rem_size());
                        let _ = size;
                        let caret = outline_caret_at(window, &text, if title { 13.0 } else { 12.0 }, title, f32::from(event.position.x), indent);
                        if event.modifiers.shift && this.is_outline_editing() {
                            this.extend_outline_selection(i, caret, window, cx);
                        } else {
                            this.begin_outline_edit(i, caret, window, cx);
                        }
                    }))
                    .on_mouse_move({
                        let text = line.text.clone();
                        cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                            // The button held down: the selection reaches to here.
                            if event.pressed_button == Some(MouseButton::Left) && this.is_outline_editing() {
                                let caret = outline_caret_at(window, &text, if title { 13.0 } else { 12.0 }, title, f32::from(event.position.x), indent);
                                this.extend_outline_selection(i, caret, window, cx);
                            }
                        })
                    })
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_outline_context_menu(i, id, event.position, window, cx);
                    }))
                    .child(if title {
                        div().w(px(18.)).flex_none().text_xs().text_right().text_color(if current { theme.primary } else { theme.muted_foreground }).when(hidden, |d| d.line_through()).child(SharedString::from(number.to_string())).into_any_element()
                    } else {
                        div().w(px(indent)).flex_none().text_right().child("•").into_any_element()
                    })
                    .child(content)
                    .into_any_element()
            })
            .collect()
    }

    pub(crate) fn render_thumbnails(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_outline = self.is_outline_view();
        let pane_w = self.pane_width_now();

        // The slide master view lists the file's masters and layouts, and has no outline (nor tabs).
        let master_view = self.is_master_view();
        let items = if master_view {
            self.render_master_page_items(cx)
        } else if is_outline {
            self.render_outline_items(cx)
        } else {
            self.render_thumbnail_items(cx)
        };

        let theme = cx.theme();

        let tab_bar = h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(theme.sidebar_border)
            .child(
                div()
                    .id("tab-thumbnails")
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .cursor_pointer()
                    .bg(if !is_outline { theme.accent } else { gpui_kit::transparent_black() })
                    .text_color(if !is_outline { theme.primary } else { theme.muted_foreground })
                    .font_weight(FontWeight::BOLD)
                    .on_click(cx.listener(|this, _, window, cx| this.switch_to_normal_view(window, cx)))
                    .child(crate::shell::i18n::text("pptx.thumbnails-view")),
            )
            .child(
                div()
                    .id("tab-outline")
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .text_xs()
                    .cursor_pointer()
                    .bg(if is_outline { theme.accent } else { gpui_kit::transparent_black() })
                    .text_color(if is_outline { theme.primary } else { theme.muted_foreground })
                    .font_weight(FontWeight::BOLD)
                    .on_click(cx.listener(|this, _, window, cx| this.switch_to_outline_view(window, cx)))
                    .child(crate::shell::i18n::text("pptx.outline-view")),
            );

        v_flex()
            .relative()
            .w(px(pane_w))
            .flex_shrink_0()
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .children((!master_view).then_some(tab_bar))
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        v_flex()
                            .id("slide-list")
                            .key_context(crate::shell::editor::LIST_CONTEXT)
                            .when(!master_view, |d| d.track_focus(&self.list_focus))
                            .when_some(self.master_view_focus(), |d, focus| d.track_focus(&focus))
                            .track_scroll(&self.list_scroll)
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.list_focus.focus(window, cx)))
                            // ⌘↑ / ⌘↓ move the current slide up or down the list.
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                let k = &event.keystroke;
                                if (k.modifiers.platform || k.modifiers.control) && this.pages_are_slides() {
                                    match k.key.as_str() {
                                        "up" => {
                                            cx.stop_propagation();
                                            this.move_current_slide(-1, cx);
                                        }
                                        "down" => {
                                            cx.stop_propagation();
                                            this.move_current_slide(1, cx);
                                        }
                                        // ⌘C / ⌘X / ⌘V: the slide copied, cut or pasted.
                                        "c" => {
                                            cx.stop_propagation();
                                            this.copy_current_slide(window, cx);
                                        }
                                        "x" => {
                                            cx.stop_propagation();
                                            this.cut_current_slide(window, cx);
                                        }
                                        "v" => {
                                            cx.stop_propagation();
                                            this.paste_slides(window, cx);
                                        }
                                        _ => {}
                                    }
                                }
                            }))
                            .size_full()
                            .py_2()
                            .overflow_y_scroll()
                            .children(items),
                    )
                    .vertical_scrollbar(&self.list_scroll),
            )
    }

    /// Context menu entries for a slide (used in thumbnails list and slide sorter grid view).
    pub fn slide_context_menu_entries(&self) -> Vec<MenuEntry> {
        let has_clip = crate::editing::slide_clipboard().is_some();
        let can_delete = self.presentation().slides.len() >= 2;
        let is_skipped = self.slide().hidden;
        let mut entries = Vec::new();
        if self.is_grid_view() {
            entries.push(
                MenuItem::tr("pptx.normal-view")
                    .icon("slideshow")
                    .shortcut("Enter")
                    .on(|e, _, cx| e.close_grid_view(cx))
                    .into(),
            );
            entries.push(MenuEntry::Separator);
        }
        entries.extend([
            MenuItem::tr("pptx.new-slide").icon("add").shortcut("⌘M").on(|e, w, cx| e.add_slide(w, cx)).into(),
            Submenu::tr("pptx.new-slide-layout", "view_quilt", crate::chrome::new_slide_menu(self)).into(),
            MenuItem::tr("pptx.duplicate-slide").icon("control_point_duplicate").shortcut("⌘D").on(|e, w, cx| e.duplicate_current_slide(w, cx)).into(),
            MenuItem::tr("pptx.delete-slide").icon("delete").disabled(!can_delete).on(|e, w, cx| e.delete_slide(w, cx)).into(),
            MenuItem::tr("pptx.section.add").icon("create_new_folder").on(|e, w, cx| e.open_section_panel(false, w, cx)).into(),
            Submenu::tr("pptx.sections", "view_quilt", crate::chrome::section_menu_items(self)).into(),
            MenuEntry::Separator,
            Submenu::tr("pptx.layout", "dashboard", crate::chrome::apply_layout_items(self)).into(),
            MenuItem::tr("pptx.reset-slide").icon("restart_alt").disabled(self.slide().layout.is_none()).on(|e, w, cx| e.reset_slide(w, cx)).into(),
            MenuItem::tr("pptx.reset-placement").disabled(self.slide().layout.is_none()).on(|e, w, cx| e.reset_placeholder_frames(w, cx)).into(),
            Submenu::tr("pptx.transition", "animation", crate::chrome::transition_menu(self)).into(),
            Submenu::tr("pptx.slide-size", "aspect_ratio", crate::chrome::slide_size_menu_items(self)).into(),
            Submenu::tr("pptx.grid-and-guides", "grid_4x4", vec![
                MenuItem::tr("pptx.gridlines").icon("grid_on").checked(self.is_pptx_gridlines_enabled()).on(|e, _, cx| e.toggle_pptx_gridlines(cx)).into(),
                MenuItem::tr("pptx.guides").icon("straighten").checked(self.is_pptx_guides_enabled()).on(|e, _, cx| e.toggle_pptx_guides(cx)).into(),
                MenuItem::tr("pptx.guides.add-vertical").icon("horizontal_rule_rotate90").on(|e, _, cx| e.add_guide(false, cx)).into(),
                MenuItem::tr("pptx.guides.add-horizontal").icon("horizontal_rule").on(|e, _, cx| e.add_guide(true, cx)).into(),
                MenuItem::tr("pptx.guides.remove-all").icon("delete_sweep").disabled(self.guide_list().is_empty()).on(|e, _, cx| e.remove_all_guides(cx)).into(),
                MenuItem::tr("pptx.snap-to-grid").icon("grid_goldenratio").checked(self.is_pptx_snap_to_grid_enabled()).on(|e, _, cx| e.toggle_pptx_snap_to_grid(cx)).into(),
                MenuItem::tr("pptx.snap-to-shape").icon("view_compact").checked(self.is_pptx_snap_to_shape_enabled()).on(|e, _, cx| e.toggle_pptx_snap_to_shape(cx)).into(),
            ]).into(),
            Submenu::tr("pptx.background", "wallpaper", crate::chrome::background_menu(self)).into(),
            Submenu::tr("pptx.theme.palette-selected", "palette", crate::chrome::theme_palette_selected_items(self)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.cut-slide").icon("content_cut").shortcut("⌘X").disabled(!can_delete).on(|e, w, cx| e.cut_current_slide(w, cx)).into(),
            MenuItem::tr("pptx.copy-slide").icon("content_copy").shortcut("⌘C").on(|e, w, cx| e.copy_current_slide(w, cx)).into(),
            MenuItem::tr("pptx.paste-slide").icon("content_paste").shortcut("⌘V").disabled(!has_clip).on(|e, w, cx| { e.paste_slides(w, cx); }).into(),
            MenuItem::tr("pptx.paste-slide-keep").icon("content_paste_go").disabled(!has_clip).on(|e, w, cx| { e.paste_slides_keeping_formatting(w, cx); }).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.skip-slide").icon("visibility_off").checked(is_skipped).on(|e, w, cx| e.toggle_skip_slide(w, cx)).into(),
            MenuItem::tr("pptx.slide.add-hidden-title").icon("title").on(|e, _, cx| {
                let s_id = e.current_slide;
                e.add_hidden_slide_title(s_id, None, cx);
            }).into(),
            MenuItem::tr("insert.comment").icon("add_comment").shortcut("⌥⌘M").on(|e, w, cx| e.open_slide_comments(w, cx)).into(),
            MenuItem::tr("pptx.notes").icon("notes").checked(self.notes_shown()).on(|e, w, cx| e.toggle_notes(w, cx)).into(),
            MenuItem::tr("pptx.header-footer").icon("view_quilt").on(|e, w, cx| e.open_header_footer(w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.export-slide-png").icon("image").on(|e, w, cx| e.export_current_slide_png(w, cx)).into(),
            MenuItem::tr("pptx.export-all-slides-png").icon("image").on(|e, w, cx| e.export_all_slides_png(w, cx)).into(),
            MenuItem::tr("pptx.export-gif").icon("animation").on(|e, w, cx| e.export_gif(w, cx)).into(),
        ]);
        entries
    }

    /// Open context menu for the given slide at pointer `position`.
    pub(crate) fn open_slide_context_menu(&mut self, slide_id: ibis_doc_model::SlideId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_slide_ids().contains(&slide_id) {
            self.select_slide(slide_id, window, cx);
        }
        let entries = self.slide_context_menu_entries();
        self.open_popup_entries(entries, position, window, cx);
    }

    /// Open context menu for the given section header at pointer `position`.
    pub(crate) fn open_section_context_menu(&mut self, section_id: &str, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.select_section(section_id, window, cx);
        let entries = crate::chrome::section_menu_items(self);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// Context menu entries for an outline line in Outline View.
    pub fn outline_context_menu_entries(&self) -> Vec<MenuEntry> {
        let has_sel = self.outline_selected_text().is_some();
        let is_editing = self.is_outline_editing();
        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("edit.cut").icon("content_cut").shortcut("⌘X").disabled(!has_sel).on(|e, _, cx| {
                e.outline_copy(true, cx);
            }).into(),
            MenuItem::tr("edit.copy").icon("content_copy").shortcut("⌘C").disabled(!has_sel).on(|e, _, cx| {
                e.outline_copy(false, cx);
            }).into(),
            MenuItem::tr("edit.paste").icon("content_paste").shortcut("⌘V").disabled(!is_editing).on(|e, _, cx| {
                e.outline_paste(cx);
            }).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.smartart.promote").icon("format_indent_decrease").shortcut("⇧Tab").disabled(!is_editing).on(|e, w, cx| {
                e.outline_indent(false, w, cx);
            }).into(),
            MenuItem::tr("pptx.smartart.demote").icon("format_indent_increase").shortcut("Tab").disabled(!is_editing).on(|e, w, cx| {
                e.outline_indent(true, w, cx);
            }).into(),
            MenuItem::tr("pptx.smartart.move-up").icon("arrow_upward").shortcut("⌥⇧↑").disabled(!is_editing).on(|e, w, cx| {
                e.outline_move(false, w, cx);
            }).into(),
            MenuItem::tr("pptx.smartart.move-down").icon("arrow_downward").shortcut("⌥⇧↓").disabled(!is_editing).on(|e, w, cx| {
                e.outline_move(true, w, cx);
            }).into(),
            MenuEntry::Separator,
        ];
        entries.extend(self.slide_context_menu_entries());
        entries
    }

    /// Open context menu for an outline line at pointer `position`.
    pub(crate) fn open_outline_context_menu(&mut self, line_index: usize, slide_id: ibis_doc_model::SlideId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let already_on_line = self.outline_edit().is_some_and(|e| {
            let (l1, l2) = (e.anchor_line.min(e.line), e.anchor_line.max(e.line));
            (l1..=l2).contains(&line_index)
        });
        if !already_on_line {
            self.begin_outline_edit(line_index, usize::MAX, window, cx);
        }
        if self.current_slide != slide_id && !self.selected_slide_ids().contains(&slide_id) {
            self.select_slide(slide_id, window, cx);
        }
        let entries = self.outline_context_menu_entries();
        self.open_popup_entries(entries, position, window, cx);
    }
}


#[ibis_office_macros::editor_ext(Thumbnails2)]
impl SlideEditor {
    /// A click on slide `id` of the list: Shift takes the slides from the one shown to it, ⌘ adds or
    /// takes it away; alone, it is the one shown.
    pub fn click_slide(&mut self, id: ibis_doc_model::SlideId, modifiers: &Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() || !(modifiers.shift || modifiers.secondary()) {
            return self.select_slide(id, window, cx);
        }
        let slides: Vec<ibis_doc_model::SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
        let (Some(from), Some(to)) = (slides.iter().position(|s| *s == self.current_slide), slides.iter().position(|s| *s == id)) else { return };
        let mut chosen = self.selected_slide_ids();
        if modifiers.shift {
            let (a, b) = (from.min(to), from.max(to));
            chosen = slides[a..=b].to_vec();
        } else if let Some(at) = chosen.iter().position(|s| *s == id) {
            // Taken away (not the last one).
            if chosen.len() > 1 {
                chosen.remove(at);
            }
        } else {
            chosen.push(id);
        }
        let shown = if modifiers.shift || chosen.contains(&id) { id } else { *chosen.last().unwrap_or(&id) };
        self.select_slide(shown, window, cx);
        if let Some(s) = self.pptx_mut() {
            s.slide_selection = chosen;
        }
        cx.notify();
    }
}

#[ibis_office_macros::editor_ext(Thumbnails3)]
impl SlideEditor {
    /// The slides selected in the list moved together to where slide `target` is (above it when they come
    /// from below, below it when from above), in their order.
    pub(crate) fn move_selected_slides_to(&mut self, target: ibis_doc_model::SlideId, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self.selected_slide_ids();
        if chosen.contains(&target) {
            return;
        }
        let all: Vec<ibis_doc_model::SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
        let (Some(t), Some(first)) = (all.iter().position(|s| *s == target), all.iter().position(|s| chosen.contains(s))) else { return };
        let rest: Vec<ibis_doc_model::SlideId> = all.iter().copied().filter(|s| !chosen.contains(s)).collect();
        let k = rest.iter().position(|s| *s == target).map_or(rest.len(), |p| p + usize::from(first < t));
        let desired: Vec<ibis_doc_model::SlideId> = rest[..k].iter().chain(chosen.iter()).chain(rest[k..].iter()).copied().collect();
        // Each place from the first, the slide that belongs there moved to it.
        let mut now = all.clone();
        let mut commands = Vec::new();
        for (j, id) in desired.iter().enumerate() {
            if now[j] != *id {
                commands.push(ibis_doc_model::Command::MoveSlide { slide: *id, index: j });
                let at = now.iter().position(|s| s == id).unwrap_or(j);
                now.remove(at);
                now.insert(j, *id);
            }
        }
        if commands.is_empty() {
            return;
        }
        self.run(vec![ibis_doc_model::Command::Group(commands)], cx);
        self.select_slide(chosen[0], window, cx);
        if let Some(s) = self.pptx_mut() {
            s.slide_selection = chosen;
        }
        cx.notify();
    }
}

/// The character of `text` a click at window x `x` is at, the text drawn from x `left + 8 + 18 + 6` (its padding, its
/// number or bullet and the gap) in the outline: by the widths the text system gives it.
fn outline_caret_at(window: &mut Window, text: &str, size: f32, bold: bool, x: f32, indent: f32) -> usize {
    use gpui_kit::TextRun;
    let _ = indent;
    if text.is_empty() {
        return 0;
    }
    let mut f = window.text_style().font();
    if bold {
        f.weight = FontWeight::BOLD;
    }
    let run = TextRun { len: text.len(), font: f, color: gpui_kit::black(), background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(text.to_string().into(), px(size), &[run], None);
    // (The pane starts at x 0 of the window beside the slide list's place: its left padding 8, the mark 18, the gap 6.)
    let local = (x - (8.0 + 18.0 + 6.0 + if bold { 0.0 } else { 0.0 })).max(0.0);
    let index = line.closest_index_for_x(px(local));
    text[..index.min(text.len())].chars().count()
}
