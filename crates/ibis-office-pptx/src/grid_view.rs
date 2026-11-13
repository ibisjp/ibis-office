// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Slide Sorter (スライド一覧 / グリッド表示) view for presentations.
//!
//! Shows all slides in a scrollable multi-column grid, allowing rapid navigation,
//! reordering by drag-and-drop, and overview of the presentation structure.

use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::thumbnails::DraggedSlide;
use crate::render::shape_view::{shape_element, slide_backdrop};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::ext::*;

const GRID_CARD_WIDTH: f32 = 220.0;

/// What follows the pointer while a slide is dragged in the grid view.
struct DraggedGridSlideView(usize);

impl Render for DraggedGridSlideView {
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

#[ibis_office_macros::editor_ext(GridView)]
impl SlideEditor {
    /// Whether the slide sorter grid view is active.
    pub(crate) fn is_grid_view(&self) -> bool {
        self.pptx().map_or(false, |p| p.grid_view)
    }

    /// Toggle between normal editor view and slide sorter grid view.
    pub(crate) fn toggle_grid_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            pptx.grid_view = !pptx.grid_view;
            if pptx.grid_view {
                pptx.outline_view = false;
                pptx.notes_page_view = false;
                self.commit_edit(window, cx);
            }
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// Close the slide sorter grid view and return to normal slide editor view.
    pub(crate) fn close_grid_view(&mut self, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            if pptx.grid_view {
                pptx.grid_view = false;
                cx.notify();
            }
        }
    }

    /// Select a slide and exit the grid view to edit it directly.
    pub(crate) fn open_slide_from_grid(&mut self, slide_id: ibis_doc_model::SlideId, window: &mut Window, cx: &mut Context<Self>) {
        self.select_slide(slide_id, window, cx);
        self.close_grid_view(cx);
    }

