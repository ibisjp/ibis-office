// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shape-decoration editor shared by the document, presentation and spreadsheet screens (moved out of the document screen's
//! text box panel, 2026-11-12): gradient, shadow, glow, soft edge, reflection, and the text's outline, shadow and glow.
//! A screen keeps the on/off flags and the model it edits; this module owns the fields, the sections and the arithmetic.
//! It retains OOXML parameters the UI does not edit (skew, alignment, fade).
use crate::chrome::{
    self,
    collapsible_section::SectionState,
    gradient_slider::{self, GradientSliderState},
};
use crate::editor::SlideEditor;
use crate::i18n::text;
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

/// How many sections there are: 0 gradient, 1 shadow, 2 glow, 3 soft edge, 4 reflection, 5 text outline, 6 text shadow, 7 text glow.
pub const SECTIONS: usize = 8;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct EffectModel {
    pub fill: Fill,
    pub shadow: Option<Shadow>,
    pub glow: Option<(Color, i64)>,
    pub soft: Option<i64>,
    pub reflection: Option<Reflection>,
    /// The characters' outline (colour, width in EMU), shadow and glow.
    pub text_outline: Option<(Color, i64)>,
    pub text_shadow: Option<Shadow>,
    pub text_glow: Option<(Color, i64)>,
}

impl EffectModel {
    /// What a drawn shape (and its first run, for the text part) has; `None` for a shape that is not a drawn shape.
    pub fn of_shape(shape: &ibis_doc_model::Shape) -> Option<Self> {
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { return None };
        let mut model = EffectModel { fill: a.fill.clone(), shadow: a.shadow, glow: a.glow, soft: a.soft_edge, reflection: a.reflection, ..Default::default() };
        if let Some(run) = shape.text_body().and_then(|b| b.paragraphs.iter().flat_map(|p| p.runs.iter()).next()) {
            model.text_outline = run.style.outline;
            model.text_shadow = run.style.shadow;
            model.text_glow = run.style.glow;
        }
        Some(model)
    }
    /// Which sections are on (the check boxes) for this model.
    pub fn flags(&self) -> [bool; SECTIONS] {
        [false, self.shadow.is_some(), self.glow.is_some(), self.soft.is_some(), self.reflection.is_some(), self.text_outline.is_some(), self.text_shadow.is_some(), self.text_glow.is_some()]
    }
    /// `auto` with the shape effects that differ between `initial` and `self` written in (the others are left as they are).
    pub fn written_to(&self, initial: &EffectModel, auto: &ibis_doc_model::AutoShape) -> ibis_doc_model::AutoShape {
        let mut next = auto.clone();
        if self.shadow != initial.shadow {
            next.shadow = self.shadow;
        }
        if self.glow != initial.glow {
            next.glow = self.glow;
        }
        if self.soft != initial.soft {
            next.soft_edge = self.soft;
        }
        if self.reflection != initial.reflection {
            next.reflection = self.reflection;
        }
        next
    }
    /// The character change for the text effects that differ between `initial` and `self`.
    pub fn text_change(&self, initial: &EffectModel) -> ibis_doc_model::CharacterFormat {
        ibis_doc_model::CharacterFormat {
            outline: (self.text_outline != initial.text_outline).then_some(self.text_outline),
            shadow: (self.text_shadow != initial.text_shadow).then_some(self.text_shadow),
            glow: (self.text_glow != initial.text_glow).then_some(self.text_glow),
            ..Default::default()
        }
    }
}

/// What a screen gives the editor: where its `EffectEditor` is, what a toggle does, and what to run after a change.
#[derive(Clone, Copy)]
pub struct EffectHooks {
    pub get: fn(&mut SlideEditor) -> Option<&mut EffectEditor>,
    pub toggle: fn(&mut SlideEditor, usize),
    pub apply: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>),
}

