// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An equal-width button group: the buttons are measured first, and every button takes the widest one's width.
use super::{ActionButtonKind, ActionButtonStyle, styled_action_button};
use gpui_kit::component::button::Button;
use gpui_kit::component::Disableable as _;
use gpui_kit::*;
use std::rc::Rc;

type Click = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// One button of a [`ButtonGroup`].
#[derive(Clone)]
pub struct ButtonGroupItem {
    id: ElementId,
    label: SharedString,
    kind: ActionButtonKind,
    disabled: bool,
    click: Option<Click>,
}
impl ButtonGroupItem {
    /// `label` is shown as given: translate it before.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self { id: id.into(), label: label.into(), kind: ActionButtonKind::Normal, disabled: false, click: None }
    }
    pub fn kind(mut self, kind: ActionButtonKind) -> Self { self.kind = kind; self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn on_click(mut self, click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.click = Some(Rc::new(click));
        self
    }
    fn button(&self, cx: &App, width: Option<Pixels>) -> Button {
        let click = self.click.clone();
        let mut button = styled_action_button(self.id.clone(), ActionButtonStyle { kind: self.kind }, Some(cx))
            .label(self.label.clone()).disabled(self.disabled);
        if let Some(width) = width { button = button.w(width).max_w_full(); }
        if let Some(click) = click { button = button.on_click(move |e, w, cx| click(e, w, cx)); }
        button
    }
}

/// Buttons of one width. Each button is laid out at its natural width first (the label's measured width plus the
/// button's own padding); the widest becomes the width of all of them. Side by side they wrap onto the next line when
/// the pane is too narrow for them (a button never grows wider than the pane); `.stacked()` puts them one under another.
pub struct ButtonGroup {
    items: Vec<ButtonGroupItem>,
    stacked: bool,
}
pub fn button_group(items: impl IntoIterator<Item = ButtonGroupItem>) -> ButtonGroup {
    ButtonGroup { items: items.into_iter().collect(), stacked: false }
}
impl ButtonGroup {
    pub fn stacked(mut self) -> Self { self.stacked = true; self }
}
impl IntoElement for ButtonGroup {
    type Element = Self;
    fn into_element(self) -> Self { self }
}
impl Element for ButtonGroup {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, AnyElement) {
        let widest = self.items.iter().map(|item| {
            let mut probe = item.button(cx, None).into_any_element();
            probe.layout_as_root(size(AvailableSpace::MaxContent, AvailableSpace::MaxContent), window, cx).width
        }).fold(px(0.), |a, b| a.max(b));
        let buttons: Vec<Button> = self.items.iter().map(|item| item.button(cx, Some(widest))).collect();
        let mut root = if self.stacked {
            gpui_kit::component::v_flex().items_start().gap_2().children(buttons).max_w_full().into_any_element()
        } else {
            gpui_kit::component::h_flex().w_full().flex_wrap().gap_2().children(buttons).into_any_element()
        };
        (root.request_layout(window, cx), root)
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, root: &mut AnyElement, window: &mut Window, cx: &mut App) {
        root.prepaint(window, cx);
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, root: &mut AnyElement, _: &mut (), window: &mut Window, cx: &mut App) {
        root.paint(window, cx);
    }
}
