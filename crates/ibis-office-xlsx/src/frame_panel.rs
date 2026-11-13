// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置 (書式 > サイズと位置…): 選択中の図形の高さ・幅・回転・横位置・縦位置の
//! 数値入力および縦横比固定の設定を行うサイドバーパネル。

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EmuRect, ShapeId};

use crate::shell::chrome::{ShownNumber, SidebarPanel, SpinnerSpec, Typed};
use crate::shell::effect_editor::{self, EffectEditor, EffectHooks, EffectModel, SECTIONS};
use crate::shell::shape_format_pane::{self as pane, DashUi};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The fields, in the panel's order: height, width (cm), rotation (°),
/// horizontal and vertical place (cm).
pub struct SheetFramePanel {
    pub(crate) shape_id: ShapeId,
    pub fields: Vec<Entity<InputState>>,
    pub(crate) error: Option<SharedString>,
    pub locked: bool,
    pub placement: ibis_doc_model::ObjectPlacement,
    pub print: bool,
    pub(crate) initial: Vec<String>,
    /// The height, width, rotation and place as the shape has them (EMU, 60 000ths of a degree): an Apply without an edit (a field
    /// entered and left) writes these exact values, not the digits the fields show.
    pub(crate) numbers: [ShownNumber; 5],
    /// The fill and the outline's colours (`RRGGBB`, empty for none), the outline's width (pt), and the text's size (pt);
    /// `has_fill` is whether the shape has a fill and an outline (a picture or a line has no fill), `has_text` whether it holds text.
    pub attrs: Vec<Entity<InputState>>,
    pub(crate) attrs_initial: Vec<String>,
    pub(crate) has_fill: bool,
    pub(crate) has_text: bool,
    /// The shared 図形の書式 pane's tab and line-style dropdown, and its decoration editor (a drawn shape only).
    pub(crate) tab: usize,
    pub(crate) dash: DashUi,
    pub(crate) effects: Option<SheetEffects>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

/// The decoration editor of the sheet's shape pane: the shared editor, which sections are on, and the model it was last shown or applied with.
pub(crate) struct SheetEffects {
    pub(crate) editor: EffectEditor,
    pub(crate) enabled: [bool; SECTIONS],
    pub(crate) initial: EffectModel,
}

fn effect_hooks() -> EffectHooks {
    EffectHooks {
        get: |e| e.sheet_state_mut().and_then(|s| s.panels.frame.as_mut()).and_then(|p| p.effects.as_mut()).map(|f| &mut f.editor),
        toggle: |e, i| {
            if let Some(f) = e.sheet_state_mut().and_then(|s| s.panels.frame.as_mut()).and_then(|p| p.effects.as_mut()) {
                f.enabled[i] = !f.enabled[i];
            }
        },
        apply: |e, w, cx| e.apply_sheet_effects(w, cx),
    }
}

/// The dash pattern the shape's outline has.
fn dash_of(shape: &ibis_doc_model::Shape) -> ibis_doc_model::LineDash {
    use ibis_doc_model::ShapeKind;
    match &shape.kind {
        ShapeKind::Auto(a) => a.stroke.as_ref().map(|s| s.dash),
        ShapeKind::Line(s) => Some(s.dash),
        _ => None,
    }
    .unwrap_or(ibis_doc_model::LineDash::Solid)
}

/// The shape's attributes as the panel's fields show them: fill, outline colour, outline width, text size.
fn shape_attrs(shape: &ibis_doc_model::Shape) -> (Vec<String>, bool, bool) {
    use ibis_doc_model::{Fill, ShapeKind};
    let hex = |c: &ibis_doc_model::Color| format!("{:06X}", c.rgb.0 & 0xffffff);
    let points = |emu: i64| {
        let v = emu as f64 / 12_700.;
        if (v - v.round()).abs() < 0.005 { format!("{}", v.round() as i64) } else { format!("{v:.2}").trim_end_matches('0').to_string() }
    };
    let (fill, line, width, has_fill) = match &shape.kind {
        ShapeKind::Auto(a) => (
            match &a.fill {
                Fill::Solid(c) => hex(c),
                _ => String::new(),
            },
            a.stroke.as_ref().map_or(String::new(), |s| hex(&s.color)),
            a.stroke.as_ref().map_or("0.75".to_string(), |s| points(s.width)),
            true,
        ),
        ShapeKind::TextBox(_) => (String::new(), String::new(), "0.75".to_string(), true),
        ShapeKind::Line(s) => (String::new(), hex(&s.color), points(s.width), false),
        _ => (String::new(), String::new(), String::new(), false),
    };
    let size = shape.text_body().map_or(String::new(), |b| {
        let first = b.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
        let size = first.and_then(|r| r.style.font_size).unwrap_or(b.style.font_size);
        points(size as i64 * 127)
    });
    let color = shape.text_body().map_or(String::new(), |b| {
        let first = b.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
        format!("{:06X}", first.and_then(|r| r.style.color).unwrap_or(b.style.color).0 & 0xffffff)
    });
    (vec![fill, line, width, size, color], has_fill, shape.text_body().is_some())
}

/// The largest size and place Excel takes, in EMU.
const LIMIT: f64 = 570_240_000.;

/// The digits a length (cm) and an angle (degrees) are shown with.
fn cm_spec() -> SpinnerSpec { SpinnerSpec::new(0.1).decimals(2) }
fn degrees_spec() -> SpinnerSpec { SpinnerSpec::new(1.0).decimals(1) }

/// The fields of the panel as the shape has them now: height, width, rotation, horizontal and vertical place.
fn frame_numbers(shape: &ibis_doc_model::Shape) -> [ShownNumber; 5] {
    [
        ShownNumber::one(shape.frame.height, 360_000., cm_spec()),
        ShownNumber::one(shape.frame.width, 360_000., cm_spec()),
        ShownNumber::one(shape.rotation as i64, 60_000., degrees_spec()),
        ShownNumber::one(shape.frame.x, 360_000., cm_spec()),
        ShownNumber::one(shape.frame.y, 360_000., cm_spec()),
    ]
}

fn length(number: &ShownNumber, value: &str, signed: bool) -> Option<i64> {
    match number.resolve(value) {
        Typed::Value(emu) => (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64),
        _ => None,
    }
}

fn angle(number: &ShownNumber, value: &str) -> Option<i32> {
    match number.resolve(value) {
        Typed::Value(units) if units.abs() <= 1e6 * 60_000. => Some((units as i64).rem_euclid(21_600_000) as i32),
        _ => None,
    }
}

#[ibis_office_macros::editor_ext(FramePanel)]
impl SlideEditor {
    /// 選択中の図形のサイズと位置パネルを開く.
    pub fn open_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shape_id) = self.editing.as_ref().map(|e| e.shape).or_else(|| self.selected_sheet_shape()) else { return };
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };
        let locked = self.slide().aspect_locked.contains(&shape_id);

        let numbers = frame_numbers(&shape);
        let initial: Vec<String> = numbers.iter().map(|n| n.text().to_string()).collect();
        let fields: Vec<_> = numbers.iter().map(|n| n.input(window, cx)).collect();

        let placement = self.slide().shape_placements.get(&shape_id).copied().unwrap_or_default();
        let print = self.slide().shape_prints.get(&shape_id).copied().unwrap_or(true);
        let (attrs_initial, has_fill, has_text) = shape_attrs(&shape);
        let attrs: Vec<Entity<InputState>> = attrs_initial.iter().map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v.clone()))).collect();
        // A value typed is applied when it is entered or the field is left (no 適用 button).
        let mut subscriptions = Vec::new();
        for input in fields.iter().chain(attrs.iter()) {
            subscriptions.push(cx.subscribe_in(input, window, |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.apply_sheet_frame_panel(window, cx);
                }
            }));
        }

        let effects = EffectModel::of_shape(&shape).map(|initial| {
            let (editor, subs) = EffectEditor::new(&initial, effect_hooks(), window, cx);
            subscriptions.extend(subs);
            SheetEffects { enabled: initial.flags(), editor, initial }
        });
        let state = self.wb_mut();
        state.close_settings();
        state.panels.frame = Some(SheetFramePanel {
            shape_id,
            fields,
            error: None,
            locked,
            placement,
            print,
            initial,
            numbers,
            attrs,
            attrs_initial,
            has_fill,
            has_text,
            tab: 0,
            dash: DashUi::new(dash_of(&shape)),
            effects,
            _subscriptions: subscriptions,
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel shows the shape's place, size and attributes as they are now (after a drag or an undo), unless a field is being typed in.
    pub(crate) fn refresh_sheet_frame_panel(&mut self, id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref().filter(|p| p.shape_id == id) else { return };
        let Some(shape) = self.slide().shape(id).cloned() else { return };
        let typing = panel.fields.iter().chain(panel.attrs.iter()).any(|f| f.read(cx).focus_handle(cx).is_focused(window));
        let numbers_now = frame_numbers(&shape);
        let now: Vec<String> = numbers_now.iter().map(|n| n.text().to_string()).collect();
        let (attrs_now, _, _) = shape_attrs(&shape);
        let model_now = EffectModel::of_shape(&shape);
        let effects_stale = panel.effects.as_ref().zip(model_now.as_ref()).is_some_and(|(e, m)| e.initial != *m);
        let dash_now = dash_of(&shape);
        let dash_stale = !panel.dash.open && panel.dash.dash != dash_now;
        if effects_stale && let (Some(f), Some(m)) = (panel.effects.as_ref(), model_now.as_ref()) {
            f.editor.show(m, effect_hooks(), window, cx);
        }
        if effects_stale || dash_stale {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                if let (Some(f), Some(m)) = (p.effects.as_mut(), model_now) {
                    f.enabled = m.flags();
                    f.initial = m;
                }
                if !p.dash.open {
                    p.dash.dash = dash_now;
                }
            }
            cx.notify();
        }
        let Some(panel) = self.wb().panels.frame.as_ref().filter(|p| p.shape_id == id) else { return };
        if typing || (numbers_now == panel.numbers && now == panel.initial && attrs_now == panel.attrs_initial) {
            return;
        }
        let (fields, attrs) = (panel.fields.clone(), panel.attrs.clone());
        for (field, value) in fields.iter().zip(now.iter()) {
            field.update(cx, |i, cx| i.set_value(value.clone(), window, cx));
        }
        for (field, value) in attrs.iter().zip(attrs_now.iter()) {
            field.update(cx, |i, cx| i.set_value(value.clone(), window, cx));
        }
        if let Some(p) = self.wb_mut().panels.frame.as_mut() {
            p.initial = now;
            p.numbers = numbers_now;
            p.attrs_initial = attrs_now;
        }
        cx.notify();
    }


    /// サイズと位置パネルの設定を適用.
    pub fn apply_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let shape_id = panel.shape_id;
        let locked = panel.locked;
        let values: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().trim().to_string()).collect();
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };

        let Some(height) = length(&panel.numbers[0], &values[0], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-height").into());
            }
            cx.notify();
            return;
        };
        let Some(width) = length(&panel.numbers[1], &values[1], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-width").into());
            }
            cx.notify();
            return;
        };
        let Some(rotation) = angle(&panel.numbers[2], &values[2]) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-the-rotation-as-a-number").into());
            }
            cx.notify();
            return;
        };
        let Some(x) = length(&panel.numbers[3], &values[3], true) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-number-for-the-horizontal-position").into());
            }
            cx.notify();
            return;
        };
        let Some(y) = length(&panel.numbers[4], &values[4], true) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-number-for-the-vertical-position").into());
            }
            cx.notify();
            return;
        };

        let mut new_frame = EmuRect::new(x, y, width, height);
        if locked && shape.frame.height > 0 && shape.frame.width > 0 {
            let height_changed = values[0] != panel.initial[0];
            let width_changed = values[1] != panel.initial[1];
            if height_changed && !width_changed {
                new_frame.width = (height as f64 * shape.frame.width as f64 / shape.frame.height as f64).round() as i64;
            } else if width_changed && !height_changed {
                new_frame.height = (width as f64 * shape.frame.height as f64 / shape.frame.width as f64).round() as i64;
            }
        }

        let slide = self.current_slide;
        let mut commands = Vec::new();
        if self.slide().aspect_locked.contains(&shape_id) != locked {
            commands.push(Command::SetAspectLocked { slide, shape: shape_id, locked });
        }
        if shape.frame != new_frame {
            commands.push(Command::SetFrame { slide, shape: shape_id, frame: new_frame });
        }
        if shape.rotation != rotation {
            commands.push(Command::SetTransform { slide, shape: shape_id, rotation, flip_h: shape.flip_h, flip_v: shape.flip_v });
        }
        let cur_placement = self.slide().shape_placements.get(&shape_id).copied().unwrap_or_default();
        if cur_placement != panel.placement {
            commands.push(Command::SetShapePlacement { slide, shape: shape_id, placement: panel.placement });
        }
        let cur_print = self.slide().shape_prints.get(&shape_id).copied().unwrap_or(true);
        if cur_print != panel.print {
            commands.push(Command::SetShapePrint { slide, shape: shape_id, print: panel.print });
        }

        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        self.apply_sheet_shape_attrs(window, cx);
        self.apply_sheet_dash(cx);
        // What the panel holds is what the shape has now.
        if let Some(shape) = self.slide().shape(shape_id).cloned()
            && let Some(p) = self.wb_mut().panels.frame.as_mut()
        {
            p.numbers = frame_numbers(&shape);
            p.initial = p.numbers.iter().map(|n| n.text().to_string()).collect();
            p.attrs_initial = shape_attrs(&shape).0;
            p.error = None;
        }
        cx.notify();
    }

    /// The fill, the outline and the text size typed in the panel, given to the shape (each one that changed).
    fn apply_sheet_shape_attrs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let values: Vec<String> = panel.attrs.iter().map(|f| f.read(cx).value().trim().trim_start_matches('#').to_string()).collect();
        let before = panel.attrs_initial.clone();
        let shape_id = panel.shape_id;
        if values.len() < 5
            || values == before
            || (self.selected_sheet_shape() != Some(shape_id) && !self.selected_ids().contains(&shape_id))
        {
            return;
        }
        let colour = |t: &str| (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten().map(|v| ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(v)));
        if values[0] != before[0] {
            if values[0].is_empty() {
                self.fill_sheet_shape(ibis_doc_model::Fill::None, cx);
            } else if let Some(c) = colour(&values[0]) {
                self.fill_sheet_shape(ibis_doc_model::Fill::Solid(c), cx);
            }
        }
        if values[1] != before[1] {
            if values[1].is_empty() {
                self.stroke_sheet_shape_color(None, cx);
            } else if let Some(c) = colour(&values[1]) {
                self.stroke_sheet_shape_color(Some(c), cx);
            }
        }
        if values[2] != before[2]
            && let Ok(pt) = values[2].parse::<f64>()
            && pt > 0.0
            && pt <= 1584.0
        {
            self.stroke_sheet_shape_width((pt * 12_700.).round() as i64, cx);
        }
        if values[3] != before[3]
            && let Ok(pt) = values[3].parse::<f32>()
            && (1.0..=409.0).contains(&pt)
        {
            let _ = window;
            self.format_sheet_shape_text(&ibis_doc_model::StyleChange::FontSize((pt * 100.0).round() as u32), cx);
        }
        if values[4] != before[4]
            && values[4].len() == 6
            && let Ok(v) = u32::from_str_radix(&values[4], 16)
        {
            self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Color(ibis_doc_model::Rgb(v)), cx);
        }
    }

    /// The input of the shape pane's attribute `index` (0 fill, 1 outline, 2 outline width, 3 text size, 4 text colour).
    pub fn frame_panel_attr(&self, index: usize) -> Option<Entity<InputState>> {
        self.sheet_state()?.panels.frame.as_ref()?.attrs.get(index).cloned()
    }
    /// Choose `dash` in the shape pane's line-style dropdown and apply it.
    pub fn choose_frame_panel_dash(&mut self, dash: ibis_doc_model::LineDash, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.frame.as_mut() {
            p.dash.dash = dash;
        }
        self.apply_sheet_frame_panel(window, cx);
    }
    /// Whether the shape pane is open.
    pub fn frame_panel_open(&self) -> bool {
        self.sheet_state().is_some_and(|s| s.panels.frame.is_some())
    }

    /// The dash pattern chosen in the pane is written to the shape's outline when it differs.
    fn apply_sheet_dash(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let (id, dash) = (panel.shape_id, panel.dash.dash);
        if self.slide().shape(id).is_some_and(|s| dash_of(s) != dash) {
            self.stroke_sheet_shape_dash(dash, cx);
        }
    }

    /// The decoration fields changed: write what differs from what was shown (the shape's shadow, glow, soft edge, reflection, and the
    /// text's outline, shadow, glow), as one step.
    pub(crate) fn apply_sheet_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let Some(st) = panel.effects.as_ref() else { return };
        let Some(model) = effect_editor::model(&st.editor, &st.initial, &st.enabled, false, "", cx) else { return };
        let initial = st.initial.clone();
        if model == initial {
            return;
        }
        let id = panel.shape_id;
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(shape) = self.slide().shape(id) {
            if let ibis_doc_model::ShapeKind::Auto(auto) = &shape.kind {
                let next = model.written_to(&initial, auto);
                if next != *auto {
                    commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ibis_doc_model::ShapeKind::Auto(next)) });
                }
            }
            let text = model.text_change(&initial);
            if text != ibis_doc_model::CharacterFormat::default()
                && let Some(body) = shape.text_body()
            {
                commands.push(Command::SetCharacterFormat { slide, shape: id, start: 0, end: body.text().chars().count(), format: text });
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        let _ = window;
        if let Some(f) = self.wb_mut().panels.frame.as_mut().and_then(|p| p.effects.as_mut()) {
            f.initial = model;
        }
        cx.notify();
    }



    /// 図形の書式 pane of the sheet: the shared pane (the document and presentation screens' too), with this screen's fields; its
    /// own rows (aspect lock, object positioning, print) come at the end of the basic tab.
    pub fn sheet_frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.panels.frame.as_ref()?;
        let (fields, error, locked, placement, print) = (panel.fields.clone(), panel.error.clone(), panel.locked, panel.placement, panel.print);
        let (attrs, has_fill, has_text, tab, dash) = (panel.attrs.clone(), panel.has_fill, panel.has_text, panel.tab, panel.dash);

        let mut tail: Vec<AnyElement> = Vec::new();
        tail.push(
            crate::shell::chrome::checkbox_button("xlsx-frame-lock", crate::shell::i18n::text("pptx.frame-panel.lock-aspect-ratio"), locked, false, cx.listener(|this, _, window, cx| {
                    if let Some(p) = this.wb_mut().panels.frame.as_mut() {
                        p.locked = !p.locked;
                    }
                    this.apply_sheet_frame_panel(window, cx);
                })).into_any_element(),
        );
        tail.push(div().pt(px(8.)).text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.properties")).into_any_element());
        tail.push(div().text_sm().text_color(rgb(0x666666)).child(crate::shell::i18n::text("_xlsx.object-positioning")).into_any_element());
        for (p, label, btn_id) in [
            (ibis_doc_model::ObjectPlacement::MoveAndSize, "xlsx.placement.move-and-size", "xlsx-placement-move-and-size"),
            (ibis_doc_model::ObjectPlacement::MoveOnly, "xlsx.placement.move-only", "xlsx-placement-move-only"),
            (ibis_doc_model::ObjectPlacement::Free, "xlsx.placement.free", "xlsx-placement-free"),
        ] {
            let is_selected = placement == p;
            tail.push(
                crate::shell::chrome::radio_button(btn_id, is_selected, false, crate::shell::i18n::text(label), cx.listener(move |this, _, window, cx| {
                        if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                            panel.placement = p;
                        }
                        this.apply_sheet_frame_panel(window, cx);
                    })).into_any_element(),
            );
        }
        tail.push(
            crate::shell::chrome::checkbox_button("xlsx-frame-print", crate::shell::i18n::text("_xlsx.print-object"), print, false, cx.listener(|this, _, window, cx| {
                    if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                        panel.print = !panel.print;
                    }
                    this.apply_sheet_frame_panel(window, cx);
                })).into_any_element(),
        );
        if let Some(error) = error {
            tail.push(div().text_sm().text_color(rgb(0xb3261e)).child(error).into_any_element());
        }

        let shared = pane::ShapeFormatFields {
            unit: pane::SizeUnit::Centimetre,
            width: fields[1].clone(),
            height: fields[0].clone(),
            rotation: Some(fields[2].clone()),
            left: Some(fields[3].clone()),
            top: Some(fields[4].clone()),
            fill: (has_fill && attrs.len() > 4).then(|| attrs[0].clone()),
            line: (attrs.len() > 4).then(|| attrs[1].clone()),
            line_width: (attrs.len() > 4).then(|| attrs[2].clone()),
            font_size: (has_text && attrs.len() > 4).then(|| attrs[3].clone()),
            text_color: (has_text && attrs.len() > 4).then(|| attrs[4].clone()),
        };
        let hooks = pane::ShapeFormatHooks {
            set_tab: |e, index| {
                if let Some(p) = e.sheet_state_mut().and_then(|s| s.panels.frame.as_mut()) {
                    p.tab = index;
                }
            },
            dash_ui: |e| e.sheet_state_mut().and_then(|s| s.panels.frame.as_mut()).map(|p| &mut p.dash),
            apply: |e, w, cx| e.apply_sheet_frame_panel(w, cx),
        };
        let panel = self.sheet_state()?.panels.frame.as_ref()?;
        let spec = pane::ShapeFormatSpec {
            id: "xlsx",
            plain_title_key: "shape.effects.panel-title",
            tab,
            fields: shared,
            hooks,
            dash: Some(dash),
            effects: panel.effects.as_ref().map(|f| pane::ShapeFormatEffects { editor: &f.editor, enabled: f.enabled, hooks: effect_hooks(), scroll: self.sidebar_scroll.clone() }),
            lead: Vec::new(),
            after_place: Vec::new(),
            after_line: Vec::new(),
            tail,
        };
        Some(pane::shape_format_panel(spec, cx))
    }
}

