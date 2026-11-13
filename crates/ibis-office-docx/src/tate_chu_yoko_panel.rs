// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 縦中横 (Word's 拡張書式 > 縦中横): the dialog that sets the selected characters of a text box's vertical text horizontal in
//! vertical, with 行の幅に合わせる, or takes it away (`w:eastAsianLayout w:vert`, `w:vertCompress`).
use crate::shell::{
    chrome::{self, SidebarPanel},
    editor::SlideEditor,
    i18n::text,
};
use gpui_kit::*;
use ibis_doc_model::{tate_chu_yoko_fit_scale, CharacterFormat, FlowCommand, FlowPos, FlowRunKind, RunChange, TateChuYoko};
use crate::ext::*;

pub(crate) struct TateChuYokoState {
    /// The characters of the text box's text (from, to).
    range: (usize, usize),
    /// The selection of the document's body (a paragraph's characters), when the text is not a text box's.
    body: Option<(FlowPos, FlowPos)>,
    /// 行の幅に合わせる.
    fit: bool,
    /// Whether the selection is 縦中横 already (the dialog's 削除 is then enabled).
    existing: bool,
}

impl TateChuYokoState {
    /// 行の幅に合わせる, as the checkbox sets it.
    pub(crate) fn set_fit(&mut self, fit: bool) {
        self.fit = fit;
    }
}

