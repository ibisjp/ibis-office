// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The Word-like parts of the window for a flowing document (.docx): the
//! pages stacked in one scrolling column (Word's print layout), the ruler
//! above them, and scrolling by lines and pages.
//!
//! They are drawn in the editor only; a page itself (and what is exported)
//! is drawn by `page_view.rs`.

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Block, EMU_PER_POINT, EmuRect, EmuSize, FlowAlign, FlowDocument, FlowRunKind, ShapeKind};

use crate::docx::editing::{self as ed, Move};
use crate::shell::editor::{SlideEditor, WorkspaceContent};

/// Space between two pages, and above the first and below the last, in pixels.
pub(crate) const PAGE_GAP: f32 = 20.0;
/// Collapsed space between two pages when white space is hidden, in pixels (DOCX-VIEW-017).
pub(crate) const COLLAPSED_PAGE_GAP: f32 = 4.0;
/// Space kept at least on each side of the pages, in pixels.
const SIDE_MARGIN: f32 = 20.0;
/// One press of ↑ / ↓ scrolls this far, in pixels.
const LINE_SCROLL: f32 = 48.0;
/// Height of the ruler above the pages, in pixels.
const RULER_HEIGHT: f32 = 22.0;
/// Width of the vertical ruler beside the pages, in pixels (DOCX-VIEW-012).
const RULER_WIDTH: f32 = 22.0;
/// Size of the ruler's numbers, in pixels.
const RULER_FONT_SIZE: f32 = 10.0;
/// The ruler's colours: its background, the margins, the text area, ticks.
const RULER_BACKGROUND: u32 = 0xf3f3f3;
const RULER_MARGIN: u32 = 0xc8c8c8;
const RULER_TEXT_AREA: u32 = 0xffffff;
const RULER_TICK: u32 = 0x666666;

/// The scrolled content for `count` pages of `page` size in a workspace of
/// `view` size: at `scale` (pixels per EMU), or fitted with `None`.
pub(crate) fn flow_content(view: Size<Pixels>, pages: &[EmuSize], scale: Option<f32>, gap: f32, side_to_side: bool) -> WorkspaceContent {
    let (view_w, view_h) = (f32::from(view.width), f32::from(view.height));
    if side_to_side {
        let tallest = pages.iter().map(|p| p.height).max().unwrap_or(1).max(1);
        let scale = scale.unwrap_or_else(|| (view_h - 2.0 * gap).max(40.0) / tallest as f32);
        let page_h = tallest as f32 * scale;
        let total_pages_w: f32 = pages.iter().map(|p| p.width as f32 * scale + gap).sum();
        let content_w = view_w.max(gap + total_pages_w);
        let content_h = view_h.max(page_h + 2.0 * gap);
        WorkspaceContent {
            size: size(px(content_w), px(content_h)),
            slide_origin: point(px(gap), px((content_h - page_h) / 2.0)),
            scale,
        }
    } else {
        let widest = pages.iter().map(|p| p.width).max().unwrap_or(1).max(1);
        let scale = scale.unwrap_or_else(|| (view_w - 2.0 * SIDE_MARGIN).max(40.0) / widest as f32);
        let page_w = widest as f32 * scale;
        let content_w = view_w.max(page_w + 2.0 * SIDE_MARGIN);
        let content_h = view_h.max(gap + pages.iter().map(|p| p.height as f32 * scale + gap).sum::<f32>());
        WorkspaceContent {
            size: size(px(content_w), px(content_h)),
            slide_origin: point(px((content_w - page_w) / 2.0), px(gap)),
            scale,
        }
    }
}

/// Where page `index` is in the content (pixels): its left and top, the
/// pages being of `pages`' sizes.
fn page_place(content: &WorkspaceContent, pages: &[EmuSize], index: usize, side_to_side: bool) -> (f32, f32) {
    if side_to_side {
        let gap = f32::from(content.slide_origin.x);
        let tallest = pages.iter().map(|p| p.height).max().unwrap_or(1);
        let left: f32 = pages.iter().take(index).map(|p| p.width as f32 * content.scale + gap).sum();
        let height = pages.get(index).map_or(tallest, |p| p.height);
        (gap + left, f32::from(content.slide_origin.y) + (tallest - height) as f32 * content.scale / 2.0)
    } else {
        let gap = f32::from(content.slide_origin.y);
        let widest = pages.iter().map(|p| p.width).max().unwrap_or(1);
        let top: f32 = pages.iter().take(index).map(|p| p.height as f32 * content.scale + gap).sum();
        let width = pages.get(index).map_or(widest, |p| p.width);
        (f32::from(content.slide_origin.x) + (widest - width) as f32 * content.scale / 2.0, gap + top)
    }
}

/// The page at `offset` in the content (pixels) and how far through it.
fn page_at(content: &WorkspaceContent, pages: &[EmuSize], offset: f32, side_to_side: bool) -> (usize, f32) {
    if side_to_side {
        let gap = f32::from(content.slide_origin.x);
        let mut left = gap;
        for (i, p) in pages.iter().enumerate() {
            let w = p.width as f32 * content.scale + gap;
            if offset < left + w || i + 1 == pages.len() {
                return (i, ((offset - left) / w).max(0.0));
            }
            left += w;
        }
        (0, 0.0)
    } else {
        let gap = f32::from(content.slide_origin.y);
        let mut top = gap;
        for (i, p) in pages.iter().enumerate() {
            let h = p.height as f32 * content.scale + gap;
            if offset < top + h || i + 1 == pages.len() {
                return (i, ((offset - top) / h).max(0.0));
            }
            top += h;
        }
        (0, 0.0)
    }
}

/// The characters of the body text, not counting white space (Word's
/// 文字数 in the status bar).
pub(crate) fn count_characters(document: &FlowDocument) -> usize {
    fn count(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .map(|block| match block {
                Block::Paragraph(p) => p
                    .runs
                    .iter()
                    .map(|run| match &run.kind {
                        FlowRunKind::Text(text) => text.chars().filter(|c| !c.is_whitespace()).count(),
                        _ => 0,
                    })
                    .sum::<usize>(),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).map(|c| count(&c.blocks)).sum(),
            })
            .sum()
    }
    count(&document.body)
}

impl SlideEditor {
    /// The sizes of the pages, in order.
    pub(crate) fn page_sizes(&self) -> Vec<EmuSize> {
        let p = self.presentation();
        p.slides.iter().map(|s| p.size_of(s)).collect()
    }

    /// The index of the page in the middle of the workspace.
    pub(crate) fn visible_page(&self) -> usize {
        let count = self.presentation().slides.len();
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else {
            return self.presentation().slide_index(self.current_slide).unwrap_or(0);
        };
        let active_scroll = if self.doc_split_active && self.doc_split.is_some() {
            &self.doc_split_scroll
        } else {
            &self.workspace_scroll
        };
        if self.page_movement_side_to_side {
            let view_w = f32::from(ws.size.width);
            let middle = -f32::from(active_scroll.offset().x) + view_w / 2.0;
            let (at, _) = page_at(&content, &self.page_sizes(), middle, true);
            at.min(count.saturating_sub(1))
        } else {
            let view_h = if self.doc_split.is_some() {
                f32::from(ws.size.height) / 2.0
            } else {
                f32::from(ws.size.height)
            };
            let middle = -f32::from(active_scroll.offset().y) + view_h / 2.0;
            let (at, _) = page_at(&content, &self.page_sizes(), middle, false);
            at.min(count.saturating_sub(1))
        }
    }

    /// Scroll the pages so `top` (in the content) is at the top of the workspace.
    fn scroll_flow_to(&self, top: f32) {
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let view_h = if self.doc_split.is_some() {
            f32::from(ws.size.height) / 2.0
        } else {
            f32::from(ws.size.height)
        };
        let max = (f32::from(content.size.height) - view_h).max(0.0);
        let target_scroll = if self.doc_split_active && self.doc_split.is_some() {
            &self.doc_split_scroll
        } else {
            &self.workspace_scroll
        };
        let offset = target_scroll.offset();
        target_scroll.set_offset(point(offset.x, px(-top.clamp(0.0, max))));
    }

    /// Scroll the pages so `left` (in the content) is at the left of the workspace (DOCX-VIEW-008).
    fn scroll_flow_horizontal_to(&self, left: f32) {
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let view_w = f32::from(ws.size.width);
        let max = (f32::from(content.size.width) - view_w).max(0.0);
        let target_scroll = if self.doc_split_active && self.doc_split.is_some() {
            &self.doc_split_scroll
        } else {
            &self.workspace_scroll
        };
        let offset = target_scroll.offset();
        target_scroll.set_offset(point(px(-left.clamp(0.0, max)), offset.y));
    }

    /// Scroll so page `index` starts at the top (or left if side-to-side) of the workspace.
    pub(crate) fn scroll_to_page(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(content) = self.workspace_content().filter(|_| self.workspace_bounds.is_some()) else {
            if let Some(state) = self.doc_state_mut() {
                state.pending_page = Some(index);
            }
            return;
        };
        if self.page_movement_side_to_side {
            let gap = f32::from(content.slide_origin.x);
            let (left, _) = page_place(&content, &self.page_sizes(), index, true);
            self.scroll_flow_horizontal_to(left - gap);
        } else {
            let gap = f32::from(content.slide_origin.y);
            let (_, top) = page_place(&content, &self.page_sizes(), index, false);
            self.scroll_flow_to(top - gap);
        }
        cx.notify();
    }

