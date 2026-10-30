// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A tool for the docx owner (not a check): lays out documents of a corpus as the export does and writes where each run of text is, so
//! that it can be compared with a reference PDF's. Run with `--ignored`, with the environment variables
//! `IBIS_DUMP_ROOT` (the folder of the .docx files), `IBIS_DUMP_DOCS` (names without the extension, comma-separated; all when empty)
//! and `IBIS_DUMP_OUT` (the folder to write `<name>.tsv` and `pages.tsv` to), and `IBIS_DUMP_PDF=1` (also write the export's PDF).

use super::flow_layout::Item;

#[test]
#[ignore = "a tool: needs IBIS_DUMP_ROOT and IBIS_DUMP_OUT"]
fn dump_corpus_layout() {
    let (Ok(root), Ok(out)) = (std::env::var("IBIS_DUMP_ROOT"), std::env::var("IBIS_DUMP_OUT")) else { return };
    let want: Vec<String> = std::env::var("IBIS_DUMP_DOCS").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    std::fs::create_dir_all(&out).expect("the output folder");
    let mut names: Vec<String> = std::fs::read_dir(&root).expect("the root").filter_map(|e| e.ok()).filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()).filter(|_| e.path().extension().is_some_and(|x| x == "docx"))).collect();
    names.sort();
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let mut summary = String::new();
    for name in names.iter().filter(|n| want.is_empty() || want.contains(n)) {
        let Ok(bytes) = std::fs::read(std::path::Path::new(&root).join(format!("{name}.docx"))) else { continue };
        let Ok(mut presentation) = ibis_pptx::read(&bytes) else { continue };
        let resources = cx.update(|app| {
            let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
            resources.print = true;
            resources.exact = true;
            resources.preload.wait_all();
            resources
        });
        super::flow_layout::update_fields_on_open(&mut presentation, &resources);
        let Some(flow) = presentation.flow.as_ref() else { continue };
        let pagination = super::flow_layout::pages(flow, &resources);
        summary.push_str(&format!("{name}\t{}\n", pagination.pages.len()));
        let mut tsv = String::new();
        for (p, page) in pagination.pages.iter().enumerate() {
            for item in &page.items {
                if let Item::Line { para, x, y, h, .. } = item {
                    tsv.push_str(&format!("{}	{:.2}	{:.2}	{:.2}	{:.2}	#line para {para} h {h:.2}
", p + 1, page.height, y, x, h));
                }
                if let Item::Text { x, y, text, size, offsets, .. } = item {
                    let shown: String = text.chars().take(200).collect();
                    // The column after the text: where its last glyph ends (its x plus its size), to compare the right edge with a reference PDF's.
                    let end = x + offsets.last().copied().unwrap_or(0.0) + size;
                    // And each glyph's offset from the item's x (comma separated), to compare the advances of single characters.
                    let steps: Vec<String> = offsets.iter().take(200).map(|o| format!("{o:.2}")).collect();
                    tsv.push_str(&format!("{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{:.2}\t{}\n", p + 1, page.height, y, x, size, shown, end, steps.join(",")));
                }
            }
        }
        std::fs::write(std::path::Path::new(&out).join(format!("{name}.tsv")), tsv).expect("write");
        // IBIS_DUMP_PDF=1: the PDF the export writes, to look at (`<name>.pdf` in the output folder).
        if std::env::var("IBIS_DUMP_PDF").is_ok_and(|v| v == "1") {
            let again = ibis_pptx::read(&bytes).expect("a document");
            let options = crate::export::Options {
                format: crate::export::Format::Pdf,
                out_dir: out.clone().into(),
                name: name.clone(),
                width: 1920,
                slides: Vec::new(),
                layout: false,
                file_name: Some(format!("{name}.docx")),
                print_backgrounds: false,
                print_hidden_text: false,
                print_markup: true,
                color: Default::default(),
                handout: None,
                frame: false,
                handout_landscape: false,
                notes_pages: false,
                outline_pages: false,
                notes_source: None,
                handout_marks: Default::default(),
            };
            crate::export::export(again, &options).expect("the PDF");
        }
    }
    std::fs::write(std::path::Path::new(&out).join("pages.tsv"), summary).expect("write");
}
