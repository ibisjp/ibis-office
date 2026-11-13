// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換 on a document (the panel is the shell's, `shell/replace_fonts.rs`): the fonts of the text runs and
//! the commands that replace one of them.

use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind, RunChange};

use crate::shell::replace_fonts::same_font;

/// The typefaces the document's text runs are drawn with (Latin and East Asian), in order, without repeats.
pub(crate) fn document_used_fonts(doc: &ibis_doc_model::FlowDocument) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    for p in ibis_doc_model::all_paragraphs(doc) {
        for r in &p.runs {
            if !matches!(r.kind, FlowRunKind::Text(_)) {
                continue;
            }
            for f in [&r.style.font, &r.style.east_asian_font].into_iter().flatten() {
                let f = f.trim();
                if !f.is_empty() {
                    fonts.insert(f.to_string());
                }
            }
        }
    }
    fonts.into_iter().collect()
}

/// The commands that set `replacement` on the text runs of `doc` drawn in `target`, and how many runs they change.
pub(crate) fn document_replace_commands(doc: &ibis_doc_model::FlowDocument, target: &str, replacement: &str) -> (Vec<FlowCommand>, usize) {
    let mut commands = Vec::new();
    let mut count = 0;
    if target.trim().is_empty() || replacement.trim().is_empty() || same_font(target, replacement) {
        return (commands, count);
    }
    for p in ibis_doc_model::all_paragraphs(doc) {
        let mut offset = 0;
        for r in &p.runs {
            let len = r.len();
            if matches!(r.kind, FlowRunKind::Text(_)) && len > 0 {
                let (from, to) = (FlowPos::new(p.id, offset), FlowPos::new(p.id, offset + len));
                let mut hit = false;
                if r.style.font.as_deref().is_some_and(|f| same_font(f, target)) {
                    commands.push(FlowCommand::SetRunFormat { from, to, change: RunChange::Font(replacement.trim().to_string()) });
                    hit = true;
                }
                if r.style.east_asian_font.as_deref().is_some_and(|f| same_font(f, target)) {
                    commands.push(FlowCommand::SetRunFormat { from, to, change: RunChange::EastAsianFont(replacement.trim().to_string()) });
                    hit = true;
                }
                count += usize::from(hit);
            }
            offset += len;
        }
    }
    (commands, count)
}
