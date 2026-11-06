// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old Word file (`.doc`, Word 97-2003) opens like an old workbook or presentation does (#71): `ibis_pptx::open` and
//! `ibis_pptx::read` find it by its content (a compound file with a `WordDocument` stream), not by its name, and read its text
//! from the piece table.

use ibis_doc_model::Block;
use ibis_ooxml::cfb::{self, Item};
use ibis_pptx::Kind;

/// A Word 97-2003 file as Word writes the text: the pieces of the piece table lie in the `WordDocument` stream in an order
/// that is not the text's, one part 8-bit (compressed) and one UTF-16.
fn word_file(text_pieces: &[(&str, bool)], ccp_text: usize) -> Vec<u8> {
    // The text of each piece in the WordDocument stream after the FIB (0x600 bytes), last piece first.
    let mut wd = vec![0u8; 0x600];
    let mut fcs = vec![0u32; text_pieces.len()];
    for (i, (text, compressed)) in text_pieces.iter().enumerate().rev() {
        let at = wd.len() as u32;
        if *compressed {
            wd.extend(text.bytes());
            fcs[i] = (at * 2) | 0x4000_0000;
        } else {
            wd.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
            fcs[i] = at;
        }
    }
    // The CLX: one Pcdt holding the PlcPcd.
    let mut plc = Vec::new();
    let mut cp = 0u32;
    plc.extend(cp.to_le_bytes());
    for (text, _) in text_pieces {
        cp += text.encode_utf16().count() as u32;
        plc.extend(cp.to_le_bytes());
    }
    for fc in &fcs {
        plc.extend(0u16.to_le_bytes());
        plc.extend(fc.to_le_bytes());
        plc.extend(0u16.to_le_bytes());
    }
    let mut clx = vec![0x02u8];
    clx.extend((plc.len() as u32).to_le_bytes());
    clx.extend(plc);
    // The FIB: magic, nFib, the flags (the table stream is 1Table), ccpText, fcClx and lcbClx.
    wd[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
    wd[2..4].copy_from_slice(&0x00C1u16.to_le_bytes());
    wd[10..12].copy_from_slice(&0x0200u16.to_le_bytes());
    wd[76..80].copy_from_slice(&(ccp_text as u32).to_le_bytes());
    wd[0x1A2..0x1A6].copy_from_slice(&0u32.to_le_bytes());
    wd[0x1A6..0x1AA].copy_from_slice(&(clx.len() as u32).to_le_bytes());
    cfb::write(&[Item::stream("WordDocument", wd), Item::stream("1Table", clx)])
}

fn paragraph_texts(presentation: &ibis_doc_model::Presentation) -> Vec<String> {
    let flow = presentation.flow.as_ref().expect("a document");
    flow.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect()
}

#[test]
fn doc_extension_is_a_document() {
    assert_eq!(Kind::from_extension("doc"), Some(Kind::Document));
    assert_eq!(Kind::from_extension("DOC"), Some(Kind::Document));
    assert!(Kind::OPENABLE_EXTENSIONS.contains(&"doc"));
}

#[test]
fn dot_extension_is_a_document_and_opens_like_a_doc() {
    assert_eq!(Kind::from_extension("dot"), Some(Kind::Document));
    assert_eq!(Kind::from_extension("DOT"), Some(Kind::Document));
    assert!(Kind::OPENABLE_EXTENSIONS.contains(&"dot"));
    // A .dot has the same compound-file layout as a .doc; it is found by its content, not its name.
    let text = "Template text\r";
    let bytes = word_file(&[(text, true)], text.encode_utf16().count());
    let opened = ibis_pptx::open(bytes).expect("the .dot opens");
    assert_eq!(paragraph_texts(&opened.presentation), vec!["Template text"]);
}

#[test]
fn old_word_file_opens_by_content_with_pieces_in_any_order() {
    let pieces = [("Hello old Word\rSecond ", true), ("日本語の段落\r", false)];
    let total: usize = pieces.iter().map(|(t, _)| t.encode_utf16().count()).sum();
    let bytes = word_file(&pieces, total);

    // Not taken for an old workbook (a compound file too), and opened as a document.
    let opened = ibis_pptx::open(bytes.clone()).expect("the .doc opens");
    assert_eq!(Kind::of(&opened.presentation), Kind::Document);
    assert_eq!(paragraph_texts(&opened.presentation), vec!["Hello old Word", "Second 日本語の段落"]);

    let read = ibis_pptx::read(&bytes).expect("the .doc is read");
    assert_eq!(paragraph_texts(&read), vec!["Hello old Word", "Second 日本語の段落"]);
}

#[test]
fn old_word_file_drops_field_codes_and_text_after_the_main_story() {
    // A hyperlink field (instruction 0x13 .. 0x14, result .. 0x15), then a footnote story after ccpText.
    let main = "See \u{13} HYPERLINK \"http://example.com\" \u{14}the site\u{15} now\r";
    let rest = "footnote text\r";
    let pieces = [(main, true), (rest, true)];
    let ccp = main.encode_utf16().count();
    let bytes = word_file(&pieces, ccp);
    let opened = ibis_pptx::open(bytes).expect("the .doc opens");
    assert_eq!(paragraph_texts(&opened.presentation), vec!["See the site now"]);
}

#[test]
fn old_word_file_table_rows_end_at_the_row_mark() {
    // Two rows of two cells: each cell ends with 0x07 and each row with one more 0x07; a paragraph follows the table.
    let main = "a\u{7}b\u{7}\u{7}c\u{7}d\u{7}\u{7}after\r";
    let total = main.encode_utf16().count();
    let bytes = word_file(&[(main, true)], total);
    let opened = ibis_pptx::open(bytes).expect("the .doc opens");
    let flow = opened.presentation.flow.as_ref().unwrap();
    let Block::Table(table) = &flow.body[0] else { panic!("a table first") };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert!(matches!(&flow.body[1], Block::Paragraph(p) if p.text() == "after"));
}
