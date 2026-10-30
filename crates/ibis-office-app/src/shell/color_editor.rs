// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared staged RGB editor: palette, hue ring, SV diamond, HSB/RGB sliders and numeric fields.
use super::{
    chrome::{self, SpinnerSpec},
    editor::SlideEditor,
    i18n::text,
};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::Rgb;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
pub(crate) enum ColorAction {
    Number(usize),
    Channel(usize, f32),
    Hue(f32),
    Sv(f32, f32),
    Pick(Rgb),
    Hex,
    Mode(bool),
}
pub(crate) type ColorChanged =
    fn(&mut SlideEditor, ColorAction, &mut Window, &mut Context<SlideEditor>);

pub(crate) struct ColorEditorState {
    pub original: Rgb,
    pub color: Rgb,
    pub hsv: [f32; 3],
    pub rgb_mode: bool,
    pub numbers: [Entity<InputState>; 6],
    pub sliders: [Entity<SliderState>; 6],
    drag: Rc<Cell<Option<bool>>>,
    pub wheel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}
pub(crate) fn rgb_to_hsv(color: Rgb) -> [f32; 3] {
    let [r, g, b] = [
        ((color.0 >> 16) & 255) as f32 / 255.,
        ((color.0 >> 8) & 255) as f32 / 255.,
        (color.0 & 255) as f32 / 255.,
    ];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    };
    [h, if max == 0. { 0. } else { d / max }, max]
}
pub(crate) fn hsv_to_rgb([h, s, v]: [f32; 3]) -> Rgb {
    let h = h.rem_euclid(360.) / 60.;
    let c = v * s;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let m = v - c;
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    let byte = |x: f32| ((x + m) * 255.).round().clamp(0., 255.) as u32;
    Rgb((byte(r) << 16) | (byte(g) << 8) | byte(b))
}
fn numeric(value: f32) -> String {
    let s = format!("{value:.1}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
impl ColorEditorState {
    pub fn values(&self) -> [f32; 6] {
        [
            self.hsv[0],
            self.hsv[1] * 100.,
            self.hsv[2] * 100.,
            ((self.color.0 >> 16) & 255) as f32,
            ((self.color.0 >> 8) & 255) as f32,
            (self.color.0 & 255) as f32,
        ]
    }
    pub fn new(
        color: Rgb,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
        changed: ColorChanged,
    ) -> Self {
        let hsv = rgb_to_hsv(color);
        let values = [
            hsv[0],
            hsv[1] * 100.,
            hsv[2] * 100.,
            ((color.0 >> 16) & 255) as f32,
            ((color.0 >> 8) & 255) as f32,
            (color.0 & 255) as f32,
        ];
        let numbers = std::array::from_fn(|i| {
            cx.new(|cx| InputState::new(window, cx).default_value(numeric(values[i])))
        });
        let sliders = std::array::from_fn(|i| {
            cx.new(|_| {
                SliderState::new()
                    .min(0.)
                    .max(channel_max(i))
                    .step(if i < 3 { 0.1 } else { 1. })
                    .default_value(values[i])
            })
        });
        let mut subscriptions = Vec::new();
        for i in 0..6 {
            subscriptions.push(cx.subscribe_in(
                &numbers[i],
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        changed(this, ColorAction::Number(i), window, cx);
                    }
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &sliders[i],
                window,
                move |this, _, event: &SliderEvent, window, cx| {
                    if let SliderEvent::Change(value) = event {
                        changed(this, ColorAction::Channel(i, value.end()), window, cx);
                    }
                },
            ));
        }
        Self {
            original: color,
            color,
            hsv,
            rgb_mode: false,
            numbers,
            sliders,
            drag: Rc::new(Cell::new(None)),
            wheel_bounds: Rc::new(Cell::new(None)),
            _subscriptions: subscriptions,
        }
    }
    pub fn sync(
        &mut self,
        except: Option<usize>,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) {
        for (i, value) in self.values().into_iter().enumerate() {
            self.sliders[i].update(cx, |s, cx| s.set_value(value, window, cx));
            let formatted = numeric(value);
            if except != Some(i) && self.numbers[i].read(cx).value().as_str() != formatted {
                self.numbers[i].update(cx, |s, cx| s.set_value(formatted, window, cx));
            }
        }
    }
    pub fn number_value(&self, i: usize, cx: &App) -> Option<f32> {
        self.numbers[i]
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=channel_max(i)).contains(v))
    }
    pub fn number_is_current(&self, i: usize, cx: &App) -> bool {
        self.numbers[i].read(cx).value().as_str() == numeric(self.values()[i])
    }
}
fn channel_max(i: usize) -> f32 {
    match i {
        0 => 360.,
        1 | 2 => 100.,
        _ => 255.,
    }
}

