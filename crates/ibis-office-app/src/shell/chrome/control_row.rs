// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Label/control rows shared by every document format. Flex wrapping keeps a control intact.
use super::segment_control::{Mode, SegmentControl, SegmentItem};
use super::*;

pub(crate) fn control_row(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .flex_wrap()
        .gap_2()
        .child(div().w(FIELD_WIDTH).flex_none().child(label.into()))
        .child(div().flex_1().min_w(FIELD_WIDTH).child(control))
}
pub(crate) fn text_field_row(label: impl Into<SharedString>, field: TextField) -> Div {
    let min = field.width.unwrap_or(FIELD_WIDTH)
        + field.unit.as_ref().map_or(px(0.), |(_, w)| *w + px(4.));
    h_flex()
        .w_full()
        .items_center()
        .flex_wrap()
        .gap_2()
        .child(div().w(FIELD_WIDTH).flex_none().child(label.into()))
        .child(div().flex_1().min_w(min).child(field))
}
pub(crate) fn color_field_row(label: impl Into<SharedString>, field: impl IntoElement) -> Div {
    control_row(label, field)
}
pub(crate) fn dropdown_row(label: impl Into<SharedString>, dropdown: impl IntoElement) -> Div {
    control_row(label, dropdown)
}

pub(crate) struct SegmentControlRow {
    mode: Mode,
    label: Option<SharedString>,
    items: Vec<SegmentItem>,
    available: Option<Pixels>,
}
impl SegmentControlRow {
    pub(crate) fn new(mode: Mode) -> Self {
        Self {
            mode,
            label: None,
            items: vec![],
            available: None,
        }
    }
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub(crate) fn available_width(mut self, width: Pixels) -> Self {
        self.available = Some(width);
        self
    }
    pub(crate) fn child(mut self, item: SegmentItem) -> Self {
        self.items.push(item);
        self
    }
    pub(crate) fn children(mut self, items: impl IntoIterator<Item = SegmentItem>) -> Self {
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
pub(crate) struct LabeledSegments {
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
pub(crate) fn segment_control_row(
    label: impl Into<SharedString>,
    items: impl IntoIterator<Item = SegmentItem>,
) -> SegmentControlRow {
    SegmentControlRow::new(Mode::Intrinsic)
        .label(label)
        .children(items)
}
pub(crate) fn color_text_field_row(label: impl Into<SharedString>, field: impl IntoElement) -> Div {
    color_field_row(label, field)
}
