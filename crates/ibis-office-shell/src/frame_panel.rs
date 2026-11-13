// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置: the selected shapes' height, width, rotation, scale and place
//! typed as numbers (cm, % and degrees), as PowerPoint's 図形の書式設定 >
//! サイズとプロパティ. The place is the frame's top-left corner from the
//! slide's, before rotation, as PowerPoint shows it.
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Command, EmuRect, ObjectWrap, ObjectWrapKind, ShapeId, ShapeKind, SlideId};

use crate::shape_state::ContextPopup;
use crate::chrome::{Shown, ShownNumber, SidebarPanel, SpinnerSpec, Typed};
use crate::editor::SlideEditor;
use crate::format::with_format;

/// The fields, in the panel's internal order:
/// 0: height (cm)
/// 1: width (cm)
/// 2: rotation (°)
/// 3: horizontal place (cm)
/// 4: vertical place (cm)
/// 5: scale height (%)
/// 6: scale width (%)
pub struct FramePanel {
    pub fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, ShapeId)>,
    base_sizes: Vec<(i64, i64)>,
    error: Option<SharedString>,
    /// 縦横比を固定 (all the targets lock their proportions).
    locked: bool,
    /// What the fields said when the panel opened (to tell what was typed).
    initial: Vec<String>,
    /// Fields 0 to 4 (height, width, rotation, horizontal and vertical place) showed "mixed": the selected shapes differ in
    /// them, the field was empty, and while it is still empty nothing is written for it (each shape keeps its own value).
    mixed: [bool; 5],
    /// Fields 0 to 4 as the model has them (EMU, 60 000ths of a degree): an Apply without an edit writes these exact values,
    /// not the digits the fields show (a width of 1 234 567 EMU is shown 3.43 cm).
    numbers: [ShownNumber; 5],
    /// テキストの回り込み: the section for pictures and videos among the selected shapes (`None`: there are none).
    pub wrap: Option<WrapSection>,
}

/// The section テキストの回り込み of the panel: how the text wraps (as chosen by its segments) and the four distances (mm: left, top,
/// right, bottom; PowerPoint's 文字列の折り返し distances), shown as the pictures have them (blank where they differ).
pub struct WrapSection {
    pub(crate) kind: ObjectWrapKind,
    targets: Vec<ShapeId>,
    pub fields: Vec<Entity<InputState>>,
    numbers: [ShownNumber; 4],
}

/// The distances: 0 to 50 mm in steps of 0.1 mm (a millimetre is 36 000 EMU).
fn wrap_spec() -> SpinnerSpec { SpinnerSpec::new(0.1).range(0.0, 50.0).decimals(1) }
const WRAP_LIMIT: f64 = 50. * 36_000.;

impl FramePanel {
    #[allow(dead_code)]
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }
}

/// The largest size and place PowerPoint takes, 1584 cm, in EMU.
const LIMIT: f64 = 570_240_000.;

/// The digits a length (cm) and an angle (degrees) are shown with.
fn cm_spec() -> SpinnerSpec { SpinnerSpec::new(0.1).decimals(2) }
fn degrees_spec() -> SpinnerSpec { SpinnerSpec::new(1.0).decimals(1) }

/// What a length field (cm) means in EMU, if within ±1584 cm (`signed`) or 0 to 1584 cm; `Untouched` for a mixed field left empty.
fn length(number: &ShownNumber, value: &str, signed: bool) -> Typed<i64> {
    number.resolve(value).and_then(|emu| (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64))
}

/// What the rotation field means in 60 000ths, turned into 0..360°.
fn angle(number: &ShownNumber, value: &str) -> Typed<i32> {
    number.resolve(value).and_then(|units| (units.abs() <= 1e6 * 60_000.).then(|| (units as i64).rem_euclid(21_600_000) as i32))
}

