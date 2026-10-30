// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The status bar: the page, the format's items, hints, and the zoom
//! (− slider + percentage ▲ fit) at the bottom right, as in the mock.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::editor::{MAX_ZOOM, MIN_ZOOM, SlideEditor, Tool, zoom_levels};
use crate::shell::format::with_format;

impl SlideEditor {
    /// The status bar's texts, left to right: the page, the format's items,
    /// the hint (the zoom is not among them). For the scenario tests too.
    pub(crate) fn status_texts(&self) -> Vec<SharedString> {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let hint: SharedString = match (self.tool, self.editing.is_some(), &self.status) {
            (Tool::TextBox, _, _) => crate::shell::i18n::text("status.hint.textbox"),
            (_, true, _) => crate::shell::i18n::text("status.hint.esc"),
            (_, _, Some(status)) => status.clone(),
            _ if crate::render::web_image::curl_missing_notice() => "xlsx.image.curl-missing".into(),
            _ => "".into(),
        };
        // In the UI's language (the tables' texts by the Japanese one).
        let tr = |t: &str| crate::shell::i18n::text(t);
        let mut texts = vec![SharedString::from(format!("{} {} / {}", tr(with_format!(self, PAGE_NOUN)), index + 1, slides.len()))];
        texts.extend(with_format!(self, status_items(self)).into_iter().map(|t| tr(&t)));
        texts.push(tr(&hint));
        texts
    }

    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut texts = self.status_texts();
        let hint = texts.pop().unwrap_or_default();
        let layout = self.status_layout.clone();
        // A workbook's sheet tabs scroll with a scrollbar here, from the
        // tabs' left edge to the zoom slider.
        let tab_scrollbar = self.sheet_state().and_then(|state| {
            let scroll = state.tab_scroll.clone();
            let [bar_left, bar_right, zoom_left] = layout.get();
            let left = f32::from(scroll.bounds().left()) - bar_left;
            let right = zoom_left - bar_left - 8.0;
            // Only while the tabs scroll, and where there is room.
            let visible = bar_right > bar_left && right - left > 24.0 && scroll.max_offset().x > px(0.5);
            visible.then(|| {
                let width = px(right - left);
                let wheel = scroll.clone();
                div().absolute().top_0().h(px(12.)).left(px(left)).w(width)
                // The wheel over the bar scrolls the tabs sideways, a vertical
                // one too (as over the tabs themselves).
                .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(px(20.));
                    let amount = if delta.x.abs() >= delta.y.abs() { delta.x } else { delta.y };
                    let (offset, max) = (wheel.offset(), wheel.max_offset().x);
                    let x = (offset.x + amount).clamp(-max, px(0.));
                    wheel.set_offset(point(x, offset.y));
                    window.refresh();
                    cx.stop_propagation();
                })
                .child(
                    div().absolute().inset_0().child(
                        Scrollbar::new(&TabsScroll { handle: scroll, width })
                            .id("sheet-tabs-scrollbar")
                            .axis(ScrollbarAxis::Horizontal)
                            .viewport_from_layout(),
                    ),
                )
            })
        });
        let is_workbook = self.is_workbook();
        let (left_texts, right_texts) = if is_workbook && !texts.is_empty() {
            let right = texts.split_off(1);
            (texts, right)
        } else {
            (texts, Vec::new())
        };
        let right_layout = self.status_layout.clone();
        h_flex()
            .relative()
            .h(px(26.))
            .flex_shrink_0()
            .px_3()
            .gap_4()
            .items_center()
            .text_xs()
            .text_color(theme.muted_foreground)
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(theme.title_bar_border)
            .child(crate::shell::editor::bounds_observer({
                let layout = layout.clone();
                move |bounds, window, _| {
                    let mut now = layout.get();
                    let was = now;
                    (now[0], now[1]) = (f32::from(bounds.left()), f32::from(bounds.right()));
                    layout.set(now);
                    // Drawn again once with a measure that changed (a window
                    // resized), so the scrollbar is not a frame behind.
                    if (was[0] - now[0]).abs() > 0.5 || (was[1] - now[1]).abs() > 0.5 {
                        window.refresh();
                    }
                }
            }))
            .children(left_texts)
            .child(div().flex_1().child(hint))
            .child(
                h_flex()
                    .relative()
                    .gap_4()
                    .items_center()
                    // Where the right-aligned summary + zoom controls begin:
                    // the sheet tabs' scrollbar ends before it.
                    .child(crate::shell::editor::bounds_observer(move |bounds, window, _| {
                        let mut now = right_layout.get();
                        let was = now[2];
                        now[2] = f32::from(bounds.left());
                        right_layout.set(now);
                        if (was - now[2]).abs() > 0.5 {
                            window.refresh();
                        }
                    }))
                    .children(right_texts)
                    .child(self.render_zoom(cx)),
            )
            .children(tab_scrollbar)
    }

    /// Zoom of the status bar: − / slider / +, the percentage with ▲ for
    /// the list of zooms, and fitting the slide to the window.
    fn render_zoom(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.current_zoom();
        // A little slack, so a zoom rounded onto a limit still counts as there.
        let (at_min, at_max) = (zoom <= MIN_ZOOM * 1.001, zoom >= MAX_ZOOM * 0.999);
        let bounds = self.zoom_controls_bounds.clone();
        h_flex()
            .relative()
            .child(crate::shell::editor::bounds_observer(move |b, _, _| bounds.set(Some(b))))
            .gap_1()
            .items_center()
            .child(
                zoom_step_button("zoom-out", IconName::Minus, "view.zoom-out", at_min)
                    .when(!at_min, |d| d.on_click(cx.listener(|this, _, window, cx| this.step_zoom(false, window, cx)))),
            )
            .child(div().w(px(120.)).child(Slider::new(&self.zoom_slider)))
            .child(
                zoom_step_button("zoom-in", IconName::Plus, "view.zoom-in", at_max)
                    .when(!at_max, |d| d.on_click(cx.listener(|this, _, window, cx| this.step_zoom(true, window, cx)))),
            )
            .child(self.render_zoom_menu(cx))
            .when(with_format!(self, CAN_FIT), |d| d.child(
                Button::new("zoom-fit")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Fullscreen)
                    .tooltip(crate::shell::i18n::text(with_format!(self, FIT_TOOLTIP)))
                    .selected(self.zoom.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.set_zoom(None, window, cx))),
            ))
    }

    /// The percentage with the pulldown's arrow (up: the list opens upwards), as one button, and the list of zooms it opens: the
    /// zooms of `zoom_levels`, 「指定」 (a zoom typed in), and 「画面にフィット」
    /// (not for a worksheet, which is not fitted from here).
    fn render_zoom_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let shown = (self.current_zoom() * 100.0).round() as u32;
        let fitted = self.zoom.is_none();
        let can_fit = with_format!(self, CAN_FIT);
        let input = self.zoom_input.clone();
        let on_open_change = {
            let editor = editor.clone();
            move |open: &bool, window: &mut Window, cx: &mut App| {
                editor.update(cx, |this, cx| this.set_zoom_menu_open(*open, window, cx)).ok();
            }
        };
        Popover::new("zoom-menu")
            .anchor(Anchor::BottomRight)
            .open(self.zoom_menu_open)
            .on_open_change(on_open_change)
            .trigger(ZoomTrigger { percent: format!("{:.0}%", self.current_zoom() * 100.0).into(), selected: false, bounds: self.zoom_trigger_bounds.clone() })
            .p_1()
            .min_w(px(132.))
            .content(move |_, _, cx| {
                let theme = cx.theme();
                let (hover, muted) = (theme.accent, theme.muted_foreground);
                // A row of the list, with a check mark if it is the zoom shown.
                let row = |id: ElementId, checked: bool| {
                    crate::shell::chrome::dropdown_list::option(id,checked,false,true)
                        .h(px(24.)).px_2().gap_2().text_sm()
                        .child(div().w(px(12.)).children(crate::shell::chrome::dropdown_list::checkmark(checked,true,crate::shell::palette::rgb(0x1f1f1f).into())))
                };
                let choose = |zoom: Option<f32>| {
                    let editor = editor.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        editor
                            .update(cx, |this, cx| {
                                this.set_zoom(zoom, window, cx);
                                this.set_zoom_menu_open(false, window, cx);
                            })
                            .ok();
                    }
                };
                let levels = zoom_levels().into_iter().map(|percent| {
                    row(("zoom-level", percent as usize).into(), !fitted && percent == shown)
                        .cursor_pointer()
                        .hover(|s| s.bg(hover))
                        .child(SharedString::from(format!("{percent}%")))
                        .on_click(choose(Some(percent as f32 / 100.0)))
                });
                v_flex()
                    .children(levels)
                    .child(
                        row("zoom-custom".into(), false)
                            .child(crate::shell::i18n::text("view.zoom-custom"))
                            .child(Input::new(&input).xsmall().w(px(56.)))
                            .child(div().text_color(muted).child("%")),
                    )
                    .when(can_fit, |d| {
                        d.child(
                            row("zoom-fit-item".into(), fitted)
                                .cursor_pointer()
                                .hover(|s| s.bg(hover))
                                .child(crate::shell::i18n::text("view.zoom-fit"))
                                .on_click(choose(None)),
                        )
                    })
            })
    }

    /// Open or close the list of zooms; opening puts the zoom shown in 「指定」.
    pub(crate) fn set_zoom_menu_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open {
            let percent = format!("{:.0}", self.current_zoom() * 100.0);
            self.zoom_input.update(cx, |input, cx| input.set_value(percent, window, cx));
        } else if self.zoom_menu_open {
            self.focus_handle.focus(window, cx);
        }
        self.zoom_menu_open = open;
        cx.notify();
    }
}

