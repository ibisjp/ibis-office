// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The colour under the caret in a document: what is really drawn where the caret is, so the caret can be told apart from it (GitHub #245).
//! From the top: the highlight of the text the caret is in (the run before it, or the first run at the start of the paragraph), the paragraph's shading, the shading of the table cell (the inner one first) the paragraph is in, then the paper: the dark paper
//! of 表示 > ダークモード（ページ） when it is on, else the page's colour, else white (the same choice `page_view` paints with); a fill is the colour
//! `page_view` draws it in (on the dark page a very dark fill is drawn light).
use ibis_doc_model::{Block, FlowCell, FlowDocument, FlowParagraph, Rgb};

/// The paper 表示 > ダークモード（ページ） draws (`page_view`).
pub(crate) const DARK_PAPER: Rgb = Rgb(0x262626);

/// The colour of the paper: the dark paper when the dark page is on, else the page's colour, else white.
pub(crate) fn paper(page_color: Option<Rgb>, dark_page: bool) -> Rgb {
    if dark_page { DARK_PAPER } else { page_color.unwrap_or(Rgb::WHITE) }
}

fn paragraph_fill(p: &FlowParagraph) -> Option<Rgb> {
    if p.shading_pattern.is_some() { p.shading_fill } else { p.shading }
}

fn cell_fill(c: &FlowCell) -> Option<Rgb> {
    if c.shading_pattern.is_some() { c.shading_fill } else { c.shading.or(c.style_fill) }
}

/// The fills of the cells around the paragraph `id` in `blocks` (innermost first), and the paragraph's own; `None` if it is not here.
fn find(blocks: &[Block], id: u32, out: &mut Vec<Option<Rgb>>) -> bool {
    for block in blocks {
        match block {
            Block::Paragraph(p) if p.id == id => {
                out.push(paragraph_fill(p));
                return true;
            }
            Block::Paragraph(_) => {}
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        if find(&cell.blocks, id, out) {
                            out.push(cell_fill(cell));
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Every list of blocks of the document a paragraph can be in: the body, the headers and footers, the notes.
fn containers(doc: &FlowDocument) -> impl Iterator<Item = &[Block]> {
    [&doc.body, &doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even]
        .into_iter()
        .map(|b| b.as_slice())
        .chain(doc.footnotes.iter().chain(doc.endnotes.iter()).map(|n| n.blocks.as_slice()))
}

/// The highlight of the text at `offset` in `p`: of the run before the caret, or, at the start, of the run after it.
fn highlight_at(p: &FlowParagraph, offset: usize) -> Option<Rgb> {
    let mut start = 0;
    let mut first = None;
    for (i, run) in p.runs.iter().enumerate() {
        let end = start + run.len();
        if i == 0 {
            first = run.style.highlight;
        }
        if offset > start && offset <= end {
            return run.style.highlight;
        }
        start = end;
    }
    if offset == 0 { first } else { None }
}

/// The colour under the caret at `offset` of paragraph `para` of `doc`.
pub(crate) fn under_caret(doc: &FlowDocument, para: u32, offset: usize, dark_page: bool) -> Rgb {
    let mut fills = Vec::new();
    if containers(doc).any(|blocks| {
        fills.clear();
        find(blocks, para, &mut fills)
    }) {
        // The highlight is drawn over the paragraph's and the cell's shading.
        let highlight = ibis_doc_model::flow_command::paragraph(doc, para).and_then(|p| highlight_at(p, offset));
        if let Some(fill) = highlight.into_iter().chain(fills.into_iter().flatten()).next() {
            return crate::page_view::fill_as_drawn(fill, dark_page);
        }
    }
    paper(doc.page_color, dark_page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paper_is_the_dark_one_when_the_dark_page_is_on() {
        assert_eq!(paper(None, false), Rgb::WHITE);
        assert_eq!(paper(Some(Rgb(0xffffcc)), false), Rgb(0xffffcc));
        assert_eq!(paper(Some(Rgb(0xffffcc)), true), DARK_PAPER);
        assert_eq!(paper(None, true), DARK_PAPER);
    }

    fn paragraph(id: u32, runs: &[(&str, Option<Rgb>)], shading: Option<Rgb>) -> FlowParagraph {
        let mut p = FlowParagraph { id, shading, ..Default::default() };
        for (text, highlight) in runs {
            let mut style = ibis_doc_model::FlowRunStyle::default();
            style.highlight = *highlight;
            p.runs.push(ibis_doc_model::FlowRun::new(ibis_doc_model::FlowRunKind::Text(text.to_string()), style));
        }
        p
    }

    #[test]
    fn the_colour_under_the_caret_reads_the_highlight_the_header_and_the_notes() {
        let dark = Rgb(0x000080);
        let mut doc = FlowDocument::default();
        doc.body.push(Block::Paragraph(paragraph(1, &[("ab", None), ("cd", Some(dark))], None)));
        doc.header.push(Block::Paragraph(paragraph(2, &[("h", None)], Some(dark))));
        doc.footnotes.push(ibis_doc_model::Note { id: 1, blocks: vec![Block::Paragraph(paragraph(3, &[("n", None)], Some(dark)))], custom_mark: false });
        // After "abc" the caret is in the highlighted run; before "c" (after "ab") it is in the plain one.
        assert_eq!(under_caret(&doc, 1, 3, false), dark);
        assert_eq!(under_caret(&doc, 1, 2, false), Rgb::WHITE);
        assert_eq!(under_caret(&doc, 1, 0, false), Rgb::WHITE);
        // The header's and the footnote's own shading (they are not in the body).
        assert_eq!(under_caret(&doc, 2, 1, false), dark);
        assert_eq!(under_caret(&doc, 3, 1, false), dark);
        // A highlight is drawn over the paragraph's shading.
        doc.body.push(Block::Paragraph(paragraph(4, &[("x", Some(Rgb(0xffff00)))], Some(dark))));
        assert_eq!(under_caret(&doc, 4, 1, false), Rgb(0xffff00));
    }
}
