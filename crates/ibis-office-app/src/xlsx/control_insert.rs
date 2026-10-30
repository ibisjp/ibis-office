// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 開発 > コントロール: フォーム コントロールの挿入 (ボタン, チェック ボックス,
//! オプション ボタン, コンボ ボックス, リスト ボックス, スピン ボタン, スクロール バー,
//! グループ ボックス, ラベル) および コントロールの書式設定/プロパティ.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, ControlKind, EmuRect, FormControl, Shape, ShapeId, ShapeKind, SheetCommand};

use crate::shell::chrome::{MenuEntry, MenuItem, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// Default size in EMU for a form control.
pub(crate) fn default_control(kind: ControlKind, name: &str, text: &str) -> (FormControl, (i64, i64)) {
    let pt = 12_700i64;
    let (w, h) = match kind {
        ControlKind::Button => (96 * pt, 24 * pt),
        ControlKind::CheckBox => (96 * pt, 18 * pt),
        ControlKind::Radio => (96 * pt, 18 * pt),
        ControlKind::Drop => (96 * pt, 20 * pt),
        ControlKind::List => (96 * pt, 72 * pt),
        ControlKind::Scroll => (18 * pt, 96 * pt),
        ControlKind::Spin => (18 * pt, 36 * pt),
        ControlKind::GroupBox => (160 * pt, 100 * pt),
        ControlKind::Label => (96 * pt, 18 * pt),
    };
    let mut c = FormControl::new(kind, name);
    c.text = text.to_string();
    (c, (w, h))
}

pub(crate) struct ControlPropertiesPanel {
    pub(crate) shape: ShapeId,
    pub(crate) kind: ControlKind,
    pub(crate) text: Entity<InputState>,
    pub(crate) link: Entity<InputState>,
    pub(crate) range: Entity<InputState>,
    pub(crate) min: Entity<InputState>,
    pub(crate) max: Entity<InputState>,
    pub(crate) step: Entity<InputState>,
    pub(crate) page: Entity<InputState>,
    pub(crate) font: Entity<InputState>,
    pub(crate) font_size: Entity<InputState>,
    pub(crate) text_color: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) three_d: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// The currently selected control, or the first control on the sheet.
    pub(crate) fn selected_control(&self) -> Option<(ShapeId, FormControl)> {
        if let Some(id) = self.selection {
            if let Some(ShapeKind::Control(c)) = self.slide().shape(id).map(|s| &s.kind) {
                return Some((id, (**c).clone()));
            }
        }
        self.slide().shapes.iter().find_map(|s| {
            if let ShapeKind::Control(c) = &s.kind {
                Some((s.id, (**c).clone()))
            } else {
                None
            }
        })
    }

    /// フォーム コントロールの新規挿入.
    pub(crate) fn insert_form_control(&mut self, kind: ControlKind, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((sheet_shape, sheet)) = self.current_sheet() else { return };
        let (col, row) = self.current_cell().unwrap_or((1, 1));
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();
        let x = sheet_shape.frame.x + xs.get(col as usize).copied().unwrap_or(0);
        let y = sheet_shape.frame.y + ys.get(row as usize).copied().unwrap_or(0);

        let base_label = match kind {
            ControlKind::Button => "Button",
            ControlKind::CheckBox => "Check Box",
            ControlKind::Radio => "Option Button",
            ControlKind::Drop => "Drop Down",
            ControlKind::List => "List Box",
            ControlKind::Scroll => "Scroll Bar",
            ControlKind::Spin => "Spin Button",
            ControlKind::GroupBox => "Group Box",
            ControlKind::Label => "Label",
        };
        let existing: Vec<String> = self
            .slide()
            .shapes
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Control(c) => Some(c.name.clone()),
                _ => None,
            })
            .collect();
        let name = (1..)
            .map(|n| format!("{base_label} {n}"))
            .find(|n| !existing.iter().any(|e| e.eq_ignore_ascii_case(n)))
            .unwrap_or_else(|| format!("{base_label} 1"));
        let default_text = match kind {
            ControlKind::Button
            | ControlKind::CheckBox
            | ControlKind::Radio
            | ControlKind::GroupBox
            | ControlKind::Label => name.clone(),
            _ => String::new(),
        };

        let (mut control, (w, h)) = default_control(kind, &name, &default_text);
        if kind == ControlKind::Radio {
            control.first_button = existing.iter().all(|n| !n.starts_with("Option Button"));
        }
        let id = self.document.ids().new_shape_id();
        let frame = EmuRect::new(x, y, w, h);
        let shape = Shape::new(id, frame, ShapeKind::Control(Arc::new(control)));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        cx.notify();
    }

    /// コントロールの書式設定 / プロパティ パネルを開く.
    pub(crate) fn open_control_properties_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((shape, control)) = self.selected_control() else {
            self.status = Some(text("xlsx.control-properties.select"));
            return cx.notify();
        };
        let make = |val: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(val, window, cx));
            state
        };
        let text_input = make(control.text.clone(), window, cx);
        let link_input = make(control.link.clone().unwrap_or_default(), window, cx);
        let range_input = make(control.range.clone().unwrap_or_default(), window, cx);
        let min_input = make(control.min.to_string(), window, cx);
        let max_input = make(control.max.to_string(), window, cx);
        let step_input = make(control.step.to_string(), window, cx);
        let page_input = make(control.page.to_string(), window, cx);
        let font_input = make(control.font.clone().unwrap_or_default(), window, cx);
        let font_size_input = make(
            control.font_size.map(|sz| (sz / 100).to_string()).unwrap_or_default(),
            window,
            cx,
        );
        let text_color_input = make(
            control.text_color.map(|col| format!("#{:06X}", col.0)).unwrap_or_default(),
            window,
            cx,
        );

        let panel = ControlPropertiesPanel {
            shape,
            kind: control.kind,
            text: text_input,
            link: link_input,
            range: range_input,
            min: min_input,
            max: max_input,
            step: step_input,
            page: page_input,
            font: font_input,
            font_size: font_size_input,
            text_color: text_color_input,
            bold: control.bold,
            italic: control.italic,
            three_d: control.three_d,
            message: String::new(),
            ui: Default::default(),
        };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.control_properties = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// プロパティ パネルの入力内容を反映.
    pub(crate) fn apply_control_properties_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.control_properties.as_ref()) else { return };
        let shape = p.shape;
        let _kind = p.kind;
        let text_val = p.text.read(cx).value();
        let link_val = p.link.read(cx).value();
        let range_val = p.range.read(cx).value();
        let min_val = p.min.read(cx).value().parse::<i64>().unwrap_or(0);
        let max_val = p.max.read(cx).value().parse::<i64>().unwrap_or(30000);
        let step_val = p.step.read(cx).value().parse::<i64>().unwrap_or(1);
        let page_val = p.page.read(cx).value().parse::<i64>().unwrap_or(10);
        let font_val = p.font.read(cx).value();
        let font_size_val = p.font_size.read(cx).value();
        let text_color_val = p.text_color.read(cx).value();
        let bold_val = p.bold;
        let italic_val = p.italic;
        let three_d_val = p.three_d;

        let Some(existing_sh) = self.slide().shape(shape) else { return };
        let ShapeKind::Control(c) = &existing_sh.kind else { return };
        let mut next = (**c).clone();
        next.text = text_val.to_string();
        next.link = if link_val.trim().is_empty() { None } else { Some(link_val.trim().to_string()) };
        next.range = if range_val.trim().is_empty() { None } else { Some(range_val.trim().to_string()) };
        next.min = min_val;
        next.max = max_val;
        next.step = step_val.max(1);
        next.page = page_val.max(1);
        next.font = if font_val.trim().is_empty() { None } else { Some(font_val.trim().to_string()) };
        next.font_size = font_size_val.trim().parse::<f64>().ok().filter(|&pt| pt > 0.0).map(|pt| (pt * 100.0).round() as i32);
        next.text_color = {
            let hex = text_color_val.trim().trim_start_matches('#');
            if hex.len() == 6 {
                u32::from_str_radix(hex, 16).ok().map(ibis_doc_model::Rgb)
            } else {
                None
            }
        };
        next.bold = bold_val;
        next.italic = italic_val;
        next.three_d = three_d_val;

        let slide = self.current_slide;
        let edit = SheetCommand::EditControl { slide, shape, control: next };
        self.run_sheet(vec![Command::Sheet(edit)], cx);
        self.wb_mut().panels.control_properties = None;
        cx.notify();
    }

    pub(crate) fn control_properties_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.control_properties.as_ref()?;
        let kind = p.kind;
        let bold = p.bold;
        let italic = p.italic;
        let three_d = p.three_d;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
        };
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        // 表示テキスト・文字の書式 (Button, CheckBox, Radio, GroupBox, Label)
        if matches!(
            kind,
            ControlKind::Button
                | ControlKind::CheckBox
                | ControlKind::Radio
                | ControlKind::GroupBox
                | ControlKind::Label
        ) {
            body = body
                .child(div().text_sm().child(text("xlsx.control-properties.text")))
                .child(mark("text", crate::shell::chrome::text_field(&p.text).full_width().into_any_element()))
                .child(div().text_sm().child(text("xlsx.control-properties.font")))
                .child(mark("font", crate::shell::chrome::panel_font_dropdown("panel-font-xlsx-control-insert-1",&p.font,cx).into_any_element()))
                .child(div().text_sm().child(text("xlsx.control-properties.font-size")))
                .child(mark("font_size", control_spin(&p.font_size, "control-prop-font-size", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 409.0), cx).into_any_element()))
                .child(div().text_sm().child(text("xlsx.control-properties.text-color")))
                .child(mark("text_color", crate::shell::chrome::panel_color_field("panel-color-xlsx-control-insert-2",&p.text_color,cx).into_any_element()))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(mark(
                            "bold",
                            {
                                let btn = crate::shell::chrome::action_button("control-prop-bold")
                                    .small()
                                    .label(text("xlsx.text.bold"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.control_properties.as_mut() {
                                            p.bold = !p.bold;
                                        }
                                        cx.notify();
                                    }));
                                if bold { btn.primary() } else { btn }.into_any_element()
                            },
                        ))
                        .child(mark(
                            "italic",
                            {
                                let btn = crate::shell::chrome::action_button("control-prop-italic")
                                    .small()
                                    .label(text("xlsx.text.italic"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.control_properties.as_mut() {
                                            p.italic = !p.italic;
                                        }
                                        cx.notify();
                                    }));
                                if italic { btn.primary() } else { btn }.into_any_element()
                            },
                        )),
                );
        }

        // リンクするセル (CheckBox, Radio, Scroll, Spin, Drop, List)
        if matches!(
            kind,
            ControlKind::CheckBox
                | ControlKind::Radio
                | ControlKind::Scroll
                | ControlKind::Spin
                | ControlKind::Drop
                | ControlKind::List
        ) {
            body = body
                .child(div().text_sm().child(text("xlsx.control-properties.link")))
                .child(mark("link", super::range_field::local_range_field(&p.link,"control_insert-p-link",true,cx).into_any_element()));
        }

        // 入力範囲 (Drop, List)
        if matches!(kind, ControlKind::Drop | ControlKind::List) {
            body = body
                .child(div().text_sm().child(text("xlsx.control-properties.range")))
                .child(mark("range", super::range_field::local_range_field(&p.range,"control_insert-p-range",false,cx).into_any_element()));
        }

        // 最小値・最大値・増分・ページ変化量 (Scroll, Spin)
        if matches!(kind, ControlKind::Scroll | ControlKind::Spin) {
            body = body
                .child(div().text_sm().child(text("xlsx.control-properties.min")))
                .child(mark("min", control_spin(&p.min, "control-prop-min", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 30000.0).integer_only(), cx).into_any_element()))
                .child(div().text_sm().child(text("xlsx.control-properties.max")))
                .child(mark("max", control_spin(&p.max, "control-prop-max", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 30000.0).integer_only(), cx).into_any_element()))
                .child(div().text_sm().child(text("xlsx.control-properties.step")))
                .child(mark("step", control_spin(&p.step, "control-prop-step", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 30000.0).integer_only(), cx).into_any_element()));
            if kind == ControlKind::Scroll {
                body = body
                    .child(div().text_sm().child(text("xlsx.control-properties.page")))
                    .child(mark("page", control_spin(&p.page, "control-prop-page", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 30000.0).integer_only(), cx).into_any_element()));
            }
        }

        // 3-D 陰影表示 (CheckBox, Radio, Drop, List, GroupBox)
        if matches!(
            kind,
            ControlKind::CheckBox
                | ControlKind::Radio
                | ControlKind::Drop
                | ControlKind::List
                | ControlKind::GroupBox
        ) {
            body = body.child(mark(
                "three_d",
                {
                    let btn = crate::shell::chrome::action_button("control-prop-three-d")
                        .small()
                        .label(text("xlsx.control-properties.three-d"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.control_properties.as_mut() {
                                p.three_d = !p.three_d;
                            }
                            cx.notify();
                        }));
                    if three_d { btn.primary() } else { btn }.into_any_element()
                },
            ));
        }

        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0xD9534F)).child(SharedString::from(p.message.clone())));
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("control-properties-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_control_properties_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.control-properties.title"), body.into_any_element(), panel_terminal_actions))
    }

    pub(crate) fn is_design_mode_active(&self) -> bool {
        self.sheet_state().map_or(false, |s| s.design_mode)
    }

    pub(crate) fn toggle_design_mode(&mut self, cx: &mut Context<Self>) {
        let active = !self.is_design_mode_active();
        self.wb_mut().design_mode = active;
        let msg = if active {
            text("xlsx.design-mode.enabled")
        } else {
            text("xlsx.design-mode.disabled")
        };
        self.status = Some(msg);
        cx.notify();
    }

    pub(crate) fn view_code(&mut self, cx: &mut Context<Self>) {
        if let Some((_id, ctrl)) = self.selected_control() {
            let msg = format(
                "xlsx.view-code.control-info",
                &[("name", &ctrl.name), ("kind", &format!("{:?}", ctrl.kind))],
            );
            self.status = Some(msg.into());
        } else {
            self.status = Some(text("xlsx.view-code.no-code"));
        }
        cx.notify();
    }
}

