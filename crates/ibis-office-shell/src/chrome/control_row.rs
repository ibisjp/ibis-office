// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Label/control rows shared by every document format. Flex wrapping keeps a control intact.
use super::segment_control::{Mode, SegmentControl, SegmentItem};
use super::*;

/// The label column's width is [`FIELD_WIDTH`] (times `.label_scale(f32)` for long labels; the controls still line up) so the
/// controls of a pane line up. A row is ONE line when the label, the gap and the control (its minimum width, or its natural
/// width for a control that does not stretch) fit the pane, and TWO lines (the label, then the control under it) when they do
/// not. The decision is the layout engine's flex wrap on the controls' measured widths: no pixel threshold is written anywhere.
#[derive(IntoElement)]
pub struct ControlRow {
    label: SharedString,
    control: Div,
    label_scale: f32,
    stacked: bool,
}
impl ControlRow {
    fn new(label: impl Into<SharedString>, control: Div) -> Self {
        Self { label: label.into(), control, label_scale: 1., stacked: false }
    }
    /// The label column is `scale` times [`FIELD_WIDTH`] wide (for long labels).
    pub fn label_scale(mut self, scale: f32) -> Self {
        self.label_scale = scale;
        self
    }
}
impl ControlRow {
    /// Always two lines: the label, then the control under it (left-aligned).
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }
}
impl RenderOnce for ControlRow {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .flex_wrap()
            .gap_2()
            .when(self.stacked, |d| d.flex_col().items_start().gap_1())
            // The label is the size of the checkboxes' and radios' (14 px), the same in every row.
            .child(div().w(FIELD_WIDTH * self.label_scale).flex_none().text_size(px(14.)).child(self.label))
            .child(self.control)
    }
}
fn adaptive(label: impl Into<SharedString>, control: Div) -> ControlRow {
    ControlRow::new(label, control)
}
/// A label and any control that stretches (a dropdown, a field…). Its minimum width is [`FIELD_WIDTH`].
pub fn control_row(label: impl Into<SharedString>, control: impl IntoElement) -> ControlRow {
    adaptive(label, div().flex_1().min_w(FIELD_WIDTH).child(control))
}
/// A label and a [`TextField`]. The field keeps its own width (and a unit set on it with `.unit("pt", px(20.))`,
/// drawn inside the field's box, adds nothing to the row's width).
pub fn text_field_row(label: impl Into<SharedString>, field: TextField) -> ControlRow {
    let min = field.width.unwrap_or(FIELD_WIDTH);
    adaptive(label, div().flex_1().min_w(min).child(field))
}
/// A label and a [`super::slider`] (the track, and the value at its left when it was made with `.value(cx)`). ONE line when the label,
/// the gap and the slider's minimum width ([`ValueSlider::min_width`]) fit the pane, TWO lines when they do not.
pub fn slider_row(label: impl Into<SharedString>, slider: super::ValueSlider) -> ControlRow {
    let min = slider.min_width();
    adaptive(label, div().flex_1().min_w(min).child(slider))
}
/// A label and a colour choice (a swatch dropdown, a swatch with its code…).
pub fn color_row(label: impl Into<SharedString>, color: impl IntoElement) -> ControlRow {
    control_row(label, color)
}
pub fn color_field_row(label: impl Into<SharedString>, field: impl IntoElement) -> ControlRow {
    color_row(label, field)
}
/// A label and the font dropdown of a pane (the font list, the name of the font chosen in `input`; `picked` is called with the
/// font chosen, which the pane applies at once). The row is ONE line when the label, the gap and the dropdown's minimum
/// width ([`FIELD_WIDTH`]) fit the pane, and TWO lines when they do not; on one line the dropdown grows up to [`FONT_DROPDOWN_WIDTH`].
pub fn font_dropdown_row(
    label: impl Into<SharedString>,
    id: &'static str,
    input: &Entity<gpui_kit::component::input::InputState>,
    cx: &Context<SlideEditor>,
    picked: impl Fn(&mut SlideEditor, &str, &mut Window, &mut Context<SlideEditor>) + Clone + 'static,
) -> ControlRow {
    adaptive(label, div().flex_1().min_w(FIELD_WIDTH).max_w(FONT_DROPDOWN_WIDTH).child(font_dropdown_in_pane(id, input, cx, picked, true)))
}
/// [`font_dropdown_row`] whose list is grouped "used in this file" (`used`) / "all" (`include_all`; with false only the used fonts are
/// listed, e.g. the "replace from" font of the font replacement).
pub fn document_font_dropdown_row(
    label: impl Into<SharedString>,
    id: &'static str,
    input: &Entity<gpui_kit::component::input::InputState>,
    used: &[String],
    include_all: bool,
    cx: &Context<SlideEditor>,
    picked: impl Fn(&mut SlideEditor, &str, &mut Window, &mut Context<SlideEditor>) + Clone + 'static,
) -> ControlRow {
    adaptive(label, div().flex_1().min_w(FIELD_WIDTH).max_w(FONT_DROPDOWN_WIDTH).child(panel_document_font_dropdown(id, input, used, include_all, cx, picked)))
}
pub fn dropdown_row(label: impl Into<SharedString>, dropdown: impl IntoElement) -> ControlRow {
    control_row(label, dropdown)
}
/// [`dropdown_row`] for a dropdown `scale` times [`FIELD_WIDTH`] wide (made with `.button_width(FIELD_WIDTH * scale)`): the row
/// keeps that width on one line and goes to two lines when it does not fit.
pub fn dropdown_row_scaled(label: impl Into<SharedString>, dropdown: impl IntoElement, scale: f32) -> ControlRow {
    adaptive(label, div().flex_1().min_w(FIELD_WIDTH * scale).child(dropdown))
}

type ToolbarClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// One button of a [`toolbar_row`]: the toolbar's own ghost button (28 px, an icon and/or a text), pressed when `on`.
pub struct ToolbarItem {
    id: ElementId,
    icon: Option<&'static str>,
    text: Option<SharedString>,
    tooltip: Option<SharedString>,
    on: bool,
    disabled: bool,
    click: Option<ToolbarClick>,
}
impl ToolbarItem {
    /// An icon button (`format_bold`, `format_italic`, …: a Material icon name).
    pub fn icon(id: impl Into<ElementId>, icon: &'static str) -> Self {
        Self { id: id.into(), icon: Some(icon), text: None, tooltip: None, on: false, disabled: false, click: None }
    }
    /// A text button (`A`, `x²`…). The text is shown as given: translate it before.
    #[allow(dead_code)] // Shared-control option (a text button in a toolbar row); no row uses one yet.
    pub(crate) fn text(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self { id: id.into(), icon: None, text: Some(text.into()), tooltip: None, on: false, disabled: false, click: None }
    }
    /// Pressed: a toggle that is on.
    pub fn on(mut self, on: bool) -> Self { self.on = on; self }
    #[allow(dead_code)] // Shared-control option; no toolbar-row button is disabled yet.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self { self.tooltip = Some(tooltip.into()); self }
    pub fn on_click(mut self, click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.click = Some(Rc::new(click));
        self
    }
}
/// The toolbar-coloured strip of buttons (B I U…), as a control. [`toolbar_row`] puts a label in front of it.
pub(crate) fn toolbar_group(items: impl IntoIterator<Item = ToolbarItem>) -> Div {
    let mut group = h_flex().flex_none().items_center().gap(px(1.)).p(px(2.)).rounded(px(6.)).bg(rgb(TOOLBAR_BG));
    for item in items {
        let color: Hsla = if item.disabled { disabled_color() } else if item.on { rgb(0x041e49).into() } else { rgb(ICON).into() };
        let mut button = action_button(item.id).ghost().xsmall().h(px(BUTTON_HEIGHT)).min_w(px(BUTTON_HEIGHT)).px(px(3.)).rounded(px(4.))
            .selected(item.on).disabled(item.disabled)
            .child(h_flex().items_center().justify_center()
                .when_some(item.icon, |d, name| d.child(icon(name, super::TOOLBAR_ICON_SIZE, color)))
                .when_some(item.text, |d, text| d.child(div().px(px(3.)).text_size(px(14.)).text_color(color).child(text))));
        if let Some(tooltip) = item.tooltip { button = button.tooltip(tooltip); }
        if let Some(click) = item.click { button = button.on_click(move |e, w, cx| click(e, w, cx)); }
        group = group.child(button);
    }
    group
}
/// A label and a toolbar-style button group (B I U…). The group does not stretch: it is one line with the label when its
/// measured width fits, and under the label when it does not.
pub fn toolbar_row(label: impl Into<SharedString>, items: impl IntoIterator<Item = ToolbarItem>) -> ControlRow {
    adaptive(label, toolbar_group(items))
}