pub struct EffectEditor {
    pub tab: usize,
    pub open: [bool; SECTIONS],
    pub sections: [Rc<SectionState>; SECTIONS],
    pub fields: BTreeMap<&'static str, Entity<InputState>>,
    pub stops: Vec<GradientStop>,
    pub selected: usize,
    pub slider: GradientSliderState,
}
fn rgb(c: Color) -> String {
    format!("{:06X}", c.rgb.0 & 0xffffff)
}
fn num(v: f64) -> String {
    format!("{v:.4}").trim_end_matches('0').trim_end_matches('.').to_string()
}
fn default_shadow() -> Shadow {
    Shadow::simple(Color { rgb: Rgb(0), alpha: 102 }, 50800, 38100, 2700000)
}
fn shadow_values(prefix: &str, s: &Shadow) -> Vec<(String, String)> {
    vec![
        (format!("{prefix}-color"), rgb(s.color)),
        (format!("{prefix}-transparency"), num(100. - s.color.alpha as f64 / 255. * 100.)),
        (format!("{prefix}-blur"), num(s.blur as f64 / 12700.)),
        (format!("{prefix}-distance"), num(s.distance as f64 / 12700.)),
        (format!("{prefix}-angle"), num(s.direction as f64 / 60000.)),
        (format!("{prefix}-size"), num(s.sx as f64 / 1000.)),
    ]
}
fn leak(s: String) -> &'static str {
    // The keys are a fixed small set; leaking them once per editor is bounded by interning.
    use std::sync::Mutex;
    static KEYS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut keys = KEYS.lock().unwrap();
    if let Some(k) = keys.iter().find(|k| **k == s) {
        return k;
    }
    let k: &'static str = Box::leak(s.into_boxed_str());
    keys.push(k);
    k
}

