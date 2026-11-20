// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The preview of ツールバーのカスタマイズ (GitHub issue #241): the toolbar as the order makes it, drawn as the real one is, whose
//! buttons are dragged to the left and right to change the order. The button held floats (it stays on the toolbar's line, and
//! follows the pointer's height when it is lifted a toolbar's height above the bar); a neighbour slides aside when the held
//! button's centre passes the neighbour's centre, with the move animation the rows of a list have (`REORDER_MOVE_MS`). While a button
//! is held a trash circle shows in the dialog; dropped in it, the button is taken out.
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::data_table::{REORDER_MOVE_MS, lifted_row_shadow, reorder_ease};
use crate::chrome::{APP_BG, TOOLBAR_BG, ToolButton, ToolItem, tool_item};
use crate::i18n::text;
use crate::editor::SlideEditor;
use crate::palette::rgb;

pub(crate) const BAR_HEIGHT: f32 = 36.;
const PAD: f32 = 6.;
const GAP: f32 = 1.;
const START_DISTANCE: f32 = 4.;
/// The trash circle's size, and the colours of its ring and of its inside when a button is over it.
const TRASH_SIZE: f32 = 44.;
const TRASH_RING: u32 = 0xd93025;
const TRASH_OVER: u32 = 0xfce8e6;

/// The width of the ︙ button that has the buttons that do not fit.
pub(crate) const MORE_WIDTH: f32 = 28.;

/// How many of the buttons (`widths`) a bar `bar` wide shows: all of them when they fit, else the ones that fit with the room of ︙ kept (the
/// rest are in ︙, as the real toolbar folds them). A bar not drawn yet (no width) shows all.
pub(crate) fn fit(widths: &[f32], bar: f32) -> usize {
    if bar <= 0. {
        return widths.len();
    }
    let total: f32 = widths.iter().map(|w| w + GAP).sum::<f32>() + 2. * PAD;
    if total <= bar {
        return widths.len();
    }
    let room = bar - 2. * PAD - MORE_WIDTH - GAP;
    let (mut used, mut n) = (0., 0);
    for w in widths {
        if used + w + GAP > room {
            break;
        }
        used += w + GAP;
        n += 1;
    }
    n
}

/// The left edge of each button of a bar whose buttons are `widths` wide.
pub(crate) fn positions(widths: &[f32]) -> Vec<f32> {
    let mut x = PAD;
    widths
        .iter()
        .map(|w| {
            let at = x;
            x += w + GAP;
            at
        })
        .collect()
}

/// Where the button `from` of the bar would stand if it were let go with its centre at `center` (the bar's own x): among the
/// buttons `lo..=hi` it may change places with, it passes a neighbour when its centre passes the neighbour's centre.
pub(crate) fn target(widths: &[f32], from: usize, lo: usize, hi: usize, center: f32) -> usize {
    let xs = positions(widths);
    let middle = |k: usize| xs[k] + widths[k] / 2.;
    let mut at = from;
    while at < hi && center >= middle(at + 1) {
        at += 1;
    }
    while at > lo && center <= middle(at - 1) {
        at -= 1;
    }
    at
}

/// How far (px) the button `j` is moved aside while the button `from` is carried to `at`.
pub(crate) fn offset(widths: &[f32], from: usize, at: usize, j: usize) -> f32 {
    let w = widths[from] + GAP;
    if at > from && j > from && j <= at {
        -w
    } else if at < from && j >= at && j < from {
        w
    } else {
        0.
    }
}

/// A button being held.
pub(crate) struct Held {
    /// Its place in the order.
    pub index: usize,
    /// Where the pointer went down and where it is (the window's).
    pub start: Point<Pixels>,
    pub pointer: Point<Pixels>,
    /// Where in the button the pointer holds it.
    pub grab: Point<Pixels>,
    /// Whether the pointer has gone far enough for it to be a drag.
    pub dragging: bool,
    /// Where it would stand (place in the order).
    pub at: usize,
    /// For each button: the offset it is moved aside to, how many moves it has made (the key of its slide) and the offset it moved from.
    pub shifts: std::cell::RefCell<HashMap<usize, (f32, usize, f32)>>,
}

/// What the preview keeps (in the dialog's state).
#[derive(Default)]
pub struct Preview {
    pub(crate) bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) trash: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) held: Option<Held>,
}

impl Preview {
    /// Whether the pointer is in the trash circle.
    pub(crate) fn over_trash(&self) -> bool {
        let Some(held) = self.held.as_ref().filter(|h| h.dragging) else { return false };
        let bounds = self.trash.get();
        let (c, r) = (bounds.center(), f32::from(bounds.size.width) / 2.);
        let (dx, dy) = (f32::from(held.pointer.x - c.x), f32::from(held.pointer.y - c.y));
        r > 0. && (dx * dx + dy * dy).sqrt() <= r
    }
}

