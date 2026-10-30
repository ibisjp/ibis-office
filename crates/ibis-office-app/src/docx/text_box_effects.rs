// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Live shape-decoration fields, retaining OOXML parameters which the UI does not edit.
use super::text_box_panel::TextBoxState;
use crate::shell::{
    chrome::{
        self,
        collapsible_section::SectionState,
        gradient_slider::{self, GradientSliderState},
    },
    editor::SlideEditor,
    i18n::text,
};
use gpui_kit::{
    component::{
        Disableable as _,
        input::{InputEvent, InputState},
        v_flex,
    },
    *,
};
use ibis_doc_model::{Color, Fill, GradientStop, Reflection, Rgb, Shadow};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct EffectModel {
    pub fill: Fill,
    pub shadow: Option<Shadow>,
    pub glow: Option<(Color, i64)>,
    pub soft: Option<i64>,
    pub reflection: Option<Reflection>,
}
pub(crate) struct EffectEditor {
    pub tab: usize,
    pub open: [bool; 5],
    pub sections: [Rc<SectionState>; 5],
    pub fields: BTreeMap<&'static str, Entity<InputState>>,
    pub stops: Vec<GradientStop>,
    pub selected: usize,
    pub slider: GradientSliderState,
}
fn rgb(c: Color) -> String {
    format!("{:06X}", c.rgb.0 & 0xffffff)
}
impl EffectEditor {
    pub fn new(
        model: &EffectModel,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) -> (Self, Vec<Subscription>) {
        // Initial values match the presets used by this editor before parameter editing was added.
        // Existing document parameters always take precedence, including skew/alignment and fade values.
        let color = match model.fill {
            Fill::Solid(c) => c,
            _ => Color::opaque(Rgb(0x4472c4)),
        };
        let shadow = model.shadow.clone().unwrap_or_else(|| {
            Shadow::simple(
                Color {
                    rgb: Rgb(0),
                    alpha: 102,
                },
                50800,
                38100,
                2700000,
            )
        });
        let glow = model.glow.unwrap_or((
            Color {
                alpha: 102,
                ..color
            },
            63500,
        ));
        let reflect = model
            .reflection
            .clone()
            .unwrap_or_else(|| Reflection::standard(0, 45500));
        let (angle, stops) = if let Fill::Linear { angle, stops, .. } = &model.fill {
            (*angle, stops.clone())
        } else {
            (
                5400000,
                vec![
                    GradientStop { position: 0, color },
                    GradientStop {
                        position: 100000,
                        color: Color::opaque(Rgb(0xffffff)),
                    },
                ],
            )
        };
        let selected = 0;
        let first = stops
            .first()
            .cloned()
            .unwrap_or(GradientStop { position: 0, color });
        let values = [
            ("gradient-angle", (angle as f64 / 60000.).to_string()),
            ("stop-position", (first.position as f64 / 1000.).to_string()),
            ("stop-color", rgb(first.color)),
            ("shadow-color", rgb(shadow.color)),
            (
                "shadow-transparency",
                (100. - shadow.color.alpha as f64 / 255. * 100.).to_string(),
            ),
            ("shadow-blur", (shadow.blur as f64 / 12700.).to_string()),
            (
                "shadow-distance",
                (shadow.distance as f64 / 12700.).to_string(),
            ),
            (
                "shadow-angle",
                (shadow.direction as f64 / 60000.).to_string(),
            ),
            ("shadow-size", (shadow.sx as f64 / 1000.).to_string()),
            ("glow-color", rgb(glow.0)),
            (
                "glow-transparency",
                (100. - glow.0.alpha as f64 / 255. * 100.).to_string(),
            ),
            ("glow-size", (glow.1 as f64 / 12700.).to_string()),
            (
                "soft-size",
                (model.soft.unwrap_or(63500) as f64 / 12700.).to_string(),
            ),
            (
                "reflection-transparency",
                (100. - reflect.start_alpha as f64 / 1000.).to_string(),
            ),
            (
                "reflection-blur",
                (reflect.blur as f64 / 12700.).to_string(),
            ),
            (
                "reflection-distance",
                (reflect.distance as f64 / 12700.).to_string(),
            ),
            (
                "reflection-size",
                (reflect.end_pos as f64 / 1000.).to_string(),
            ),
        ];
        let mut fields = BTreeMap::new();
        let mut subscriptions = Vec::new();
        for (key, value) in values {
            let field = cx.new(|cx| {
                let mut i = InputState::new(window, cx);
                i.set_value(value, window, cx);
                i
            });
            subscriptions.push(cx.subscribe_in(
                &field,
                window,
                move |e, _, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Change | InputEvent::PressEnter { .. }) {
                        e.update_text_box_effect_field(key, cx);
                    }
                },
            ));
            fields.insert(key, field);
        }
        (
            Self {
                tab: 0,
                open: [false; 5],
                sections: std::array::from_fn(|_| Rc::new(SectionState::default())),
                fields,
                stops,
                selected,
                slider: Default::default(),
            },
            subscriptions,
        )
    }
}
fn number(s: &TextBoxState, key: &str, cx: &App, low: f64, high: f64) -> Option<f64> {
    s.effects
        .fields
        .get(key)?
        .read(cx)
        .value()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= low && *n <= high)
}
fn color(s: &TextBoxState, key: &str, cx: &App) -> Option<Color> {
    let raw = s.effects.fields.get(key)?.read(cx).value();
    let raw = raw.trim().trim_start_matches('#');
    (raw.len() == 6)
        .then(|| u32::from_str_radix(raw, 16).ok())
        .flatten()
        .map(|n| Color::opaque(Rgb(n)))
}
pub(crate) fn model(s: &TextBoxState, cx: &App) -> Option<EffectModel> {
    let mut out = s.initial.effects.clone();
    if s.gradient {
        let mut stops = s.effects.stops.clone();
        stops.sort_by_key(|p| p.position);
        out.fill = Fill::Linear {
            angle: (number(s, "gradient-angle", cx, 0., 360.)? * 60000.).round() as i32,
            scaled: if let Fill::Linear { scaled, .. } = out.fill {
                scaled
            } else {
                false
            },
            stops,
        };
    } else if s.initial.gradient {
        let raw = s.fill.read(cx).value();
        out.fill = u32::from_str_radix(raw.trim().trim_start_matches('#'), 16)
            .ok()
            .map(|n| Fill::Solid(Color::opaque(Rgb(n))))
            // Switching a gradient off leaves a solid fill (the accent blue when no colour is typed), as it did before.
            .unwrap_or(Fill::Solid(Color::opaque(Rgb(0x4472C4))));
    }
    if s.shadow {
        let mut v = out
            .shadow
            .unwrap_or_else(|| Shadow::simple(Color::opaque(Rgb(0)), 0, 0, 0));
        v.color = color(s, "shadow-color", cx)?;
        v.color.alpha =
            ((100. - number(s, "shadow-transparency", cx, 0., 100.)?) * 255. / 100.).round() as u8;
        v.blur = (number(s, "shadow-blur", cx, 0., 200.)? * 12700.).round() as i64;
        v.distance = (number(s, "shadow-distance", cx, 0., 200.)? * 12700.).round() as i64;
        v.direction = (number(s, "shadow-angle", cx, 0., 360.)? * 60000.).round() as i32;
        let scale = (number(s, "shadow-size", cx, 1., 1000.)? * 1000.).round() as i32;
        v.sy=if v.sx!=0 {(v.sy as f64*scale as f64/v.sx as f64).round()as i32}else{scale};
        v.sx = scale;
        out.shadow = Some(v);
    } else {
        out.shadow = None;
    }
    out.glow = if s.glow {
        let mut c = color(s, "glow-color", cx)?;
        c.alpha =
            ((100. - number(s, "glow-transparency", cx, 0., 100.)?) * 255. / 100.).round() as u8;
        Some((
            c,
            (number(s, "glow-size", cx, 0., 200.)? * 12700.).round() as i64,
        ))
    } else {
        None
    };
    out.soft = if s.soft_edge {
        Some((number(s, "soft-size", cx, 0., 100.)? * 12700.).round() as i64)
    } else {
        None
    };
    if s.reflection {
        let mut v = out
            .reflection
            .unwrap_or_else(|| Reflection::standard(0, 45500));
        v.start_alpha =
            ((100. - number(s, "reflection-transparency", cx, 0., 100.)?) * 1000.).round() as i32;
        v.blur = (number(s, "reflection-blur", cx, 0., 200.)? * 12700.).round() as i64;
        v.distance = (number(s, "reflection-distance", cx, 0., 200.)? * 12700.).round() as i64;
        v.end_pos = (number(s, "reflection-size", cx, 0., 100.)? * 1000.).round() as i32;
        out.reflection = Some(v);
    } else {
        out.reflection = None;
    }
    Some(out)
}
impl SlideEditor {
    fn update_text_box_effect_field(&mut self, key: &str, cx: &mut Context<Self>) {
        let changed = self
            .doc_state()
            .and_then(|d| d.text_box.as_ref())
            .and_then(|s| {
                let selected = s.effects.selected;
                let old = s.effects.stops.get(selected)?;
                if key == "stop-position" {
                    Some((
                        selected,
                        GradientStop {
                            position: (number(s, key, cx, 0., 100.)? * 1000.).round() as u32,
                            color: old.color,
                        },
                    ))
                } else if key == "stop-color" {
                    Some((
                        selected,
                        GradientStop {
                            color: color(s, key, cx)?,
                            ..old.clone()
                        },
                    ))
                } else {
                    None
                }
            });
        if let Some((i, stop)) = changed {
            if let Some(s) = self.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                s.effects.stops[i] = stop;
            }
        }
        self.apply_text_box(cx);
    }
    pub(crate) fn text_box_decoration_panel(&self, cx: &Context<Self>) -> Option<Div> {
        let s = self.doc_state()?.text_box.as_ref()?;
        let effects = &s.effects;
        let mut body = v_flex().gap_2();
        for (index, (label, on, keys)) in [
            (
                "docx.text-box-panel.gradient",
                s.gradient,
                &["gradient-angle", "stop-position", "stop-color"][..],
            ),
            (
                "docx.text-box-panel.shadow",
                s.shadow,
                &[
                    "shadow-color",
                    "shadow-transparency",
                    "shadow-size",
                    "shadow-blur",
                    "shadow-angle",
                    "shadow-distance",
                ][..],
            ),
            (
                "docx.text-box-panel.glow",
                s.glow,
                &["glow-color", "glow-transparency", "glow-size"][..],
            ),
            (
                "docx.text-box-panel.soft-edge",
                s.soft_edge,
                &["soft-size"][..],
            ),
            (
                "docx.text-box-panel.reflection",
                s.reflection,
                &[
                    "reflection-transparency",
                    "reflection-size",
                    "reflection-blur",
                    "reflection-distance",
                ][..],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut content = v_flex()
                .gap_2()
                .px_3()
                .pb_2()
                .child(chrome::checkbox_button(
                    ("shape-effect-enable", index),
                    text(&format!("shape.effects.enable-{index}")),
                    on,
                    false,
                    cx.listener(move |e, _, _, cx| {
                        if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                            match index {
                                0 => s.gradient = !s.gradient,
                                1 => s.shadow = !s.shadow,
                                2 => s.glow = !s.glow,
                                3 => s.soft_edge = !s.soft_edge,
                                _ => s.reflection = !s.reflection,
                            }
                        }
                        e.apply_text_box(cx);
                    }),
                ));
            if index == 0 {
                content = content.child(gradient_slider::gradient_slider(
                    effects.stops.clone(),
                    effects.selected,
                    on,
                    effects.slider.clone(),
                    cx,
                    |e, stops, selected, w, cx| {
                        if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                            s.effects.stops = stops;
                            s.effects.selected = selected;
                            if let Some(stop) = s.effects.stops.get(selected) {
                                let c = rgb(stop.color);
                                let pos = (stop.position as f64 / 1000.).to_string();
                                for (key, value) in [("stop-color", c), ("stop-position", pos)] {
                                    s.effects.fields[key]
                                        .update(cx, |i, cx| i.set_value(value, w, cx));
                                }
                            }
                        }
                        e.apply_text_box(cx);
                    },
                ));
            }
            for &key in keys {
                let label = text(&format!("shape.effects.{key}"));
                let field = &effects.fields[key];
                let color = key.ends_with("color");
                if color {
                    content = content.child(chrome::color_field_row(
                        label,
                        chrome::panel_color_field_enabled(
                            match key {
                                "stop-color" => "gradient-stop-color",
                                "shadow-color" => "shape-shadow-color",
                                _ => "shape-glow-color",
                            },
                            field,
                            on,
                            cx,
                        )
                        .into_any_element(),
                    ));
                } else {
                    let (unit, max) = if key.contains("angle") {
                        ("°", 360.)
                    } else if key.contains("transparency")
                        || key.contains("position")
                        || key.contains("size") && index == 4
                    {
                        ("%", 100.)
                    } else if key == "shadow-size" {
                        ("%", 1000.)
                    } else {
                        ("pt", 200.)
                    };
                    content = content.child(chrome::text_field_row(
                        label,
                        chrome::text_field(field)
                            .disabled(!on)
                            .spinner(
                                SharedString::from(format!("shape-{key}")),
                                chrome::SpinnerSpec::new(1.).range(0., max),
                                cx,
                                move |e, _, _, cx| e.update_text_box_effect_field(key, cx),
                            )
                            .unit(unit, px(22.)),
                    ));
                }
            }
            if index == 0 {
                content = content.child(
                    chrome::action_button("gradient-remove-stop")
                        .label(text("shape.effects.remove-stop"))
                        .disabled(!on || effects.stops.len() <= 2)
                        .on_click(cx.listener(|e, _, w, cx| {
                            if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                                if s.effects.stops.len() > 2 {
                                    s.effects.stops.remove(s.effects.selected);
                                    s.effects.selected = 0;
                                    let stop = &s.effects.stops[0];
                                    s.effects.fields["stop-color"]
                                        .update(cx, |i, cx| i.set_value(rgb(stop.color), w, cx));
                                    s.effects.fields["stop-position"].update(cx, |i, cx| {
                                        i.set_value(
                                            (stop.position as f64 / 1000.).to_string(),
                                            w,
                                            cx,
                                        )
                                    });
                                }
                            }
                            e.apply_text_box(cx);
                        })),
                );
            }
            let open = effects.open[index];
            body = body.child(chrome::collapsible_section(
                ("shape-effect-section", index),
                text(label),
                open,
                content.into_any_element(),
                effects.sections[index].clone(),
                self.sidebar_scroll.clone(),
                cx.listener(move |e, _, _, cx| {
                    if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                        s.effects.open[index] = !open;
                    }
                    cx.notify();
                }),
            ));
        }
        Some(body)
    }
}
