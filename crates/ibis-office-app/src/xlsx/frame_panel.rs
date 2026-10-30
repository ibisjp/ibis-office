// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置 (書式 > サイズと位置…): 選択中の図形の高さ・幅・回転・横位置・縦位置の
//! 数値入力および縦横比固定の設定を行うサイドバーパネル。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EmuRect, ShapeId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The fields, in the panel's order: height, width (cm), rotation (°),
/// horizontal and vertical place (cm).
pub(crate) struct SheetFramePanel {
    pub(crate) shape_id: ShapeId,
    pub(crate) fields: Vec<Entity<InputState>>,
    pub(crate) error: Option<SharedString>,
    pub(crate) locked: bool,
    pub(crate) placement: ibis_doc_model::ObjectPlacement,
    pub(crate) print: bool,
    pub(crate) initial: Vec<String>,
    /// The fill and the outline's colours (`RRGGBB`, empty for none), the outline's width (pt), and the text's size (pt);
    /// `has_fill` is whether the shape has a fill and an outline (a picture or a line has no fill), `has_text` whether it holds text.
    pub(crate) attrs: Vec<Entity<InputState>>,
    pub(crate) attrs_initial: Vec<String>,
    pub(crate) has_fill: bool,
    pub(crate) has_text: bool,
    pub(crate) _subscriptions: Vec<Subscription>,
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
    (vec![fill, line, width, size], has_fill, shape.text_body().is_some())
}

/// The largest size and place Excel takes, in EMU.
const LIMIT: f64 = 570_240_000.;

fn length(value: &str, signed: bool) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64)
}

fn angle(value: &str) -> Option<i32> {
    let degrees: f64 = value.trim().parse().ok()?;
    if !degrees.is_finite() || degrees.abs() > 1e6 {
        return None;
    }
    Some(((degrees * 60_000.).round() as i64).rem_euclid(21_600_000) as i32)
}

