// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Optional pointer-reorder behavior shared by editable data tables.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
pub(crate) fn table_surface() -> Div {
    v_flex()
        .w_full()
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_md()
        .overflow_hidden()
}
pub(crate) const TABLE_ROW_HEIGHT: f32 = 36.;
#[derive(Default)]
pub(crate) struct RowReorder {
    pub order: Vec<usize>,
    original: Vec<usize>,
    pub dragging: Option<usize>,
    hover: Option<usize>,
    pointer: f32,
    grab: f32,
    bounds: Bounds<Pixels>,
    rows: BTreeMap<usize, Bounds<Pixels>>,
    exclusions: BTreeMap<(usize, u8), Bounds<Pixels>>,
    starts: BTreeMap<usize, f32>,
    generation: usize,
}
impl RowReorder {
    pub(crate) fn row_bounds(&self, id: usize) -> Option<Bounds<Pixels>> {
        self.rows.get(&id).copied()
    }
    pub(crate) fn hovered(&self) -> Option<usize> {
        self.hover
    }
    pub(crate) fn cancel(&mut self) -> bool {
        if self.dragging.take().is_none() {
            return false;
        }
        self.order = self.original.clone();
        self.hover = None;
        self.generation += 1;
        self.starts.clear();
        true
    }
    fn allowed(&self, id: usize, p: Point<Pixels>) -> bool {
        !self
            .exclusions
            .iter()
            .any(|((row, _), b)| *row == id && b.contains(&p))
    }
    fn start(&mut self, id: usize, p: Point<Pixels>) {
        self.original = self.order.clone();
        self.dragging = Some(id);
        self.grab = self
            .rows
            .get(&id)
            .map_or(0., |b| f32::from(p.y - b.origin.y));
        self.pointer = f32::from(p.y);
    }
    fn move_to(&mut self, p: Point<Pixels>, height: f32) {
        self.pointer = f32::from(p.y).clamp(
            f32::from(self.bounds.origin.y) + self.grab,
            (f32::from(self.bounds.bottom()) - height + self.grab).max(f32::from(self.bounds.origin.y) + self.grab),
        );
        let Some(id) = self.dragging else {
            return;
        };
        let center = self.pointer - self.grab + height / 2. - f32::from(self.bounds.origin.y);
        let mut at = self.order.iter().position(|i| *i == id).unwrap();
        let original = at;
        while at + 1 < self.order.len() && center >= (at + 1) as f32 * height + height / 2. {
            self.order.swap(at, at + 1);
            at += 1;
        }
        while at > 0 && center <= (at - 1) as f32 * height + height / 2. {
            self.order.swap(at, at - 1);
            at -= 1;
        }
        if at != original {
            self.generation += 1;
            self.starts = self
                .rows
                .iter()
                .map(|(id, b)| (*id, f32::from(b.origin.y - self.bounds.origin.y)))
                .collect();
        }
    }
    fn finish(&mut self) -> Option<(usize, usize)> {
        let row = self.dragging.take()?;
        let to = self.order.iter().position(|i| *i == row)?;
        let target = *self.original.get(to)?;
        self.order.clear();
        self.starts.clear();
        self.hover = None;
        (row != target).then_some((row, target))
    }
}
/// Mark controls which must retain their own click behavior and must not start a row drag.
pub(crate) fn row_controls(
    state: Rc<RefCell<RowReorder>>,
    row: usize,
    part: u8,
    child: impl IntoElement,
) -> Div {
    div().relative().child(child).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, _, _| {
                state.borrow_mut().exclusions.insert((row, part), bounds);
            },
        )
        .absolute()
        .inset_0(),
    )
}
type Commit = Rc<dyn Fn(&mut SlideEditor, usize, usize, &mut Window, &mut Context<SlideEditor>)>;
#[derive(IntoElement)]
pub(crate) struct DataTable {
    id: &'static str,
    header: Option<AnyElement>,
    rows: Vec<(usize, AnyElement, bool)>,
    height: f32,
    reorder: Option<(Rc<RefCell<RowReorder>>, Commit)>,
    editor: WeakEntity<SlideEditor>,
}
impl DataTable {
    pub(crate) fn new(
        id: &'static str,
        header: Option<AnyElement>,
        rows: Vec<(usize, AnyElement, bool)>,
        cx: &Context<SlideEditor>,
    ) -> Self {
        Self {
            id,
            header,
            rows,
            height: TABLE_ROW_HEIGHT,
            reorder: None,
            editor: cx.weak_entity(),
        }
    }
    pub(crate) fn row_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
    pub(crate) fn reorderable(
        mut self,
        state: Rc<RefCell<RowReorder>>,
        commit: impl Fn(&mut SlideEditor, usize, usize, &mut Window, &mut Context<SlideEditor>)
        + 'static,
    ) -> Self {
        self.reorder = Some((state, Rc::new(commit)));
        self
    }
}
impl RenderOnce for DataTable {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut surface = table_surface().id(self.id);
        if let Some(header) = self.header {
            surface = surface.child(
                div()
                    .h(px(self.height))
                    .min_h(px(self.height))
                    .flex_none()
                    .bg(rgb(0xf3f4f6))
                    .overflow_hidden()
                    .child(header),
            );
        }
        let Some((state, commit)) = self.reorder else {
            return surface.children(
                self.rows
                    .into_iter()
                    .map(|(_, row, _)| div().h(px(self.height)).child(row)),
            );
        };
        let ids = self.rows.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
        {
            let mut s = state.borrow_mut();
            if s.dragging.is_none() && s.order != ids {
                s.rows.retain(|id, _| ids.contains(id));
                s.exclusions.retain(|(id, _), _| ids.contains(id));
                s.order = ids;
            }
        }
        let editor = self.editor.clone();
        let active = state.clone();
        cx.defer(move |cx| {
            let _ = editor.update(cx, |e, _| e.active_table_reorder = Some(active));
        });
        let (order, dragging, hover, generation, starts, drag_top) = {
            let s = state.borrow();
            (
                s.order.clone(),
                s.dragging,
                s.hover,
                s.generation,
                s.starts.clone(),
                s.pointer - s.grab - f32::from(s.bounds.origin.y),
            )
        };
        let height = self.height;
        let mut entries = self
            .rows
            .into_iter()
            .map(|(id, element, selected)| (id, (element, selected)))
            .collect::<BTreeMap<_, _>>();
        let mut body = div()
            .relative()
            .w_full()
            .h(px(order.len() as f32 * height))
            .overflow_hidden();
        // The capture surface is painted before rows, so controls keep their own hitboxes.
        let tracking = state.clone();
        let editor = self.editor.clone();
        body = body.child(
            canvas(
                |b, w, _| w.insert_hitbox(b, HitboxBehavior::Normal),
                move |bounds, hitbox, w, _| {
                    tracking.borrow_mut().bounds = bounds;
                    let st = tracking.clone();
                    let e = editor.clone();
                    let capture = hitbox.id;
                    let down_hitbox = hitbox.clone();
                    w.on_mouse_event(move |ev: &MouseDownEvent, phase, w, cx| {
                        if phase != DispatchPhase::Bubble
                            || ev.button != MouseButton::Left
                            || !down_hitbox.is_hovered(w)
                        {
                            return;
                        }
                        let id = {
                            let s = st.borrow();
                            s.rows
                                .iter()
                                .find(|(id, b)| {
                                    b.contains(&ev.position) && s.allowed(**id, ev.position)
                                })
                                .map(|(id, _)| *id)
                        };
                        if let Some(id) = id {
                            st.borrow_mut().start(id, ev.position);
                            w.capture_pointer(capture);
                            let _ = e.update(cx, |editor, cx| {
                                if editor.panel_dialog_is_modal() {
                                    if let Some(f) = editor.panel_dialog.focus.as_ref() {
                                        f.focus(w, cx);
                                    }
                                } else {
                                    editor.focus_handle.focus(w, cx);
                                }
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }
                    });
                    let st = tracking.clone();
                    let e = editor.clone();
                    w.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        let active = st.borrow().dragging.is_some();
                        if active {
                            st.borrow_mut().move_to(ev.position, height);
                        } else {
                            let mut s = st.borrow_mut();
                            let next = s
                                .rows
                                .iter()
                                .find(|(id, b)| {
                                    b.contains(&ev.position) && s.allowed(**id, ev.position)
                                })
                                .map(|(id, _)| *id);
                            if s.hover == next {
                                return;
                            }
                            s.hover = next;
                        }
                        let _ = e.update(cx, |_, cx| cx.notify());
                        if active {
                            cx.stop_propagation();
                        }
                    });
                    let st = tracking.clone();
                    let e = editor.clone();
                    let commit = commit.clone();
                    w.on_mouse_event(move |ev: &MouseUpEvent, phase, w, cx| {
                        if phase != DispatchPhase::Capture
                            || ev.button != MouseButton::Left
                            || st.borrow().dragging.is_none()
                        {
                            return;
                        }
                        let changed = st.borrow_mut().finish();
                        let _ = e.update(cx, |e, cx| {
                            if let Some((from, to)) = changed {
                                commit(e, from, to, w, cx);
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                    });
                },
            )
            .absolute()
            .inset_0(),
        );
        let make_row = |id: usize, content: AnyElement, selected: bool, slot: usize| {
            let recording = state.clone();
            let target = slot as f32 * height;
            let lifted = dragging == Some(id);
            let start = starts.get(&id).copied().unwrap_or(target);
            let row = div()
                .id((self.id, id))
                .absolute()
                .left_0()
                .w_full()
                .h(px(height))
                .flex_none()
                .border_b_1()
                .border_color(rgb(BORDER))
                .bg(rgb(if hover == Some(id) && !lifted {
                    0xf1f3f4
                } else if selected && !lifted {
                    SELECTED_ROW_BG
                } else {
                    0xffffff
                }))
                .child(content)
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, _, _| {
                            recording.borrow_mut().rows.insert(id, bounds);
                        },
                    )
                    .absolute()
                    .inset_0(),
                );
            if lifted {
                row.top(px(drag_top))
                    .shadow(vec![
                        BoxShadow {
                            color: gpui_kit::black().opacity(0.18),
                            offset: point(px(0.), px(-3.)),
                            blur_radius: px(6.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                        BoxShadow {
                            color: gpui_kit::black().opacity(0.18),
                            offset: point(px(0.), px(3.)),
                            blur_radius: px(6.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                    ])
                    .into_any_element()
            } else {
                row.with_animation(
                    SharedString::from(format!("{}-move-{generation}-{id}", self.id)),
                    Animation::new(std::time::Duration::from_millis(160))
                        .with_easing(|t| 1. - (1. - t).powi(3)),
                    move |row, t| row.top(px(start + (target - start) * t)),
                )
                .into_any_element()
            }
        };
        let mut floating = None;
        for (slot, id) in order.into_iter().enumerate() {
            if let Some((content, selected)) = entries.remove(&id) {
                let row = make_row(id, content, selected, slot);
                if dragging == Some(id) {
                    floating = Some(row);
                } else {
                    body = body.child(row);
                }
            }
        }
        surface = surface.child(body.children(floating));
        surface
    }
}