pub struct SegmentControlRow {
    mode: Mode,
    label: Option<SharedString>,
    items: Vec<SegmentItem>,
    available: Option<Pixels>,
}
impl SegmentControlRow {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            label: None,
            items: vec![],
            available: None,
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn available_width(mut self, width: Pixels) -> Self {
        self.available = Some(width);
        self
    }
    pub fn child(mut self, item: SegmentItem) -> Self {
        self.items.push(item);
        self
    }
    pub fn children(mut self, items: impl IntoIterator<Item = SegmentItem>) -> Self {
        self.items.extend(items);
        self
    }
}
impl IntoElement for SegmentControlRow {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for SegmentControlRow {
    type RequestLayoutState = (AnyElement, std::rc::Rc<std::cell::Cell<Pixels>>);
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        self.items.first().map(|i| i.id.clone())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measured = id
            .map(|id| {
                window.with_element_state(
                    id,
                    |old: Option<std::rc::Rc<std::cell::Cell<Pixels>>>, _| {
                        let state =
                            old.unwrap_or_else(|| std::rc::Rc::new(std::cell::Cell::new(px(264.))));
                        (state.clone(), state)
                    },
                )
            })
            .unwrap_or_default();
        let available = self.available.unwrap_or_else(|| measured.get()).max(px(1.));
        let mode = match self.mode {
            Mode::Intrinsic => Mode::Intrinsic,
            Mode::Proportional { width } => Mode::Proportional {
                width: width.min(available),
            },
        };
        let control = SegmentControl::new(mode)
            .available_width(available)
            .children(self.items.clone());
        let mut row = h_flex().w_full().items_center().flex_wrap().gap_2();
        if let Some(label) = &self.label {
            row = row.child(div().w(FIELD_WIDTH).flex_none().child(label.clone()));
        }
        let mut row = row
            .child(div().flex_none().child(control))
            .into_any_element();
        (row.request_layout(window, cx), (row, measured))
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.available.is_none() && (state.1.get() - bounds.size.width).abs() > px(0.5) {
            state.1.set(bounds.size.width);
            window.refresh();
        }
        state.0.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        state.0.paint(window, cx);
    }
}

/// The row wraps as a whole, based on the same GPUI measurement used by the control itself.
#[derive(IntoElement)]
pub struct LabeledSegments {
    pub label: SharedString,
    pub items: Vec<super::segment_control::SegmentItem>,
    pub available: Pixels,
}
impl RenderOnce for LabeledSegments {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        SegmentControlRow::new(Mode::Intrinsic)
            .label(self.label)
            .available_width(self.available)
            .children(self.items)
    }
}

/// High-level public entry point for a labeled segmented row.
pub fn segment_control_row(
    label: impl Into<SharedString>,
    items: impl IntoIterator<Item = SegmentItem>,
) -> SegmentControlRow {
    SegmentControlRow::new(Mode::Intrinsic)
        .label(label)
        .children(items)
}
pub fn color_text_field_row(label: impl Into<SharedString>, field: impl IntoElement) -> ControlRow {
    color_field_row(label, field)
}
