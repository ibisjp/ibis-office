// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188 (D3a): Word's PDF has the ideographic space U+3000 as a glyph of its text (a search and a copy find it), as it has U+0020, U+00A0,
//! U+2007 and U+202F. The screen skips a blank cluster (no ink); a PDF being written draws it. The documents are `test/editing/space-advance-<face>-<h|v>.docx`
//! (え, a space, お for each of 16 spaces).

use crate::export::{Format, Options, export};

/// The text of the PDF `file` exports to, as the PDF viewer reads it (every page).
fn pdf_text(file: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = std::fs::read(root.join("test/editing").join(file)).expect("the file");
    let presentation = ibis_formats::read(&data).expect("a document");
    let out_dir = root.join("target/scenario/pdf_blank_characters");
    std::fs::create_dir_all(&out_dir).unwrap();
    let name = std::path::Path::new(file).file_stem().unwrap().to_string_lossy().into_owned();
    let options = Options {
        format: Format::Pdf,
        out_dir: out_dir.clone(),
        name: name.clone(),
        width: 1920,
        slides: Vec::new(),
        layout: false,
        file_name: Some(file.into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        paper: None,
        fit_paper: true,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    export(presentation, &options).expect("the PDF");
    let bytes = std::fs::read(out_dir.join(format!("{name}.pdf"))).expect("the PDF");
    let mut viewer = ibis_office_pdf::PdfViewerState::open(bytes).expect("a PDF");
    viewer.select_all();
    viewer.selected_text()
}

#[test]
fn the_ideographic_space_is_a_character_of_the_pdf_in_a_line_and_in_a_column() {
    for file in ["space-advance-mspmincho-h.docx", "space-advance-mspmincho-v.docx", "space-advance-msmincho-v.docx"] {
        let text = pdf_text(file);
        assert!(text.contains('\u{3000}'), "{file}: U+3000 is in the PDF's text: {text:?}");
        assert!(text.contains('\u{202f}'), "{file}: U+202F is in the PDF's text: {text:?}");
    }
}
