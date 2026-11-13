// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DOCX character properties and recursive selection replacement.
//! Use existing paragraph replacement commands to preserve one-step undo.
use gpui_kit::*;
use ibis_doc_model::{Block, FlowCommand, InsertFormat, Revision, RevisionKind, RunFormat, UnderlineStyle};
use ibis_doc_model::flow_command;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(TextFormat)]
impl SlideEditor {
    pub fn apply_text_properties(&mut self, properties: RunFormat, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        if start == end {
            let Some(p) = self.paragraph_of(start.para) else { return false };
            let (format, char_style, _) = super::editing::visible_format_at(&self.resources.markup_view, p, start.offset);
            let mut pending = self.doc_state().and_then(|s| s.pending.clone()).unwrap_or(InsertFormat { format, char_style });
            pending.format.overlay(&properties);
            if let Some(state) = self.doc_state_mut() { state.pending = Some(pending); }
            cx.notify();
            return true;
        }
        self.apply_selected_text_format(|format| format.overlay(&properties), cx)
    }

    pub(crate) fn apply_selected_text_format(&mut self, update: impl Fn(&mut RunFormat), cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let (Some((container, first)), Some((last_container, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false };
        if container != last_container || first > last { return false; }
        let Some(list) = flow_command::blocks(doc, container) else { return false };
        let mut blocks = list[first..=last].to_vec();
        let styles = doc.styles.clone();
        let track = doc.track_changes;
        let who = super::comments::comment_author();
        let apply=|p:&mut ibis_doc_model::FlowParagraph,a:usize,b:usize| {
            // Paragraph-mark revisions are not represented by the model. Do not
            // leave an untracked mark change behind when run changes are rejected.
            if !track && a == 0 && b == p.len() { update(&mut p.mark_format); }
            let after = p.split_off_runs(b);
            let mut selected = p.split_off_runs(a);
            for r in &mut selected {
                if !self.resources.markup_view.showing_markup() && matches!(r.revision.as_ref().map(|v| &v.kind), Some(RevisionKind::Delete)) { continue; }
                if track && r.revision.is_none() {
                    r.revision = Some(Revision { kind: RevisionKind::Format(r.format.clone()), author: who.author.clone(), date: who.date.clone() });
                }
                let colour = (r.format.color, r.format.theme_color.clone());
                update(&mut r.format);
                // A colour given to a whole formula replaces those of its runs, as Word's does.
                if let ibis_doc_model::FlowRunKind::Math { math, .. } = &mut r.kind && colour != (r.format.color, r.format.theme_color.clone()) {
                    *math = math.without_colors();
                }
            }
            p.runs.extend(selected);
            p.runs.extend(after);
            p.resolve(&styles);
            p.normalize();
        };
        for (i,block) in blocks.iter_mut().enumerate() {
            match block {
                Block::Paragraph(p)=> {
                    let a=if i==0 {start.offset} else {0};let b=if first+i==last {end.offset} else {p.len()};apply(p,a,b);
                }
                Block::Table(_)=>super::block_tree::each_paragraph(std::slice::from_mut(block),&mut |p| {let end=p.len();apply(p,0,end);}),
            }
        }
        self.run_flow(vec![FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks }], cx)
    }

    pub(crate) fn toggle_doc_format_brush(&mut self, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.format_brush.is_some()) {
            if let Some(state) = self.doc_state_mut() { state.format_brush = None; }
            cx.notify();
            return;
        }
        self.capture_doc_format_brush(false, cx);
    }