pub(crate) fn color_editor(
    state: &ColorEditorState,
    hex: &Entity<InputState>,
    recent: &[Rgb],
    changed: ColorChanged,
    cx: &Context<SlideEditor>,
) -> Div {
    let weak = cx.entity().downgrade();
    let swatch = |id: SharedString, color: Rgb, w: f32, h: f32| {
        let weak = weak.clone();
        div()
            .id(id)
            .w(px(w))
            .h(px(h))
            .rounded(px(3.))
            .border_1()
            .border_color(rgb(0xc7c7c7))
            .bg(rgb(color.0))
            .cursor_pointer()
            .aria_label(format!("#{:06X}", color.0))
            .on_click(move |_, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    changed(this, ColorAction::Pick(color), window, cx)
                });
            })
    };
    let palette_colors = palette_colors();
    let palette =
        v_flex()
            .w(px(176.))
            .gap_2()
            .child(div().text_sm().child(text("color.editor.palette")))
            .child(
                h_flex().flex_wrap().gap_1().children(
                    palette_colors.into_iter().enumerate().map(|(i, color)| {
                        swatch(format!("color-grid-{i}").into(), color, 25., 25.)
                    }),
                ),
            )
            .child(
                h_flex().flex_wrap().gap_1().children(
                    recent.iter().copied().enumerate().map(|(i, color)| {
                        swatch(format!("color-recent-{i}").into(), color, 25., 25.)
                    }),
                ),
            );
    let previews = h_flex()
        .gap_3()
        .flex_wrap()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_sm().child(text("color.editor.previous")))
                .child(swatch("color-previous".into(), state.original, 100., 32.)),
        )
        .child(
            v_flex()
                .gap_1()
                .child(div().text_sm().child(text("color.editor.current")))
                .child(swatch("color-current".into(), state.color, 100., 32.)),
        );
    let first = if state.rgb_mode { 3 } else { 0 };
    let labels = [
        "color.editor.hue",
        "color.editor.saturation",
        "color.editor.brightness",
        "color.editor.red",
        "color.editor.green",
        "color.editor.blue",
    ];
    let rows = (first..first + 3).map(|i| {
        h_flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                div()
                    .id(("color-channel-label", i))
                    .w(px(28.))
                    .flex_none()
                    .text_sm()
                    .aria_label(text(labels[i]))
                    .child(["H", "S", "B", "R", "G", "B"][i]),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(48.))
                    .child(Slider::new(&state.sliders[i])),
            )
            .child(chrome::text_field(&state.numbers[i]).no_clear().compact_spinner().spinner(
                format!("color-channel-{i}"),
                SpinnerSpec::new(if i < 3 { 0.1 } else { 1. }).range(0., channel_max(i)),
                cx,
                move |this, value, window, cx| {
                    changed(this, ColorAction::Channel(i, value), window, cx)
                },
            ))
    });
    let mode = |id: &'static str, label: &'static str, on: bool| {
        chrome::segment_control::SegmentItem::label(id, label, state.rgb_mode == on)
            .on_click(cx.listener(move |this, _, window, cx| {
                changed(this, ColorAction::Mode(on), window, cx)
            }))
    };
    v_flex()
        .gap_3()
        .w_full()
        .child(previews)
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .items_start()
                .child(palette)
                .child(hue_sv_wheel(state, changed, cx)),
        )
        .child(
            chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(mode("color-mode-hsb", "HSB", false))
                .child(mode("color-mode-rgb", "RGB", true)),
        )
        .children(rows)
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(div().w(px(42.)).text_sm().child("HEX"))
                .child(chrome::text_field(hex)),
        )
}