impl EffectEditor {
    pub fn new(model: &EffectModel, hooks: EffectHooks, window: &mut Window, cx: &mut Context<SlideEditor>) -> (Self, Vec<Subscription>) {
        // Existing document parameters always take precedence, including skew/alignment and fade values.
        let color = match model.fill {
            Fill::Solid(c) => c,
            _ => Color::opaque(Rgb(0x4472c4)),
        };
        let shadow = model.shadow.clone().unwrap_or_else(default_shadow);
        let glow = model.glow.unwrap_or((Color { alpha: 102, ..color }, 63500));
        let text_shadow = model.text_shadow.clone().unwrap_or_else(default_shadow);
        let text_glow = model.text_glow.unwrap_or((Color { alpha: 102, ..color }, 63500));
        let text_outline = model.text_outline.unwrap_or((Color::opaque(Rgb(0)), 12700));
        let reflect = model.reflection.clone().unwrap_or_else(|| Reflection::standard(0, 45500));
        let (angle, stops) = if let Fill::Linear { angle, stops, .. } = &model.fill {
            (*angle, stops.clone())
        } else {
            (
                5400000,
                vec![
                    GradientStop { position: 0, color },
                    GradientStop { position: 100000, color: Color::opaque(Rgb(0xffffff)) },
                ],
            )
        };
        let first = stops.first().cloned().unwrap_or(GradientStop { position: 0, color });
        let mut values: Vec<(String, String)> = vec![
            ("gradient-angle".into(), num(angle as f64 / 60000.)),
            ("stop-position".into(), num(first.position as f64 / 1000.)),
            ("stop-color".into(), rgb(first.color)),
        ];
        values.extend(shadow_values("shadow", &shadow));
        values.extend([
            ("glow-color".into(), rgb(glow.0)),
            ("glow-transparency".into(), num(100. - glow.0.alpha as f64 / 255. * 100.)),
            ("glow-size".into(), num(glow.1 as f64 / 12700.)),
            ("soft-size".into(), num(model.soft.unwrap_or(63500) as f64 / 12700.)),
            ("reflection-transparency".into(), num(100. - reflect.start_alpha as f64 / 1000.)),
            ("reflection-blur".into(), num(reflect.blur as f64 / 12700.)),
            ("reflection-distance".into(), num(reflect.distance as f64 / 12700.)),
            ("reflection-size".into(), num(reflect.end_pos as f64 / 1000.)),
            ("text-outline-color".into(), rgb(text_outline.0)),
            ("text-outline-width".into(), num(text_outline.1 as f64 / 12700.)),
        ]);
        values.extend(shadow_values("text-shadow", &text_shadow));
        values.extend([
            ("text-glow-color".into(), rgb(text_glow.0)),
            ("text-glow-transparency".into(), num(100. - text_glow.0.alpha as f64 / 255. * 100.)),
            ("text-glow-size".into(), num(text_glow.1 as f64 / 12700.)),
        ]);
        let mut fields = BTreeMap::new();
        let mut subscriptions = Vec::new();
        for (key, value) in values {
            let key = leak(key);
            let field = cx.new(|cx| {
                let mut i = InputState::new(window, cx);
                i.set_value(value, window, cx);
                i
            });
            subscriptions.push(cx.subscribe_in(&field, window, move |e, _, ev: &InputEvent, w, cx| {
                if matches!(ev, InputEvent::Change | InputEvent::PressEnter { .. }) {
                    field_changed(e, key, hooks, w, cx);
                }
            }));
            fields.insert(key, field);
        }
        (
            Self { tab: 0, open: [false; SECTIONS], sections: std::array::from_fn(|_| Rc::new(SectionState::default())), fields, stops, selected: 0, slider: Default::default() },
            subscriptions,
        )
    }
    /// Show the values of a model (after the document changed under the panel): fields whose text differs are rewritten.
    pub fn show(&self, model: &EffectModel, hooks: EffectHooks, window: &mut Window, cx: &mut Context<SlideEditor>) {
        let (fresh, _) = Self::new(model, hooks, window, cx);
        for (key, field) in &fresh.fields {
            let value = field.read(cx).value().to_string();
            let mine = &self.fields[key];
            if mine.read(cx).value().as_str() != value {
                mine.update(cx, |i, cx| i.set_value(value, window, cx));
            }
        }
    }
}
fn number(s: &EffectEditor, key: &str, cx: &App, low: f64, high: f64) -> Option<f64> {
    s.fields.get(key)?.read(cx).value().parse::<f64>().ok().filter(|n| n.is_finite() && *n >= low && *n <= high)
}
fn color(s: &EffectEditor, key: &str, cx: &App) -> Option<Color> {
    let raw = s.fields.get(key)?.read(cx).value();
    let raw = raw.trim().trim_start_matches('#');
    (raw.len() == 6).then(|| u32::from_str_radix(raw, 16).ok()).flatten().map(|n| Color::opaque(Rgb(n)))
}
fn shadow_model(s: &EffectEditor, prefix: &str, own: Option<Shadow>, cx: &App) -> Option<Shadow> {
    let k = |name: &str| leak(format!("{prefix}-{name}"));
    let mut v = own.unwrap_or_else(|| Shadow::simple(Color::opaque(Rgb(0)), 0, 0, 0));
    v.color = color(s, k("color"), cx)?;
    v.color.alpha = ((100. - number(s, k("transparency"), cx, 0., 100.)?) * 255. / 100.).round() as u8;
    v.blur = (number(s, k("blur"), cx, 0., 200.)? * 12700.).round() as i64;
    v.distance = (number(s, k("distance"), cx, 0., 200.)? * 12700.).round() as i64;
    v.direction = (number(s, k("angle"), cx, 0., 360.)? * 60000.).round() as i32;
    let scale = (number(s, k("size"), cx, 1., 1000.)? * 1000.).round() as i32;
    v.sy = if v.sx != 0 { (v.sy as f64 * scale as f64 / v.sx as f64).round() as i32 } else { scale };
    v.sx = scale;
    Some(v)
}
/// The model the fields describe, starting from `initial`: `enabled` says which sections are on. `fill_text` is the solid fill field
/// a screen shows when a gradient is switched off. `None` when a shown field holds something that is not a value.
pub fn model(s: &EffectEditor, initial: &EffectModel, enabled: &[bool; SECTIONS], gradient_was_on: bool, fill_text: &str, cx: &App) -> Option<EffectModel> {
    let mut out = initial.clone();
    if enabled[0] {
        let mut stops = s.stops.clone();
        stops.sort_by_key(|p| p.position);
        out.fill = Fill::Linear {
            angle: (number(s, "gradient-angle", cx, 0., 360.)? * 60000.).round() as i32,
            scaled: if let Fill::Linear { scaled, .. } = out.fill { scaled } else { false },
            stops,
        };
    } else if gradient_was_on {
        out.fill = u32::from_str_radix(fill_text.trim().trim_start_matches('#'), 16)
            .ok()
            .map(|n| Fill::Solid(Color::opaque(Rgb(n))))
            // Switching a gradient off leaves a solid fill (the accent blue when no colour is typed).
            .unwrap_or(Fill::Solid(Color::opaque(Rgb(0x4472C4))));
    }
    out.shadow = if enabled[1] { Some(shadow_model(s, "shadow", out.shadow.clone(), cx)?) } else { None };
    out.glow = if enabled[2] {
        let mut c = color(s, "glow-color", cx)?;
        c.alpha = ((100. - number(s, "glow-transparency", cx, 0., 100.)?) * 255. / 100.).round() as u8;
        Some((c, (number(s, "glow-size", cx, 0., 200.)? * 12700.).round() as i64))
    } else {
        None
    };
    out.soft = if enabled[3] { Some((number(s, "soft-size", cx, 0., 100.)? * 12700.).round() as i64) } else { None };
    if enabled[4] {
        let mut v = out.reflection.unwrap_or_else(|| Reflection::standard(0, 45500));
        v.start_alpha = ((100. - number(s, "reflection-transparency", cx, 0., 100.)?) * 1000.).round() as i32;
        v.blur = (number(s, "reflection-blur", cx, 0., 200.)? * 12700.).round() as i64;
        v.distance = (number(s, "reflection-distance", cx, 0., 200.)? * 12700.).round() as i64;
        v.end_pos = (number(s, "reflection-size", cx, 0., 100.)? * 1000.).round() as i32;
        out.reflection = Some(v);
    } else {
        out.reflection = None;
    }
    out.text_outline = if enabled[5] {
        Some((color(s, "text-outline-color", cx)?, (number(s, "text-outline-width", cx, 0., 200.)? * 12700.).round() as i64))
    } else {
        None
    };
    out.text_shadow = if enabled[6] { Some(shadow_model(s, "text-shadow", out.text_shadow.clone(), cx)?) } else { None };
    out.text_glow = if enabled[7] {
        let mut c = color(s, "text-glow-color", cx)?;
        c.alpha = ((100. - number(s, "text-glow-transparency", cx, 0., 100.)?) * 255. / 100.).round() as u8;
        Some((c, (number(s, "text-glow-size", cx, 0., 200.)? * 12700.).round() as i64))
    } else {
        None
    };
    Some(out)
}

