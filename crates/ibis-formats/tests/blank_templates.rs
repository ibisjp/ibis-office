// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The empty files ibisWorks hands out for 新規作成 (K0037): the files ibisOffice's 新規作成 writes, opened and saved the way a
//! user's save does. `scripts/make-blank-templates.sh` sets `IBIS_BLANK_TEMPLATES_OUT` to have them written to a folder (then scrubs them).

use ibis_formats::{Kind, SaveOptions};

fn saved(kind: Kind) -> Vec<u8> {
    let opened = ibis_formats::open(ibis_formats::blank_file(kind).unwrap()).unwrap();
    opened.source.save(&opened.presentation, SaveOptions::default()).unwrap()
}

#[test]
fn a_saved_new_file_opens_again_and_is_written_for_the_templates() {
    for (kind, name) in [(Kind::Document, "blank.docx"), (Kind::Workbook, "blank.xlsx"), (Kind::Presentation, "blank.pptx")] {
        let bytes = saved(kind);
        let again = ibis_formats::open(bytes.clone()).unwrap_or_else(|e| panic!("{name} does not open again: {e:?}"));
        // Saved again, it still opens.
        let twice = again.source.save(&again.presentation, SaveOptions::default()).unwrap();
        ibis_formats::open(twice).unwrap_or_else(|e| panic!("{name} does not open after a second save: {e:?}"));
        assert!(bytes.len() < 50_000, "{name} is {} bytes", bytes.len());
        if let Ok(dir) = std::env::var("IBIS_BLANK_TEMPLATES_OUT") {
            std::fs::write(std::path::Path::new(&dir).join(name), &bytes).unwrap();
        }
    }
}