fn hue_sv_wheel(
    state: &ColorEditorState,
    changed: ColorChanged,
    cx: &Context<SlideEditor>,
) -> impl IntoElement {
    let hsv = state.hsv;
    let drag = state.drag.clone();
    let measured = state.wheel_bounds.clone();
    let weak = cx.entity().downgrade();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds, hitbox, window, _| {
            measured.set(Some(bounds));
            let center = bounds.center();
            let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2. - 3.;
            let inner = radius - 22.;
            let diamond = inner - 8.;
            let point_at = |angle: f32, r: f32| {
                point(
                    center.x + px(angle.cos() * r),
                    center.y - px(angle.sin() * r),
                )
            };
            for degree in 0..360 {
                let a = (degree as f32).to_radians(); // Overlap adjacent sectors by one physical pixel to avoid antialiased seams.
                let b = ((degree + 1) as f32).to_radians() + 1. / window.scale_factor() / radius;
                let mut path = PathBuilder::fill();
                path.move_to(point_at(a, inner));
                path.line_to(point_at(a, radius));
                path.line_to(point_at(b, radius));
                path.line_to(point_at(b, inner));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(hsv_to_rgb([degree as f32, 1., 1.]).0));
                }
            }
            let diamond_path = || {
                let mut path = PathBuilder::fill();
                for (i, (x, y)) in [(0., -diamond), (diamond, 0.), (0., diamond), (-diamond, 0.)]
                    .into_iter()
                    .enumerate()
                {
                    let p = point(center.x + px(x), center.y + px(y));
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                path.close();
                path.build().ok()
            };
            // A rotated square occupies the middle half of its axis-aligned gradient projection.
            if let Some(path) = diamond_path() {
                window.paint_path(
                    path,
                    linear_gradient(
                        135.,
                        linear_color_stop(rgb(0xffffff), 0.25),
                        linear_color_stop(rgb(hsv_to_rgb([hsv[0], 1., 1.]).0), 0.75),
                    ),
                );
            }
            if let Some(path) = diamond_path() {
                window.paint_path(
                    path,
                    linear_gradient(
                        225.,
                        linear_color_stop(rgba(0x00000000), 0.25),
                        linear_color_stop(rgb(0x000000), 0.75),
                    ),
                );
            }
            let marker = |p: Point<Pixels>, window: &mut Window| {
                window.paint_quad(
                    outline(
                        Bounds::new(p - point(px(6.), px(6.)), size(px(12.), px(12.))),
                        rgb(0x444444),
                        BorderStyle::Solid,
                    )
                    .corner_radii(px(6.)),
                );
                window.paint_quad(
                    outline(
                        Bounds::new(p - point(px(5.), px(5.)), size(px(10.), px(10.))),
                        rgb(0xffffff),
                        BorderStyle::Solid,
                    )
                    .corner_radii(px(5.)),
                );
            };
            marker(point_at(hsv[0].to_radians(), (radius + inner) / 2.), window);
            let u = hsv[1];
            let b = 1. - hsv[2];
            marker(
                point(
                    center.x + px(diamond * (u - b)),
                    center.y + px(diamond * (u + b - 1.)),
                ),
                window,
            );
            let change_at =
                move |position: Point<Pixels>, ring: bool, window: &mut Window, cx: &mut App| {
                    let x = f32::from(position.x - center.x);
                    let y = f32::from(position.y - center.y);
                    let action = if ring {
                        ColorAction::Hue((-y).atan2(x).to_degrees().rem_euclid(360.))
                    } else {
                        ColorAction::Sv(
                            ((x / diamond + y / diamond + 1.) / 2.).clamp(0., 1.),
                            1. - ((y / diamond - x / diamond + 1.) / 2.).clamp(0., 1.),
                        )
                    };
                    let _ = weak.update(cx, |this, cx| changed(this, action, window, cx));
                };
            let down_drag = drag.clone();
            let down_change = change_at.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && hitbox.is_hovered(window)
                {
                    let x = f32::from(event.position.x - center.x);
                    let y = f32::from(event.position.y - center.y);
                    let dist = x.hypot(y);
                    let ring = dist >= inner;
                    if dist <= radius && (ring || x.abs() + y.abs() <= diamond) {
                        down_drag.set(Some(ring));
                        window.capture_pointer(hitbox.id);
                        down_change(event.position, ring, window, cx);
                        cx.stop_propagation();
                    }
                }
            });
            let moving = drag.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && let Some(ring) = moving.get()
                {
                    change_at(event.position, ring, window, cx);
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    drag.set(None);
                }
            });
        },
    )
    .w(px(240.))
    .h(px(240.))
    .flex_none()
}

