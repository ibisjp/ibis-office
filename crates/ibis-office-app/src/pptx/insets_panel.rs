// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Custom text-box inset inputs, including the selected table cell.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, Insets, ShapeId, ShapeKind, SlideId, TextBody};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

type Target = (SlideId, ShapeId, Option<(usize, usize)>, TextBody);
pub(crate) struct InsetsPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<Target>,
    pub(crate) error: Option<SharedString>,
}

fn inset(value: &str) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (cm.is_finite() && (0. ..=i32::MAX as f64).contains(&emu)).then_some(emu as i64)
}

impl SlideEditor {
    pub(crate) fn open_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let (targets, initial) = if let Some(edit) = self.text_editing() {
            if let Some((r, c)) = edit.cell {
                let (ar, ac) = edit.cell_anchor.unwrap_or((r, c));
                let (r0, r1) = (r.min(ar), r.max(ar));
                let (c0, c1) = (c.min(ac), c.max(ac));
                let mut cell_targets = Vec::new();
                if let Some(ShapeKind::Table(t)) = self.slide().shape(edit.shape).map(|s| &s.kind) {
                    for row in r0..=r1 {
                        for col in c0..=c1 {
                            if let Some(cell) = t.rows.get(row).and_then(|r_obj| r_obj.cells.get(col)) {
                                cell_targets.push((self.current_slide, edit.shape, Some((row, col)), cell.text.clone()));
                            }
                        }
                    }
                }
                let init = self.edited_body().map(|b| b.insets).or_else(|| cell_targets.first().map(|t| t.3.insets));
                (cell_targets, init)
            } else {
                (self.edited_body().map(|b| vec![(self.current_slide, edit.shape, None, b.clone())]).unwrap_or_default(), self.edited_body().map(|b| b.insets))
            }
        } else {
            let shapes = self.selected_shapes();
            let mut all_targets = Vec::new();
            for s in &shapes {
                if let ShapeKind::Table(t) = &s.kind {
                    for (row, r_obj) in t.rows.iter().enumerate() {
                        for (col, cell) in r_obj.cells.iter().enumerate() {
                            all_targets.push((self.current_slide, s.id, Some((row, col)), cell.text.clone()));
                        }
                    }
                } else if let Some(b) = s.text_body() {
                    all_targets.push((self.current_slide, s.id, None, b.clone()));
                }
            }
            let init = all_targets.first().map(|t| t.3.insets);
            (all_targets, init)
        };
        let Some(initial) = initial else { return };
        self.end_text_edit(window, cx);
        let mut fields: Vec<_> = [initial.left, initial.top, initial.right, initial.bottom].into_iter().map(|value| {
            cx.new(|cx| InputState::new(window, cx).default_value(format!("{:.6}", value as f64 / 360_000.)))
        }).collect();
        fields.push(cx.new(|cx| InputState::new(window,cx).placeholder(crate::shell::i18n::text("pptx.common.automatic")).default_value("")));
        let first = fields[0].clone();
        if let Some(s) = self.pptx_mut() {
            s.crop_panel = None; s.timing_panel = None; s.section_panel=None;s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;
            s.insets_panel = Some(InsetsPanel { fields, targets, error: None });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.insets_panel.as_ref()) else { return };
        let values: Option<Vec<_>> = panel.fields.iter().take(4).map(|f| inset(f.read(cx).value().as_ref())).collect();
        let Some(v) = values else {
            if let Some(panel) = self.pptx_mut().and_then(|s| s.insets_panel.as_mut()) { panel.error = Some(crate::shell::i18n::text("pptx.insets-panel.enter-finite-number-0-more").into()); }
            return cx.notify();
        };
        // Check every frozen target before applying to any of them.
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || panel.targets.iter().any(|(slide,shape,cell,snapshot)| {
            let live=self.presentation().slide(*slide).and_then(|s|s.shape(*shape)).and_then(|target|match (cell,&target.kind) {
                (Some((row,column)),ShapeKind::Table(t))=>t.rows.get(*row).and_then(|r|r.cells.get(*column)).map(|c|&c.text),
                (None,_)=>target.text_body(),_=>None,
            });live!=Some(snapshot)
        });
        if conflict {
            if let Some(p)=self.pptx_mut().and_then(|s|s.insets_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.paragraph-panel.target-text-has-changed-confirm").into());}
            return cx.notify();
        }
        let value=panel.fields[4].read(cx).value();let value=value.trim();
        let scale=if value.is_empty() {None} else {
            let scale=value.parse::<f64>().ok().filter(|v|v.is_finite() && *v>=1. && *v<=100.).map(|v|(v*1000.).round() as u32).filter(|v|*v>0);
            let Some(scale)=scale else {if let Some(p)=self.pptx_mut().and_then(|s|s.insets_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.insets-panel.enter-text-scale-1-100").into());}return cx.notify();};Some(scale)
        };
        let insets = Insets { left: v[0], top: v[1], right: v[2], bottom: v[3] };
        let commands = panel.targets.clone().into_iter().filter_map(|(slide, shape, cell, _)| {
            let target = self.presentation().slide(slide)?.shape(shape)?;
            let body = match (cell, &target.kind) {
                (Some((row, column)), ShapeKind::Table(table)) => &table.rows.get(row)?.cells.get(column)?.text,
                (None, _) => target.text_body()?,
                _ => return None,
            };
            if body.insets == insets && scale.is_none_or(|v|body.font_scale==v) { return None; }
            let mut body = body.clone(); body.insets = insets;if let Some(scale)=scale {body.font_scale=scale;}
            let command = Command::SetTextBody { slide, shape, body };
            Some(match cell { Some((row, column)) => Command::InCell { row, column, command: Box::new(command) }, None => command })
        }).collect();
        if scale.is_some() {
            if !self.apply_pptx_commands_checked(commands,cx) {
                if let Some(p)=self.pptx_mut().and_then(|s|s.insets_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.insets-panel.text-scale-cannot-changed-target").into());}
                return cx.notify();
            }
        } else {self.run(commands,cx);}
        self.close_text_insets_panel(window, cx);
    }

    pub(crate) fn close_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() { s.insets_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn text_insets_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.insets_panel.as_ref()?;
        let (fields, error) = (panel.fields.clone(), panel.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        for (i, (name, field)) in [crate::shell::i18n::text("pptx.insets-panel.left-cm"), crate::shell::i18n::text("pptx.insets-panel.top-cm"), crate::shell::i18n::text("pptx.insets-panel.right-cm"), crate::shell::i18n::text("pptx.insets-panel.bottom-cm"), crate::shell::i18n::text("pptx.insets-panel.text-zoom-percent-blank-keep")].into_iter().zip(fields).enumerate() {
            // The margins (cm) and the text zoom (%); applied with 適用, so ▲▼ only change the field.
            let spec = if i < 4 { crate::shell::chrome::SpinnerSpec::new(0.1).min(0.0) } else { crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 100.0).integer_only() };
            body = body.child(crate::shell::chrome::text_field_row(name, crate::shell::chrome::text_field(&field).unit(if i < 4 { "cm" } else { "%" }, px(24.)).spinner(format!("pptx-insets-field-{i}"), spec, cx, |_, _, _, _| {})));
        }
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let body = body;
        let panel_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-insets-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_text_insets_panel(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-insets-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_text_insets_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.insets-panel.internal-margins-text-zoom"), body.into_any_element(), panel_actions))
    }
}