    pub(crate) fn toggle_doc_format_brush_persistent(&mut self, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.format_brush.as_ref().is_some_and(|b| b.persistent)) {
            if let Some(state) = self.doc_state_mut() { state.format_brush = None; }
            cx.notify();
            return;
        }
        self.capture_doc_format_brush(true, cx);
    }

    pub(crate) fn capture_doc_format_brush(&mut self, persistent: bool, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        let Some(p) = self.paragraph_of(start.para) else { return };
        let format = if !self.resources.markup_view.showing_markup() {
            // A selection copies its first visible character. A caret copies
            // its visible predecessor, or the mark if every run is deleted.
            let offset = if start != end { start.offset + 1 } else { start.offset };
            let (format, char_style, style) = super::editing::visible_format_at(&self.resources.markup_view, p, offset);
            let source = ibis_doc_model::FlowRun { kind: ibis_doc_model::FlowRunKind::Text(String::new()), format, char_style, style, link: None, revision: None, show_field_code: false, field_locked: false, control: None };
            super::clipboard::resolved_format(&source)
        } else {
            let mut offset = 0;
            let source = p.runs.iter().find(|r| {
                let end = offset + r.len();
                let found = start.offset >= offset && start.offset < end;
                offset = end;
                found
            }).or_else(|| p.runs.last());
            let Some(source) = source else { return };
            super::clipboard::resolved_format(source)
        };
        let whole_paragraph = start.para != end.para || start == end || (start.offset == 0 && end.offset >= p.len());
        let para_fmt = if whole_paragraph {
            Some(ibis_doc_model::ParaFormat {
                align: Some(p.align),
                line: Some(p.line),
                before: Some(p.space_before),
                after: Some(p.space_after),
                left: Some(p.left),
                right: Some(p.right),
                first_line: Some(p.first_line),
                ..p.direct.clone()
            })
        } else {
            None
        };
        if let Some(state) = self.doc_state_mut() {
            state.format_brush = Some(super::FormatBrush { run: format, para: para_fmt, persistent });
        }
        cx.notify();
    }

    pub(crate) fn apply_format_brush(&mut self, brush: &super::FormatBrush, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        if start == end {
            if let Some(p) = self.paragraph_of(start.para) {
                let (format, char_style, _) = super::editing::visible_format_at(&self.resources.markup_view, p, start.offset);
                let mut pending = self.doc_state().and_then(|s| s.pending.clone()).unwrap_or(InsertFormat { format, char_style });
                pending.format.overlay(&brush.run);
                if let Some(state) = self.doc_state_mut() { state.pending = Some(pending); }
            }
            if let Some(para) = &brush.para {
                let mut changes = Vec::new();
                if let Some(align) = para.align { changes.push(ibis_doc_model::flow_command::ParaChange::Align(align)); }
                if let Some(before) = para.before { changes.push(ibis_doc_model::flow_command::ParaChange::SpaceBefore(before)); }
                if let Some(after) = para.after { changes.push(ibis_doc_model::flow_command::ParaChange::SpaceAfter(after)); }
                if let Some(line) = para.line { changes.push(ibis_doc_model::flow_command::ParaChange::Line(line)); }
                if !changes.is_empty() {
                    let commands: Vec<_> = changes.into_iter().map(|change| {
                        FlowCommand::SetParagraphFormat { first: start.para, last: end.para, change }
                    }).collect();
                    self.run_flow(commands, cx);
                }
            }
            cx.notify();
            return true;
        }

        let Some(doc) = self.flow() else { return false };
        let (Some((container, first)), Some((last_container, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false };
        if container != last_container || first > last { return false; }
        let Some(list) = flow_command::blocks(doc, container) else { return false };
        let mut blocks = list[first..=last].to_vec();
        let styles = doc.styles.clone();
        let track = doc.track_changes;
        let who = super::comments::comment_author();
        let apply = |p: &mut ibis_doc_model::FlowParagraph, a: usize, b: usize| {
            if !track && a == 0 && b == p.len() { p.mark_format.overlay(&brush.run); }
            let after = p.split_off_runs(b);
            let mut selected = p.split_off_runs(a);
            for r in &mut selected {
                if !self.resources.markup_view.showing_markup() && matches!(r.revision.as_ref().map(|v| &v.kind), Some(RevisionKind::Delete)) { continue; }
                if track && r.revision.is_none() {
                    r.revision = Some(Revision { kind: RevisionKind::Format(r.format.clone()), author: who.author.clone(), date: who.date.clone() });
                }
                r.format.overlay(&brush.run);
            }
            p.runs.extend(selected);
            p.runs.extend(after);
            p.resolve(&styles);
            p.normalize();
            if let Some(para) = &brush.para {
                if let Some(align) = para.align { p.align = align; p.direct.align = Some(align); }
                if let Some(line) = para.line { p.line = line; p.direct.line = Some(line); }
                if let Some(before) = para.before { p.space_before = before; p.direct.before = Some(before); }
                if let Some(after) = para.after { p.space_after = after; p.direct.after = Some(after); }
                if let Some(left) = para.left { p.left = left; p.direct.left = Some(left); }
                if let Some(right) = para.right { p.right = right; p.direct.right = Some(right); }
                if let Some(first_line) = para.first_line { p.first_line = first_line; p.direct.first_line = Some(first_line); }
            }
        };
        for (i, block) in blocks.iter_mut().enumerate() {
            match block {
                Block::Paragraph(p) => {
                    let a = if i == 0 { start.offset } else { 0 };
                    let b = if first + i == last { end.offset } else { p.len() };
                    apply(p, a, b);
                }
                Block::Table(_) => super::block_tree::each_paragraph(std::slice::from_mut(block), &mut |p| {
                    let end = p.len();
                    apply(p, 0, end);
                }),
            }
        }
        self.run_flow(vec![FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks }], cx)
    }

    pub(crate) fn finish_doc_format_brush(&mut self, cx: &mut Context<Self>) {
        if !self.ordered_selection().is_some_and(|(a, b)| a != b) { return; }
        let Some(brush) = self.doc_state().and_then(|s| s.format_brush.clone()) else { return };
        let applied = self.apply_format_brush(&brush, cx);
        if applied {
            if !brush.persistent {
                if let Some(state) = self.doc_state_mut() { state.format_brush = None; }
            }
            cx.notify();
        }
    }

    pub(crate) fn copy_format(&mut self, cx: &mut Context<Self>) {
        self.capture_doc_format_brush(true, cx);
    }

    pub(crate) fn paste_format(&mut self, cx: &mut Context<Self>) {
        let Some(brush) = self.doc_state().and_then(|s| s.format_brush.clone()) else { return };
        self.apply_format_brush(&brush, cx);
    }

    pub(crate) fn cancel_doc_format_brush(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if state.format_brush.is_some() {
                state.format_brush = None;
                cx.notify();
            }
        }
    }

    pub(crate) fn set_underline_style(&mut self, style: Option<UnderlineStyle>, cx: &mut Context<Self>) {
        if self.mutate_active_shape_text(
            |body, range| {
                let (s, e) = range.unwrap_or_else(|| (0, body.text().chars().count()));
                let fmt = ibis_doc_model::CharacterFormat {
                    underline: Some(style.is_some()),
                    underline_kind: style.map(UnderlineStyle::to_drawing_underline),
                    ..Default::default()
                };
                body.format_runs(s, e, &fmt);
            },
            cx,
        ) {
            return;
        }
        if self.apply_box_edit_run_change(&ibis_doc_model::RunChange::Underline(style.is_some()), cx) {
            return;
        }
        self.apply_text_properties(RunFormat { underline: Some(style.is_some()), underline_style: style, ..Default::default() }, cx);
    }

    pub(crate) fn set_underline_color(&mut self, color: Option<ibis_doc_model::Rgb>, cx: &mut Context<Self>) {
        self.apply_text_properties(RunFormat { underline: Some(true), underline_color: Some(color), ..Default::default() }, cx);
    }
}
