// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A form whose label column is as wide as its widest label. The labels are measured unwrapped (max-content, at the
//! labels' 14 px), the widest sets the column, and every control of the form starts at the same x (label column + gap).
//! Dialogs with many rows (page setup) use this instead of a fixed label width, so a long label (or a longer English one)
//! neither wraps nor leaves a wide empty column, and the controls still line up.
use super::*;

/// The gap between the label column and the controls.
pub const LABEL_GAP: f32 = 8.;
/// The labels' text size, the same as the checkboxes' and radios'.
pub const LABEL_SIZE: f32 = 14.;

enum Item {
    /// A label and its control (the control starts at the form's common x).
    Row(SharedString, AnyElement),
    /// An element across the form's width (a note, a sample, a stacked group).
    Full(AnyElement),
    /// An element under the controls (its left edge is the controls' x): a status line.
    Note(AnyElement),
}

#[derive(IntoElement)]
pub struct LabelForm {
    id: SharedString,
    items: Vec<Item>,
    gap: f32,
}

/// `id` names the form (the scenarios read the measured boxes `<id>-label-<n>` (the column cell), `<id>-text-<n>` (the label's own width) and `<id>-control-<n>`).
pub fn label_form(id: impl Into<SharedString>) -> LabelForm {
    LabelForm { id: id.into(), items: Vec::new(), gap: 10. }
}

impl LabelForm {
    pub fn row(mut self, label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        self.items.push(Item::Row(label.into(), control.into_any_element()));
        self
    }
    pub fn note(mut self, element: impl IntoElement) -> Self {
        self.items.push(Item::Note(element.into_any_element()));
        self
    }
    pub fn full(mut self, element: impl IntoElement) -> Self {
        self.items.push(Item::Full(element.into_any_element()));
        self
    }
}

/// The unwrapped width of a label at the label size.
fn measure(label: &SharedString, window: &mut Window, cx: &mut App) -> f32 {
    let mut probe = div().text_size(px(LABEL_SIZE)).whitespace_nowrap().flex_none().child(label.clone()).into_any_element();
    f32::from(probe.layout_as_root(size(AvailableSpace::MaxContent, AvailableSpace::MaxContent), window, cx).width)
}

/// The label column: the widest of the labels' unwrapped widths (rounded up to a whole pixel).
pub fn label_column_width(widths: &[f32]) -> f32 {
    widths.iter().copied().fold(0., f32::max).ceil()
}

#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
fn observe(d: Div, key: String) -> Div {
    d.relative().child(canvas(|_, _, _| (), move |b, _, _, _| crate::scenario::button_probe::record_bounds(&key, b)).absolute().inset_0())
}
#[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
fn observe(d: Div, _key: String) -> Div {
    d
}

impl RenderOnce for LabelForm {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let widths: Vec<f32> = self.items.iter().filter_map(|i| if let Item::Row(l, _) = i { Some(measure(l, window, cx)) } else { None }).collect();
        let column = label_column_width(&widths);
        let mut n = 0;
        let id = self.id.clone();
        v_flex().w_full().gap(px(self.gap)).children(self.items.into_iter().map(|item| match item {
            Item::Full(e) => div().w_full().child(e).into_any_element(),
            Item::Note(e) => h_flex().w_full().gap(px(LABEL_GAP)).child(div().w(px(column)).flex_none()).child(div().flex_1().min_w_0().child(e)).into_any_element(),
            Item::Row(label, control) => {
                let k = n;
                n += 1;
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(px(LABEL_GAP))
                    .child(observe(div().w(px(column)).flex_none().text_size(px(LABEL_SIZE)).whitespace_nowrap().child(observe(div().flex_none().child(label), format!("{id}-text-{k}"))), format!("{id}-label-{k}")))
                    .child(observe(div().flex_1().min_w_0().child(control), format!("{id}-control-{k}")))
                    .into_any_element()
            }
        }))
    }
}
