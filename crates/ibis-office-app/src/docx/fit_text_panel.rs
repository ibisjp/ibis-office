// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Character distribution uses the existing OOXML fitText layout and editing history.
use crate::shell::{
    chrome::{self, SidebarPanel},
    editor::SlideEditor,
    i18n::text,
};
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowPos, RunChange};

pub(crate) struct FitTextState {
    pub input: Entity<InputState>,
    pub from: FlowPos,
    pub to: FlowPos,
    size: u32,
    id: u32,
}
impl SlideEditor {
    /// The whole contiguous fitText group containing the insertion point.
    pub(crate) fn fit_text_at_caret(&self) -> Option<(FlowPos, FlowPos, u32, u32)> {
        let caret = self.doc_state()?.caret?;
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, caret.para)?;
        let mut spans = Vec::new();
        let mut offset = 0;
        for run in &p.runs {
            let end = offset + run.len();
            spans.push((offset, end, run.style.fit_text));
            offset = end;
        }
        let index = spans
            .iter()
            .position(|(a, b, fit)| *a <= caret.offset && caret.offset < *b && fit.is_some())?;
        let (width, id) = spans[index].2?;
        let mut first = index;
        let mut last = index;
        while first > 0 && spans[first - 1].2.is_some_and(|(_, other)| other == id) {
            first -= 1;
        }
        while last + 1 < spans.len() && spans[last + 1].2.is_some_and(|(_, other)| other == id) {
            last += 1;
        }
        Some((
            FlowPos::new(caret.para, spans[first].0),
            FlowPos::new(caret.para, spans[last].1),
            width,
            id,
        ))
    }
    pub(crate) fn open_fit_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing = self.fit_text_at_caret();
        let selection = self
            .ordered_selection()
            .filter(|(a, b)| a.para == b.para && a != b);
        let Some((from, to)) = selection.or_else(|| existing.map(|(a, b, _, _)| (a, b))) else {
            return;
        };
        let Some(p) = self
            .flow()
            .and_then(|d| ibis_doc_model::flow_command::paragraph(d, from.para))
        else {
            return;
        };
        let size = p.format_at(from.offset + 1).2.size.max(1);
        let id = existing.map(|(_, _, _, id)| id).unwrap_or_else(|| {
            p.runs
                .iter()
                .filter_map(|r| r.style.fit_text.map(|(_, id)| id))
                .max()
                .unwrap_or(0)
                .saturating_add(1)
        });
        let width = existing
            .map(|(_, _, w, _)| w as f64 * 5. / size as f64)
            .unwrap_or((to.offset - from.offset) as f64);
        if self.panel_dialog.active {
            self.close_panel_dialog(window, cx);
        }
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| {
            i.set_value(
                format!("{width:.2}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string(),
                window,
                cx,
            );
            i.focus(window, cx);
        });
        self.doc_state_mut().unwrap().fit_text_edit = Some(FitTextState {
            input,
            from,
            to,
            size,
            id,
        });
        self.sidebar_open = true;
        cx.notify();
    }
    pub(crate) fn apply_fit_text(
        &mut self,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self.doc_state().and_then(|s| s.fit_text_edit.as_ref()) else {
            return;
        };
        let value = if remove {
            None
        } else {
            let Ok(chars) = p.input.read(cx).value().trim().parse::<f64>() else {
                return;
            };
            let twips = chars * p.size as f64 / 5.;
            if !twips.is_finite() || twips < 1. || twips > u32::MAX as f64 {
                return;
            }
            Some((twips.round() as u32, p.id))
        };
        let command = FlowCommand::SetRunFormat {
            from: p.from,
            to: p.to,
            change: RunChange::FitText(value),
        };
        if self.run_flow(vec![command], cx) {
            self.doc_state_mut().unwrap().fit_text_edit = None;
            self.sidebar_open = false;
            self.focus_handle.focus(window, cx);
        }
    }
    pub(crate) fn fit_text_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.fit_text_edit.as_ref()?;
        let field = chrome::text_field(&p.input).spinner(
            "fit-text-width",
            chrome::SpinnerSpec::new(0.5).min(0.05),
            cx,
            |e, v, w, cx| {
                if let Some(p) = e.doc_state().and_then(|s| s.fit_text_edit.as_ref()) {
                    let input = p.input.clone();
                    input.update(cx, |i, cx| i.set_value(v.to_string(), w, cx));
                }
            },
        );
        let body = chrome::panel_form().child(chrome::text_field_row(
            text("docx.fit-text.width"),
            field.unit(text("docx.fit-text.characters"), px(20.)),
        ));
        let footer = chrome::action_bar()
            .action(
                chrome::ActionDefinition::new(
                    "fit-text-ok",
                    text("shell.action.ok"),
                    chrome::ActionButtonKind::Primary,
                )
                .on_click(cx.listener(|e, _, w, cx| e.apply_fit_text(false, w, cx))),
            )
            .action(chrome::dialog_cancel_definition(cx))
            .action(
                chrome::ActionDefinition::new(
                    "fit-text-delete",
                    text("_shell.delete"),
                    chrome::ActionButtonKind::Destructive,
                )
                .on_click(cx.listener(|e, _, w, cx| e.apply_fit_text(true, w, cx))),
            );
        Some(SidebarPanel::with_footer(
            text("docx.fit-text.title"),
            body.into_any_element(),
            footer,
        ))
    }
    pub(crate) fn fit_text_indicator(
        &self,
        lines: &[super::editing::LineGeom],
        page: usize,
        k: f32,
    ) -> Vec<AnyElement> {
        let Some((from, to, _, _)) = self.fit_text_at_caret() else {
            return Vec::new();
        };
        lines
            .iter()
            .filter(|l| l.page == page && l.para == from.para)
            .filter_map(|line| {
                let inside: Vec<_> = line
                    .stops
                    .iter()
                    .filter(|(o, _)| *o >= from.offset && *o <= to.offset)
                    .map(|(_, x)| *x)
                    .collect();
                let a = inside.iter().copied().reduce(f32::min)?;
                let b = inside.iter().copied().reduce(f32::max)?;
                if b <= a {
                    return None;
                }
                Some(
                    if line.vertical {
                        div()
                            .absolute()
                            .left(px((line.x + line.h) * k))
                            .top(px((line.y + a) * k))
                            .w(px(2.))
                            .h(px((b - a) * k))
                    } else {
                        div()
                            .absolute()
                            .left(px((line.x + a) * k))
                            .top(px((line.y + line.h) * k))
                            .w(px((b - a) * k))
                            .h(px(2.))
                    }
                    .bg(crate::shell::palette::rgb(0x66ccff))
                    .into_any_element(),
                )
            })
            .collect()
    }
}
