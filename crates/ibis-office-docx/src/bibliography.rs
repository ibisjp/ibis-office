// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 文献目録と引用文献 (Word's CITATION and BIBLIOGRAPHY fields):
//! 文書内の CITATION フィールド（引用文献）を収集し、文献目録（BIBLIOGRAPHY フィールド）の段落一覧を
//! 生成・更新する（タグ順ソート、見出し、ぶら下げインデント）。

use ibis_doc_model::{Block, FlowCommand, FlowDocument, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle, RunFormat};

/// What a CITATION field says: `CITATION Tag \l 1041`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CitationSpec {
    pub(crate) tag: String,
    pub(crate) style: Option<String>,
    pub(crate) locale: Option<u32>,
    pub(crate) page: Option<String>,
}

/// Parse a `CITATION <tag> [\s <style>] [\l <locale>] [\p <pages>]` field instruction.
pub(crate) fn parse_citation(instruction: &str) -> Option<CitationSpec> {
    let words = super::index_fields::words(instruction);
    if !words.first()?.eq_ignore_ascii_case("CITATION") {
        return None;
    }
    let tag = words.get(1)?.clone();
    if tag.trim().is_empty() {
        return None;
    }
    let style = super::index_fields::switch_value(&words, "\\s");
    let locale = super::index_fields::switch_value(&words, "\\l").and_then(|v| v.parse().ok());
    let page = super::index_fields::switch_value(&words, "\\p");
    Some(CitationSpec { tag, style, locale, page })
}

/// Collect all CITATION fields from the document.
pub(crate) fn collect_citations(doc: &FlowDocument) -> Vec<CitationSpec> {
    let mut citations = Vec::new();
    fn walk_blocks(blocks: &[Block], citations: &mut Vec<CitationSpec>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction, .. } = &r.kind {
                            if let Some(spec) = parse_citation(instruction) {
                                citations.push(spec);
                            }
                        }
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk_blocks(&cell.blocks, citations);
                        }
                    }
                }
            }
        }
    }
    walk_blocks(&doc.body, &mut citations);
    citations
}

/// The first paragraph of the document's BIBLIOGRAPHY field and the field's instruction.
pub(crate) fn bibliography_field(doc: &FlowDocument) -> Option<(u32, String)> {
    doc.body.iter().find_map(|b| match b {
        Block::Paragraph(p) => p.field_start.as_ref().filter(|i| {
            let u = i.trim_start().to_ascii_uppercase();
            u.starts_with("BIBLIOGRAPHY")
        }).map(|i| (p.id, i.clone())),
        _ => None,
    })
}

/// The paragraphs of the BIBLIOGRAPHY field:
/// 1. Heading paragraph ("文献目録" / "Bibliography") with field_start
/// 2. Entry paragraphs for each unique citation tag (sorted alphabetically)
/// 3. The last paragraph has field_end = true.
pub(crate) fn bibliography_paragraphs(
    doc: &FlowDocument,
    instruction: &str,
    mut next_id: u32,
) -> Vec<FlowParagraph> {
    let citations = collect_citations(doc);
    let mut unique_tags: Vec<String> = citations.into_iter().map(|c| c.tag).collect();
    unique_tags.sort_by(|a, b| super::index_fields::sort_key(a).cmp(&super::index_fields::sort_key(b)));
    unique_tags.dedup();

    let styles = &doc.styles;
    let heading_style_name = styles.paragraph_style_named("bibliography heading")
        .or_else(|| styles.paragraph_style_named("heading 1"))
        .or_else(|| styles.paragraph_style_named("見出し 1")); // i18n-ok（文書の中の名前・全角の区切り・記号）
    let entry_style_name = styles.paragraph_style_named("bibliography");

    let mut paragraphs = Vec::new();

    // 1. Heading paragraph
    let mut heading_para = FlowParagraph {
        id: next_id,
        style: heading_style_name.map(str::to_string),
        ..FlowParagraph::default()
    };
    next_id += 1;
    let heading_text = crate::shell::i18n::text("docx.reference.bibliography.heading");
    let mut heading_run_style = FlowRunStyle::default();
    heading_run_style.bold = true;
    heading_para.runs.push(FlowRun::new(FlowRunKind::Text(heading_text.to_string()), heading_run_style));
    heading_para.field_start = Some(instruction.trim().to_string());
    heading_para.resolve(styles);
    paragraphs.push(heading_para);

    // 2. Entries
    if unique_tags.is_empty() {
        let mut empty_para = FlowParagraph {
            id: next_id,
            style: entry_style_name.map(str::to_string),
            field_end: true,
            ..FlowParagraph::default()
        };
        let msg = crate::shell::i18n::text("docx.reference.bibliography.no-citations");
        empty_para.runs.push(FlowRun::new(FlowRunKind::Text(msg.to_string()), FlowRunStyle::default()));
        empty_para.resolve(styles);
        paragraphs.push(empty_para);
    } else {
        let count = unique_tags.len();
        for (i, tag) in unique_tags.into_iter().enumerate() {
            let is_last = i + 1 == count;
            let mut p = FlowParagraph {
                id: next_id,
                style: entry_style_name.map(str::to_string),
                field_end: is_last,
                ..FlowParagraph::default()
            };
            next_id += 1;
            if let Some(src) = super::source_manager::find_source_in_doc(doc, &tag).filter(|s| !s.author.is_empty() || !s.title.is_empty()) {
                p.runs.push(FlowRun::new(FlowRunKind::Text(src.bibliography_preview()), FlowRunStyle::default()));
            } else {
                // Label e.g. "[Tag] "
                let label = format!("[{tag}] ");
                let mut label_style = FlowRunStyle::default();
                label_style.bold = true;
                p.runs.push(FlowRun::new(FlowRunKind::Text(label), label_style));
                // Entry text
                p.runs.push(FlowRun::new(FlowRunKind::Text(tag), FlowRunStyle::default()));
            }
            p.resolve(styles);
            paragraphs.push(p);
        }
    }

    paragraphs
}