fn cm(emu: i64) -> String {
    let v = format!("{:.2}", emu as f64 / 360_000.);
    v.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl SlideEditor {
    /// 選択中の図形のサイズと位置パネルを開く.
    pub(crate) fn open_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };
        let locked = self.slide().aspect_locked.contains(&shape_id);

        let height_str = cm(shape.frame.height);
        let width_str = cm(shape.frame.width);
        let rotation_str = format!("{:.1}", shape.rotation as f64 / 60_000.).trim_end_matches('0').trim_end_matches('.').to_string();
        let x_str = cm(shape.frame.x);
        let y_str = cm(shape.frame.y);

        let initial = vec![height_str.clone(), width_str.clone(), rotation_str.clone(), x_str.clone(), y_str.clone()];
        let fields = vec![
            cx.new(|cx| InputState::new(window, cx).default_value(height_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(width_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(rotation_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(x_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(y_str)),
        ];

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
            attrs,
            attrs_initial,
            has_fill,
            has_text,
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
        let now = vec![cm(shape.frame.height), cm(shape.frame.width), format!("{:.1}", shape.rotation as f64 / 60_000.).trim_end_matches('0').trim_end_matches('.').to_string(), cm(shape.frame.x), cm(shape.frame.y)];
        let (attrs_now, _, _) = shape_attrs(&shape);
        if typing || (now == panel.initial && attrs_now == panel.attrs_initial) {
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
            p.attrs_initial = attrs_now;
        }
        cx.notify();
    }

    /// サイズと位置パネルを閉じる.
    pub(crate) fn close_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().panels.frame = None;
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// サイズと位置パネルの設定を適用.
    pub(crate) fn apply_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let shape_id = panel.shape_id;
        let locked = panel.locked;
        let values: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().trim().to_string()).collect();
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };

        let Some(height) = length(&values[0], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-height").into());
            }
            cx.notify();
            return;
        };
        let Some(width) = length(&values[1], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-width").into());
            }
            cx.notify();
            return;
        };
        let Some(rotation) = angle(&values[2]) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-the-rotation-as-a-number").into());
            }
            cx.notify();
            return;
        };
        let Some(x) = length(&values[3], true) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-number-for-the-horizontal-position").into());
            }
            cx.notify();
            return;
        };
        let Some(y) = length(&values[4], true) else {
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
        // What the panel holds is what the shape has now.
        if let Some(shape) = self.slide().shape(shape_id).cloned()
            && let Some(p) = self.wb_mut().panels.frame.as_mut()
        {
            p.initial = vec![cm(shape.frame.height), cm(shape.frame.width), format!("{:.1}", shape.rotation as f64 / 60_000.).trim_end_matches('0').trim_end_matches('.').to_string(), cm(shape.frame.x), cm(shape.frame.y)];
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
        if values.len() < 4
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
    }



    /// サイズと位置パネルのサイドバーUIを生成.
    pub(crate) fn sheet_frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.panels.frame.as_ref()?;
        let (fields, error, locked, placement, print) = (panel.fields.clone(), panel.error.clone(), panel.locked, panel.placement, panel.print);
        let (attrs, has_fill, has_text) = (panel.attrs.clone(), panel.has_fill, panel.has_text);

        let mut body = v_flex().gap_2().px(px(16.)).py(px(12.)).p_0();
        // The shape's own attributes (todo-fix #79): fill, outline, text size, as they are applied when typed.
        let rows = [("xlsx.shape-panel.fill-color", has_fill && attrs.len() > 3, 0), ("xlsx.shape-panel.outline-color", attrs.len() > 3, 1), ("xlsx.shape-panel.outline-width", attrs.len() > 3, 2), ("xlsx.shape-panel.text-size", has_text && attrs.len() > 3, 3)];
        for (label, show, i) in rows {
            if show {
                body = body.child(crate::shell::chrome::control_row(crate::shell::i18n::text(label),attr_field(&attrs[i], i, cx)));
            }
        }
        for (i, (label, field)) in [
            crate::shell::i18n::text("xlsx.shape-panel.height"),
            crate::shell::i18n::text("xlsx.shape-panel.width"),
            crate::shell::i18n::text("xlsx.shape-panel.rotation"),
            crate::shell::i18n::text("xlsx.shape-panel.horizontal"),
            crate::shell::i18n::text("xlsx.shape-panel.vertical"),
        ]
        .into_iter()
        .zip(fields)
        .enumerate()
        {
            body = body.child(crate::shell::chrome::text_field_row(label,
                crate::shell::chrome::text_field(&field)
                    .unit(if i==2 {"°"}else{"cm"},px(24.))
                    .spinner(format!("xlsx-frame-{i}"), FRAME_SPINNERS[i], cx, |this, _, window, cx| this.apply_sheet_frame_panel(window, cx)),
            ));
        }

        body = body.child(
            crate::shell::chrome::checkbox_button("xlsx-frame-lock", crate::shell::i18n::text("pptx.frame-panel.lock-aspect-ratio"), locked, false, cx.listener(|this, _, window, cx| {
                    if let Some(p) = this.wb_mut().panels.frame.as_mut() {
                        p.locked = !p.locked;
                    }
                    this.apply_sheet_frame_panel(window, cx);
                })),
        );

        body = body.child(div().pt(px(8.)).text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.properties")));
        body = body.child(div().text_sm().text_color(rgb(0x666666)).child(crate::shell::i18n::text("_xlsx.object-positioning")));
        for (p, label, btn_id) in [
            (ibis_doc_model::ObjectPlacement::MoveAndSize, "xlsx.placement.move-and-size", "xlsx-placement-move-and-size"),
            (ibis_doc_model::ObjectPlacement::MoveOnly, "xlsx.placement.move-only", "xlsx-placement-move-only"),
            (ibis_doc_model::ObjectPlacement::Free, "xlsx.placement.free", "xlsx-placement-free"),
        ] {
            let is_selected = placement == p;
            body = body.child(
                crate::shell::chrome::radio_button(btn_id, is_selected, false, crate::shell::i18n::text(label), cx.listener(move |this, _, window, cx| {
                        if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                            panel.placement = p;
                        }
                        this.apply_sheet_frame_panel(window, cx);
                    })),
            );
        }

        body = body.child(
            crate::shell::chrome::checkbox_button("xlsx-frame-print", crate::shell::i18n::text("_xlsx.print-object"), print, false, cx.listener(|this, _, window, cx| {
                    if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                        panel.print = !panel.print;
                    }
                    this.apply_sheet_frame_panel(window, cx);
                })),
        );

        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }

        Some(SidebarPanel {
            title: crate::shell::i18n::text("xlsx.shape-panel.title"),
            body: body.into_any_element(),
        })
    }
}

/// The spinners of the frame fields: height and width (cm), rotation (degrees), horizontal and vertical position (cm).
const FRAME_SPINNERS: [crate::shell::chrome::SpinnerSpec; 5] = {
    use crate::shell::chrome::SpinnerSpec;
    [SpinnerSpec::new(0.1).min(0.0), SpinnerSpec::new(0.1).min(0.0), SpinnerSpec::new(1.0).range(-360.0, 360.0), SpinnerSpec::new(0.1), SpinnerSpec::new(0.1)]
};

/// Shape colors use the application color field; numeric attributes share the unit spinner.
fn attr_field(input:&Entity<InputState>,i:usize,cx:&mut Context<SlideEditor>)->AnyElement{
    use crate::shell::chrome::{SpinnerSpec,text_field};
    if i<2 {return crate::shell::chrome::panel_color_field(if i==0{"xlsx-frame-fill"}else{"xlsx-frame-outline"},input,cx);}
    let field=text_field(input).unit("pt",px(24.));
    let apply=|this:&mut SlideEditor,_:f32,w:&mut Window,cx:&mut Context<SlideEditor>|this.apply_sheet_frame_panel(w,cx);
    if i==2{field.spinner("xlsx-frame-outline-width",SpinnerSpec::new(0.25).range(0.,1584.),cx,apply).into_any_element()}
    else{field.spinner("xlsx-frame-text-size",SpinnerSpec::new(1.).range(1.,409.),cx,apply).into_any_element()}
}