    /// Render the slide sorter grid view filling the workspace.
    pub(crate) fn render_slide_grid_view(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_bg = cx.theme().background;
        let theme_muted = cx.theme().muted;
        let theme_border = cx.theme().border;
        let theme_fg = cx.theme().foreground;
        let theme_muted_fg = cx.theme().muted_foreground;
        let theme_primary = cx.theme().primary;
        let pres = self.presentation();
        let current_id = self.current_slide;
        let slide_count = pres.slides.len();

        let header = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .px_6()
            .py_3()
            .border_b_1()
            .border_color(theme_border)
            .bg(theme_bg)
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("slide-grid-close")
                            .p_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme_muted))
                            .child(Icon::new(Material("arrow_left")).size(px(20.)).text_color(theme_fg))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| this.close_grid_view(cx))),
                    )
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme_fg)
                            .child(crate::shell::i18n::text("pptx.grid-view.slide-sorter")),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(theme_muted_fg)
                    .child(crate::shell::i18n::format("pptx.grid-view.n-slides", &[("n", &slide_count)])),
            );

        let sections = &pres.sections;
        let has_sections = !sections.is_empty() && pres.valid_sections(sections);

        let grid_body = if has_sections {
            let mut sec_sections = Vec::new();
            for sec in sections {
                let sec_name = sec.name.clone();
                let mut sec_cards = Vec::new();
                for (idx, slide) in pres.slides.iter().enumerate() {
                    if sec.slides.contains(&slide.id) {
                        sec_cards.push(self.build_card(idx, slide, current_id, cx));
                    }
                }

                let sec_id = sec.id.clone();
                let click_sec_id = sec.id.clone();
                let sec_header = h_flex()
                    .id(("pptx-grid-section-header", sec_sections.len()))
                    .w_full()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .px_1()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(theme_border)
                    .on_click(cx.listener(move |this, _, w, cx| this.select_section(&click_sec_id, w, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            this.open_section_context_menu(&sec_id, event.position, w, cx);
                        }),
                    )
                    .child(Icon::new(Material("view_quilt")).size(px(16.)).text_color(theme_primary))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(14.))
                            .text_color(theme_fg)
                            .child(sec_name),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme_muted_fg)
                            .child(crate::shell::i18n::format("_pptx.count-slides", &[("count", &sec_cards.len())])),
                    );

                let cards_flex = h_flex()
                    .flex_wrap()
                    .gap_6()
                    .py_4()
                    .children(sec_cards);

                sec_sections.push(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .mb_6()
                        .child(sec_header)
                        .child(cards_flex),
                );
            }
            v_flex().w_full().children(sec_sections)
        } else {
            let mut cards = Vec::new();
            for (idx, slide) in pres.slides.iter().enumerate() {
                cards.push(self.build_card(idx, slide, current_id, cx));
            }
            let cards_flex = h_flex().flex_wrap().gap_6().children(cards);
            v_flex().w_full().child(cards_flex)
        };

        v_flex()
            .id("slide-grid-view")
            .key_context(crate::shell::editor::CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::shell::actions::CancelTool, _, cx| this.close_grid_view(cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let k = &event.keystroke;
                if k.key == "escape" {
                    cx.stop_propagation();
                    this.close_grid_view(cx);
                    return;
                }
                if (k.modifiers.platform || k.modifiers.control) && !k.modifiers.shift && !k.modifiers.alt {
                    match k.key.as_str() {
                        "d" => {
                            cx.stop_propagation();
                            this.duplicate_current_slide(window, cx);
                        }
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
                } else if k.key == "delete" || k.key == "backspace" {
                    cx.stop_propagation();
                    this.delete_slide(window, cx);
                } else if k.key == "enter" {
                    cx.stop_propagation();
                    this.close_grid_view(cx);
                }
            }))
            .size_full()
            .bg(theme_muted)
            .child(header)
            .child(
                div()
                    .id("slide-grid-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_6()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            let s_id = this.current_slide;
                            this.open_slide_context_menu(s_id, event.position, window, cx);
                        }),
                    )
                    .child(grid_body),
            )
    }

    fn build_card(
        &self,
        index: usize,
        slide: &ibis_doc_model::Slide,
        current_id: ibis_doc_model::SlideId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme_border = cx.theme().border;
        let theme_muted_fg = cx.theme().muted_foreground;
        let theme_primary = cx.theme().primary;
        let theme_accent = cx.theme().accent;
        let pres = self.presentation();
        let slide_id = slide.id;
        let is_current = slide_id == current_id;
        let is_hidden = slide.hidden;
        let size = pres.size_of(slide);
        let scale = GRID_CARD_WIDTH / size.width.max(1) as f32;
        let card_h = size.height as f32 * scale;

        let ready = self.resources.preload.slide_ready(slide);
        self.resources.set_wrap(slide);
        let shapes: Vec<_> = slide
            .shown_shapes()
            .filter(|_| ready)
            .map(|shape| shape_element(shape, shape.frame, scale, &self.resources, None))
            .collect();

        v_flex()
            .id(SharedString::from(format!("slide-grid-card-{}", index + 1)))
            .gap_1()
            .cursor_pointer()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .px_1()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(if is_current { FontWeight::BOLD } else { FontWeight::NORMAL })
                            .text_color(if is_current { theme_primary } else { theme_muted_fg })
                            .child(format!("{}.", index + 1)),
                    )
                    .when(is_hidden, |d| {
                        d.child(
                            Icon::new(Material("visibility_off"))
                                .size(px(14.))
                                .text_color(theme_muted_fg),
                        )
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(GRID_CARD_WIDTH))
                    .h(px(card_h))
                    .rounded_sm()
                    .overflow_hidden()
                    .bg(crate::shell::palette::doc(0xffffff))
                    .border(if is_current { px(2.5) } else { px(1.0) })
                    .border_color(if is_current {
                        theme_primary
                    } else {
                        theme_border
                    })
                    .hover(|s| {
                        if !is_current {
                            s.border_color(theme_primary)
                        } else {
                            s
                        }
                    })
                    .when(is_hidden, |d| d.opacity(0.55))
                    .child(slide_backdrop(slide, scale, &self.resources))
                    .children(shapes),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                if e.click_count == 2 {
                    cx.stop_propagation();
                    this.open_slide_from_grid(slide_id, window, cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_slide(slide_id, window, cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_slide_context_menu(slide_id, event.position, window, cx);
            }))
            .on_drag(DraggedSlide { slide: slide_id, number: index + 1 }, |dragged, _, _, cx| {
                cx.new(|_| DraggedGridSlideView(dragged.number))
            })
            .drag_over::<DraggedSlide>(move |style, _, _, _| {
                style.border_color(theme_primary).bg(theme_accent)
            })
            .on_drop(cx.listener(move |this, dragged: &DraggedSlide, window, cx| {
                if dragged.slide != slide_id {
                    this.run(vec![ibis_doc_model::Command::MoveSlide { slide: dragged.slide, index }], cx);
                    this.select_slide(dragged.slide, window, cx);
                }
            }))
            .into_any_element()
    }
}