    /// PageDown / PageUp: scroll by a screen, forward or backward.
    pub(crate) fn step_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace_bounds else { return };
        if self.page_movement_side_to_side {
            let page_step = f32::from(ws.size.width) * 0.9;
            let target_scroll = if self.doc_split_active && self.doc_split.is_some() {
                &self.doc_split_scroll
            } else {
                &self.workspace_scroll
            };
            let left = -f32::from(target_scroll.offset().x);
            self.scroll_flow_horizontal_to(left + if forward { page_step } else { -page_step });
        } else {
            let page = (f32::from(ws.size.height) - LINE_SCROLL).max(LINE_SCROLL);
            let top = -f32::from(self.workspace_scroll.offset().y);
            self.scroll_flow_to(top + if forward { page } else { -page });
        }
        cx.notify();
    }

    /// ↑ / ↓: scroll by a few lines, up if `lines` is negative.
    pub(crate) fn scroll_lines(&mut self, lines: f32, cx: &mut Context<Self>) {
        if self.page_movement_side_to_side {
            let left = -f32::from(self.workspace_scroll.offset().x);
            self.scroll_flow_horizontal_to(left + lines * LINE_SCROLL);
        } else {
            let top = -f32::from(self.workspace_scroll.offset().y);
            self.scroll_flow_to(top + lines * LINE_SCROLL);
        }
        cx.notify();
    }

    /// Home / End: to the start or the end of the document.
    pub(crate) fn scroll_to_end(&mut self, end: bool, cx: &mut Context<Self>) {
        if self.page_movement_side_to_side {
            self.scroll_flow_horizontal_to(if end { f32::INFINITY } else { 0.0 });
        } else {
            self.scroll_flow_to(if end { f32::INFINITY } else { 0.0 });
        }
        cx.notify();
    }

    /// The vertical/horizontal scrolling that keeps the same place of the document at
    /// `anchor` (from the top/left of the workspace) when the zoom changes.
    pub(crate) fn flow_zoom_offset(
        &self,
        before: &WorkspaceContent,
        after: &WorkspaceContent,
        old: Pixels,
        view: Pixels,
        anchor: Pixels,
    ) -> Pixels {
        let pages = self.page_sizes();
        let (view, anchor) = (f32::from(view), f32::from(anchor));
        let place = -f32::from(old) + anchor;
        let (at, through) = page_at(before, &pages, place, self.page_movement_side_to_side);
        if self.page_movement_side_to_side {
            let left = page_place(after, &pages, at, true).0;
            let gap = f32::from(after.slide_origin.x);
            let place = left + through * pages.get(at).map_or(0.0, |p| p.width as f32 * after.scale + gap);
            let max = (f32::from(after.size.width) - view).max(0.0);
            px(-(place - anchor).clamp(0.0, max))
        } else {
            let top = page_place(after, &pages, at, false).1;
            let gap = f32::from(after.slide_origin.y);
            let place = top + through * pages.get(at).map_or(0.0, |p| p.height as f32 * after.scale + gap);
            let max = (f32::from(after.size.height) - view).max(0.0);
            px(-(place - anchor).clamp(0.0, max))
        }
    }

    /// The workspace of a document: the pages near the view (the others
    /// are not built, so a long document scrolls fast), each with a shadow.
    pub(crate) fn render_document_workspace(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_docx_edited_box_resource();
        let entity = cx.entity();
        let content = self.workspace_content();
        let muted = cx.theme().muted;

        let content_view = if let Some(ratio) = self.doc_split {
            let top_offset = if self.page_movement_side_to_side {
                -f32::from(self.doc_split_scroll.offset().x)
            } else {
                -f32::from(self.doc_split_scroll.offset().y)
            };
            let bottom_offset = if self.page_movement_side_to_side {
                -f32::from(self.workspace_scroll.offset().x)
            } else {
                -f32::from(self.workspace_scroll.offset().y)
            };
            let top_pages = content.map(|c| self.render_pages_at(c, top_offset, true, cx)).unwrap_or_default();
            let bottom_pages = content.map(|c| self.render_pages_at(c, bottom_offset, false, cx)).unwrap_or_default();
            let theme = cx.theme().clone();

            let top_container = if let Some(ws) = self.workspace_bounds {
                let available_h = (f32::from(ws.size.height) - 6.0).max(80.0);
                let top_h = (available_h * ratio).clamp(40.0, available_h - 40.0);
                div().h(px(top_h)).flex_none()
            } else {
                div().flex_1()
            };

            div()
                .flex()
                .flex_col()
                .size_full()
                .child(
                    top_container
                        .relative()
                        .w_full()
                        .min_h_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("workspace-scroll-top")
                                .size_full()
                                .overflow_scroll()
                                .track_scroll(&self.doc_split_scroll)
                                .when_some(content, |d, content| {
                                    d.child(
                                        div()
                                            .relative()
                                            .flex_none()
                                            .w(content.size.width)
                                            .h(content.size.height)
                                            .children(top_pages),
                                    )
                                }),
                        )
                        .when(self.panel_dialog_is_modal(), |d| d.child(crate::shell::panel_dialog::viewport_shield()))
                        .scrollbar(&self.doc_split_scroll, ScrollbarAxis::Both),
                )
                .child(
                    div()
                        .id("doc-split-divider")
                        .flex_none()
                        .h(px(6.0))
                        .w_full()
                        .bg(rgb(0xe0e0e0))
                        .border_t_1()
                        .border_b_1()
                        .border_color(theme.border)
                        .cursor_row_resize()
                        .hover(|s| s.bg(rgb(0xd0d0d0)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if event.click_count == 2 {
                                    this.toggle_document_split(cx);
                                    window.refresh();
                                } else {
                                    this.doc_split_dragging = true;
                                }
                            }),
                        ),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("workspace-scroll")
                                .size_full()
                                .overflow_scroll()
                                .track_scroll(&self.workspace_scroll)
                                .when_some(content, |d, content| {
                                    d.child(
                                        div()
                                            .relative()
                                            .flex_none()
                                            .w(content.size.width)
                                            .h(content.size.height)
                                            .children(bottom_pages),
                                    )
                                }),
                        )
                        .when(self.panel_dialog_is_modal(), |d| d.child(crate::shell::panel_dialog::viewport_shield()))
                        .scrollbar(&self.workspace_scroll, ScrollbarAxis::Both),
                )
        } else {
            let offset = if self.page_movement_side_to_side {
                -f32::from(self.workspace_scroll.offset().x)
            } else {
                -f32::from(self.workspace_scroll.offset().y)
            };
            let pages = content.map(|content| self.render_pages_at(content, offset, false, cx)).unwrap_or_default();
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .child(
                    div()
                        .id("workspace-scroll")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&self.workspace_scroll)
                        .when_some(content, |d, content| {
                            d.child(
                                div()
                                    .relative()
                                    .flex_none()
                                    .w(content.size.width)
                                    .h(content.size.height)
                                    .children(pages),
                            )
                        }),
                )
                .when(self.panel_dialog_is_modal(), |d| d.child(crate::shell::panel_dialog::viewport_shield()))
                .scrollbar(&self.workspace_scroll, ScrollbarAxis::Both)
        };

        div()
            .id("workspace")
            .when(!self.doc_state().is_some_and(|s| s.box_edit.is_some()), |d| d.key_context(ed::TEXT_CONTEXT))
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_text_key))
            .map(|d| Self::text_actions(d, &mut *cx))

            // The wheel scrolls the pages; it turns none.
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                if this.page_movement_side_to_side {
                    let delta = match event.delta {
                        ScrollDelta::Lines(d) => -d.y * 120.0,
                        ScrollDelta::Pixels(d) => -f32::from(d.y),
                    };
                    let target_scroll = if this.doc_split_active && this.doc_split.is_some() {
                        &this.doc_split_scroll
                    } else {
                        &this.workspace_scroll
                    };
                    let cur = -f32::from(target_scroll.offset().x);
                    this.scroll_flow_horizontal_to(cur + delta);
                    cx.notify();
                    window.refresh();
                    cx.stop_propagation();
                    return;
                }
                Self::on_workspace_scroll(this, event, window, cx);
            }))
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(muted)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_docx_workspace_down))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.doc_split_dragging {
                    if let Some(ws) = this.workspace_bounds {
                        let rel_y = (f32::from(event.position.y - ws.origin.y) / f32::from(ws.size.height)).clamp(0.1, 0.9);
                        this.doc_split = Some(rel_y);
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| {
                if this.doc_split_dragging {
                    this.doc_split_dragging = false;
                    cx.notify();
                }
            }))
            .child(content_view)
            .children(self.render_loading(cx))
            .child(crate::shell::editor::wheel_zoom_listener(cx))
            .child(crate::shell::editor::bounds_observer(move |bounds, window, cx| {
                let changed = entity.update(cx, |this, cx| {
                    let mut changed = false;
                    if this.workspace_bounds != Some(bounds) {
                        this.workspace_bounds = Some(bounds);
                        cx.notify();
                        changed = true;
                    }
                    if let Some(index) = this.doc_state_mut().and_then(|s| s.pending_page.take()) {
                        this.scroll_to_page(index, cx);
                        changed = true;
                    }
                    changed
                });
                if changed {
                    window.on_next_frame(move |window, _| window.refresh());
                }
            }))
    }

    /// Workspace background mouse down: click outside page on comment bubble focuses comment; double click toggles page white space (DOCX-VIEW-017).
    fn on_docx_workspace_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.click_count == 1 {
            for index in 0..self.presentation().slides.len() {
                if let Some((x, y)) = self.page_point_pane(index, event.position, false) {
                    if let Some(comment_id) = self.comment_at_point(index, x, y) {
                        self.open_comments(window, cx);
                        self.focus_comment(comment_id, cx);
                        return;
                    }
                }
            }
        }
        if event.click_count == 2 {
            self.toggle_white_space(cx);
            return;
        }
        self.on_workspace_down(event, window, cx);
    }

    /// The pages that are in view or next to it, placed in the content.
    #[allow(dead_code)]
    pub(crate) fn render_pages(&self, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.render_pages_at(content, -f32::from(self.workspace_scroll.offset().y), false, cx)
    }

    fn render_pages_at(&self, content: WorkspaceContent, scroll_pos: f32, is_top: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pages = self.page_sizes();
        let side_to_side = self.page_movement_side_to_side;
        let (first, to) = if side_to_side {
            let view_w = self.workspace_bounds.map_or(0.0, |ws| f32::from(ws.size.width));
            let from = scroll_pos - view_w / 2.0;
            let to = scroll_pos + view_w * 1.5;
            let (first, _) = page_at(&content, &pages, from, true);
            (first, to)
        } else {
            let view_h = self.workspace_bounds.map_or(0.0, |ws| f32::from(ws.size.height));
            let pane_view_h = if self.doc_split.is_some() { view_h / 2.0 } else { view_h };
            let from = scroll_pos - pane_view_h / 2.0;
            let to = scroll_pos + pane_view_h * 1.5;
            let (first, _) = page_at(&content, &pages, from, false);
            (first, to)
        };
        let scale = content.scale;
        let k = scale * EMU_PER_POINT as f32;
        let lines = self.flow_lines();
        let selection = self.ordered_selection();
        let caret = self.text_selection().map(|(_, c)| c);
        let marked = self.doc_state().and_then(|s| s.marked.clone());
        // Text being composed is drawn in the font and size it will be typed in (the installed font standing in for
        // the typeface, as the page draws it), not the window's: its kanji took a Chinese font where that has none.
        let marked_font = marked.as_ref().and_then(|text| {
            let (_, style) = self.current_format()?;
            let (main, other) = if text.chars().any(|c| !c.is_ascii()) { (&style.east_asian_font, &style.font) } else { (&style.font, &style.east_asian_font) };
            let font = self.resources.font(&ibis_doc_model::TextStyle {
                font: main.clone().or_else(|| other.clone()),
                fallback_font: other.clone(),
                bold: style.bold,
                italic: style.italic,
                ..Default::default()
            });
            Some((font, style.size as f32 / 100.0))
        });
        let gridlines = self.gridlines_open;
        let formatting_marks = self.formatting_marks_open;
        let formatting_marks_options = self.formatting_marks_options;
        let show_white_space = self.show_white_space && !self.is_web_layout_mode() && !self.is_draft_mode();
        let flow_doc = self.flow().cloned();
        let flow_setup = self.presentation().flow.as_ref().map(|doc| doc.page);
        let gap = if side_to_side {
            f32::from(content.slide_origin.x)
        } else {
            f32::from(content.slide_origin.y)
        };
        self.presentation()
            .slides
            .iter()
            .enumerate()
            .skip(first)
            .take_while(|(index, _)| {
                let (left, page_top) = page_place(&content, &pages, *index, side_to_side);
                if side_to_side { left < to } else { page_top < to }
            })
            .flat_map(|(index, slide)| {
                let (left, page_top) = page_place(&content, &pages, index, side_to_side);
                let size = pages[index];
                let (w, h) = (size.width as f32 * scale, size.height as f32 * scale);
                let shapes = slide.shapes.iter().map(|shape| {
                    // The page is drawn, not handled: it cannot be selected,
                    // moved or removed.
                    let frame = if matches!(shape.kind, ShapeKind::Page { .. }) {
                        EmuRect { x: 0, y: 0, ..shape.frame }
                    } else {
                        shape.frame
                    };
                    crate::render::shape_view::shape_element(shape, frame, scale, &self.resources, None)
                });
                let gridlines_element = gridlines.then(|| {
                    Self::render_page_gridlines(w, h, scale, flow_setup)
                });
                let formatting_marks_element = formatting_marks_options.any_visible(formatting_marks).then(|| {
                    Self::render_page_formatting_marks(&lines, index, flow_doc.as_deref(), formatting_marks, formatting_marks_options, w, h, k)
                });
                let page_element = div()
                    .id(("page", index))
                    .absolute()
                    .left(px(left))
                    .top(px(page_top))
                    .w(px(w))
                    .h(px(h))
                    .bg(crate::shell::palette::doc(if self.resources.dark_page { 0x262626 } else { 0xffffff }))
                    .when(show_white_space, |d| d.shadow_md())
                    .when(!show_white_space, |d| d.border_1().border_color(crate::shell::palette::doc(0xd0d0d0)))
                    .children(shapes)
                    .children(gridlines_element)
                    .children(formatting_marks_element)
                    .children(self.selection_rects(&lines, index, selection, k))
                    .children(self.fit_text_indicator(&lines,index,k))
                    .children(self.text_box_overlay(index, k, cx))
                    .children(self.doc_state().map(|s| s.extra_selections.clone()).unwrap_or_default().into_iter().flat_map(|r| self.selection_rects(&lines, index, Some(r), k)).collect::<Vec<_>>())
                    .children(caret.filter(|_| !self.doc_state().is_some_and(|s| s.box_edit.is_some() || s.shape_selected)).and_then(|c| {
                        let line = Self::line_of(&lines, c)?;
                        let b = self.caret_box(&lines, c)?;
                        Some((line.vertical, b))
                    }).filter(|(_, b)| b.0 == index).map(|(vertical, (_, x, y, h))| {
                        let caret = if vertical {
                            div().absolute().left(px(x * k)).top(px(y * k - 0.5)).w(px(h * k)).h(px(1.5)).bg(crate::shell::palette::doc(0x000000))
                        } else {
                            div().absolute().left(px(x * k - 0.5)).top(px(y * k)).w(px(1.5)).h(px(h * k)).bg(crate::shell::palette::doc(0x000000))
                        };
                        match &marked {
                            // Text being composed, underlined at the caret.
                            Some(text) => div()
                                .absolute()
                                .left(px(x * k))
                                .top(px(y * k))
                                .h(px(h * k))
                                .flex()
                                .items_end()
                                .text_color(crate::shell::palette::doc(0x000000))
                                .when(vertical,|d|d.flex_col().items_center().h_auto().w(px(h*k)))
                                .bg(crate::shell::palette::doc(0xffffff))
                                .border_b_1()
                                .border_color(crate::shell::palette::doc(0x000000))
                                .map(|d| match &marked_font {
                                    Some((font, size)) => d.font_family(font.family.clone()).font_weight(font.weight).when(vertical,|d|d.font_features(crate::render::vertical_text::font(font).features)).when(font.style == FontStyle::Italic, |d| d.italic()).text_size(px((size * k).max(8.0))),
                                    None => d.text_size(px((h * k * 0.7).max(8.0))),
                                })
                                .children(if vertical{vec![crate::render::vertical_text::marked(text.clone(),marked_font.as_ref().map(|(font,_)|font.clone()).unwrap_or_else(||gpui_kit::font("sans-serif")),marked_font.as_ref().map_or(h,|(_,size)|*size)*k)]}else{vec![div().child(text.clone()).into_any_element()]})
                                .into_any_element(),
                            None => crate::shell::caret::element(caret,format!("doc:{index}:{x}:{y}:{h}"),self.focus_handle.clone()).into_any_element(),
                        }
                    }))
                    .when(self.link_hover,|d|d.cursor_pointer())
                    .on_mouse_down(MouseButton::Right,cx.listener(move|e,event:&MouseDownEvent,w,cx|{
                        if let Some(pos)=e.page_point_pane(index,event.position,is_top).and_then(|(x,y)|e.doc_link_position_at_point(index,x,y)){
                            if let Some((a,b,_))=e.doc_link_range(pos){e.set_caret(a,false,cx);e.set_caret(b,true,cx);let entries=e.doc_hyperlink_menu();e.open_hyperlink_context(event.position,entries,w,cx);cx.stop_propagation();}
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| this.on_page_down(index, event, window, is_top, cx)))
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| this.on_page_move(index, event, is_top, cx)))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, window, cx| {
                        if let Some(state) = this.doc_state_mut() {
                            state.dragging = false;
                        }
                        this.end_docx_ink(cx);
                        let clicked=this.doc_state().and_then(|s|s.shape_drag).filter(|d|d.delta.0.abs()<2.&&d.delta.1.abs()<2. && !crate::shell::selection_handles::on_border(d.frame,0,d.start,3.)).map(|d|d.at);
                        // A plain click on the border of the box being edited ends the edit (a drag moves the box, text still in edit).
                        let border_click_exit=this.doc_state().and_then(|s|s.shape_drag.filter(|d|d.mode==crate::docx::text_box_panel::DragMode::Move&&d.delta.0.abs()<2.&&d.delta.1.abs()<2.&&s.box_edit.as_ref().is_some_and(|b|b.at==d.at))).is_some();
                        this.end_shape_drag(cx);
                        if border_click_exit {this.commit_box_edit(window,cx);}
                        if let Some(at)=clicked {this.start_box_edit_at(at,window,cx);}
                        this.sync_text_box_panel(window, cx);
                        this.end_draw_table(cx);
                        this.finish_doc_format_brush(cx);
                    }))
                    .into_any_element();

                let top_gap = (!side_to_side && index == 0).then(|| {
                    div()
                        .id("page-gap-top")
                        .absolute()
                        .left(px(left))
                        .top(px(0.0))
                        .w(px(w))
                        .h(px(gap))
                        .cursor_row_resize()
                        .when(!show_white_space, |d| d.bg(rgb(0x9e9e9e)))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            if event.click_count == 2 {
                                this.toggle_white_space(cx);
                            }
                        }))
                        .into_any_element()
                });

                let bottom_gap = (!side_to_side && index + 1 < pages.len()).then(|| {
                    div()
                        .id(("page-gap", index))
                        .absolute()
                        .left(px(left))
                        .top(px(page_top + h))
                        .w(px(w))
                        .h(px(gap))
                        .cursor_row_resize()
                        .when(!show_white_space, |d| d.bg(rgb(0x9e9e9e)))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            if event.click_count == 2 {
                                this.toggle_white_space(cx);
                            }
                        }))
                        .into_any_element()
                });

                top_gap.into_iter().chain(std::iter::once(page_element)).chain(bottom_gap)
            })
            .collect()
    }

    /// The top-left corner of page `index` in the window for a pane, and pixels per point (DOCX-VIEW-019).
    pub(crate) fn page_origin_pane(&self, index: usize, is_top: bool) -> Option<(Point<Pixels>, f32)> {
        let content = self.workspace_content()?;
        let ws = self.workspace_bounds?;
        let (left, top) = page_place(&content, &self.page_sizes(), index, self.page_movement_side_to_side);
        if let Some(ratio) = self.doc_split {
            let available_h = (f32::from(ws.size.height) - 6.0).max(80.0);
            let top_h = (available_h * ratio).clamp(40.0, available_h - 40.0);
            let (pane_origin, scroll_offset) = if is_top {
                (ws.origin, self.doc_split_scroll.offset())
            } else {
                (point(ws.origin.x, ws.origin.y + px(top_h + 6.0)), self.workspace_scroll.offset())
            };
            let origin = pane_origin + scroll_offset + point(px(left), px(top));
            Some((origin, content.scale * EMU_PER_POINT as f32))
        } else {
            let origin = ws.origin + self.workspace_scroll.offset() + point(px(left), px(top));
            Some((origin, content.scale * EMU_PER_POINT as f32))
        }
    }

    /// The top-left corner of page `index` in the window, and pixels per point.
    pub(crate) fn page_origin(&self, index: usize) -> Option<(Point<Pixels>, f32)> {
        self.page_origin_pane(index, false)
    }

    /// Scroll so the band `y`..`y + h` (points) of page `index` is in view.
    pub(crate) fn scroll_into_view(&mut self, index: usize, y: f32, h: f32, cx: &mut Context<Self>) {
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let k = content.scale * EMU_PER_POINT as f32;
        if self.page_movement_side_to_side {
            let (left, _) = page_place(&content, &self.page_sizes(), index, true);
            let page_w = self.page_sizes().get(index).map_or(0.0, |p| p.width as f32 * content.scale);
            let target_scroll = if self.doc_split_active && self.doc_split.is_some() {
                &self.doc_split_scroll
            } else {
                &self.workspace_scroll
            };
            let view_left = -f32::from(target_scroll.offset().x);
            let view_w = f32::from(ws.size.width);
            if left < view_left {
                self.scroll_flow_horizontal_to(left - SIDE_MARGIN);
            } else if left + page_w > view_left + view_w {
                self.scroll_flow_horizontal_to(left + page_w - view_w + SIDE_MARGIN);
            }
            return;
        }
        let top = page_place(&content, &self.page_sizes(), index, false).1 + y * k;
        let bottom = top + h * k;
        let target_scroll = if self.doc_split_active && self.doc_split.is_some() {
            &self.doc_split_scroll
        } else {
            &self.workspace_scroll
        };
        let view_top = -f32::from(target_scroll.offset().y);
        let view_h = if self.doc_split.is_some() {
            f32::from(ws.size.height) / 2.0
        } else {
            f32::from(ws.size.height)
        };
        if top < view_top {
            self.scroll_flow_to(top - LINE_SCROLL);
        } else if bottom > view_top + view_h {
            self.scroll_flow_to(bottom - view_h + LINE_SCROLL);
        }
        cx.notify();
    }

    /// Where on page `index` a window point is, in points.
    pub(crate) fn page_point_pane(&self, index: usize, position: Point<Pixels>, is_top: bool) -> Option<(f32, f32)> {
        let (origin, k) = self.page_origin_pane(index, is_top)?;
        Some((f32::from(position.x - origin.x) / k, f32::from(position.y - origin.y) / k))
    }

    #[allow(dead_code)]
    fn page_point(&self, index: usize, position: Point<Pixels>) -> Option<(f32, f32)> {
        self.page_point_pane(index, position, false)
    }

    /// A click or drag of the mouse; with ⌘ it adds a range to the selection (Word's
    /// 離れた範囲の選択) instead of replacing it.
    pub(super) fn on_page_down(&mut self, index: usize, event: &MouseDownEvent, window: &mut Window, is_top: bool, cx: &mut Context<Self>) {
        let adding = event.modifiers.platform && !event.modifiers.shift;
        let kept = if adding { self.selection_ranges() } else { Vec::new() };
        let following = event.click_count == 1 && self.page_point_pane(index, event.position, is_top).and_then(|(x, y)| self.doc_link_position_at_point(index, x, y)).is_some();
        self.on_page_down_one(index, event, window, is_top, cx);
        if adding && !following && !kept.is_empty() {
            if let Some(state) = self.doc_state_mut() {
                state.extra_selections = kept;
            }
        }
    }

    fn on_page_down_one(&mut self, index: usize, event: &MouseDownEvent, window: &mut Window, is_top: bool, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.doc_split_active = is_top;
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.commit_box_edit(window, cx);
        }
        self.focus_handle.focus(window, cx);
        let Some((x, y)) = self.page_point_pane(index, event.position, is_top) else { return };
        if event.click_count == 1 {
            if let Some(comment_id) = self.comment_at_point(index, x, y) {
                self.open_comments(window, cx);
                self.focus_comment(comment_id, cx);
                return;
            }
        }
        if event.click_count == 1 && self.begin_docx_ink(index, x, y, cx) {
            return;
        }
        if event.click_count == 1 && self.begin_draw_table(index, x, y, cx) {
            return;
        }
        // A double click on a floating text box edits its text in place.
        if event.click_count == 2 && !event.modifiers.platform && !event.modifiers.shift && self.begin_box_edit_or_handle(index, x, y, window, cx) {
            self.sync_text_box_panel(window, cx);
            return;
        }
        // Shift-click on a shape adds/toggles it in the multi-shape selection.
        if event.click_count == 1 && crate::shell::selection_handles::additive(event.modifiers) && self.shift_click_shape(index, x, y, cx) {
            self.sync_text_box_panel(window, cx);
            return;
        }
        // A press on a floating text box selects it and moves it with the mouse.
        if event.click_count == 1 && !event.modifiers.platform && !event.modifiers.control && !event.modifiers.shift && self.begin_shape_drag(index, x, y, cx) {
            self.sync_text_box_panel(window, cx);
            return;
        }
        if let Some(state) = self.doc_state_mut() {
            if state.shape_selected || !state.selected_shapes.is_empty() {
                state.shape_selected = false;
                state.selected_shapes.clear();
                cx.notify();
            }
        }
        let Some(pos) = self.position_at(index, x, y) else { return };
        // ⌘-click follows a link to its bookmark (a table of contents' entry).
        if event.click_count==1 && !event.modifiers.shift {
            if let Some(link_pos)=self.doc_link_position_at_point(index,x,y){if self.follow_link(link_pos,cx){return;}}
        }
        if event.click_count == 1 {
            if self.toggle_checkbox_at(pos, cx) || self.cycle_dropdown_item_at(pos, cx) || self.step_date_picker_at(pos, 1, cx) {
                self.set_caret(pos, false, cx);
                return;
            }
            if self.is_picture_control_at(pos) {
                self.set_caret(pos, false, cx);
                return;
            }
            if pos.offset > 0 && self.is_picture_control_at(ibis_doc_model::FlowPos::new(pos.para, pos.offset - 1)) {
                self.set_caret(ibis_doc_model::FlowPos::new(pos.para, pos.offset - 1), false, cx);
                return;
            }
        }
        if event.click_count == 2 {
            let (a, b) = self.word_at(pos);
            self.set_caret(a, false, cx);
            self.set_caret(b, true, cx);
            return;
        }
        if event.click_count >= 3 {
            let len = ibis_doc_model::flow_command::paragraph(self.flow().map(|d| d.as_ref()).unwrap_or(&FlowDocument::default()), pos.para).map_or(0, |p| p.len());
            self.set_caret(ibis_doc_model::FlowPos::new(pos.para, 0), false, cx);
            self.set_caret(ibis_doc_model::FlowPos::new(pos.para, len), true, cx);
            return;
        }
        self.set_caret(pos, event.modifiers.shift, cx);
        if event.click_count == 1 && !event.modifiers.shift {
            if let Some(doc) = self.flow() {
                if let Some(c_id) = ibis_doc_model::flow_command::comment_at_pos(doc, pos) {
                    let parent_id = doc.comments.iter().find(|c| c.id == c_id).and_then(|c| c.parent).unwrap_or(c_id);
                    if !self.sidebar_open || self.doc_state().is_some_and(|s| s.comments.is_none()) {
                        self.open_comments(window, cx);
                    }
                    if let Some(c) = self.doc_state_mut().and_then(|s| s.comments.as_mut()) {
                        c.active_comment_id = Some(parent_id);
                    }
                }
            }
        }
        if let Some(state) = self.doc_state_mut() {
            state.dragging = true;
            state.goal_x = None;
        }
    }

    fn on_page_move(&mut self, index: usize, event: &MouseMoveEvent, is_top: bool, cx: &mut Context<Self>) {
        self.update_link_hover(event,cx);
        if self.doc_state().and_then(|s| s.docx_ink.as_ref()).is_some_and(|i| i.active_stroke.is_some() || i.lasso_rect.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                if let Some((x, y)) = self.page_point_pane(index, event.position, is_top) {
                    self.drag_docx_ink(index, x, y, cx);
                }
            } else {
                self.end_docx_ink(cx);
            }
            return;
        }
        if self.doc_state().is_some_and(|s| s.draw_box.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                if let Some((x, y)) = self.page_point_pane(index, event.position, is_top) {
                    self.drag_draw_table(x, y, cx);
                }
            } else if let Some(state) = self.doc_state_mut() {
                state.draw_box = None;
            }
            return;
        }
        if self.doc_state().is_some_and(|s| s.shape_drag.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                if let Some((x, y)) = self.page_point_pane(index, event.position, is_top) {
                    self.drag_shape(x, y, cx);
                }
            } else if let Some(state) = self.doc_state_mut() {
                state.shape_drag = None;
            }
            return;
        }
        if !self.doc_state().is_some_and(|s| s.dragging) {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            if let Some(state) = self.doc_state_mut() {
                state.dragging = false;
            }
            return;
        }
        let Some((x, y)) = self.page_point_pane(index, event.position, is_top) else { return };
        if let Some(pos) = self.position_at(index, x, y) {
            self.set_caret(pos, true, cx);
        }
    }

    /// The selection's boxes on page `index` (pixels in the page).
    fn selection_rects(&self, lines: &[ed::LineGeom], index: usize, selection: Option<(ibis_doc_model::FlowPos, ibis_doc_model::FlowPos)>, k: f32) -> Vec<AnyElement> {
        let Some((start, end)) = selection.filter(|(a, b)| a != b) else { return Vec::new() };
        let Some(doc) = self.flow() else { return Vec::new() };
        // Paragraphs from the start's to the end's, in reading order.
        let mut ids: Vec<u32> = Vec::new();
        fn walk(blocks: &[Block], out: &mut Vec<u32>) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => out.push(p.id),
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        walk(&doc.body, &mut ids);
        walk(&doc.header, &mut ids);
        walk(&doc.footer, &mut ids);
        let (Some(i), Some(j)) = (ids.iter().position(|&p| p == start.para), ids.iter().position(|&p| p == end.para)) else { return Vec::new() };
        let span = &ids[i.min(j)..=i.max(j)];
        let mut out = Vec::new();
        for line in lines.iter().filter(|l| l.page == index && span.contains(&l.para)) {
            let lo = if line.para == start.para { start.offset } else { 0 };
            let hi = if line.para == end.para { end.offset } else { usize::MAX };
            let inside: Vec<f32> = line.stops.iter().filter(|(o, _)| *o >= lo && *o <= hi).map(|(_, x)| *x).collect();
            let (Some(a), Some(b)) = (inside.iter().copied().reduce(f32::min), inside.iter().copied().reduce(f32::max)) else { continue };
            // A paragraph mark selected shows as a little extra.
            let b = if hi == usize::MAX || line.para != end.para && line.stops.last().is_some_and(|s| s.1 == b) { b + 4.0 } else { b };
            if b - a < 0.1 {
                continue;
            }
            if line.vertical {
                out.push(div().absolute().left(px(line.x * k)).top(px((line.y + a) * k)).w(px(line.h * k)).h(px((b - a) * k)).bg(hsla(0.6, 0.8, 0.5, 0.25)).into_any_element());
            } else {
                out.push(div().absolute().left(px((line.x + a) * k)).top(px(line.y * k)).w(px((b - a) * k)).h(px(line.h * k)).bg(hsla(0.6, 0.8, 0.5, 0.25)).into_any_element());
            }
        }
        out
    }

    /// The document's keys (see `editing::key_bindings`).
    fn text_actions(d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        macro_rules! mv {
            ($d:expr, $action:ty, $how:expr, $extend:expr) => {
                $d.on_action(cx.listener(|this, _: &$action, _, cx| this.move_caret($how, $extend, cx)))
            };
        }
        let d = mv!(d, ed::MoveLeft, Move::Left, false);
        let d = mv!(d, ed::MoveRight, Move::Right, false);
        let d = mv!(d, ed::MoveUp, Move::Up, false);
        let d = mv!(d, ed::MoveDown, Move::Down, false);
        let d = mv!(d, ed::SelectLeft, Move::Left, true);
        let d = mv!(d, ed::SelectRight, Move::Right, true);
        let d = mv!(d, ed::SelectUp, Move::Up, true);
        let d = mv!(d, ed::SelectDown, Move::Down, true);
        let d = mv!(d, ed::WordLeft, Move::WordLeft, false);
        let d = mv!(d, ed::WordRight, Move::WordRight, false);
        let d = mv!(d, ed::SelectWordLeft, Move::WordLeft, true);
        let d = mv!(d, ed::SelectWordRight, Move::WordRight, true);
        let d = mv!(d, ed::LineStart, Move::LineStart, false);
        let d = mv!(d, ed::LineEnd, Move::LineEnd, false);
        let d = mv!(d, ed::SelectLineStart, Move::LineStart, true);
        let d = mv!(d, ed::SelectLineEnd, Move::LineEnd, true);
        let d = mv!(d, ed::DocumentStart, Move::DocumentStart, false);
        let d = mv!(d, ed::DocumentEnd, Move::DocumentEnd, false);
        let d = mv!(d, ed::SelectDocumentStart, Move::DocumentStart, true);
        let d = mv!(d, ed::SelectDocumentEnd, Move::DocumentEnd, true);
        macro_rules! page {
            ($d:expr, $action:ty, $sign:expr, $extend:expr) => {
                $d.on_action(cx.listener(|this, _: &$action, window, cx| {
                    // The lines of a screen: the window's height over a line of the zoom's size.
                    let lines = ((f32::from(window.viewport_size().height) / (16.0 * this.zoom.unwrap_or(1.0).max(0.25))).max(5.0) as i32 - 2).max(3);
                    this.move_caret(Move::Page($sign * lines), $extend, cx)
                }))
            };
        }
        let d = page!(d, ed::PageUp, -1, false);
        let d = page!(d, ed::PageDown, 1, false);
        let d = page!(d, ed::SelectPageUp, -1, true);
        let d = page!(d, ed::SelectPageDown, 1, true);
        d.on_action(cx.listener(|this, _: &ed::DeleteWordBackward, _, cx| this.delete_word(false, cx)))
            .on_action(cx.listener(|this, _: &ed::DeleteWordForward, _, cx| this.delete_word(true, cx)))
            .on_action(cx.listener(|this, _: &ed::LineSpacingSingle, _, cx| this.set_line_spacing_multiple(240, cx)))
            .on_action(cx.listener(|this, _: &ed::LineSpacingOneAndHalf, _, cx| this.set_line_spacing_multiple(360, cx)))
            .on_action(cx.listener(|this, _: &ed::LineSpacingDouble, _, cx| this.set_line_spacing_multiple(480, cx)))
            .on_action(cx.listener(|this, _: &ed::ApplyHeading1, _, cx| this.apply_named_style(Some("heading 1"), cx)))
            .on_action(cx.listener(|this, _: &ed::ApplyHeading2, _, cx| this.apply_named_style(Some("heading 2"), cx)))
            .on_action(cx.listener(|this, _: &ed::ApplyHeading3, _, cx| this.apply_named_style(Some("heading 3"), cx)))
            .on_action(cx.listener(|this, _: &ed::ApplyNormalStyle, _, cx| this.apply_named_style(None, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleOvertype, _, cx| this.toggle_overtype(cx)))
            .on_action(cx.listener(|this, _: &ed::ChangeUserName, window, cx| this.open_user_info_panel(window, cx)))
            .on_action(cx.listener(|this, _: &ed::DeleteBackward, _, cx| this.delete_char(false, cx)))
            .on_action(cx.listener(|this, _: &ed::DeleteForward, _, cx| this.delete_char(true, cx)))
            .on_action(cx.listener(|this, _: &ed::NewParagraph, _, cx| this.enter_key(cx)))
            .on_action(cx.listener(|this, _: &ed::OutdentList, _, cx| this.outdent_list(cx)))
            .on_action(cx.listener(|this, _: &ed::InsertLineBreak, _, cx| this.type_text("\u{b}", cx)))
            .on_action(cx.listener(|this, _: &ed::InsertTab, _, cx| this.tab_key(cx)))
            .on_action(cx.listener(|this, _: &ed::InsertPageBreak, _, cx| this.type_text("\u{c}", cx)))
            .on_action(cx.listener(|this, _: &ed::InsertColumnBreak, _, cx| {
                if this.can_split_table() {
                    this.split_table(cx);
                } else {
                    this.type_text("\u{e}", cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ed::SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &ed::Copy, _, cx| this.copy_text(cx)))
            .on_action(cx.listener(|this, _: &ed::Cut, _, cx| this.cut_text(cx)))
            .on_action(cx.listener(|this, _: &ed::Paste, _, cx| this.paste_text(cx)))
            .on_action(cx.listener(|this, _: &ed::PasteKeepSource, _, cx| this.paste_keep_source(cx)))
            .on_action(cx.listener(|this, _: &ed::PastePlain, _, cx| this.paste_plain_text(cx)))
            .on_action(cx.listener(|this, _: &ed::PasteMatchFormat, _, cx| this.paste_match_format(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleBold, _, cx| this.toggle_bold(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleItalic, _, cx| this.toggle_italic(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleUnderline, _, cx| this.toggle_underline(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleDoubleUnderline, _, cx| this.toggle_underline_kind(ibis_doc_model::UnderlineStyle::Double, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleWordUnderline, _, cx| this.toggle_underline_kind(ibis_doc_model::UnderlineStyle::Words, cx)))
            .on_action(cx.listener(|this, _: &ed::InsertFootnote, _, cx| this.insert_note(false, cx)))
            .on_action(cx.listener(|this, _: &ed::InsertEndnote, _, cx| this.insert_note(true, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleBulletList, _, cx| this.toggle_list(true, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleStrike, _, cx| this.toggle_strike(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleCaps, _, cx| this.toggle_caps(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleSmallCaps, _, cx| this.toggle_small_caps(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleHidden, _, cx| this.toggle_hidden(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleSuperscript, _, cx| this.toggle_text_position(ibis_doc_model::VerticalPosition::Superscript, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleSubscript, _, cx| this.toggle_text_position(ibis_doc_model::VerticalPosition::Subscript, cx)))
            .on_action(cx.listener(|this, _: &ed::AlignLeft, _, cx| this.set_alignment(FlowAlign::Left, cx)))
            .on_action(cx.listener(|this, _: &ed::AlignCenter, _, cx| this.set_alignment(FlowAlign::Center, cx)))
            .on_action(cx.listener(|this, _: &ed::AlignRight, _, cx| this.set_alignment(FlowAlign::Right, cx)))
            .on_action(cx.listener(|this, _: &ed::AlignJustify, _, cx| this.set_alignment(FlowAlign::Justify, cx)))
            .on_action(cx.listener(|this, _: &ed::AlignDistribute, _, cx| this.set_alignment(FlowAlign::Distribute, cx)))
            .on_action(cx.listener(|this, _: &ed::GrowFont, _, cx| this.step_text_size(true, cx)))
            .on_action(cx.listener(|this, _: &ed::ShrinkFont, _, cx| this.step_text_size(false, cx)))
            .on_action(cx.listener(|this, _: &ed::ClearFormatting, _, cx| this.clear_formatting(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleCharacterBorder, _, cx| this.toggle_character_border(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleCharacterShading, _, cx| this.toggle_character_shading(cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseSentence, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Sentence, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseLower, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Lower, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseUpper, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Upper, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseTitle, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Title, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseToggle, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Toggle, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseHalfWidth, _, cx| this.convert_case(ibis_doc_model::CaseConversion::HalfWidth, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseFullWidth, _, cx| this.convert_case(ibis_doc_model::CaseConversion::FullWidth, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseKatakana, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Katakana, cx)))
            .on_action(cx.listener(|this, _: &ed::ConvertCaseHiragana, _, cx| this.convert_case(ibis_doc_model::CaseConversion::Hiragana, cx)))
            .on_action(cx.listener(|this, _: &ed::OpenFind, window, cx| this.open_find(window, cx)))
            .on_action(cx.listener(|this, _: &ed::FindNext, _, cx| this.find_next_in_direction(cx)))
            .on_action(cx.listener(|this, _: &ed::FindPrevious, _, cx| this.find_next(false, cx)))
            .on_action(cx.listener(|this, _: &ed::OpenGoTo, window, cx| this.open_go_to_panel(window, cx)))
            .on_action(cx.listener(|this, _: &ed::OpenThesaurus, window, cx| this.open_thesaurus_panel(window, cx)))
            .on_action(cx.listener(|this, _: &ed::OpenLink, window, cx| this.open_link(window, cx)))
            .on_action(cx.listener(|this, _: &ed::CopyFormat, _, cx| this.copy_format(cx)))
            .on_action(cx.listener(|this, _: &ed::PasteFormat, _, cx| this.paste_format(cx)))
            .on_action(cx.listener(|this, _: &ed::CancelFormatBrush, window, cx| this.handle_escape(window, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleFormattingMarks, _, cx| this.toggle_formatting_marks(cx)))
            .on_action(cx.listener(|this, _: &ed::UpdateFields, _, cx| this.update_fields(false, cx)))
            .on_action(cx.listener(|this, _: &ed::UnlinkFields, _, cx| this.unlink_fields(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleFieldCodes, _, cx| this.toggle_field_codes(cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleAllFieldCodes, _, cx| this.toggle_all_field_codes(cx)))
            .on_action(cx.listener(|this, _: &ed::LockField, _, cx| this.lock_fields(true, cx)))
            .on_action(cx.listener(|this, _: &ed::UnlockField, _, cx| this.lock_fields(false, cx)))
            .on_action(cx.listener(|this, _: &ed::InsertField, _, cx| this.insert_field(cx)))
            .on_action(cx.listener(|this, _: &ed::EditField, window, cx| this.open_field_panel(window, cx)))
            .on_action(cx.listener(|this, _: &ed::ToggleSplit, _, cx| this.toggle_document_split(cx)))
            .on_action(cx.listener(|this, _: &ed::OutlinePromote, _, cx| { this.change_heading_level(-1, cx); }))
            .on_action(cx.listener(|this, _: &ed::OutlineDemote, _, cx| { this.change_heading_level(1, cx); }))
            .on_action(cx.listener(|this, _: &ed::OutlineMoveUp, _, cx| { this.move_heading(true, cx); }))
            .on_action(cx.listener(|this, _: &ed::OutlineMoveDown, _, cx| { this.move_heading(false, cx); }))
    }

    /// Word's horizontal ruler over the pages: the page's width with its
    /// left and right margins shaded, according to ruler_unit (DOCX-VIEW-011, DOCX-VIEW-013).
    /// It follows the pages as they scroll sideways.
    pub(crate) fn render_ruler(&self, border: Hsla, font_family: SharedString) -> impl IntoElement + 'static {
        let font = font(font_family);
        let setup = self.presentation().flow.as_ref().map(|doc| doc.page);
        let content = self.workspace_content();
        let scroll = self.workspace_scroll.clone();
        let unit = self.ruler_unit;

        let painter = move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
            window.paint_quad(fill(bounds, rgb(RULER_BACKGROUND)));
            let (Some(setup), Some(content)) = (setup, content) else { return };
            let scale = content.scale;
            let o = bounds.origin;
            let left = o.x + scroll.offset().x + content.slide_origin.x;
            let x = |emu: f32| left + px(emu * scale);
            let (top, height) = (o.y + px(4.0), px(RULER_HEIGHT - 8.0));
            let width = setup.size.width as f32;
            let (margin_l, margin_r) = (setup.left as f32, width - setup.right as f32);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let bar = |window: &mut Window, from: f32, to: f32, color: u32| {
                    if to > from {
                        window.paint_quad(fill(Bounds::new(point(x(from), top), size(px((to - from) * scale), height)), rgb(color)));
                    }
                };
                bar(window, 0.0, margin_l, RULER_MARGIN);
                bar(window, margin_l, margin_r, RULER_TEXT_AREA);
                bar(window, margin_r, width, RULER_MARGIN);

                let step_emu = unit.tick_step_emu();
                let px_per_unit = unit.emu_per_unit() * scale;
                let candidates = unit.label_candidates();
                let every = candidates.iter().copied().find(|&n| n as f32 * px_per_unit >= 22.0).unwrap_or(*candidates.last().unwrap_or(&20));
                let tick = rgb(RULER_TICK);
                let first = -(margin_l / step_emu).floor() as i32;
                let last = ((width - margin_l) / step_emu).floor() as i32;
                for idx in first..=last {
                    let at = margin_l + idx as f32 * step_emu;
                    if idx == 0 || at < 0.0 || at > width {
                        continue;
                    }
                    let (is_major, is_medium, val) = unit.tick_info(idx);
                    let mid = top + height / 2.0;
                    if is_major {
                        if val % every == 0 {
                            let label: SharedString = val.to_string().into();
                            let run = TextRun {
                                len: label.len(),
                                font: font.clone(),
                                color: tick.into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(label, px(RULER_FONT_SIZE), &[run], None);
                            let origin = point(x(at) - shaped.width / 2.0, mid - px(RULER_FONT_SIZE * 0.65));
                            shaped.paint(origin, px(RULER_FONT_SIZE * 1.3), TextAlign::Left, None, window, cx).ok();
                        } else {
                            window.paint_quad(fill(Bounds::new(point(x(at), mid - px(1.0)), size(px(1.0), px(2.0))), tick));
                        }
                    } else if is_medium {
                        window.paint_quad(fill(Bounds::new(point(x(at), mid - px(2.0)), size(px(1.0), px(4.0))), tick));
                    } else {
                        let px_per_step = step_emu * scale;
                        if px_per_step >= 3.0 {
                            window.paint_quad(fill(Bounds::new(point(x(at), mid - px(1.0)), size(px(1.0), px(2.0))), tick));
                        }
                    }
                }
            });
        };

        div()
            .id("ruler")
            .flex_shrink_0()
            .w_full()
            .h(px(RULER_HEIGHT))
            .border_b_1()
            .border_color(border)
            .child(canvas(|_, _, _| {}, move |bounds, _, window, cx| painter(bounds, window, cx)).size_full())
    }

    /// Word's vertical ruler along the left of the pages (DOCX-VIEW-012, DOCX-VIEW-013):
    /// the page's height with its top and bottom margins shaded, according to ruler_unit
    /// from the top margin. It follows the pages as they scroll vertically.
    pub(crate) fn render_vertical_ruler(&self, border: Hsla, font_family: SharedString) -> impl IntoElement + 'static {
        let font = font(font_family);
        let setup = self.presentation().flow.as_ref().map(|doc| doc.page);
        let content = self.workspace_content();
        let scroll = self.workspace_scroll.clone();
        let pages = self.page_sizes();
        let unit = self.ruler_unit;
        let side_to_side = self.page_movement_side_to_side;

        let painter = move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
            window.paint_quad(fill(bounds, rgb(RULER_BACKGROUND)));
            let (Some(setup), Some(content)) = (setup, content) else { return };
            let scale = content.scale;
            let o = bounds.origin;
            let (left, width) = (o.x + px(4.0), px(RULER_WIDTH - 8.0));
            let step_emu = unit.tick_step_emu();
            let px_per_unit = unit.emu_per_unit() * scale;
            let candidates = unit.label_candidates();
            let every = candidates.iter().copied().find(|&n| n as f32 * px_per_unit >= 22.0).unwrap_or(*candidates.last().unwrap_or(&20));
            let tick = rgb(RULER_TICK);

            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                // Right border line of vertical ruler
                let right_edge = o.x + bounds.size.width - px(1.0);
                window.paint_quad(fill(Bounds::new(point(right_edge, o.y), size(px(1.0), bounds.size.height)), border));

                for (index, page_size) in pages.iter().enumerate() {
                    let (_, page_y) = page_place(&content, &pages, index, side_to_side);
                    let page_top = o.y + scroll.offset().y + px(page_y);
                    let page_height = page_size.height as f32;
                    let page_height_px = px(page_height * scale);
                    let page_bottom = page_top + page_height_px;

                    // Skip pages outside visible bounds
                    if page_bottom < bounds.origin.y || page_top > bounds.origin.y + bounds.size.height {
                        continue;
                    }

                    let margin_t = setup.top as f32;
                    let margin_b = setup.bottom as f32;
                    let text_top = page_top + px(margin_t * scale);
                    let text_bottom = page_bottom - px(margin_b * scale);

                    let bar = |window: &mut Window, from_y: Pixels, to_y: Pixels, color: u32| {
                        if to_y > from_y {
                            window.paint_quad(fill(Bounds::new(point(left, from_y), size(width, to_y - from_y)), rgb(color)));
                        }
                    };

                    // Top margin shaded
                    bar(window, page_top, text_top, RULER_MARGIN);
                    // Text area white
                    bar(window, text_top, text_bottom, RULER_TEXT_AREA);
                    // Bottom margin shaded
                    bar(window, text_bottom, page_bottom, RULER_MARGIN);

                    // Ticks and numbers: 0 is at text_top (top margin)
                    let first = -(margin_t / step_emu).floor() as i32;
                    let last = ((page_height - margin_t) / step_emu).floor() as i32;
                    for idx in first..=last {
                        let emu_from_margin = idx as f32 * step_emu;
                        let emu_from_page_top = margin_t + emu_from_margin;
                        if idx == 0 || emu_from_page_top < 0.0 || emu_from_page_top > page_height {
                            continue;
                        }
                        let at_y = page_top + px(emu_from_page_top * scale);
                        if at_y < bounds.origin.y - px(10.0) || at_y > bounds.origin.y + bounds.size.height + px(10.0) {
                            continue;
                        }
                        let (is_major, is_medium, val) = unit.tick_info(idx);
                        let bar_right = left + width;
                        if is_major {
                            if val % every == 0 {
                                let label: SharedString = val.to_string().into();
                                let run = TextRun {
                                    len: label.len(),
                                    font: font.clone(),
                                    color: tick.into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                let shaped = window.text_system().shape_line(label, px(RULER_FONT_SIZE), &[run], None);
                                let origin = point(left + (width - shaped.width) / 2.0, at_y - px(RULER_FONT_SIZE * 0.5));
                                shaped.paint(origin, px(RULER_FONT_SIZE * 1.3), TextAlign::Left, None, window, cx).ok();
                            } else {
                                window.paint_quad(fill(Bounds::new(point(bar_right - px(2.0), at_y), size(px(2.0), px(1.0))), tick));
                            }
                        } else if is_medium {
                            window.paint_quad(fill(Bounds::new(point(bar_right - px(4.0), at_y), size(px(4.0), px(1.0))), tick));
                        } else {
                            let px_per_step = step_emu * scale;
                            if px_per_step >= 3.0 {
                                window.paint_quad(fill(Bounds::new(point(bar_right - px(2.0), at_y), size(px(2.0), px(1.0))), tick));
                            }
                        }
                    }
                }
            });
        };

        div()
            .id("vertical-ruler")
            .flex_shrink_0()
            .w(px(RULER_WIDTH))
            .h_full()
            .child(canvas(|_, _, _| {}, move |bounds, _, window, cx| painter(bounds, window, cx)).size_full())
    }

    /// The corner box where the horizontal and vertical rulers meet (DOCX-VIEW-012, DOCX-VIEW-013).
    pub(crate) fn render_ruler_corner(border: Hsla, unit: crate::shell::editor::RulerUnit) -> Stateful<Div> {
        div()
            .id("ruler-corner")
            .flex_shrink_0()
            .w(px(RULER_WIDTH))
            .h(px(RULER_HEIGHT))
            .bg(rgb(RULER_BACKGROUND))
            .border_b_1()
            .border_r_1()
            .border_color(border)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(9.0))
            .text_color(rgb(RULER_TICK))
            .cursor_pointer()
            .child(unit.abbreviation())
    }

    /// Word's gridlines (表示 > グリッド線): light grid lines inside page margins.
    fn render_page_gridlines(w: f32, h: f32, scale: f32, setup: Option<ibis_doc_model::PageSetup>) -> impl IntoElement {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _cx| {
                let visible = window.content_mask().bounds;
                if !bounds.intersects(&visible) {
                    return;
                }
                let o = bounds.origin;
                let (margin_l, margin_r, margin_t, margin_b, pitch_emu) = match setup {
                    Some(s) => (
                        s.left as f32 * scale,
                        s.right as f32 * scale,
                        s.top as f32 * scale,
                        s.bottom as f32 * scale,
                        s.line_pitch.unwrap_or(ibis_doc_model::EMU_PER_POINT * 18),
                    ),
                    None => {
                        let d = 72.0 * EMU_PER_POINT as f32 * scale;
                        (d, d, d, d, ibis_doc_model::EMU_PER_POINT * 18)
                    }
                };
                let area_left = margin_l;
                let area_right = (w - margin_r).max(area_left);
                let area_top = margin_t;
                let area_bottom = (h - margin_b).max(area_top);
                if area_right <= area_left || area_bottom <= area_top {
                    return;
                }

                let pitch = (pitch_emu as f32 * scale).max(8.0);
                let line_color = rgb(0xd0d4dc);
                let min_thick = 1.0;

                // 水平グリッド線（横線）
                let mut y = area_top;
                while y <= area_bottom + 0.5 {
                    let r = Bounds::new(
                        point(o.x + px(area_left), o.y + px(y)),
                        size(px(area_right - area_left), px(min_thick)),
                    );
                    if r.intersects(&visible) {
                        window.paint_quad(fill(r, line_color));
                    }
                    y += pitch;
                }

                // 垂直グリッド線（縦線）
                let mut x = area_left;
                while x <= area_right + 0.5 {
                    let r = Bounds::new(
                        point(o.x + px(x), o.y + px(area_top)),
                        size(px(min_thick), px(area_bottom - area_top)),
                    );
                    if r.intersects(&visible) {
                        window.paint_quad(fill(r, line_color));
                    }
                    x += pitch;
                }
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
    }

    /// Word's formatting marks (表示 > 編集記号 ¶, DOCX-OPTIONS-008): non-printing characters (¶, ↵, →, ·, □).
    fn render_page_formatting_marks(
        lines: &[ed::LineGeom],
        page_index: usize,
        doc: Option<&FlowDocument>,
        all_open: bool,
        options: crate::shell::editor::FormattingMarksOptions,
        w: f32,
        h: f32,
        k: f32,
    ) -> impl IntoElement {
        let mut para_texts: std::collections::HashMap<u32, std::sync::Arc<[char]>> = std::collections::HashMap::new();
        if let Some(doc) = doc {
            fn collect_paras(blocks: &[Block], out: &mut std::collections::HashMap<u32, std::sync::Arc<[char]>>) {
                for b in blocks {
                    match b {
                        Block::Paragraph(p) => {
                            out.insert(p.id, p.text().chars().collect::<Vec<_>>().into());
                        }
                        Block::Table(t) => {
                            for row in &t.rows {
                                for cell in &row.cells {
                                    collect_paras(&cell.blocks, out);
                                }
                            }
                        }
                    }
                }
            }
            collect_paras(&doc.body, &mut para_texts);
            collect_paras(&doc.header, &mut para_texts);
            collect_paras(&doc.footer, &mut para_texts);
        }

        let mut mark_lines = Vec::new();
        for line in lines.iter().filter(|l| l.page == page_index) {
            let chars = para_texts.get(&line.para).cloned().unwrap_or_else(|| std::sync::Arc::from([]));
            let is_last_line = !lines.iter().any(|l| {
                l.para == line.para
                    && (l.page > line.page
                        || (l.page == line.page && (if l.vertical { l.x < line.x - 0.1 } else { l.y > line.y + 0.1 })))
            });
            mark_lines.push(MarkLine {
                para: line.para,
                x: line.x,
                y: line.y,
                h: line.h,
                stops: line.stops.clone(),
                vertical: line.vertical,
                chars,
                is_last_line,
            });
        }

        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let visible = window.content_mask().bounds;
                if !bounds.intersects(&visible) {
                    return;
                }
                let o = bounds.origin;
                let mark_color = rgb(0x8295a8);
                let font = Font::default();

                for line in &mark_lines {
                    let top = o.y + px(line.y * k);
                    let bottom = o.y + px((line.y + line.h) * k);
                    if bottom < visible.origin.y || top > visible.origin.y + visible.size.height {
                        continue;
                    }

                    // 1. 各文字位置の記号（半角スペース、全角スペース、タブ、改行）
                    let n_stops = line.stops.len();
                    for i in 0..n_stops.saturating_sub(1) {
                        let (offset, x_offset) = line.stops[i];
                        let next_x_offset = line.stops[i + 1].1;
                        let char_w = (next_x_offset - x_offset).max(0.0);

                        if let Some(&c) = line.chars.get(offset) {
                            match c {
                                ' ' if options.is_spaces_visible(all_open) => {
                                    // 半角スペース: 中央に小さなドット
                                    if !line.vertical {
                                        let cx_pt = line.x + x_offset + char_w / 2.0;
                                        let cy_pt = line.y + line.h * 0.55;
                                        let r = Bounds::new(
                                            point(o.x + px(cx_pt * k - 0.75), o.y + px(cy_pt * k - 0.75)),
                                            size(px(1.5), px(1.5)),
                                        );
                                        if r.intersects(&visible) {
                                            window.paint_quad(fill(r, mark_color));
                                        }
                                    } else {
                                        let cx_pt = line.x + line.h * 0.55;
                                        let cy_pt = line.y + x_offset + char_w / 2.0;
                                        let r = Bounds::new(
                                            point(o.x + px(cx_pt * k - 0.75), o.y + px(cy_pt * k - 0.75)),
                                            size(px(1.5), px(1.5)),
                                        );
                                        if r.intersects(&visible) {
                                            window.paint_quad(fill(r, mark_color));
                                        }
                                    }
                                }
                                '\u{3000}' if options.is_spaces_visible(all_open) => {
                                    // 全角スペース: 小さな四角枠（□）
                                    if !line.vertical {
                                        let sq_size = (char_w.min(line.h) * 0.5 * k).clamp(3.0, 9.0);
                                        let cx_px = (line.x + x_offset + char_w / 2.0) * k;
                                        let cy_px = (line.y + line.h * 0.55) * k;
                                        let r = Bounds::new(
                                            point(o.x + px(cx_px - sq_size / 2.0), o.y + px(cy_px - sq_size / 2.0)),
                                            size(px(sq_size), px(sq_size)),
                                        );
                                        if r.intersects(&visible) {
                                            let t = 1.0;
                                            let left_edge = Bounds::new(point(r.origin.x, r.origin.y), size(px(t), r.size.height));
                                            let right_edge = Bounds::new(point(r.origin.x + r.size.width - px(t), r.origin.y), size(px(t), r.size.height));
                                            let top_edge = Bounds::new(point(r.origin.x, r.origin.y), size(r.size.width, px(t)));
                                            let bottom_edge = Bounds::new(point(r.origin.x, r.origin.y + r.size.height - px(t)), size(r.size.width, px(t)));
                                            window.paint_quad(fill(left_edge, mark_color));
                                            window.paint_quad(fill(right_edge, mark_color));
                                            window.paint_quad(fill(top_edge, mark_color));
                                            window.paint_quad(fill(bottom_edge, mark_color));
                                        }
                                    }
                                }
                                '\t' if options.is_tabs_visible(all_open) => {
                                    // タブ: 右矢印（→）
                                    if !line.vertical {
                                        let font_size_px = (line.h * 0.65 * k).clamp(8.0, 16.0);
                                        let run = TextRun {
                                            len: "→".len(),
                                            font: font.clone(),
                                            color: mark_color.into(),
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        };
                                        let shaped = window.text_system().shape_line(
                                            SharedString::from("→"),
                                            px(font_size_px),
                                            &[run],
                                            None,
                                        );
                                        let start_x = line.x + x_offset;
                                        let arrow_x = start_x + (char_w - f32::from(shaped.width) / k) * 0.5;
                                        let origin = point(
                                            o.x + px(arrow_x.max(start_x + 1.0) * k),
                                            o.y + px(line.y * k + (line.h * k - font_size_px) * 0.5),
                                        );
                                        shaped.paint(origin, px(font_size_px), TextAlign::Left, None, window, cx).ok();
                                    }
                                }
                                '\u{000B}' if options.is_line_breaks_visible(all_open) => {
                                    // 手動改行 (Shift+Enter): ↵
                                    if !line.vertical {
                                        let font_size_px = (line.h * 0.7 * k).clamp(8.0, 16.0);
                                        let run = TextRun {
                                            len: "↵".len(),
                                            font: font.clone(),
                                            color: mark_color.into(),
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        };
                                        let shaped = window.text_system().shape_line(
                                            SharedString::from("↵"),
                                            px(font_size_px),
                                            &[run],
                                            None,
                                        );
                                        let origin = point(
                                            o.x + px((line.x + x_offset) * k),
                                            o.y + px(line.y * k + (line.h * k - font_size_px) * 0.5),
                                        );
                                        shaped.paint(origin, px(font_size_px), TextAlign::Left, None, window, cx).ok();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // 2. 段落末尾の段落記号（¶）
                    if line.is_last_line && options.is_paragraph_marks_visible(all_open) {
                        let end_x_offset = line.stops.last().map_or(0.0, |s| s.1);
                        if !line.vertical {
                            let font_size_px = (line.h * 0.75 * k).clamp(8.0, 16.0);
                            let run = TextRun {
                                len: "¶".len(),
                                font: font.clone(),
                                color: mark_color.into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from("¶"),
                                px(font_size_px),
                                &[run],
                                None,
                            );
                            let origin = point(
                                o.x + px((line.x + end_x_offset + 1.0) * k),
                                o.y + px(line.y * k + (line.h * k - font_size_px) * 0.5),
                            );
                            shaped.paint(origin, px(font_size_px), TextAlign::Left, None, window, cx).ok();
                        } else {
                            let font_size_px = (line.h * 0.75 * k).clamp(8.0, 16.0);
                            let run = TextRun {
                                len: "¶".len(),
                                font: font.clone(),
                                color: mark_color.into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from("¶"),
                                px(font_size_px),
                                &[run],
                                None,
                            );
                            let origin = point(
                                o.x + px(line.x * k + (line.h * k - font_size_px) * 0.5),
                                o.y + px((line.y + end_x_offset + 1.0) * k),
                            );
                            shaped.paint(origin, px(font_size_px), TextAlign::Left, None, window, cx).ok();
                        }
                    }
                }
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
    }
}

struct MarkLine {
    #[allow(dead_code)]
    para: u32,
    x: f32,
    y: f32,
    h: f32,
    stops: std::sync::Arc<[(usize, f32)]>,
    vertical: bool,
    chars: std::sync::Arc<[char]>,
    is_last_line: bool,
}

#[cfg(test)]
mod tests {
    use super::{COLLAPSED_PAGE_GAP, PAGE_GAP, SIDE_MARGIN, flow_content, page_place};
    use gpui_kit::{px, size};
    use ibis_doc_model::EmuSize;

    #[test]
    fn pages_are_stacked_and_centred() {
        let page = EmuSize::new(7_560_310, 10_692_130);
        let scale = 96.0 / 914_400.0;
        let content = flow_content(size(px(1000.0), px(600.0)), &[page; 3], Some(scale), PAGE_GAP, false);
        let (w, h) = (page.width as f32 * scale, page.height as f32 * scale);
        assert!((f32::from(content.slide_origin.x) - (1000.0 - w) / 2.0).abs() < 0.01);
        assert_eq!(f32::from(content.slide_origin.y), PAGE_GAP);
        assert!((f32::from(content.size.height) - (PAGE_GAP + 3.0 * (h + PAGE_GAP))).abs() < 0.01);
    }

    #[test]
    fn collapsed_pages_use_smaller_gap() {
        let page = EmuSize::new(7_560_310, 10_692_130);
        let scale = 96.0 / 914_400.0;
        let content = flow_content(size(px(1000.0), px(600.0)), &[page; 3], Some(scale), COLLAPSED_PAGE_GAP, false);
        let (_w, h) = (page.width as f32 * scale, page.height as f32 * scale);
        assert_eq!(f32::from(content.slide_origin.y), COLLAPSED_PAGE_GAP);
        assert!((f32::from(content.size.height) - (COLLAPSED_PAGE_GAP + 3.0 * (h + COLLAPSED_PAGE_GAP))).abs() < 0.01);
    }

    #[test]
    fn fitting_uses_the_page_width() {
        let page = EmuSize::new(7_560_310, 10_692_130);
        let content = flow_content(size(px(840.0), px(600.0)), &[page], None, PAGE_GAP, false);
        let w = page.width as f32 * content.scale;
        assert!((w - (840.0 - 2.0 * SIDE_MARGIN)).abs() < 0.01);
    }

    #[test]
    fn side_to_side_pages_are_placed_horizontally() {
        let page = EmuSize::new(7_560_310, 10_692_130);
        let scale = 96.0 / 914_400.0;
        let content = flow_content(size(px(1000.0), px(600.0)), &[page; 3], Some(scale), PAGE_GAP, true);
        let (w, h) = (page.width as f32 * scale, page.height as f32 * scale);
        assert_eq!(f32::from(content.slide_origin.x), PAGE_GAP);
        assert!((f32::from(content.slide_origin.y) - (f32::from(content.size.height) - h) / 2.0).abs() < 0.01);
        assert!((f32::from(content.size.width) - (PAGE_GAP + 3.0 * (w + PAGE_GAP))).abs() < 0.01);
        let (x0, y0) = page_place(&content, &[page; 3], 0, true);
        let (x1, _y1) = page_place(&content, &[page; 3], 1, true);
        assert_eq!(x0, PAGE_GAP);
        assert!((x1 - (PAGE_GAP + w + PAGE_GAP)).abs() < 0.01);
        assert_eq!(y0, f32::from(content.slide_origin.y));
    }
}