/// フォーム コントロール挿入メニューの項目一覧.
pub(crate) fn control_insert_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("xlsx.insert-control.button")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Button, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.checkbox")
            .on(|this, _, cx| this.insert_form_control(ControlKind::CheckBox, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.radio")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Radio, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.dropdown")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Drop, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.list")
            .on(|this, _, cx| this.insert_form_control(ControlKind::List, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.spin")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Spin, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.scroll")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Scroll, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.groupbox")
            .on(|this, _, cx| this.insert_form_control(ControlKind::GroupBox, cx))
            .into(),
        MenuItem::tr("xlsx.insert-control.label")
            .on(|this, _, cx| this.insert_form_control(ControlKind::Label, cx))
            .into(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [
            include_str!("../../i18n/ja/xlsx.tsv"),
            include_str!("../../i18n/en/xlsx.tsv"),
        ] {
            for id in [
                "xlsx.insert-control",
                "xlsx.insert-control.button",
                "xlsx.insert-control.checkbox",
                "xlsx.insert-control.radio",
                "xlsx.insert-control.dropdown",
                "xlsx.insert-control.list",
                "xlsx.insert-control.spin",
                "xlsx.insert-control.scroll",
                "xlsx.insert-control.groupbox",
                "xlsx.insert-control.label",
                "xlsx.control-properties",
                "xlsx.control-properties.title",
                "xlsx.control-properties.select",
                "xlsx.control-properties.text",
                "xlsx.control-properties.link",
                "xlsx.control-properties.range",
                "xlsx.control-properties.min",
                "xlsx.control-properties.max",
                "xlsx.control-properties.step",
                "xlsx.control-properties.page",
                "xlsx.get-data",
                "xlsx.design-mode",
                "xlsx.design-mode.enabled",
                "xlsx.design-mode.disabled",
                "xlsx.view-code",
                "xlsx.view-code.no-code",
                "xlsx.view-code.control-info",
            ] {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(id)), "{id} is missing");
            }
        }
    }
}

/// A number box of the control's properties with ▲▼ (Excel's limits: 0–30000 for the values, 1–30000 for the steps).
/// The panel applies from its OK button, not on Enter, so a step only changes the box.
fn control_spin(input: &Entity<InputState>, id: &'static str, spec: crate::shell::chrome::SpinnerSpec, cx: &Context<SlideEditor>) -> crate::shell::chrome::TextField {
    crate::shell::chrome::text_field(input).full_width().spinner(id, spec, cx, |_, _, _, _| {})
}