/// `value` percentage in positive number (e.g. "100", "150%").
fn scale(value: &str) -> Option<f64> {
    let s: f64 = value.trim().trim_end_matches('%').trim().parse().ok()?;
    (s.is_finite() && s > 0.0 && s <= 10_000.0).then_some(s)
}

/// EMU as centimetres with no trailing zeros (the tests compare the panel's fields with it).
#[cfg(test)]
pub(super) fn cm(emu: i64) -> String {
    let v = format!("{:.2}", emu as f64 / 360_000.);
    v.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl SlideEditor {
    pub fn open_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let shapes: Vec<_> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, s.rotation)).collect();
        let locked = !shapes.is_empty() && shapes.iter().all(|(id, ..)| self.slide().aspect_locked.contains(id));
        if shapes.is_empty() { return; }
        // The common value of the selected shapes, or an empty field when they differ.
        let numbers = [
            ShownNumber::new(Shown::of(&shapes, |s| s.1.height), 360_000., cm_spec()),
            ShownNumber::new(Shown::of(&shapes, |s| s.1.width), 360_000., cm_spec()),
            ShownNumber::new(Shown::of(&shapes, |s| s.2 as i64), 60_000., degrees_spec()),
            ShownNumber::new(Shown::of(&shapes, |s| s.1.x), 360_000., cm_spec()),
            ShownNumber::new(Shown::of(&shapes, |s| s.1.y), 360_000., cm_spec()),
        ];
        let mixed = numbers.each_ref().map(|n| n.text().is_empty());
        let mut fields: Vec<_> = numbers.iter().map(|n| n.input(window, cx)).collect();
        let scale_fields: Vec<_> = (0..2).map(|_| cx.new(|cx| InputState::new(window, cx).default_value("100".to_string()))).collect();
        fields.extend(scale_fields);
        let initial: Vec<String> = numbers.iter().map(|n| n.text().to_string()).chain(["100".to_string(), "100".to_string()]).collect();
        let first = fields[0].clone();
        let pictures: Vec<(ShapeId, ObjectWrap)> = self.selected_shapes().iter().filter(|s| matches!(s.kind, ShapeKind::Picture(_))).map(|s| (s.id, self.slide().ibis_wrap.get(&s.id).copied().unwrap_or(ObjectWrap::none()))).collect();
        let wrap = (!pictures.is_empty()).then(|| {
            let numbers = [0, 1, 2, 3].map(|i| ShownNumber::new(Shown::of(&pictures, |p| p.1.margins[i]), 36_000., wrap_spec()));
            let fields = numbers.iter().map(|n| n.input(window, cx)).collect();
            WrapSection { kind: pictures[0].1.kind, targets: pictures.iter().map(|p| p.0).collect(), fields, numbers }
        });
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None; self.shape_edit.insets_panel = None;
        self.shape_edit.find = None; self.shape_edit.crop_panel = None; self.shape_edit.header_footer = None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Frame));
            self.shape_edit.chart_panel = None;
            self.shape_edit.frame_panel = Some(FramePanel {
                fields,
                targets: shapes.iter().map(|(id, _, _)| (slide, *id)).collect(),
                base_sizes: shapes.iter().map(|(_, f, _)| (f.width, f.height)).collect(),
                error: None,
                locked,
                initial,
                mixed,
                numbers,
                wrap,
            });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub fn apply_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shape_edit.frame_panel.as_ref() else { return };
        let text: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().to_string()).collect();
        // A field that showed "mixed" and is still empty is not written: each shape keeps its own value.
        let untouched = |i: usize| panel.mixed[i] && text[i].trim().is_empty();
        let mut h_set = !untouched(0);
        let mut w_set = !untouched(1);
        let rot_set = !untouched(2);
        let x_set = !untouched(3);
        let y_set = !untouched(4);
        let value_of = |typed: Typed<i64>| match typed { Typed::Value(v) => Some(v), Typed::Untouched => Some(0), Typed::Invalid => None };
        let h_cm = value_of(length(&panel.numbers[0], &text[0], false));
        let w_cm = value_of(length(&panel.numbers[1], &text[1], false));
        let rot = match angle(&panel.numbers[2], &text[2]) { Typed::Value(v) => Some(v), Typed::Untouched => Some(0), Typed::Invalid => None };
        let x_cm = value_of(length(&panel.numbers[3], &text[3], true));
        let y_cm = value_of(length(&panel.numbers[4], &text[4], true));
        let scale_h = match text.get(5) {
            Some(s) => scale(s),
            None => Some(100.0),
        };
        let scale_w = match text.get(6) {
            Some(s) => scale(s),
            None => Some(100.0),
        };

        let (Some(mut height), Some(mut width), Some(rotation), Some(x), Some(y), Some(sh_pct), Some(sw_pct)) =
            (h_cm, w_cm, rot, x_cm, y_cm, scale_h, scale_w)
        else {
            if let Some(p) = self.shape_edit.frame_panel.as_mut() {
                p.error = Some(crate::i18n::text("pptx.frame-panel.height-width-must-0-1584"));
            }
            return cx.notify();
        };

        let mut commands = Vec::new();
        let locked = panel.locked;
        let typed_h = text[0] != panel.initial[0];
        let typed_w = text[1] != panel.initial[1];
        let typed_sh = text.get(5).is_some_and(|s| s != &panel.initial[5]);
        let typed_sw = text.get(6).is_some_and(|s| s != &panel.initial[6]);

        let first_base = panel.base_sizes.first().copied().unwrap_or((width, height));

        // 倍率（%）が入力された場合、優先して基準サイズに対するスケーリングを適用
        if typed_sh && !typed_h {
            height = ((first_base.1 as f64) * sh_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
            h_set = true;
        }
        if typed_sw && !typed_w {
            width = ((first_base.0 as f64) * sw_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
            w_set = true;
        }

        // 縦横比固定の処理
        if locked && first_base.0 > 0 && first_base.1 > 0 {
            if (typed_sh || (typed_h && !typed_w)) && !typed_sw {
                if typed_sh {
                    width = ((first_base.0 as f64) * sh_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
                } else {
                    width = (height as f64 * first_base.0 as f64 / first_base.1 as f64).round().clamp(1.0, LIMIT) as i64;
                }
                w_set = true;
            } else if (typed_sw || (typed_w && !typed_h)) && !typed_sh {
                if typed_sw {
                    height = ((first_base.1 as f64) * sw_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
                } else {
                    height = (width as f64 * first_base.1 as f64 / first_base.0 as f64).round().clamp(1.0, LIMIT) as i64;
                }
                h_set = true;
            }
        }

        for (i, (slide, shape)) in panel.targets.iter().copied().enumerate() {
            let Some(target) = self.presentation().slide(slide).and_then(|s| s.shape(shape)) else { continue };
            let (target_w, target_h) = if panel.targets.len() > 1 && (typed_sh || typed_sw) {
                let base = panel.base_sizes.get(i).copied().unwrap_or((target.frame.width, target.frame.height));
                let effective_sh = if typed_sh { sh_pct } else if locked && typed_sw { sw_pct } else { 100.0 };
                let effective_sw = if typed_sw { sw_pct } else if locked && typed_sh { sh_pct } else { 100.0 };
                let w = ((base.0 as f64) * effective_sw / 100.0).round().clamp(1.0, LIMIT) as i64;
                let h = ((base.1 as f64) * effective_sh / 100.0).round().clamp(1.0, LIMIT) as i64;
                (w, h)
            } else {
                (if w_set { width } else { target.frame.width }, if h_set { height } else { target.frame.height })
            };
            let frame = EmuRect::new(if x_set { x } else { target.frame.x }, if y_set { y } else { target.frame.y }, target_w, target_h);

            if self.presentation().slide(slide).is_some_and(|s| s.aspect_locked.contains(&shape) != locked) {
                commands.push(Command::SetAspectLocked { slide, shape, locked });
            }
            if target.frame != frame {
                commands.push(Command::SetFrame { slide, shape, frame });
            }
            let rotation = if rot_set { rotation } else { target.rotation };
            if target.rotation != rotation {
                commands.push(Command::SetTransform { slide, shape, rotation, flip_h: target.flip_h, flip_v: target.flip_v });
            }
        }
        // テキストの回り込み: the kind and the four distances of the pictures (asking first in a file that has not turned the features on).
        let mut wrap_commands: Vec<Command> = Vec::new();
        if let Some(section) = &panel.wrap {
            let typed: Vec<Typed<i64>> = (0..4).map(|i| section.numbers[i].resolve(section.fields[i].read(cx).value().as_ref()).and_then(|emu| (emu.is_finite() && (0. ..=WRAP_LIMIT).contains(&emu)).then_some(emu as i64))).collect();
            if typed.iter().any(Typed::is_invalid) {
                if let Some(p) = self.shape_edit.frame_panel.as_mut() {
                    p.error = Some(crate::i18n::text("pptx.frame-panel.wrap-distance-range"));
                }
                return cx.notify();
            }
            for &shape in &section.targets {
                let now = self.slide().ibis_wrap.get(&shape).copied().unwrap_or(ObjectWrap::none());
                let mut margins = now.margins;
                for (i, t) in typed.iter().enumerate() {
                    if let Typed::Value(v) = t {
                        margins[i] = *v;
                    }
                }
                let next = ObjectWrap { kind: section.kind, margins };
                // (With no wrap the distances are not kept.)
                if next != now && !(next.kind == ObjectWrapKind::None && now.kind == ObjectWrapKind::None) {
                    wrap_commands.push(Command::SetObjectWrap { slide: self.current_slide, shape, wrap: next });
                }
            }
        }
        self.run(commands, cx);
        if !wrap_commands.is_empty() {
            self.require_ibis_extensions(move |this, cx| this.run(wrap_commands.clone(), cx), cx);
        }
        self.close_frame_panel(window, cx);
    }

    /// The panel's テキストの回り込み segments: how the text wraps (applied with 適用).
    pub fn set_frame_panel_wrap_kind(&mut self, kind: ObjectWrapKind, cx: &mut Context<Self>) {
        if let Some(section) = self.shape_edit.frame_panel.as_mut().and_then(|p| p.wrap.as_mut()) {
            section.kind = kind;
            cx.notify();
        }
    }

    pub fn close_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uses_shape_edit() {
            self.shape_edit.frame_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_frame_panel_lock(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.shape_edit.frame_panel.as_mut() {
            p.locked = !p.locked;
            cx.notify();
        }
    }

    pub(crate) fn set_frame_panel_scale(&mut self, pct: f64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.shape_edit.frame_panel.as_ref() else { return };
        let sh = p.fields.get(5).cloned();
        let sw = p.fields.get(6).cloned();
        let val = format!("{}", pct.round() as i64);
        if let Some(f) = sh {
            f.update(cx, |s, cx| s.set_value(val.clone(), window, cx));
        }
        if let Some(f) = sw {
            f.update(cx, |s, cx| s.set_value(val, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn set_frame_panel_rotation(&mut self, degrees: f64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.shape_edit.frame_panel.as_ref().and_then(|p| p.fields.get(2).cloned()) else { return };
        let v = format!("{degrees:.1}");
        let val = v.trim_end_matches('0').trim_end_matches('.').to_string();
        f.update(cx, |s, cx| s.set_value(val, window, cx));
        cx.notify();
    }

    pub fn frame_panel_context_menu_entries(&self) -> Vec<crate::chrome::MenuEntry> {
        use crate::shape_items::Align;
        use crate::chrome::{MenuEntry, MenuItem};
        let Some(p) = self.shape_edit.frame_panel.as_ref() else {
            return Vec::new();
        };
        let locked = p.locked;
        let has = !self.selected_ids().is_empty();
        vec![
            MenuItem::tr("_shell.apply")
                .icon("check")
                .on(|this, window, cx| this.apply_frame_panel(window, cx))
                .into(),
            MenuItem::tr("pptx.frame-panel.lock-aspect-ratio")
                .checked(locked)
                .on(|this, _, cx| this.toggle_frame_panel_lock(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("pptx.frame.scale.50", "50%")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_scale(50.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.scale.100", "100%")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_scale(100.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.scale.150", "150%")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_scale(150.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.scale.200", "200%")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_scale(200.0, window, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("pptx.frame.rot.0", "0°")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_rotation(0.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.rot.90", "90°")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_rotation(90.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.rot.180", "180°")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_rotation(180.0, window, cx))
                .into(),
            MenuItem::new("pptx.frame.rot.270", "270°")
                .disabled(!has)
                .on(|this, window, cx| this.set_frame_panel_rotation(270.0, window, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.rotate.right")
                .icon("rotate_right")
                .disabled(!has)
                .on(|this, _, cx| this.rotate_selected(90, cx))
                .into(),
            MenuItem::tr("pptx.rotate.left")
                .icon("rotate_left")
                .disabled(!has)
                .on(|this, _, cx| this.rotate_selected(-90, cx))
                .into(),
            MenuItem::tr("pptx.rotate.flip-h")
                .icon("flip")
                .disabled(!has)
                .on(|this, _, cx| this.flip_selected(true, cx))
                .into(),
            MenuItem::tr("pptx.rotate.flip-v")
                .icon("flip")
                .disabled(!has)
                .on(|this, _, cx| this.flip_selected(false, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.align.center")
                .icon("align_horizontal_center")
                .disabled(!has)
                .on(|this, _, cx| this.align_selected(Align::Center, cx))
                .into(),
            MenuItem::tr("pptx.align.middle")
                .icon("align_vertical_center")
                .disabled(!has)
                .on(|this, _, cx| this.align_selected(Align::Middle, cx))
                .into(),
            MenuItem::tr("pptx.center.h")
                .icon("align_horizontal_center")
                .disabled(!has)
                .on(|this, _, cx| this.center_selected(true, cx))
                .into(),
            MenuItem::tr("pptx.center.v")
                .icon("align_vertical_center")
                .disabled(!has)
                .on(|this, _, cx| this.center_selected(false, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, window, cx| this.close_frame_panel(window, cx))
                .into(),
        ]
    }

    pub(crate) fn open_frame_panel_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let items = self.frame_panel_context_menu_entries();
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.shape_edit.popup = None;
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.shape_edit.popup = Some(ContextPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    pub fn frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.shape_edit.frame_panel.as_ref()?;
        let (fields, error, locked) = (p.fields.clone(), p.error.clone(), p.locked);
        let mut body = v_flex()
            .id("pptx-frame-panel")
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_frame_panel_context_menu(event.position, window, cx);
                }),
            );
        let labels = [
            ("pptx.frame-panel.height-cm", 0),
            ("pptx.frame-panel.width-cm", 1),
            ("pptx.frame-panel.scale-height-percent", 5),
            ("pptx.frame-panel.scale-width-percent", 6),
            ("pptx.frame-panel.rotation-degrees", 2),
            ("pptx.frame-panel.horizontal-position-cm-top-left", 3),
            ("pptx.frame-panel.vertical-position-cm-top-left", 4),
        ];
        for (label, idx) in labels {
            if let Some(field) = fields.get(idx) {
                // Size and position (cm), scale (%), rotation (degrees); applied with 適用, so ▲▼ only change the field.
                use crate::chrome::SpinnerSpec;
                let spec = match idx {
                    0 | 1 => SpinnerSpec::new(0.1).min(0.0),
                    5 | 6 => SpinnerSpec::new(1.0).min(1.0),
                    2 => SpinnerSpec::new(1.0).range(-360.0, 360.0),
                    _ => SpinnerSpec::new(0.1),
                };
                let unit = match idx { 2 => "°", 5 | 6 => "%", _ => "cm" };
                body = body.child(crate::chrome::text_field_row(crate::i18n::text(label), crate::chrome::text_field(field).unit(unit, px(24.)).spinner(format!("pptx-frame-field-{idx}"), spec, cx, |_, _, _, _| {})));
            }
        }
        if let Some(section) = self.shape_edit.frame_panel.as_ref().and_then(|p| p.wrap.as_ref()) {
            let (kind, wrap_fields) = (section.kind, section.fields.clone());
            body = body.child(div().pt_2().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::i18n::text("pptx.frame-panel.wrap-section")));
            body = body.child(crate::chrome::segment_control_row(
                crate::i18n::text("pptx.wrap.menu"),
                [("pptx-frame-wrap-none", "pptx.wrap.none", ObjectWrapKind::None), ("pptx-frame-wrap-square", "pptx.wrap.square", ObjectWrapKind::Square), ("pptx-frame-wrap-top-bottom", "pptx.wrap.top-bottom", ObjectWrapKind::TopBottom)].map(|(id, label, k)| {
                    crate::chrome::segment_control::SegmentItem::label(id, crate::i18n::text(label), kind == k).on_click(cx.listener(move |e, _, _, cx| e.set_frame_panel_wrap_kind(k, cx)))
                }),
            ));
            for (i, (label, field)) in ["pptx.frame-panel.wrap-left-mm", "pptx.frame-panel.wrap-top-mm", "pptx.frame-panel.wrap-right-mm", "pptx.frame-panel.wrap-bottom-mm"].into_iter().zip(wrap_fields).enumerate() {
                body = body.child(crate::chrome::text_field_row(crate::i18n::text(label), crate::chrome::text_field(&field).unit("mm", px(24.)).spinner(format!("pptx-frame-wrap-field-{i}"), wrap_spec(), cx, |_, _, _, _| {})));
            }
        }
        body = body.child(
            crate::chrome::checkbox_button("pptx-frame-lock", crate::i18n::text("pptx.frame-panel.lock-aspect-ratio"), locked, false, cx.listener(|e, _, _, cx| {
                    e.toggle_frame_panel_lock(cx);
                })),
        );
        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::chrome::action_bar()
                .action(crate::chrome::ActionDefinition::new("pptx-frame-apply", crate::i18n::text("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_frame_panel(w, cx))))
                .action(crate::chrome::ActionDefinition::new("pptx-frame-cancel", crate::i18n::text("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_frame_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::i18n::text("pptx.frame-panel.size-position"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{angle, cm, length, scale};

    #[test]
    fn lengths_and_angles_are_read_as_powerpoint_takes_them() {
        use crate::chrome::{ShownNumber, Typed};
        use super::{cm_spec, degrees_spec};
        let (len, deg) = (ShownNumber::one(5, 360_000., cm_spec()), ShownNumber::one(5, 60_000., degrees_spec()));
        let ok = |v: i64| Typed::Value(v);
        assert_eq!(length(&len, "2.54", false), ok(914_400));
        assert_eq!(length(&len, "-1", false), Typed::Invalid);
        assert_eq!(length(&len, "-1", true), ok(-360_000));
        assert_eq!(length(&len, "1585", true), Typed::Invalid);
        assert_eq!(length(&len, "abc", true), Typed::Invalid);
        assert_eq!(angle(&deg, "-90"), Typed::Value(16_200_000));
        assert_eq!(angle(&deg, "450"), Typed::Value(5_400_000));
        assert_eq!(angle(&deg, "NaN"), Typed::Invalid);
        assert_eq!(cm(914_400), "2.54");
        assert_eq!(cm(360_000), "1");
        assert_eq!(scale("100"), Some(100.0));
        assert_eq!(scale("150%"), Some(150.0));
        assert_eq!(scale("0"), None);
        assert_eq!(scale("-50"), None);
        assert_eq!(scale("20000"), None);
    }
}
