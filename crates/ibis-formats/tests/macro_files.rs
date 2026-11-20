// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Files with macros (.docm, .xlsm, .pptm) open and save as the files
//! without them do: the macros (vbaProject.bin) and the main part's
//! macro-enabled content type stay as they are; nothing runs them.

use ibis_doc_model::{Block, Command, Document, FlowCommand, FlowPos};

const VBA: &[u8] = b"not a real VBA project: the bytes must come back as they are";

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

/// `file` with macros: its main part's content type made the
/// macro-enabled one and a vbaProject.bin related to it.
fn with_macros(file: &str, main: &str, content_type: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path(file)).unwrap()).unwrap();
    package.set_override(main, content_type).unwrap();
    let dir = main.rsplit_once('/').unwrap().0;
    let vba = format!("{dir}/vbaProject.bin");
    package.set_bytes(&vba, VBA);
    package.set_override(&vba, "application/vnd.ms-office.vbaProject").unwrap();
    package.add_relationship(main, "http://schemas.microsoft.com/office/2006/relationships/vbaProject", "vbaProject.bin", false).unwrap();
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

fn part(data: &[u8], name: &str) -> Vec<u8> {
    ibis_ooxml::Package::open(data.to_vec()).unwrap().read(name).unwrap()
}

fn kept(data: &[u8], saved: &[u8], main: &str, content_type: &str) {
    let dir = main.rsplit_once('/').unwrap().0;
    assert_eq!(part(saved, &format!("{dir}/vbaProject.bin")), VBA);
    let types = String::from_utf8(part(saved, "[Content_Types].xml")).unwrap();
    assert!(types.contains(content_type), "{types}");
    assert!(types.contains("application/vnd.ms-office.vbaProject"));
    let rels = format!("{dir}/_rels/{}.rels", main.rsplit_once('/').unwrap().1);
    assert_eq!(part(saved, &rels), part(data, &rels));
}

#[test]
fn macro_files_open_and_save_unchanged() {
    for (file, main, content_type, kind) in [
        ("editing/docx-report.docx", "word/document.xml", "application/vnd.ms-word.document.macroEnabled.main+xml", ibis_formats::Kind::Document),
        ("editing/xlsx-sales.xlsx", "xl/workbook.xml", "application/vnd.ms-excel.sheet.macroEnabled.main+xml", ibis_formats::Kind::Workbook),
        ("editing/pptx-plan.pptx", "ppt/presentation.xml", "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml", ibis_formats::Kind::Presentation),
    ] {
        let data = with_macros(file, main, content_type);
        let opened = ibis_formats::open(data.clone()).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(opened.source.kind(), kind, "{file}");
        let saved = opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap();
        assert_eq!(saved, data, "{file}: saved unedited, the same file");
        kept(&data, &saved, main, content_type);
    }
}

#[test]
fn edited_docm_keeps_its_macros() {
    let (main, content_type) = ("word/document.xml", "application/vnd.ms-word.document.macroEnabled.main+xml");
    let data = with_macros("editing/docx-report.docx", main, content_type);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let flow = opened.presentation.flow.as_deref().unwrap();
    let id = flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b && p.len() > 3 { Some(p.id) } else { None }).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(id, 1), text: "マクロ".into(), format: None })]).unwrap();
    let saved = opened.source.save(document.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    kept(&data, &saved, main, content_type);
    let reread = ibis_formats::read(&saved).unwrap();
    assert!(ibis_doc_model::flow_command::paragraph(reread.flow.as_deref().unwrap(), id).unwrap().text().contains("マクロ"));
}

#[test]
fn pptm_save_as_ppsm() {
    let (main, content_type) = ("ppt/presentation.xml", "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml");
    let data = with_macros("editing/pptx-plan.pptx", main, content_type);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let saved = opened.source.save_slideshow(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    assert_eq!(
        pkg.content_type(main).unwrap().as_deref(),
        Some("application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml")
    );
}

#[test]
fn pptm_save_as_potm() {
    let (main, content_type) = ("ppt/presentation.xml", "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml");
    let data = with_macros("editing/pptx-plan.pptx", main, content_type);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let saved = opened.source.save_template(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    assert_eq!(
        pkg.content_type(main).unwrap().as_deref(),
        Some("application/vnd.ms-powerpoint.template.macroEnabled.main+xml")
    );
}
