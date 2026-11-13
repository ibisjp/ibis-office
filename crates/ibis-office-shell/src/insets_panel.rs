// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Custom text-box inset inputs, including the selected table cell.
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Command, Insets, ShapeId, ShapeKind, SlideId, TextBody};
use crate::chrome::{Shown, ShownNumber, SidebarPanel, Typed};
use crate::editor::SlideEditor;
use crate::format::with_format;

type Target = (SlideId, ShapeId, Option<(usize, usize)>, TextBody);
pub struct InsetsPanel {
    pub fields: Vec<Entity<InputState>>,
    targets: Vec<Target>,
    /// The margins as opened (EMU) and as the fields showed them (rounded to 2 digits): a field left as shown keeps its exact value.
    opened: [ShownNumber; 4],
    /// The number of columns and the space between them (EMU) as opened.
    columns: [ShownNumber; 2],
    pub(crate) error: Option<SharedString>,
}

/// A margin in EMU, if it is within 0 to `i32::MAX`.
fn inset(emu: f64) -> Option<i64> {
    (0. ..=i32::MAX as f64).contains(&emu).then_some(emu as i64)
}

/// The margins (cm) are shown with two digits after the decimal point.
/// The number of columns: 1 to 16, whole.
fn columns_spec() -> crate::chrome::SpinnerSpec { crate::chrome::SpinnerSpec::new(1.0).range(1.0, 16.0).decimals(0) }

/// A number of columns (1 to 16) and a space between columns (0 to `i32::MAX` EMU), from what the fields say.
fn column_values(panel: &InsetsPanel, cx: &App) -> Option<(Typed<i64>, Typed<i64>)> {
    let count = panel.columns[0].resolve(panel.fields[5].read(cx).value().as_ref()).and_then(|v| (v.is_finite() && (1. ..=16.).contains(&v) && v.fract() == 0.).then_some(v as i64));
    let spacing = panel.columns[1].resolve(panel.fields[6].read(cx).value().as_ref()).and_then(inset);
    (!count.is_invalid() && !spacing.is_invalid()).then_some((count, spacing))
}

fn margin_spec() -> crate::chrome::SpinnerSpec { crate::chrome::SpinnerSpec::new(0.1).min(0.0).decimals(2) }

