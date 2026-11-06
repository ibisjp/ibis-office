// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 引用文献目録 (Word's TA and TOA fields): the citations the TA fields of a document mark, and the TOA field's paragraphs made of
//! them: grouped by category (`\c`), sorted by the long citation, each with the pages it is cited on (or "passim" with `\p`).

use ibis_doc_model::{Block, FlowDocument, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle, RunFormat};

use super::index_fields::{sort_key, switch_value, words};

/// What a TA field says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TaSpec {
    /// `\l`: the citation as the table lists it.
    pub(crate) long: String,
    /// `\s`: the short form (kept; the table lists the long one).
    pub(crate) short: Option<String>,
    /// `\c`: the category (1 to 16).
    pub(crate) category: u32,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// `\r`: the bookmark whose range the citation covers.
    pub(crate) range: Option<String>,
}

/// The citation a `TA \l "long" \s "short" \c 1 \b \i \r "range"` field marks.
pub(crate) fn ta_spec(instruction: &str) -> Option<TaSpec> {
    let words = words(instruction);
    if !words.first()?.eq_ignore_ascii_case("TA") {
        return None;
    }
    let long = switch_value(&words, "\\l")?;
    if long.trim().is_empty() {
        return None;
    }
    Some(TaSpec {
        long,
        short: switch_value(&words, "\\s"),
        category: switch_value(&words, "\\c").and_then(|v| v.parse().ok()).filter(|c| (1..=16).contains(c)).unwrap_or(1),
        bold: words.iter().any(|w| w.eq_ignore_ascii_case("\\b")),
        italic: words.iter().any(|w| w.eq_ignore_ascii_case("\\i")),
        range: switch_value(&words, "\\r"),
    })
}

/// A TOA field's switches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToaSpec {
    /// `\c`: the category the table lists (0: all of them).
    pub(crate) category: u32,
    /// `\h`: a heading above each category.
    pub(crate) headings: bool,
    /// `\p`: "passim" for a citation on more than five pages.
    pub(crate) passim: bool,
    /// `\l`: between page numbers.
    pub(crate) page_separator: String,
    /// `\g`: between the first and the last page of a range.
    pub(crate) range_separator: String,
    /// `\e`: between a citation and its pages (the default is a tab with dots).
    pub(crate) entry_separator: Option<String>,
}

pub(crate) fn toa_spec(instruction: &str) -> Option<ToaSpec> {
    let words = words(instruction);
    if !words.first()?.eq_ignore_ascii_case("TOA") {
        return None;
    }
    Some(ToaSpec {
        category: switch_value(&words, "\\c").and_then(|v| v.parse().ok()).unwrap_or(0),
        headings: words.iter().any(|w| w.eq_ignore_ascii_case("\\h")),
        passim: words.iter().any(|w| w.eq_ignore_ascii_case("\\p")),
        page_separator: switch_value(&words, "\\l").unwrap_or_else(|| ", ".to_string()),
        range_separator: switch_value(&words, "\\g").unwrap_or_else(|| "\u{2013}".to_string()),
        entry_separator: switch_value(&words, "\\e"),
    })
}

/// The name of category `n`, as the heading of its part of the table.
pub(crate) fn category_name(n: u32) -> String {
    match n {
        1..=6 => crate::shell::i18n::text(&format!("_docx.toa-category-{n}")).to_string(),
        n => format!("{} {n}", crate::shell::i18n::text("_docx.toa-category-custom")),
    }
}

#[derive(Clone, Debug)]
struct Mark {
    spec: TaSpec,
    page: u32,
    end_page: Option<u32>,
}

fn marks(doc: &FlowDocument, page_of: &dyn Fn(u32) -> Option<u32>) -> Vec<Mark> {
    fn ends(blocks: &[Block], out: &mut Vec<(String, u32)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.extend(p.bookmark_ends.iter().map(|n| (n.clone(), p.id))),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| ends(&c.blocks, out)),
            }
        }
    }
    fn walk(blocks: &[Block], out: &mut Vec<(u32, TaSpec)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction, .. } = &r.kind
                            && let Some(spec) = ta_spec(instruction)
                        {
                            out.push((p.id, spec));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut bookmark_ends = Vec::new();
    ends(&doc.body, &mut bookmark_ends);
    let mut found = Vec::new();
    walk(&doc.body, &mut found);
    found
        .into_iter()
        .map(|(para, spec)| {
            let page = page_of(para).unwrap_or(1);
            let end_page = spec.range.as_ref().and_then(|name| bookmark_ends.iter().find(|(n, _)| n == name)).and_then(|(_, p)| page_of(*p)).filter(|e| *e > page);
            Mark { spec, page, end_page }
        })
        .collect()
}