#[cfg(test)]
#[path = "../tests/shell/color_editor.rs"]
mod tests;

pub(crate) fn palette_colors() -> Vec<Rgb> { [
        0x000000, 0xffffff, 0xff0000, 0xff8000, 0xffff00, 0x80ff00, 0x00ff00, 0x00ff80, 0x00ffff,
        0x0080ff, 0x0000ff, 0x8000ff, 0xff00ff, 0xff0080, 0x804000, 0x808080, 0xffc0c0, 0xffe0b0,
        0xffffc0, 0xc0ffc0, 0xc0ffff, 0xc0c0ff, 0xe0c0ff, 0xffc0ff,
    ]
    .into_iter()
    .map(Rgb)
    .collect() }

impl ColorEditorState {
 pub(crate) fn change(&mut self, action:ColorAction, hex:&Entity<InputState>, window:&mut Window,cx:&mut Context<SlideEditor>)->bool {
        let mut except = None;
        let action = match action {
            ColorAction::Number(i) => {
                if self.number_is_current(i,cx) { return false; }
                let Some(value) = self.number_value(i,cx) else { return false; };
                except = Some(i); ColorAction::Channel(i,value)
            }
            other => other,
        };
        let picker = self;
        match action {
            ColorAction::Mode(rgb) => { picker.rgb_mode=rgb; cx.notify(); return true; }
            ColorAction::Hex => {
                let value=hex.read(cx).value();let raw=value.trim().trim_start_matches('#');
                let Some(rgb)=(raw.len()==6).then(||u32::from_str_radix(raw,16).ok()).flatten().map(Rgb) else {return false;};
                if picker.color==rgb {return false;}
                picker.color=rgb;picker.hsv=rgb_to_hsv(rgb);
            }
            ColorAction::Pick(rgb) => {picker.color=rgb;picker.hsv=rgb_to_hsv(rgb);}
            ColorAction::Hue(hue) => {picker.hsv[0]=hue;picker.color=hsv_to_rgb(picker.hsv);}
            ColorAction::Sv(s,v) => {picker.hsv[1]=s;picker.hsv[2]=v;picker.color=hsv_to_rgb(picker.hsv);}
            ColorAction::Channel(i,value) if i<3 => {
                picker.hsv[i]=if i==0 {value} else {value/100.};picker.color=hsv_to_rgb(picker.hsv);
            }
            ColorAction::Channel(i,value) => {
                let shift=(5-i)*8;picker.color=Rgb((picker.color.0 & !(255<<shift)) | ((value.round().clamp(0.,255.) as u32)<<shift));picker.hsv=rgb_to_hsv(picker.color);
            }
            ColorAction::Number(_) => unreachable!(),
        }
        picker.sync(except,window,cx);
        let value=format!("{:06X}",picker.color.0);
        if hex.read(cx).value().as_str()!=value { hex.update(cx,|input,cx|input.set_value(value,window,cx)); }
        true
 }
}
