// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pasting cells whose formulas use a name the destination already has: Excel asks (はい uses the name the
//! destination has, いいえ asks for another name, すべて answers はい for the rest). The rename step has a
//! field for the new name and キャンセル, which returns to the question. Once everything is answered the
//! paste is done again with the answers (`paste_names.rs` says what comes with a paste).

use super::clipboard::{Clip, PasteSpecialOptions};
use super::paste_names::{Answer, Brought};
use crate::shell::chrome::{ActionButtonKind, ActionDefinition, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::ext::*;

/// The paste to do again when the questions are answered.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PasteAgain {
    Paste,
    Special(PasteSpecialOptions),
    /// A sheet from another workbook is put in (`sheet_transfer.rs`).
    SheetIn,
}

pub(crate) struct PasteNamesPanel {
    again: PasteAgain,
    /// The answers so far.
    answers: Vec<Answer>,
    /// The name asked about.
    asking: String,
    /// The field for another name, once いいえ was answered.
    renaming: Option<Entity<InputState>>,
    /// What is wrong with the name typed.
    message: String,
}

impl PasteNamesPanel {
    /// The question about the name `asking`, for the paste `again` once it is answered.
    pub(crate) fn asking_about(asking: String, again: PasteAgain) -> Self {
        Self { again, answers: Vec::new(), asking, renaming: None, message: String::new() }
    }

    /// The name asked about.
    #[cfg(any(test, feature = "scenarios"))]
    pub(crate) fn asking(&self) -> &str {
        &self.asking
    }

    /// What is wrong with the name typed.
    #[cfg(any(test, feature = "scenarios"))]
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

#[ibis_office_macros::editor_ext(PasteNamesDialog)]
impl SlideEditor {
    /// Type `name` in the field for another name (tests).
    #[cfg(any(test, feature = "scenarios"))]
    pub(crate) fn paste_names_set_new_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.sheet_state().and_then(|s| s.panels.paste_names.as_ref()).and_then(|p| p.renaming.clone()) {
            input.update(cx, |i, cx| i.set_value(name.to_string(), window, cx));
        }
    }