/// The − / + button of the zoom: the same hover (and size) as the percentage's button beside it.
fn zoom_step_button(id: &'static str, icon: IconName, tooltip: &'static str, disabled: bool) -> Stateful<Div> {
    let hover = hsla(0.0, 0.0, 0.27, 0.08);
    div()
        .id(id)
        .size(px(22.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .when(!disabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .child(gpui_kit::component::Icon::new(icon).size(px(14.)).when(disabled, |i| i.opacity(0.4)))
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(tooltip)).build(window, cx))
}

/// The zoom percentage with the toolbar's pulldown arrow turned up: all of it is the button of the list of zooms, with the hover of the toolbar's buttons.
#[derive(IntoElement)]
struct ZoomTrigger {
    percent: SharedString,
    selected: bool,
    bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl gpui_kit::component::Selectable for ZoomTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ZoomTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let hover = hsla(0.0, 0.0, 0.27, 0.08);
        div()
            .id("zoom-menu-button")
            .relative()
            .h(px(22.))
            .pl_1()
            .flex()
            .items_center()
            .rounded_sm()
            .cursor_pointer()
            .when(self.selected, |d| d.bg(hover))
            .hover(move |s| s.bg(hover))
            .child(crate::shell::editor::bounds_observer({
                let cell = self.bounds.clone();
                move |bounds, _, _| cell.set(Some(bounds))
            }))
            .child(div().min_w(px(36.)).text_right().child(self.percent))
            .child(gpui_kit::component::Icon::new(crate::shell::icons::Material("arrow_drop_down")).size(px(16.)).rotate(percentage(0.5)))
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("view.zoom-menu")).build(window, cx))
    }
}

/// `n` with a comma every three digits, as in `12,345`.
pub(crate) fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}

/// The sheet tabs' scroll handle as the scrollbar in the status bar sees it:
/// the scrollbar is `width` wide (it ends before the zoom controls), which is
/// the viewport the thumb travels in, so the content is the tabs' scrolling
/// range plus that width (the handle's own viewport is wider: the thumb would
/// stop short of the end).
#[derive(Clone)]
struct TabsScroll {
    handle: gpui::ScrollHandle,
    width: Pixels,
}

impl ScrollbarHandle for TabsScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        let bounds = self.handle.bounds();
        Bounds::new(bounds.origin, gpui_kit::size(self.width, bounds.size.height))
    }

    fn offset(&self) -> Point<Pixels> {
        self.handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.handle.set_offset(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        gpui_kit::size(self.width + self.handle.max_offset().x, self.handle.bounds().size.height)
    }
}