/// The paragraphs of the TOA field `instruction`: for each category, an optional heading (`toa heading`) and the citations in
/// order (`table of authorities`) with their pages. The ids start at `first_id`; `width` is the text's width (the right tab stop).
pub(crate) fn toa_paragraphs(doc: &FlowDocument, instruction: &str, page_of: &dyn Fn(u32) -> Option<u32>, first_id: u32, width: i64) -> Vec<FlowParagraph> {
    let Some(spec) = toa_spec(instruction) else { return Vec::new() };
    let marks: Vec<Mark> = marks(doc, page_of).into_iter().filter(|m| spec.category == 0 || m.spec.category == spec.category).collect();
    let styles = &doc.styles;
    let base = FlowRunStyle::default();
    let tab = ibis_doc_model::TabChange { position: width, stop: Some(ibis_doc_model::TabStop { position: width, align: ibis_doc_model::TabAlign::Right, leader: ibis_doc_model::TabLeader::Dot }) };
    let mut out: Vec<FlowParagraph> = Vec::new();
    let mut id = first_id;
    for category in 1..=16u32 {
        let mut citations: Vec<(String, Vec<((u32, Option<u32>), bool, bool)>)> = Vec::new();
        for m in marks.iter().filter(|m| m.spec.category == category) {
            let page = ((m.page, m.end_page), m.spec.bold, m.spec.italic);
            match citations.iter_mut().find(|(long, _)| *long == m.spec.long) {
                Some((_, pages)) => pages.push(page),
                None => citations.push((m.spec.long.clone(), vec![page])),
            }
        }
        if citations.is_empty() {
            continue;
        }
        citations.sort_by(|(a, _), (b, _)| {
            let (ka, kb) = (sort_key(a), sort_key(b));
            ka.0.cmp(&kb.0).then_with(|| ka.1.cmp(&kb.1))
        });
        if spec.headings {
            let mut p = FlowParagraph { id, style: styles.paragraph_style_named("toa heading").map(str::to_string), ..FlowParagraph::default() };
            id += 1;
            p.runs.push(FlowRun::new(FlowRunKind::Text(category_name(category)), base.clone()));
            p.resolve(styles);
            out.push(p);
        }
        for (long, mut pages) in citations {
            pages.sort_by_key(|(p, ..)| *p);
            let mut merged: Vec<((u32, Option<u32>), bool, bool)> = Vec::new();
            for (page, bold, italic) in pages {
                match merged.last_mut() {
                    Some(last) if last.0 == page => {
                        last.1 |= bold;
                        last.2 |= italic;
                    }
                    _ => merged.push((page, bold, italic)),
                }
            }
            let mut p = FlowParagraph { id, style: styles.paragraph_style_named("table of authorities").map(str::to_string), ..FlowParagraph::default() };
            id += 1;
            p.direct.tabs = vec![tab.clone()];
            p.runs.push(FlowRun::new(FlowRunKind::Text(long), base.clone()));
            match &spec.entry_separator {
                Some(sep) => p.runs.push(FlowRun::new(FlowRunKind::Text(sep.clone()), base.clone())),
                None => p.runs.push(FlowRun::new(FlowRunKind::Tab, base.clone())),
            }
            let run = |p: &mut FlowParagraph, text: String, bold: bool, italic: bool| {
                let mut r = FlowRun::new(FlowRunKind::Text(text), base.clone());
                r.format = RunFormat { bold: bold.then_some(true), italic: italic.then_some(true), ..RunFormat::default() };
                p.runs.push(r);
            };
            if spec.passim && merged.len() > 4 {
                run(&mut p, crate::shell::i18n::text("_docx.toa-passim").to_string(), false, false);
            } else {
                let count = merged.len();
                for (n, ((first, last), bold, italic)) in merged.into_iter().enumerate() {
                    let label = match last {
                        Some(last) => format!("{first}{}{last}", spec.range_separator),
                        None => first.to_string(),
                    };
                    run(&mut p, label, bold, italic);
                    if n + 1 < count {
                        run(&mut p, spec.page_separator.clone(), false, false);
                    }
                }
            }
            p.resolve(styles);
            out.push(p);
        }
    }
    if let Some(first) = out.first_mut() {
        first.field_start = Some(instruction.trim().to_string());
    }
    if let Some(last) = out.last_mut() {
        last.field_end = true;
    }
    out
}