/// A field of a gradient stop changed (or any other field): keep the selected stop in step, then let the screen apply.
pub fn field_changed(e: &mut SlideEditor, key: &str, hooks: EffectHooks, window: &mut Window, cx: &mut Context<SlideEditor>) {
    let changed = (hooks.get)(e).and_then(|s| {
        let old = s.stops.get(s.selected)?;
        if key == "stop-position" {
            Some((s.selected, GradientStop { position: (number(s, key, cx, 0., 100.)? * 1000.).round() as u32, color: old.color }))
        } else if key == "stop-color" {
            Some((s.selected, GradientStop { color: color(s, key, cx)?, ..old.clone() }))
        } else {
            None
        }
    });
    if let Some((i, stop)) = changed {
        if let Some(s) = (hooks.get)(e) {
            s.stops[i] = stop;
        }
    }
    (hooks.apply)(e, window, cx);
}

const SECTION_FIELDS: [(&str, &[&str]); SECTIONS] = [
    ("docx.text-box-panel.gradient", &["gradient-angle", "stop-position", "stop-color"]),
    ("docx.text-box-panel.shadow", &["shadow-color", "shadow-transparency", "shadow-size", "shadow-blur", "shadow-angle", "shadow-distance"]),
    ("docx.text-box-panel.glow", &["glow-color", "glow-transparency", "glow-size"]),
    ("docx.text-box-panel.soft-edge", &["soft-size"]),
    ("docx.text-box-panel.reflection", &["reflection-transparency", "reflection-size", "reflection-blur", "reflection-distance"]),
    ("shape.effects.title-text-outline", &["text-outline-color", "text-outline-width"]),
    ("shape.effects.title-text-shadow", &["text-shadow-color", "text-shadow-transparency", "text-shadow-size", "text-shadow-blur", "text-shadow-angle", "text-shadow-distance"]),
    ("shape.effects.title-text-glow", &["text-glow-color", "text-glow-transparency", "text-glow-size"]),
];