#[ibis_office_macros::editor_ext(TateChuYokoPanel)]
impl SlideEditor {
    /// The text of the text box the caret is in.
    fn active_text_box_body(&self) -> Option<&ibis_doc_model::TextBody> {
        let (at, _, _) = self.active_shape_text_target()?;
        let p = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == at.offset
                && let FlowRunKind::Shape(shape) = &run.kind
            {
                return match &shape.kind {
                    ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref(),
                    ibis_doc_model::ShapeKind::TextBox(b) => Some(b),
                    _ => None,
                };
            }
            offset += run.len();
        }
        None
    }

    /// The mode of the run at `from` in the active text box's text (`None`: not 縦中横).
    fn tate_chu_yoko_at(&self, from: usize) -> Option<TateChuYoko> {
        let body = self.active_text_box_body()?;
        let mut at_char = 0;
        for paragraph in &body.paragraphs {
            for r in &paragraph.runs {
                let n = r.text.chars().count();
                if from < at_char + n {
                    return r.style.tate_chu_yoko.filter(|m| *m != TateChuYoko::Off);
                }
                at_char += n;
            }
            at_char += 1;
        }
        None
    }

    /// The `w:w` 行の幅に合わせる stores for the characters `range` of the active text box (`None`: they are not wider than the line).
    fn text_box_fit_scale(&self, range: (usize, usize)) -> Option<u32> {
        let body = self.active_text_box_body()?;
        let mut advances = Vec::new();
        let mut at_char = 0;
        for paragraph in &body.paragraphs {
            for r in &paragraph.runs {
                let font = r.style.font.as_deref().or(body.style.font.as_deref());
                for c in r.text.chars() {
                    if (range.0..range.1).contains(&at_char) {
                        advances.push(crate::flow_layout::advance_em(&self.resources, font, c));
                    }
                    at_char += 1;
                }
            }
            at_char += 1;
        }
        tate_chu_yoko_fit_scale(&advances)
    }

    /// The `w:w` 行の幅に合わせる stores for the characters of the body between `start` and `end` (one paragraph).
    fn body_fit_scale(&self, start: FlowPos, end: FlowPos) -> Option<u32> {
        let p = self.paragraph_of(start.para)?;
        let mut advances = Vec::new();
        let mut offset = 0;
        for r in &p.runs {
            if let FlowRunKind::Text(t) = &r.kind {
                for c in t.chars() {
                    if (start.offset..end.offset).contains(&offset) {
                        advances.push(crate::flow_layout::advance_em(&self.resources, r.style.font_for(c), c));
                    }
                    offset += 1;
                }
            } else {
                offset += r.len();
            }
        }
        tate_chu_yoko_fit_scale(&advances)
    }

    /// Opens the dialog for the characters selected in a text box. False when no text of a text box is selected (the caller
    /// goes on to the body's own setting).
    pub fn open_tate_chu_yoko(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((_, Some(range), true)) = self.active_shape_text_target() else { return self.open_tate_chu_yoko_in_body(window, cx) };
        let current = self.tate_chu_yoko_at(range.0);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        // Word's dialog starts with 行の幅に合わせる on.
        let state = TateChuYokoState { range, body: None, fit: current.is_none_or(|m| m == TateChuYoko::Fit), existing: current.is_some() };
        if let Some(s) = self.doc_state_mut() {
            s.tate_edit = Some(state);
        }
        self.sidebar_open = true;
        cx.notify();
        true
    }

    /// The same for the characters selected in a paragraph of the body (vertical writing of the section).
    fn open_tate_chu_yoko_in_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection().filter(|(a, b)| a.para == b.para && a != b) else {
            self.show_panel_message(text("docx.fit-text.no-selection"), cx);
            return true;
        };
        let current = self.paragraph_of(start.para).and_then(|p| {
            let mut offset = 0;
            p.runs.iter().find_map(|r| {
                let at = offset;
                offset += r.len();
                (at <= start.offset && start.offset < offset && r.style.tate_chu_yoko).then_some(r.style.tate_chu_yoko_fit)
            })
        });
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let state = TateChuYokoState { range: (0, 0), body: Some((start, end)), fit: current.unwrap_or(true), existing: current.is_some() };
        if let Some(s) = self.doc_state_mut() {
            s.tate_edit = Some(state);
        }
        self.sidebar_open = true;
        cx.notify();
        true
    }

    /// OK (`remove` false) or 削除.
    pub(crate) fn apply_tate_chu_yoko(&mut self, remove: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((range, fit, body)) = self.doc_state().and_then(|s| s.tate_edit.as_ref()).map(|t| (t.range, t.fit, t.body)) else { return };
        if let Some((from, to)) = body {
            // Word stores the shrink once, as `w:w`, when 行の幅に合わせる is applied; turning it off or taking 縦中横 away stores none.
            let scale = if remove || !fit { None } else { self.body_fit_scale(from, to) };
            let commands = vec![
                FlowCommand::SetRunFormat { from, to, change: RunChange::TateChuYoko(!remove) },
                FlowCommand::SetRunFormat { from, to, change: RunChange::TateChuYokoFit(!remove && fit) },
                FlowCommand::SetRunFormat { from, to, change: RunChange::Scale(scale) },
            ];
            if self.run_flow(commands, cx) {
                if let Some(s) = self.doc_state_mut() {
                    s.tate_edit = None;
                }
                self.sidebar_open = false;
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            return;
        }
        let mode = (!remove).then_some(if fit { TateChuYoko::Fit } else { TateChuYoko::Grow });
        let scale = if remove || !fit { None } else { self.text_box_fit_scale(range) };
        let format = CharacterFormat { tate_chu_yoko: Some(mode), char_scale: Some(scale), ..Default::default() };
        let done = self.mutate_active_shape_text(|body, _| body.format_runs(range.0, range.1, &format), cx);
        if done {
            if let Some(s) = self.doc_state_mut() {
                s.tate_edit = None;
            }
            self.sidebar_open = false;
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    pub fn tate_chu_yoko_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (fit, existing) = self.doc_state()?.tate_edit.as_ref().map(|t| (t.fit, t.existing))?;
        let body = chrome::panel_form().child(chrome::checkbox_button(
            "tate-chu-yoko-fit",
            text("docx.tate-chu-yoko.fit-line"),
            fit,
            false,
            cx.listener(move |e, _, _, cx| {
                if let Some(t) = e.doc_state_mut().and_then(|s| s.tate_edit.as_mut()) {
                    t.fit = !fit;
                }
                cx.notify();
            }),
        ));
        let footer = chrome::action_bar()
            .action(
                chrome::ActionDefinition::new("tate-chu-yoko-ok", text("shell.action.ok"), chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|e, _, w, cx| e.apply_tate_chu_yoko(false, w, cx))),
            )
            .action(chrome::dialog_cancel_definition(cx))
            .action(
                chrome::ActionDefinition::new("tate-chu-yoko-delete", text("_shell.delete"), chrome::ActionButtonKind::Destructive)
                    .disabled(!existing)
                    .on_click(cx.listener(|e, _, w, cx| e.apply_tate_chu_yoko(true, w, cx))),
            );
        Some(SidebarPanel::with_footer(text("docx.toolbar.horizontal-vertical"), body.into_any_element(), footer))
    }
}
