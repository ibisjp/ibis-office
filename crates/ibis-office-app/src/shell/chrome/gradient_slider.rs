// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Spatial colour-stop editor. Original UI inspired by the ibisPaint gradient-map guide;
//! the model here is a shape fill (position 0..100000), not a luminance remapping.
use super::*;
use ibis_doc_model::{Color, GradientStop, Rgb};
use std::cell::Cell;

#[derive(Clone, Default)]
pub(crate) struct GradientSliderState {
    drag: Rc<Cell<Option<usize>>>,
}

pub(crate) fn interpolated(stops: &[GradientStop], position: u32) -> Color {
    let mut sorted=stops.to_vec();sorted.sort_by_key(|p|p.position);let stops=sorted.as_slice();
    let before = stops
        .iter()
        .rev()
        .find(|s| s.position <= position)
        .or(stops.first());
    let after = stops
        .iter()
        .find(|s| s.position >= position)
        .or(stops.last());
    let (Some(a), Some(b)) = (before, after) else {
        return Color::opaque(Rgb(0));
    };
    let t = if a.position == b.position {
        0.
    } else {
        (position.saturating_sub(a.position)) as f64 / (b.position - a.position) as f64
    };
    let mix = |x: u32, y: u32| ((x as f64 + (y as f64 - x as f64) * t).round() as u32).min(255);
    Color {
        rgb: Rgb(
            (mix(a.color.rgb.0 >> 16 & 255, b.color.rgb.0 >> 16 & 255) << 16)
                | (mix(a.color.rgb.0 >> 8 & 255, b.color.rgb.0 >> 8 & 255) << 8)
                | mix(a.color.rgb.0 & 255, b.color.rgb.0 & 255),
        ),
        alpha: mix(a.color.alpha as u32, b.color.alpha as u32) as u8,
    }
}

/// Clicking empty track adds a stop; drag a knob to move it. The owner supplies colour/position
/// fields and remove controls so those also work with keyboard and assistive technologies.
pub(crate) fn gradient_slider<T: 'static>(
    stops: Vec<GradientStop>,
    selected: usize,
    enabled: bool,
    state: GradientSliderState,
    cx: &Context<T>,
    changed: impl Fn(&mut T, Vec<GradientStop>, usize, &mut Window, &mut Context<T>) + 'static,
) -> AnyElement {
    let owner = cx.weak_entity();
    let changed = Rc::new(changed);
    canvas(
        |bounds, w, _| w.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds, hitbox, w, _| {
            let inset = px(8.);
            let width = (bounds.size.width - inset * 2.).max(px(1.));
            let origin = bounds.origin + point(inset, px(3.));
            let bar = Bounds::new(origin, size(width, px(20.)));
            let color = |c: Color| rgba(((c.rgb.0 & 0xffffff) << 8) | c.alpha as u32);
            let mut points = stops.clone();
            points.sort_by_key(|s| s.position);
            if let Some(first) = points.first().cloned() {
                if first.position > 0 {
                    points.insert(
                        0,
                        GradientStop {
                            position: 0,
                            ..first
                        },
                    );
                }
            }
            if let Some(last) = points.last().cloned() {
                if last.position < 100000 {
                    points.push(GradientStop {
                        position: 100000,
                        ..last
                    });
                }
            }
            for pair in points.windows(2) {
                let a = &pair[0];
                let b = &pair[1];
                let x = width * (a.position as f32 / 100000.);
                let end = width * (b.position as f32 / 100000.);
                if end > x {
                    w.paint_quad(fill(
                        Bounds::new(origin + point(x, px(0.)), size(end - x, px(20.))),
                        linear_gradient(
                            90.,
                            linear_color_stop(color(a.color), 0.),
                            linear_color_stop(color(b.color), 1.),
                        ),
                    ));
                }
            }
            w.paint_quad(outline(bar, rgb(BORDER), BorderStyle::Solid));
            for (i, stop) in stops.iter().enumerate() {
                let center = origin.x + width * (stop.position as f32 / 100000.);
                let knob = Bounds::new(
                    point(center - px(5.), origin.y + px(23.)),
                    size(px(10.), px(14.)),
                );
                w.paint_quad(
                    fill(knob, color(stop.color))
                        .border_widths(px(if i == selected { 2. } else { 1. }))
                        .border_color(rgb(if i == selected {
                            PRIMARY_BUTTON_BG
                        } else {
                            BORDER
                        })),
                );
            }
            if !enabled {
                return;
            }
            w.set_cursor_style(CursorStyle::PointingHand, &hitbox);
            let down = state.drag.clone();
            let drag = state.drag.clone();
            let up = state.drag.clone();
            let source = stops.clone();
            let editor = owner.clone();
            let change = changed.clone();
            let moving = changed.clone();
            let moved_owner = owner.clone();
            let moved_stops = stops.clone();
            w.on_mouse_event(move |e: &MouseDownEvent, phase, w, cx| {
                if phase != DispatchPhase::Bubble
                    || e.button != MouseButton::Left
                    || !hitbox.is_hovered(w)
                {
                    return;
                }
                let position = (f32::from(e.position.x - origin.x) / f32::from(width) * 100000.)
                    .round()
                    .clamp(0., 100000.) as u32;
                let mut values = source.clone();
                let nearest = values
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, s)| s.position.abs_diff(position))
                    .map(|(i, _)| i);
                let index = match nearest {
                    Some(i)
                        if width * (values[i].position.abs_diff(position) as f32 / 100000.)
                            <= px(8.) =>
                    {
                        i
                    }
                    _ => {
                        let color = interpolated(&values, position);
                        values.push(GradientStop { position, color });
                        values.len() - 1
                    }
                };
                let _ = editor.update(cx, |o, cx| change(o, values, index, w, cx));
                down.set(Some(index));
                w.capture_pointer(hitbox.id);
                cx.stop_propagation();
            });
            w.on_mouse_event(move |e: &MouseMoveEvent, phase, w, cx| {
                if phase == DispatchPhase::Capture
                    && let Some(index) = drag.get()
                {
                    let mut values = moved_stops.clone();
                    if let Some(stop) = values.get_mut(index) {
                        stop.position = (f32::from(e.position.x - origin.x) / f32::from(width)
                            * 100000.)
                            .round()
                            .clamp(0., 100000.) as u32;
                        let _ = moved_owner.update(cx, |o, cx| moving(o, values, index, w, cx));
                    }
                    cx.stop_propagation();
                }
            });
            w.on_mouse_event(move |e: &MouseUpEvent, phase, w, cx| {
                if phase == DispatchPhase::Capture
                    && e.button == MouseButton::Left
                    && up.take().is_some()
                {
                    w.release_pointer();
                    cx.stop_propagation();
                }
            });
        },
    )
    .w_full()
    .h(px(44.))
    .flex_none()
    .into_any_element()
}
