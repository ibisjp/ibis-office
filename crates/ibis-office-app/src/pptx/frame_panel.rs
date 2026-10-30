// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置: the selected shapes' height, width, rotation, scale and place
//! typed as numbers (cm, % and degrees), as PowerPoint's 図形の書式設定 >
//! サイズとプロパティ. The place is the frame's top-left corner from the
//! slide's, before rotation, as PowerPoint shows it.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EmuRect, ShapeId, SlideId};

use crate::pptx::SlidePopup;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The fields, in the panel's internal order:
/// 0: height (cm)
/// 1: width (cm)
/// 2: rotation (°)
/// 3: horizontal place (cm)
/// 4: vertical place (cm)
/// 5: scale height (%)
/// 6: scale width (%)
pub(crate) struct FramePanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, ShapeId)>,
    base_sizes: Vec<(i64, i64)>,
    error: Option<SharedString>,
    /// 縦横比を固定 (all the targets lock their proportions).
    locked: bool,
    /// What the fields said when the panel opened (to tell what was typed).
    initial: Vec<String>,
}

impl FramePanel {
    #[allow(dead_code)]
    pub(crate) fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }
}

/// The largest size and place PowerPoint takes, 1584 cm, in EMU.
const LIMIT: f64 = 570_240_000.;

/// `value` cm in EMU, if a number within ±1584 cm (`signed`) or 0 to 1584 cm.
fn length(value: &str, signed: bool) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64)
}

/// `value` degrees in 60 000ths, turned into 0..360°.
fn angle(value: &str) -> Option<i32> {
    let degrees: f64 = value.trim().parse().ok()?;
    if !degrees.is_finite() || degrees.abs() > 1e6 {
        return None;
    }
    Some(((degrees * 60_000.).round() as i64).rem_euclid(21_600_000) as i32)
}

/// `value` percentage in positive number (e.g. "100", "150%").
fn scale(value: &str) -> Option<f64> {
    let s: f64 = value.trim().trim_end_matches('%').trim().parse().ok()?;
    (s.is_finite() && s > 0.0 && s <= 10_000.0).then_some(s)
}