impl SlideEditor {
    fn preview_state(&mut self) -> Option<&mut crate::toolbar_customize::ToolbarCustomize> {
        self.settings.as_mut().and_then(|s| s.customize.as_mut())
    }

    /// The button of the order's place `index` was pressed at `position`.
    pub(crate) fn preview_press(&mut self, index: usize, grab: Point<Pixels>, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(c) = self.preview_state() {
            c.selected = Some(index);
            c.preview.held = Some(Held { index, start: position, pointer: position, grab, dragging: false, at: index, shifts: Default::default() });
        }
        cx.notify();
    }

    /// The pointer moved.
    pub(crate) fn preview_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(c) = self.preview_state() else { return };
        let bounds = c.preview.bounds.get();
        let entries = c.preview_widths();
        let Some(held) = c.preview.held.as_mut() else { return };
        held.pointer = position;
        if !held.dragging {
            let (dx, dy) = (f32::from(position.x - held.start.x), f32::from(position.y - held.start.y));
            if (dx * dx + dy * dy).sqrt() < START_DISTANCE {
                return;
            }
            held.dragging = true;
        }
        // Where it would stand: its centre against the others' centres (the bar's own x).
        if let Some((widths, base, last)) = entries {
            let from = base + held.index;
            if from >= widths.len() {
                return;
            }
            let center = f32::from(position.x - bounds.origin.x) - f32::from(held.grab.x) + widths[from] / 2.;
            let to = target(&widths, from, base, last, center);
            held.at = to - base;
        }
        cx.notify();
    }

    /// The button went up: moved to the place it reached, or taken out when it is over the trash.
    pub(crate) fn preview_release(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.preview_state() else { return };
        let over = c.preview.over_trash();
        let Some(held) = c.preview.held.take() else { return };
        if held.dragging {
            if over {
                c.remove(held.index);
            } else if held.at != held.index {
                c.move_row(held.index, held.at);
            }
        }
        cx.notify();
    }

    /// The trash circle, while a button is dragged.
    pub(crate) fn preview_trash(&self) -> Option<AnyElement> {
        let c = self.settings.as_ref()?.customize.as_ref()?;
        c.preview.held.as_ref().filter(|h| h.dragging)?;
        let over = c.preview.over_trash();
        let bounds = c.preview.trash.clone();
        Some(
            div()
                .absolute()
                .top(px(0.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .size(px(TRASH_SIZE))
                        .rounded_full()
                        .border_2()
                        .border_color(rgb(TRASH_RING))
                        .when(over, |d| d.bg(rgb(TRASH_OVER)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::chrome::icon("delete", 22., rgb(TRASH_RING).into()))
                        .child(canvas(|_, _, _| (), move |b, _, _, _| bounds.set(b)).absolute().size_full()),
                )
                .into_any_element(),
        )
    }

    /// The preview: the locked buttons, then the order's buttons (each held by the pointer to be moved).
    pub(crate) fn customize_preview(&self, order: &[String], editor: &WeakEntity<SlideEditor>) -> AnyElement {
        let Some(c) = self.settings.as_ref().and_then(|s| s.customize.as_ref()) else { return div().into_any_element() };
        let mut entries = c.preview_entries(order);
        // What does not fit goes to ︙, as in the real toolbar.
        let all_widths: Vec<f32> = entries.iter().map(|(item, _)| item.width()).collect();
        let shown = fit(&all_widths, f32::from(c.preview.bounds.get().size.width));
        if shown < entries.len() {
            entries.truncate(shown);
            while matches!(entries.last(), Some((ToolItem::Separator, _))) {
                entries.pop();
            }
            let mut more = ToolButton::new("toolbar-preview-more", "more_vert", text("_shell.more"));
            more.handler = Some(Rc::new(|_, _, _| {}));
            entries.push((more.into(), None));
        }
        let is_qat = c.ext == "qat";
        let widths: Vec<f32> = entries.iter().map(|(item, _)| item.width()).collect();
        let xs = positions(&widths);
        let bounds = c.preview.bounds.clone();
        let held = c.preview.held.as_ref().filter(|h| h.dragging);
        let base = entries.iter().position(|(_, o)| o.is_some()).unwrap_or(entries.len());
        let selected = c.selected;
        let mut bar = div().relative().h(px(BAR_HEIGHT)).w_full().when(!is_qat, |d| d.rounded(px(18.))).bg(rgb(if is_qat { APP_BG } else { TOOLBAR_BG })).child(
            canvas(|_, _, _| (), {
                let (editor, bounds) = (editor.clone(), bounds.clone());
                move |b, _, window, _| {
                    bounds.set(b);
                    let moving = editor.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moving.update(cx, |e, cx| e.preview_pointer(event.position, cx));
                        }
                    });
                    let released = editor.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = released.update(cx, |e, cx| e.preview_release(cx));
                        }
                    });
                }
            })
            .absolute()
            .size_full(),
        );
        let origin = bounds.get().origin;
        for (j, (item, order_index)) in entries.iter().enumerate() {
            let (w, x) = (widths[j], xs[j]);
            let is_held = held.is_some_and(|h| *order_index == Some(h.index));
            let want = held.map_or(0., |h| if order_index.is_some() { offset(&widths, base + h.index, base + h.at, j) } else { 0. });
            // Where it was moved aside to before, and how many times it has been moved (the key of its slide).
            let (was, moves) = match held {
                Some(h) => {
                    let mut shifts = h.shifts.borrow_mut();
                    let e = shifts.entry(j).or_insert((0., 0, 0.));
                    if e.0 != want {
                        e.2 = e.0;
                        e.0 = want;
                        e.1 += 1;
                    }
                    (e.2, e.1)
                }
                None => (want, 0),
            };
            let face = || {
                div()
                    .w(px(w))
                    .h(px(BAR_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(tool_item(item, 30000 + j, editor))
                    .when(order_index.is_some() && *order_index == selected, |d| d.rounded(px(4.)).border_1().border_color(rgb(crate::chrome::PRIMARY_BUTTON_BG)))
            };
            // The cover takes the pointer: the buttons of the preview do nothing themselves.
            let cover = {
                let (grab_editor, order_index, bounds) = (editor.clone(), *order_index, bounds.clone());
                div().id(("toolbar-preview-entry", j)).absolute().inset_0().when(order_index.is_some(), |d| d.cursor_pointer()).on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    let Some(index) = order_index else { return };
                    let o = bounds.get().origin;
                    let grab = point(px(f32::from(event.position.x - o.x) - x), px(f32::from(event.position.y - o.y)));
                    let _ = grab_editor.update(cx, |e, cx| e.preview_press(index, grab, event.position, cx));
                    cx.stop_propagation();
                })
            };
            let slot = div().absolute().top(px(0.)).w(px(w)).h(px(BAR_HEIGHT));
            if is_held {
                let h = held.expect("held");
                // Its own place, faded; the button itself floats with the pointer, in front.
                bar = bar.child(slot.left(px(x)).opacity(0.35).child(face()));
                let left = f32::from(h.pointer.x - origin.x) - f32::from(h.grab.x);
                let lift = f32::from(h.pointer.y - origin.y) - f32::from(h.grab.y);
                // On the bar's line, unless lifted a bar's height above it.
                let top = if lift < -BAR_HEIGHT { lift } else { 0. };
                bar = bar.child(
                    deferred(div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(BAR_HEIGHT)).rounded(px(4.)).bg(rgb(TOOLBAR_BG)).shadow(lifted_row_shadow()).child(face()).child(div().absolute().inset_0()))
                        .with_priority(2),
                );
                continue;
            }
            let slot = slot.child(face()).child(cover);
            bar = bar.child(if was != want {
                slot.with_animation(
                    SharedString::from(format!("toolbar-preview-slide-{j}-{moves}")),
                    Animation::new(std::time::Duration::from_millis(REORDER_MOVE_MS)).with_easing(reorder_ease),
                    move |el, t| el.left(px(x + was + (want - was) * t)),
                )
                .into_any_element()
            } else {
                slot.left(px(x + want)).into_any_element()
            });
        }
        bar.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // (Not `use super::*`: it would bring gpui's `test` attribute in, which expands without end here.)
    use super::{GAP, offset, positions, target};

    /// A button passes a neighbour when its centre passes the neighbour's centre (buttons of different widths), and stays among
    /// the buttons it may change places with.
    #[test]
    fn a_button_passes_a_neighbour_at_its_centre() {
        let widths = [28., 28., 60., 28.];
        let xs = positions(&widths);
        let centre = |k: usize| xs[k] + widths[k] / 2.;
        // Held button 0, carried so that its centre is a little short of the centre of button 1: it stays; just past it, it takes place 1.
        assert_eq!(target(&widths, 0, 0, 3, centre(1) - 1.), 0);
        assert_eq!(target(&widths, 0, 0, 3, centre(1) + 1.), 1);
        // Far to the right, not beyond the last it may pass; far to the left, not before the first.
        assert_eq!(target(&widths, 0, 0, 3, 10_000.), 3);
        assert_eq!(target(&widths, 3, 1, 3, -10_000.), 1);
        // The others move aside by the held button's width.
        assert_eq!(offset(&widths, 0, 2, 1), -(28. + GAP));
        assert_eq!(offset(&widths, 0, 2, 3), 0.);
        assert_eq!(offset(&widths, 3, 1, 1), 28. + GAP);
        assert_eq!(offset(&widths, 3, 1, 0), 0.);
    }
}
