// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! P1 paragraph spacing, line height and indentation controls.
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{LineHeight, ParaChange};
use crate::shell::{chrome::SidebarPanel, editor::SlideEditor};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineRule { Multiple, AtLeast, Exact }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IndentUnit { Pt, Chars }

pub(crate) struct ParagraphEditState {
    /// Before, after, left, right, first line (points/chars), line (multiple/points).
    pub(crate) fields: [Entity<InputState>; 6],
    pub(crate) rule: LineRule,
    pub(crate) rule_mixed: bool,
    pub(crate) indent_unit: IndentUnit,
    pub(crate) contextual_spacing: Option<bool>,
    pub(crate) widow_control: Option<bool>,
    pub(crate) suppress_line_numbers: Option<bool>,
    pub(crate) suppress_auto_hyphens: Option<bool>,
    target: (u32, u32),
    document: std::sync::Arc<ibis_doc_model::FlowDocument>,
    initial: ([String; 6], LineRule, IndentUnit, Option<bool>, Option<bool>, Option<bool>, Option<bool>),
    pub(super) error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

fn points(emu: i64) -> String { format!("{:.2}", emu as f64 / 12_700.) }

fn char_unit_size(p: &ibis_doc_model::FlowParagraph) -> i64 {
    if p.mark.size > 0 { p.mark.size as i64 } else { 133_350 }
}

fn indent_text(emu: i64, chars: Option<i32>, unit: IndentUnit, char_emu: i64) -> String {
    match unit {
        IndentUnit::Chars => {
            if let Some(c) = chars {
                let v = c as f64 / 100.0;
                if (v - v.round()).abs() < 0.001 {
                    format!("{:.0}", v)
                } else {
                    format!("{:.2}", v)
                }
            } else {
                let v = emu as f64 / char_emu as f64;
                if (v - v.round()).abs() < 0.001 {
                    format!("{:.0}", v)
                } else {
                    format!("{:.2}", v)
                }
            }
        }
        IndentUnit::Pt => points(emu),
    }
}

fn paragraph_values(p: &ibis_doc_model::FlowParagraph, indent_unit: IndentUnit) -> ([String; 6], LineRule) {
    let (rule, line) = match p.line {
        LineHeight::Auto(v) => (LineRule::Multiple, format!("{:.3}", v as f64 / 240.)),
        LineHeight::AtLeast(v) => (LineRule::AtLeast, points(v)),
        LineHeight::Exact(v) => (LineRule::Exact, points(v)),
    };
    let char_emu = char_unit_size(p);
    let left = indent_text(p.left, p.left_chars, indent_unit, char_emu);
    let right = indent_text(p.right, p.right_chars, indent_unit, char_emu);
    let first = indent_text(p.first_line, p.first_line_chars, indent_unit, char_emu);
    let values = [points(p.space_before), points(p.space_after), left, right, first, line];
    (values, rule)
}

fn parse_indent_value(raw: &str, current_unit: IndentUnit, char_emu: i64) -> Option<(i64, Option<i32>)> {
    let s = raw.trim();
    if s.is_empty() { return None; }
    let (num_str, is_chars) = if let Some(stripped) = s.strip_suffix("字") { // i18n-ok（入力の照合に使う語（文字数の単位「字」の接尾辞を読む））
        (stripped.trim(), true)
    } else if let Some(stripped) = s.strip_suffix("ch") {
        (stripped.trim(), true)
    } else if let Some(stripped) = s.strip_suffix("pt") {
        (stripped.trim(), false)
    } else if let Some(stripped) = s.strip_suffix("mm") {
        let val = stripped.trim().parse::<f64>().ok()?;
        let emu = (val * 36_000.0).round() as i64;
        return Some((emu, None));
    } else {
        (s, current_unit == IndentUnit::Chars)
    };
    let val = num_str.parse::<f64>().ok().filter(|v| v.is_finite())?;
    if !(-1584.0..=1584.0).contains(&val) { return None; }
    if is_chars {
        let chars = (val * 100.0).round() as i32;
        let emu = (val * char_emu as f64).round() as i64;
        Some((emu, Some(chars)))
    } else {
        let emu = (val * 20.0).round() as i64 * 635;
        Some((emu, None))
    }
}

impl SlideEditor {
    pub(crate) fn open_paragraph_edit(&mut self, rule: Option<LineRule>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target) = self.selected_paragraphs() else { return };
        let Some(document) = self.flow().cloned() else { return };
        let Some(p) = self.paragraph_of(target.0) else { return };
        let initial_unit = if p.left_chars.is_some() || p.right_chars.is_some() || p.first_line_chars.is_some() {
            IndentUnit::Chars
        } else {
            IndentUnit::Pt
        };
        let (mut values, current_rule) = paragraph_values(p, initial_unit);
        let mut rule_mixed = false;
        let mut contextual_spacing = Some(p.contextual_spacing);
        let mut widow_control = Some(p.widow_control);
        let mut suppress_line_numbers = Some(p.suppress_line_numbers);
        let mut suppress_auto_hyphens = Some(p.suppress_auto_hyphens);
        if let Some(doc) = self.flow() {
            use ibis_doc_model::flow_command;
            if let (Some((container, first)), Some((other, last))) = (flow_command::locate(doc, target.0), flow_command::locate(doc, target.1)) {
                if container == other && first <= last && let Some(blocks) = flow_command::blocks(doc, container) {
                    for paragraph in super::block_tree::paragraphs(&blocks[first..=last]) {
                        let (next, next_rule) = paragraph_values(paragraph, initial_unit);
                        rule_mixed |= next_rule != current_rule;
                        for i in 0..6 {
                            if values[i] != next[i] || (i == 5 && next_rule != current_rule) { values[i].clear(); }
                        }
                        if contextual_spacing != Some(paragraph.contextual_spacing) {
                            contextual_spacing = None;
                        }
                        if widow_control != Some(paragraph.widow_control) {
                            widow_control = None;
                        }
                        if suppress_line_numbers != Some(paragraph.suppress_line_numbers) {
                            suppress_line_numbers = None;
                        }
                        if suppress_auto_hyphens != Some(paragraph.suppress_auto_hyphens) {
                            suppress_auto_hyphens = None;
                        }
                    }
                }
            }
        }
        let fields = values.clone().map(|value| cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(value, window, cx);
            state
        }));
        let subscriptions = fields.iter().map(|field| cx.subscribe_in(field, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event { this.apply_paragraph_edit(window, cx); }
        })).collect();
        if let Some(state) = self.doc_state_mut() {
            state.paragraph_edit = Some(ParagraphEditState { fields, rule: current_rule, rule_mixed, indent_unit: initial_unit, contextual_spacing, widow_control, suppress_line_numbers, suppress_auto_hyphens, target, document,
                initial: (values, current_rule, initial_unit, contextual_spacing, widow_control, suppress_line_numbers, suppress_auto_hyphens), error: None, _subscriptions: subscriptions });
            state.font_edit=None; state.color_edit=None; state.picture_edit = None; state.table_size = None; state.style_edit = None; state.link = None; state.find = None; state.comments = None;
        }
        self.sidebar_open = true;
        if let Some(rule) = rule { self.choose_paragraph_line_rule(rule, window, cx); }
        cx.notify();
    }

    pub(crate) fn choose_paragraph_line_rule(&mut self, rule: LineRule, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) else { return };
        if rule != edit.rule || edit.rule_mixed {
            let field = edit.fields[5].clone();
            let switches_units = (rule == LineRule::Multiple) != (edit.rule == LineRule::Multiple);
            let mixed = edit.rule_mixed;
            edit.rule = rule;
            edit.rule_mixed = false;
            if switches_units || mixed { field.update(cx, |i, cx| i.set_value(if rule == LineRule::Multiple { "1" } else { "12" }, window, cx)); }
        }
        cx.notify();
    }

    pub(crate) fn choose_paragraph_indent_unit(&mut self, unit: IndentUnit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.paragraph_edit.as_ref()) else { return };
        if unit == edit.indent_unit { return; }
        let (first, _) = edit.target;
        let char_emu = self.paragraph_of(first).map(char_unit_size).unwrap_or(133_350);
        let fields = edit.fields.clone();
        let Some(edit_mut) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) else { return };
        edit_mut.indent_unit = unit;
        for i in 2..=4 {
            let field = fields[i].clone();
            let current = field.read(cx).value().trim().to_string();
            if current.is_empty() { continue; }
            if let Some(val) = current.parse::<f64>().ok() {
                let new_val = if unit == IndentUnit::Chars {
                    let emu = val * 12_700.0;
                    let chars = emu / char_emu as f64;
                    if (chars - chars.round()).abs() < 0.001 {
                        format!("{:.0}", chars)
                    } else {
                        format!("{:.2}", chars)
                    }
                } else {
                    let emu = val * char_emu as f64;
                    let pt = emu / 12_700.0;
                    if (pt - pt.round()).abs() < 0.001 {
                        format!("{:.0}", pt)
                    } else {
                        format!("{:.2}", pt)
                    }
                };
                field.update(cx, |inp, cx| inp.set_value(new_val, window, cx));
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_paragraph_contextual_spacing(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
            edit.contextual_spacing = Some(on);
            cx.notify();
        }
    }

    pub(crate) fn toggle_paragraph_widow_control(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
            edit.widow_control = Some(on);
            cx.notify();
        }
    }

    pub(crate) fn toggle_paragraph_suppress_line_numbers(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
            edit.suppress_line_numbers = Some(on);
            cx.notify();
        }
    }

    pub(crate) fn toggle_paragraph_suppress_auto_hyphens(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
            edit.suppress_auto_hyphens = Some(on);
            cx.notify();
        }
    }

    pub(crate) fn apply_paragraph_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.paragraph_edit.as_ref()) else { return };
        if self.flow() != Some(&edit.document) {
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
                edit.error = Some(crate::shell::i18n::text("docx.paragraph-panel.document-has-changed-reopen").into());
            }
            cx.notify();
            return;
        }
        let values = edit.fields.clone().map(|field| field.read(cx).value().trim().to_string());
        let (initial, old_rule, old_indent_unit, old_contextual, old_widow, old_suppress, old_suppress_hyphens) = edit.initial.clone();
        let rule = edit.rule;
        let indent_unit = edit.indent_unit;
        let (first, last) = edit.target;
        let char_emu = self.paragraph_of(first).map(char_unit_size).unwrap_or(133_350);
        let mut changes = Vec::new();
        let mut invalid = self.selected_paragraphs() != Some((first, last)) || (edit.rule_mixed && values[5] != initial[5]);
        for i in 0..6 {
            let changed = values[i] != initial[i] || (i == 5 && rule != old_rule) || ((2..=4).contains(&i) && indent_unit != old_indent_unit);
            if !changed { continue; }
            if (2..=4).contains(&i) {
                let Some((emu, chars)) = parse_indent_value(&values[i], indent_unit, char_emu) else {
                    invalid = true;
                    continue;
                };
                match i {
                    2 => {
                        changes.push(ParaChange::Left(emu));
                        changes.push(ParaChange::LeftChars(chars));
                    }
                    3 => {
                        changes.push(ParaChange::Right(emu));
                        changes.push(ParaChange::RightChars(chars));
                    }
                    4 => {
                        changes.push(ParaChange::FirstLine(emu));
                        changes.push(ParaChange::FirstLineChars(chars));
                    }
                    _ => {}
                }
            } else {
                let value = values[i].parse::<f64>().ok().filter(|v| v.is_finite());
                let lower = if i == 5 { 0.05 } else { 0. };
                let upper = if i == 5 && rule == LineRule::Multiple { 132. } else { 1584. };
                let Some(value) = value.filter(|v| (lower..=upper).contains(v)) else { invalid = true; continue };
                let emu = (value * 20.).round() as i64 * 635;
                changes.push(match i {
                    0 => ParaChange::SpaceBefore(emu),
                    1 => ParaChange::SpaceAfter(emu),
                    _ => ParaChange::Line(match rule {
                        LineRule::Multiple => LineHeight::Auto((value * 240.).round() as u32),
                        LineRule::AtLeast => LineHeight::AtLeast(emu),
                        LineRule::Exact => LineHeight::Exact(emu),
                    }),
                });
            }
        }
        if let Some(cs) = edit.contextual_spacing {
            if old_contextual != Some(cs) {
                changes.push(ParaChange::ContextualSpacing(cs));
            }
        }
        if let Some(wc) = edit.widow_control {
            if old_widow != Some(wc) {
                changes.push(ParaChange::WidowControl(wc));
            }
        }
        if let Some(sln) = edit.suppress_line_numbers {
            if old_suppress != Some(sln) {
                changes.push(ParaChange::SuppressLineNumbers(sln));
            }
        }
        if let Some(sah) = edit.suppress_auto_hyphens {
            if old_suppress_hyphens != Some(sah) {
                changes.push(ParaChange::SuppressAutoHyphens(sah));
            }
        }
        if invalid {
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
                edit.error = Some(crate::shell::i18n::text("docx.paragraph-panel.check-numbers-paragraph-selection").into());
            }
            cx.notify();
            return;
        }
        let commands = changes.into_iter().flat_map(|change| self.paragraph_format_commands(first, last, change)).collect::<Vec<_>>();
        if commands.is_empty() || self.run_flow(commands, cx) {
            let document = self.flow().unwrap().clone();
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.paragraph_edit.as_mut()) {
                edit.initial = (values, rule, indent_unit, edit.contextual_spacing, edit.widow_control, edit.suppress_line_numbers, edit.suppress_auto_hyphens);
                edit.document = document;
                edit.error = None;
            }
            if !self.panel_dialog_is_modal() { self.focus_handle.focus(window, cx); }
        }
        cx.notify();
    }

    pub(crate) fn paragraph_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let edit = self.doc_state()?.paragraph_edit.as_ref()?;
        let fields = edit.fields.clone();
        let rule = edit.rule;
        let mixed = edit.rule_mixed;
        let indent_unit = edit.indent_unit;
        let error = edit.error.clone();
        use crate::shell::chrome::SpinnerSpec;
        // ▲▼ as Word's: spacing before/after by 6 pt, indents by 0.5 character or 1 pt, line spacing by 0.5 line or 1 pt.
        let indent_spec = if indent_unit == IndentUnit::Chars { SpinnerSpec::new(0.5) } else { SpinnerSpec::new(1.0) };
        let line_spec = if rule == LineRule::Multiple { SpinnerSpec::new(0.5).range(0.06, 132.0) } else { SpinnerSpec::new(1.0).range(0.0, 1584.0) };
        let specs = [SpinnerSpec::new(6.0).range(0.0, 1584.0), SpinnerSpec::new(6.0).range(0.0, 1584.0), indent_spec, indent_spec, indent_spec, line_spec];
        let field_cx = &*cx;
        let row = |label: &'static str, i: usize| h_flex().gap_2()
            .child(div().w(px(92.)).text_sm().child(crate::shell::i18n::text(label)))
            .child(crate::shell::chrome::text_field(&fields[i])
                .spinner(format!("para-field-{i}"), specs[i], field_cx, |_, _, _, _| {}));
        let choice = |id: &'static str, label: &'static str, value: LineRule| crate::shell::chrome::radio_button(id, !mixed && rule == value, false, crate::shell::i18n::text(label),
            cx.listener(move |this, _, window, cx| this.choose_paragraph_line_rule(value, window, cx)));
        let indent_choice = |id: &'static str, label: &'static str, value: IndentUnit| crate::shell::chrome::radio_button(id, indent_unit == value, false, crate::shell::i18n::text(label),
            cx.listener(move |this, _, window, cx| this.choose_paragraph_indent_unit(value, window, cx)));
        let body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("docx.paragraph-panel.indent-units")))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(indent_choice("indent-chars", "docx.paragraph-panel.characters", IndentUnit::Chars))
                .child(indent_choice("indent-pt", "pt", IndentUnit::Pt)))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.paragraph-panel.spacing-indent-unit-pt-use")))
            .child(row("docx.paragraph-panel.before", 0)).child(row("docx.paragraph-panel.after", 1))
            .child(row("docx.paragraph-panel.left-indent", 2)).child(row("docx.paragraph-panel.right-indent", 3)).child(row("docx.paragraph-panel.first-line", 4))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.paragraph-panel.line-type")))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow).child(choice("para-multiple", "_shell.multiple", LineRule::Multiple))
                .child(choice("para-atleast", "_shell.at-least", LineRule::AtLeast)).child(choice("para-exact", "docx.common.exactly", LineRule::Exact)))
            .child(row(if mixed { "docx.paragraph-panel.line-spacing-mixed" } else if rule == LineRule::Multiple { "docx.paragraph-panel.line-spacing-multiple" } else { "docx.paragraph-panel.line-spacing-pt" }, 5))
            .child(Checkbox::new("para-widow-control")
                .label(crate::shell::i18n::text("docx.paragraph-panel.widow-orphan-control"))
                .checked(edit.widow_control == Some(true))
                .on_click(cx.listener(|this, on: &bool, _, cx| this.toggle_paragraph_widow_control(*on, cx))))
            .child(Checkbox::new("para-suppress-line-numbers")
                .label(crate::shell::i18n::text("docx.paragraph-panel.suppress-line-numbers"))
                .checked(edit.suppress_line_numbers == Some(true))
                .on_click(cx.listener(|this, on: &bool, _, cx| this.toggle_paragraph_suppress_line_numbers(*on, cx))))
            .child(Checkbox::new("para-suppress-auto-hyphens")
                .label(crate::shell::i18n::text("docx.paragraph-panel.don-t-hyphenate"))
                .checked(edit.suppress_auto_hyphens == Some(true))
                .on_click(cx.listener(|this, on: &bool, _, cx| this.toggle_paragraph_suppress_auto_hyphens(*on, cx))))
            .child(Checkbox::new("para-contextual-spacing")
                .label(crate::shell::i18n::text("fix.toolbar.don-t-add-space-between"))
                .checked(edit.contextual_spacing == Some(true))
                .on_click(cx.listener(|this, on: &bool, _, cx| this.toggle_paragraph_contextual_spacing(*on, cx))))
            .children(error.map(|error| div().text_sm().child(error)))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.paragraph-panel.blank-means-mixed-only-changed")));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("para-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.apply_paragraph_edit(window, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("para-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(state) = this.doc_state_mut() { state.paragraph_edit = None; }
                    this.sidebar_open = false;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                })))
            .action(crate::shell::chrome::ActionDefinition::new("para-tabs", crate::shell::i18n::text("docx.paragraph-panel.tabs"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| this.open_tab_edit(window, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.paragraph-panel.align-indent"), body.into_any_element(), footer))
    }
}
