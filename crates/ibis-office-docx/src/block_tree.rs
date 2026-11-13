// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paragraph traversal inside whole selected blocks, including nested tables.
use ibis_doc_model::Block;
pub(super) fn each_paragraph(blocks:&mut [Block],update:&mut impl FnMut(&mut ibis_doc_model::FlowParagraph)) {
    for block in blocks {match block {
        Block::Paragraph(p)=>update(p),
        Block::Table(t)=>for row in &mut t.rows {for cell in &mut row.cells {each_paragraph(&mut cell.blocks,update);}},
    }}
}
pub(super) fn paragraphs(blocks:&[Block])->Vec<&ibis_doc_model::FlowParagraph> {
    let mut result=Vec::new();
    for block in blocks {match block {
        Block::Paragraph(p)=>result.push(p),
        Block::Table(t)=>for row in &t.rows {for cell in &row.cells {result.extend(paragraphs(&cell.blocks));}},
    }}
    result
}