impl SlideEditor {
    pub fn open_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets = if let Some(edit) = self.text_editing() {
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
                cell_targets
            } else {
                self.edited_body().map(|b| vec![(self.current_slide, edit.shape, None, b.clone())]).unwrap_or_default()
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
            all_targets
        };
        if targets.is_empty() { return; }
        self.end_text_edit(window, cx);
        // Each margin shows the common value of the targets, or is empty when they differ (and is then not written unless a value is put in).
        let opened = [(|i: &Insets| i.left) as fn(&Insets) -> i64, |i| i.top, |i| i.right, |i| i.bottom].map(|side| ShownNumber::new(Shown::of(&targets, |t| side(&t.3.insets)), 360_000., margin_spec()));
        let mut fields: Vec<_> = opened.iter().map(|shown| shown.input(window, cx)).collect();
        fields.push(cx.new(|cx| InputState::new(window,cx).placeholder(crate::i18n::text("pptx.common.automatic")).default_value("")));
        // 段の数 and 段の間隔 (`numCol`, `spcCol`): the common value of the targets, or empty where they differ.
        let columns = [ShownNumber::new(Shown::of(&targets, |t| i64::from(t.3.columns)), 1., columns_spec()), ShownNumber::new(Shown::of(&targets, |t| t.3.column_spacing), 360_000., margin_spec())];
        fields.extend(columns.iter().map(|shown| shown.input(window, cx)));
        let first = fields[0].clone();
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None;
        self.shape_edit.crop_panel = None; self.shape_edit.find = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Insets));
            self.shape_edit.chart_panel = None;
            self.shape_edit.insets_panel = Some(InsetsPanel { fields, targets, opened, columns, error: None });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub fn apply_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shape_edit.insets_panel.as_ref() else { return };
        // A margin whose field showed "mixed" and was left empty is not written: each target keeps its own.
        let values: Vec<Typed<i64>> = panel.fields.iter().take(4).zip(&panel.opened).map(|(f, shown)| shown.resolve(f.read(cx).value().as_ref()).and_then(inset)).collect();
        let Some(v) = (!values.iter().any(Typed::is_invalid)).then_some(values) else {
            if let Some(panel) = self.shape_edit.insets_panel.as_mut() { panel.error = Some(crate::i18n::text("pptx.insets-panel.enter-finite-number-0-more").into()); }
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
            if let Some(p)=self.shape_edit.insets_panel.as_mut() {p.error=Some(crate::i18n::text("pptx.paragraph-panel.target-text-has-changed-confirm").into());}
            return cx.notify();
        }
        let Some((column_count, column_spacing)) = column_values(panel, cx) else {
            if let Some(panel) = self.shape_edit.insets_panel.as_mut() { panel.error = Some(crate::i18n::text("pptx.insets-panel.enter-columns-1-16").into()); }
            return cx.notify();
        };
        let value=panel.fields[4].read(cx).value();let value=value.trim();
        let scale=if value.is_empty() {None} else {
            let scale=value.parse::<f64>().ok().filter(|v|v.is_finite() && *v>=1. && *v<=100.).map(|v|(v*1000.).round() as u32).filter(|v|*v>0);
            let Some(scale)=scale else {if let Some(p)=self.shape_edit.insets_panel.as_mut() {p.error=Some(crate::i18n::text("pptx.insets-panel.enter-text-scale-1-100").into());}return cx.notify();};Some(scale)
        };
        let commands = panel.targets.clone().into_iter().filter_map(|(slide, shape, cell, _)| {
            let target = self.presentation().slide(slide)?.shape(shape)?;
            let body = match (cell, &target.kind) {
                (Some((row, column)), ShapeKind::Table(table)) => &table.rows.get(row)?.cells.get(column)?.text,
                (None, _) => target.text_body()?,
                _ => return None,
            };
            let own = body.insets;
            let insets = Insets { left: v[0].clone().or(own.left)?, top: v[1].clone().or(own.top)?, right: v[2].clone().or(own.right)?, bottom: v[3].clone().or(own.bottom)? };
            let (columns, spacing) = (column_count.clone().or(i64::from(body.columns))?, column_spacing.clone().or(body.column_spacing)?);
            if body.insets == insets && scale.is_none_or(|v|body.font_scale==v) && i64::from(body.columns) == columns && body.column_spacing == spacing { return None; }
            let mut body = body.clone(); body.insets = insets;if let Some(scale)=scale {body.font_scale=scale;}
            body.columns = columns as u32; body.column_spacing = spacing;
            let command = Command::SetTextBody { slide, shape, body };
            Some(match cell { Some((row, column)) => Command::InCell { row, column, command: Box::new(command) }, None => command })
        }).collect();
        if scale.is_some() {
            if !self.apply_pptx_commands_checked(commands,cx) {
                if let Some(p)=self.shape_edit.insets_panel.as_mut() {p.error=Some(crate::i18n::text("pptx.insets-panel.text-scale-cannot-changed-target").into());}
                return cx.notify();
            }
        } else {self.run(commands,cx);}
        self.close_text_insets_panel(window, cx);
    }

    pub fn close_text_insets_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uses_shape_edit() { self.shape_edit.insets_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn text_insets_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.shape_edit.insets_panel.as_ref()?;
        let (fields, error) = (panel.fields.clone(), panel.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        for (i, (name, field)) in [crate::i18n::text("pptx.insets-panel.left-cm"), crate::i18n::text("pptx.insets-panel.top-cm"), crate::i18n::text("pptx.insets-panel.right-cm"), crate::i18n::text("pptx.insets-panel.bottom-cm"), crate::i18n::text("pptx.insets-panel.text-zoom-percent-blank-keep"), crate::i18n::text("pptx.insets-panel.columns"), crate::i18n::text("pptx.insets-panel.column-spacing-cm")].into_iter().zip(fields).enumerate() {
            // The margins (cm) and the text zoom (%); applied with 適用, so ▲▼ only change the field.
            let spec = match i { 0..=3 | 6 => margin_spec(), 5 => columns_spec(), _ => crate::chrome::SpinnerSpec::new(1.0).range(1.0, 100.0).decimals(2) };
            let unit = match i { 0..=3 | 6 => Some("cm"), 4 => Some("%"), _ => None };
            let field = crate::chrome::text_field(&field);
            let field = if let Some(unit) = unit { field.unit(unit, px(24.)) } else { field };
            body = body.child(crate::chrome::text_field_row(name, field.spinner(format!("pptx-insets-field-{i}"), spec, cx, |_, _, _, _| {})));
        }
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let body = body;
        let panel_actions = crate::chrome::action_bar()
            .action(crate::chrome::ActionDefinition::new("pptx-insets-apply", crate::i18n::text("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_text_insets_panel(w, cx))))
            .action(crate::chrome::ActionDefinition::new("pptx-insets-cancel", crate::i18n::text("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_text_insets_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::i18n::text("pptx.insets-panel.internal-margins-text-zoom"), body.into_any_element(), panel_actions))
    }
}