    /// The names that come with pasting `formulas` here, answers given. Cells copied in this workbook bring the
    /// local names of their sheet to another sheet.
    pub(crate) fn names_brought(&self, clip: &Clip, formulas: &[String], answers: &[Answer]) -> Brought {
        let Some(wb) = self.presentation().workbook.as_ref() else { return Brought::default() };
        let Some(dest_sheet) = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)) else { return Brought::default() };
        let dest_sheet_names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
        let current;
        let source = if clip.foreign {
            let Some(source) = clip.source.as_ref() else { return Brought::default() };
            source
        } else {
            current = super::paste_names::NameSource {
                names: wb.names.clone(),
                sheet_names: dest_sheet_names.clone(),
                sheet: wb.sheets.iter().position(|s| s.slide == Some(clip.slide)),
                book: None,
            };
            &current
        };
        super::paste_names::names_to_bring(source, formulas, &wb.names, &dest_sheet_names, dest_sheet, !clip.foreign, answers)
    }

    /// Before pasting `clip`: what comes with it. `None` when a question is open (the paste is done again
    /// after the answers).
    pub(crate) fn paste_names_gate(&mut self, clip: &Clip, again: PasteAgain, cx: &mut Context<Self>) -> Option<Brought> {
        let formulas = clip_formulas(clip);
        let answered = self.sheet_state_mut().and_then(|s| s.panels.paste_answers.take());
        let brought = self.names_brought(clip, &formulas, answered.as_deref().unwrap_or(&[]));
        if answered.is_none()
            && let Some(first) = brought.conflicts.first()
        {
            let panel = PasteNamesPanel { again, answers: Vec::new(), asking: first.clone(), renaming: None, message: String::new() };
            let state = self.wb_mut();
            state.close_settings();
            state.panels.paste_names = Some(panel);
            self.sidebar_open = true;
            cx.notify();
            return None;
        }
        Some(brought)
    }

    /// The conflicts not answered yet, given the answers so far.
    fn paste_names_unanswered(&self, answers: &[Answer]) -> Vec<String> {
        if let Some(pending) = self.sheet_state().and_then(|s| s.panels.sheet_in.clone()) {
            return self
                .sheet_names_brought(&pending, answers)
                .conflicts
                .into_iter()
                .filter(|c| !answers.iter().any(|(n, _)| n.eq_ignore_ascii_case(c)))
                .collect();
        }
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()) else { return Vec::new() };
        let formulas = clip_formulas(&clip);
        self.names_brought(&clip, &formulas, answers)
            .conflicts
            .into_iter()
            .filter(|c| !answers.iter().any(|(n, _)| n.eq_ignore_ascii_case(c)))
            .collect()
    }

    /// An answer given: ask the next, or paste.
    fn paste_names_answered(&mut self, mut answers: Vec<Answer>, all: bool, cx: &mut Context<Self>) {
        let mut unanswered = self.paste_names_unanswered(&answers);
        if all {
            // すべて: the destination's names for what is left, however many come up.
            while !unanswered.is_empty() {
                answers.extend(unanswered.drain(..).map(|n| (n, None)));
                unanswered = self.paste_names_unanswered(&answers);
            }
        }
        if let Some(next) = unanswered.into_iter().next() {
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.paste_names.as_mut()) {
                p.answers = answers;
                p.asking = next;
                p.renaming = None;
                p.message.clear();
            }
            return cx.notify();
        }
        let Some(panel) = self.sheet_state_mut().and_then(|s| s.panels.paste_names.take()) else { return };
        self.wb_mut().panels.paste_answers = Some(answers);
        match panel.again {
            PasteAgain::Paste => self.paste_cells(false, cx),
            PasteAgain::Special(options) => self.apply_paste_special_options(options, cx),
            PasteAgain::SheetIn => self.finish_sheet_in(cx),
        }
        // Whatever the paste did not use.
        if let Some(s) = self.sheet_state_mut() {
            s.panels.paste_answers = None;
        }
        cx.notify();
    }

    pub(crate) fn paste_names_yes(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.paste_names.as_ref()) else { return };
        let mut answers = p.answers.clone();
        answers.push((p.asking.clone(), None));
        self.paste_names_answered(answers, false, cx);
    }

    pub(crate) fn paste_names_yes_to_all(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.paste_names.as_ref()) else { return };
        let mut answers = p.answers.clone();
        answers.push((p.asking.clone(), None));
        self.paste_names_answered(answers, true, cx);
    }

    pub(crate) fn paste_names_no(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx));
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.paste_names.as_mut()) {
            p.renaming = Some(input);
            p.message.clear();
        }
        cx.notify();
    }

    /// キャンセル of the rename step: back to the question.
    pub(crate) fn paste_names_back(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.paste_names.as_mut()) {
            p.renaming = None;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn paste_names_renamed(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.paste_names.as_ref()) else { return };
        let Some(input) = p.renaming.as_ref() else { return };
        let new = input.read(cx).value().trim().to_string();
        let taken = self.presentation().workbook.as_ref().is_some_and(|w| w.names.iter().any(|n| n.name.eq_ignore_ascii_case(&new)));
        let clash = p.answers.iter().any(|(_, a)| a.as_ref().is_some_and(|a| a.eq_ignore_ascii_case(&new)));
        if !super::names::valid_name(&new) || taken || clash {
            let message = text("xlsx.paste-names.invalid").to_string();
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.paste_names.as_mut()) {
                p.message = message;
            }
            return cx.notify();
        }
        let mut answers = p.answers.clone();
        answers.push((p.asking.clone(), Some(new)));
        self.paste_names_answered(answers, false, cx);
    }

    pub(crate) fn paste_names_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.paste_names.as_ref()?;
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        let footer = if let Some(input) = p.renaming.as_ref() {
            body = body
                .child(div().text_sm().child(text("xlsx.paste-names.duplicate")))
                .child(crate::shell::chrome::text_field_row(text("xlsx.paste-names.new-name"), crate::shell::chrome::text_field(input)))
                .children(crate::shell::chrome::panel_message(&p.message));
            crate::shell::chrome::action_bar()
                .action(ActionDefinition::new("paste-names-ok", text("shell.action.ok"), ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.paste_names_renamed(cx))))
                .action(ActionDefinition::new("paste-names-back", text("_shell.cancel"), ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.paste_names_back(cx))))
        } else {
            body = body.child(div().text_sm().child(format("xlsx.paste-names.exists", &[("name", &p.asking)])));
            crate::shell::chrome::action_bar()
                .action(ActionDefinition::new("paste-names-yes", text("shell.file-changed.yes"), ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.paste_names_yes(cx))))
                .action(ActionDefinition::new("paste-names-no", text("shell.file-changed.no"), ActionButtonKind::Normal).on_click(cx.listener(|this, _, window, cx| this.paste_names_no(window, cx))))
                .action(ActionDefinition::new("paste-names-all", text("xlsx.paste-names.yes-to-all"), ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.paste_names_yes_to_all(cx))))
                .action(crate::shell::chrome::dialog_cancel_definition(cx))
        };
        let title = text(if p.renaming.is_some() { "xlsx.paste-names.title-duplicate" } else { "xlsx.paste-names.title" });
        Some(SidebarPanel::with_footer(title, body.into_any_element(), footer))
    }
}

/// The formulas of the copied cells.
pub(crate) fn clip_formulas(clip: &Clip) -> Vec<String> {
    clip.cells.iter().filter_map(|c| c.cell.formula.clone()).collect()
}