/// The first paragraph of the document's TOA field and the field's instruction.
pub(crate) fn toa_field(doc: &FlowDocument) -> Option<(u32, String)> {
    doc.body.iter().find_map(|b| match b {
        Block::Paragraph(p) => p.field_start.as_ref().filter(|i| i.trim_start().to_ascii_uppercase().starts_with("TOA")).map(|i| (p.id, i.clone())),
        _ => None,
    })
}

/// The commands that make the table of authorities again (or put a new one before paragraph `before`): the styles it uses first,
/// then its paragraphs in place of the old field's.
pub(crate) fn toa_commands(doc: &FlowDocument, before: u32, replace: bool, instruction: &str, page_of: &dyn Fn(u32) -> Option<u32>) -> Vec<ibis_doc_model::FlowCommand> {
    let mut commands: Vec<ibis_doc_model::FlowCommand> = missing_toa_styles(&doc.styles).into_iter().map(|(id, style)| ibis_doc_model::FlowCommand::SetStyle { id, style: Some(style) }).collect();
    let mut with_styles = doc.clone();
    {
        let styles = std::sync::Arc::make_mut(&mut with_styles.styles);
        for c in &commands {
            if let ibis_doc_model::FlowCommand::SetStyle { id, style: Some(style) } = c {
                styles.styles.insert(id.clone(), style.clone());
            }
        }
    }
    let width = doc.page.size.width - doc.page.left - doc.page.right;
    let mut entries = toa_paragraphs(&with_styles, instruction, page_of, doc.new_id(), width);
    if entries.is_empty() {
        let mut p = FlowParagraph { id: doc.new_id(), ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text(crate::shell::i18n::text("_docx.no-toa-entries-found").into()), FlowRunStyle::default()));
        p.field_start = Some(instruction.trim().to_string());
        p.field_end = true;
        p.resolve(&with_styles.styles);
        entries.push(p);
    }
    commands.push(ibis_doc_model::FlowCommand::SetToc { before, replace, entries, bookmarks: Vec::new() });
    commands
}

/// The styles a table of authorities uses (`table of authorities`, `toa heading`) that the document lacks, as Word defines them.
pub(crate) fn missing_toa_styles(styles: &ibis_doc_model::FlowStyles) -> Vec<(String, ibis_doc_model::FlowStyle)> {
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
    let step = 210 * 635;
    define("table of authorities", "TableofAuthorities", ParaFormat { left: Some(step), first_line: Some(-step), ..ParaFormat::default() }, RunFormat::default(), None);
    define("toa heading", "TOAHeading", ParaFormat { before: Some(120 * 635), ..ParaFormat::default() }, RunFormat { bold: Some(true), ..RunFormat::default() }, styles.paragraph_style_named("table of authorities").map(str::to_string));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ta_and_toa_switches() {
        let ta = ta_spec("TA \\l \"最高裁判所 昭和 50 年\" \\s \"最判昭50\" \\c 1 \\b").unwrap();
        assert_eq!((ta.long.as_str(), ta.short.as_deref(), ta.category, ta.bold, ta.italic), ("最高裁判所 昭和 50 年", Some("最判昭50"), 1, true, false));
        assert!(ta_spec("TC \"x\"").is_none() && ta_spec("TA \\s \"x\"").is_none());
        let toa = toa_spec("TOA \\h \\c \"2\" \\p \\e \" … \"").unwrap();
        assert_eq!((toa.category, toa.headings, toa.passim, toa.entry_separator.as_deref()), (2, true, true, Some(" … ")));
        assert_eq!(toa_spec("TOA").unwrap().category, 0);
    }
}