/// The sections `shown` (indices into [`SECTIONS`]) as collapsible sections, each with its enable checkbox and its fields.
pub fn panel(s: &EffectEditor, enabled: &[bool; SECTIONS], shown: &[usize], hooks: EffectHooks, scroll: ScrollHandle, cx: &Context<SlideEditor>) -> gpui_kit::Div {
    let mut body = v_flex().gap_2();
    for &index in shown {
        let (label, keys) = SECTION_FIELDS[index];
        let on = enabled[index];
        let mut content = v_flex().gap_2().px_3().pb_2().child(chrome::checkbox_button(
            ("shape-effect-enable", index),
            text(&format!("shape.effects.enable-{index}")),
            on,
            false,
            cx.listener(move |e, _, w, cx| {
                (hooks.toggle)(e, index);
                (hooks.apply)(e, w, cx);
            }),
        ));
        if index == 0 {
            content = content.child(gradient_slider::gradient_slider(s.stops.clone(), s.selected, on, s.slider.clone(), cx, move |e, stops, selected, w, cx| {
                if let Some(s) = (hooks.get)(e) {
                    s.stops = stops;
                    s.selected = selected;
                    if let Some(stop) = s.stops.get(selected) {
                        let c = rgb(stop.color);
                        let pos = (stop.position as f64 / 1000.).to_string();
                        for (key, value) in [("stop-color", c), ("stop-position", pos)] {
                            s.fields[key].update(cx, |i, cx| i.set_value(value, w, cx));
                        }
                    }
                }
                (hooks.apply)(e, w, cx);
            }));
        }
        for &key in keys {
            let label = text(&format!("shape.effects.{key}"));
            let field = &s.fields[key];
            if key.ends_with("color") {
                content = content.child(chrome::color_field_row(
                    label,
                    chrome::panel_color_field_enabled(
                        match key {
                            "stop-color" => "gradient-stop-color",
                            "shadow-color" => "shape-shadow-color",
                            "glow-color" => "shape-glow-color",
                            "text-outline-color" => "text-outline-color",
                            "text-shadow-color" => "text-shadow-color",
                            "text-glow-color" => "text-glow-color",
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
                } else if key.contains("transparency") || key.contains("position") || key.contains("size") && index == 4 {
                    ("%", 100.)
                } else if key.ends_with("shadow-size") {
                    ("%", 1000.)
                } else {
                    ("pt", 200.)
                };
                content = content.child(chrome::text_field_row(
                    label,
                    chrome::text_field(field)
                        .disabled(!on)
                        .spinner(SharedString::from(format!("shape-{key}")), chrome::SpinnerSpec::new(1.).range(0., max), cx, move |e, _, w, cx| field_changed(e, key, hooks, w, cx))
                        .unit(unit, px(22.)),
                ));
            }
        }
        if index == 0 {
            content = content.child(
                chrome::action_button("gradient-remove-stop").label(text("shape.effects.remove-stop")).disabled(!on || s.stops.len() <= 2).on_click(cx.listener(move |e, _, w, cx| {
                    if let Some(s) = (hooks.get)(e) {
                        if s.stops.len() > 2 {
                            s.stops.remove(s.selected);
                            s.selected = 0;
                            let stop = &s.stops[0];
                            s.fields["stop-color"].update(cx, |i, cx| i.set_value(rgb(stop.color), w, cx));
                            s.fields["stop-position"].update(cx, |i, cx| i.set_value((stop.position as f64 / 1000.).to_string(), w, cx));
                        }
                    }
                    (hooks.apply)(e, w, cx);
                })),
            );
        }
        let open = s.open[index];
        body = body.child(chrome::collapsible_section(
            ("shape-effect-section", index),
            text(label),
            open,
            content.into_any_element(),
            s.sections[index].clone(),
            scroll.clone(),
            cx.listener(move |e, _, _, cx| {
                if let Some(s) = (hooks.get)(e) {
                    s.open[index] = !open;
                }
                cx.notify();
            }),
        ));
    }
    body
}