/// Missing bibliography styles (`bibliography`, `bibliography heading`) defined as Word defaults.
pub(crate) fn missing_bibliography_styles(styles: &ibis_doc_model::FlowStyles) -> Vec<(String, ibis_doc_model::FlowStyle)> {
    use ibis_doc_model::{FlowStyle, ParaFormat, StyleKind};
    let normal = styles.default_paragraph.clone();
    let mut out = Vec::new();
    let mut taken: std::collections::HashSet<String> = styles.styles.keys().cloned().collect();
    let mut define = |name: &str, id: &str, para: ParaFormat, run: RunFormat, next: Option<String>| {
        if styles.paragraph_style_named(name).is_some() {
            return;
        }
        let mut id = id.to_string();
        while taken.contains(&id) {
            id.push('_');
        }
        taken.insert(id.clone());
        out.push((id, FlowStyle { kind: StyleKind::Paragraph, name: name.to_string(), based_on: normal.clone(), next: next.or_else(|| normal.clone()), para, run, ..FlowStyle::default() }));
    };
    let step = 420 * 635; // 0.74cm / 420 twips hanging indent
    define("bibliography", "Bibliography", ParaFormat { left: Some(step), first_line: Some(-step), ..ParaFormat::default() }, RunFormat::default(), None);
    define("bibliography heading", "BibliographyHeading", ParaFormat { before: Some(180 * 635), after: Some(60 * 635), ..ParaFormat::default() }, RunFormat { bold: Some(true), ..RunFormat::default() }, styles.paragraph_style_named("bibliography").map(str::to_string));
    out
}

/// The commands that make the bibliography field (or put a new one before paragraph `before`).
pub(crate) fn bibliography_commands(
    doc: &FlowDocument,
    before: u32,
    replace: bool,
    instruction: &str,
) -> Vec<FlowCommand> {
    let mut commands: Vec<FlowCommand> = missing_bibliography_styles(&doc.styles).into_iter().map(|(id, style)| FlowCommand::SetStyle { id, style: Some(style) }).collect();
    let mut with_styles = doc.clone();
    {
        let styles = std::sync::Arc::make_mut(&mut with_styles.styles);
        for c in &commands {
            if let FlowCommand::SetStyle { id, style: Some(style) } = c {
                styles.styles.insert(id.clone(), style.clone());
            }
        }
    }
    let entries = bibliography_paragraphs(&with_styles, instruction, doc.new_id());
    commands.push(FlowCommand::SetToc { before, replace, entries, bookmarks: Vec::new() });
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_citation_switches() {
        let cit = parse_citation("CITATION Smith2020 \\l 1041 \\p 42").unwrap();
        assert_eq!((cit.tag.as_str(), cit.locale, cit.page.as_deref()), ("Smith2020", Some(1041), Some("42")));
        assert!(parse_citation("TOC \\o").is_none());
    }
}