pub(super) fn cm(emu: i64) -> String {
    let v = format!("{:.2}", emu as f64 / 360_000.);
    v.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl SlideEditor {
    pub(crate) fn open_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let shapes: Vec<_> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, s.rotation)).collect();
        let locked = !shapes.is_empty() && shapes.iter().all(|(id, ..)| self.slide().aspect_locked.contains(id));
        let Some(&(_, frame, rotation)) = shapes.first() else { return };
        let degrees = format!("{:.1}", rotation as f64 / 60_000.);
        let values = [
            cm(frame.height),
            cm(frame.width),
            degrees.trim_end_matches('0').trim_end_matches('.').to_string(),
            cm(frame.x),
            cm(frame.y),
            "100".to_string(),
            "100".to_string(),
        ];
        let initial = values.to_vec();
        let fields: Vec<_> = values.into_iter().map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v))).collect();
        let first = fields[0].clone();
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel = None; s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.insets_panel = None; s.crop_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;
            s.frame_panel = Some(FramePanel {
                fields,
                targets: shapes.iter().map(|(id, _, _)| (slide, *id)).collect(),
                base_sizes: shapes.iter().map(|(_, f, _)| (f.width, f.height)).collect(),
                error: None,
                locked,
                initial,
            });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.frame_panel.as_ref()) else { return };
        let text: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().to_string()).collect();
        let h_cm = length(&text[0], false);
        let w_cm = length(&text[1], false);
        let rot = angle(&text[2]);
        let x_cm = length(&text[3], true);
        let y_cm = length(&text[4], true);
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
            if let Some(p) = self.pptx_mut().and_then(|s| s.frame_panel.as_mut()) {
                p.error = Some(crate::shell::i18n::text("pptx.frame-panel.height-width-must-0-1584"));
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
        }
        if typed_sw && !typed_w {
            width = ((first_base.0 as f64) * sw_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
        }

        // 縦横比固定の処理
        if locked && first_base.0 > 0 && first_base.1 > 0 {
            if (typed_sh || (typed_h && !typed_w)) && !typed_sw {
                if typed_sh {
                    width = ((first_base.0 as f64) * sh_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
                } else {
                    width = (height as f64 * first_base.0 as f64 / first_base.1 as f64).round().clamp(1.0, LIMIT) as i64;
                }
            } else if (typed_sw || (typed_w && !typed_h)) && !typed_sh {
                if typed_sw {
                    height = ((first_base.1 as f64) * sw_pct / 100.0).round().clamp(1.0, LIMIT) as i64;
                } else {
                    height = (width as f64 * first_base.1 as f64 / first_base.0 as f64).round().clamp(1.0, LIMIT) as i64;
                }
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
                (width, height)
            };
            let frame = EmuRect::new(x, y, target_w, target_h);

            if self.presentation().slide(slide).is_some_and(|s| s.aspect_locked.contains(&shape) != locked) {
                commands.push(Command::SetAspectLocked { slide, shape, locked });
            }
            if target.frame != frame {
                commands.push(Command::SetFrame { slide, shape, frame });
            }
            if target.rotation != rotation {
                commands.push(Command::SetTransform { slide, shape, rotation, flip_h: target.flip_h, flip_v: target.flip_v });
            }
        }
        self.run(commands, cx);
        self.close_frame_panel(window, cx);
    }

    pub(crate) fn close_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.frame_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_frame_panel_lock(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.frame_panel.as_mut()) {
            p.locked = !p.locked;
            cx.notify();
        }
    }

    pub(crate) fn set_frame_panel_scale(&mut self, pct: f64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.frame_panel.as_ref()) else { return };
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
        let Some(f) = self.pptx().and_then(|s| s.frame_panel.as_ref()).and_then(|p| p.fields.get(2).cloned()) else { return };
        let v = format!("{degrees:.1}");
        let val = v.trim_end_matches('0').trim_end_matches('.').to_string();
        f.update(cx, |s, cx| s.set_value(val, window, cx));
        cx.notify();
    }

    pub(crate) fn frame_panel_context_menu_entries(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::pptx::editing::Align;
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(p) = self.pptx().and_then(|s| s.frame_panel.as_ref()) else {
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
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.frame_panel.as_ref()?;
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
                use crate::shell::chrome::SpinnerSpec;
                let spec = match idx {
                    0 | 1 => SpinnerSpec::new(0.1).min(0.0),
                    5 | 6 => SpinnerSpec::new(1.0).min(1.0),
                    2 => SpinnerSpec::new(1.0).range(-360.0, 360.0),
                    _ => SpinnerSpec::new(0.1),
                };
                let unit = match idx { 2 => "°", 5 | 6 => "%", _ => "cm" };
                body = body.child(crate::shell::chrome::text_field_row(crate::shell::i18n::text(label), crate::shell::chrome::text_field(field).unit(unit, px(24.)).spinner(format!("pptx-frame-field-{idx}"), spec, cx, |_, _, _, _| {})));
            }
        }
        body = body.child(
            crate::shell::chrome::checkbox_button("pptx-frame-lock", crate::shell::i18n::text("pptx.frame-panel.lock-aspect-ratio"), locked, false, cx.listener(|e, _, _, cx| {
                    e.toggle_frame_panel_lock(cx);
                })),
        );
        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("pptx-frame-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_frame_panel(w, cx))))
                .action(crate::shell::chrome::ActionDefinition::new("pptx-frame-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_frame_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.frame-panel.size-position"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{angle, cm, length, scale};

    #[test]
    fn lengths_and_angles_are_read_as_powerpoint_takes_them() {
        assert_eq!(length("2.54", false), Some(914_400));
        assert_eq!(length("-1", false), None);
        assert_eq!(length("-1", true), Some(-360_000));
        assert_eq!(length("1585", true), None);
        assert_eq!(length("abc", true), None);
        assert_eq!(angle("-90"), Some(16_200_000));
        assert_eq!(angle("450"), Some(5_400_000));
        assert_eq!(angle("NaN"), None);
        assert_eq!(cm(914_400), "2.54");
        assert_eq!(cm(360_000), "1");
        assert_eq!(scale("100"), Some(100.0));
        assert_eq!(scale("150%"), Some(150.0));
        assert_eq!(scale("0"), None);
        assert_eq!(scale("-50"), None);
        assert_eq!(scale("20000"), None);
    }
}
