// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! New, empty files (ファイル > 新規作成): the smallest packages Office opens,
//! written here (not copied from Office's templates), for the editor to
//! open as it opens a file, so a new file saves as an opened one does.

use crate::{Error, Kind};

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// An empty file of `kind`.
pub fn blank_file(kind: Kind) -> Result<Vec<u8>, Error> {
    match kind {
        Kind::Document => zip(&blank_docx_parts()),
        Kind::Workbook => crate::csv::workbook_from("", "Sheet1"),
        Kind::Presentation => crate::new_file::new_presentation(),
        Kind::Pdf => Ok(b"%PDF-1.4\n%%EOF\n".to_vec()),
    }
}

/// The parts of an empty Word document: one empty paragraph on an A4 page
/// with Word's Japanese defaults (margins 35 mm top and 30 mm elsewhere, a
/// line grid of 18 pt, 10.5 pt text in 游明朝).
fn blank_docx_parts() -> Vec<(&'static str, String)> {
    let content_types = format!(
        r#"{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#
    );
    let package_rels = format!(r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#);
    let document_rels = format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="rId2" Type="{REL}/settings" Target="settings.xml"/></Relationships>"#
    );
    let document = format!(
        r#"{XML}<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body><w:p/><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1985" w:right="1701" w:bottom="1701" w:left="1701" w:header="851" w:footer="992" w:gutter="0"/><w:cols w:space="425"/><w:docGrid w:type="lines" w:linePitch="360"/></w:sectPr></w:body></w:document>"#
    );
    let styles = format!(
        r#"{XML}<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="游明朝" w:eastAsia="游明朝" w:hAnsi="游明朝" w:cs="Times New Roman"/><w:kern w:val="2"/><w:sz w:val="21"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="a"><w:name w:val="Normal"/><w:qFormat/><w:pPr><w:widowControl w:val="0"/><w:jc w:val="both"/></w:pPr></w:style><w:style w:type="character" w:default="1" w:styleId="a0"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/><w:unhideWhenUsed/></w:style><w:style w:type="table" w:default="1" w:styleId="a1"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="numbering" w:default="1" w:styleId="a2"><w:name w:val="No List"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/></w:style></w:styles>"#
    );
    let settings = format!(
        r#"{XML}<w:settings xmlns:w="{W}"><w:defaultTabStop w:val="840"/><w:displayHorizontalDrawingGridEvery w:val="0"/><w:displayVerticalDrawingGridEvery w:val="2"/><w:characterSpacingControl w:val="compressPunctuation"/><w:strictFirstAndLastChars/><w:compat><w:spaceForUL/><w:balanceSingleByteDoubleByteWidth/><w:doNotLeaveBackslashAlone/><w:ulTrailSpace/><w:doNotExpandShiftReturn/><w:adjustLineHeightInTable/><w:useFELayout/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat><w:themeFontLang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:settings>"#
    );
    vec![
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", package_rels),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", document_rels),
        ("word/styles.xml", styles),
        ("word/settings.xml", settings),
    ]
}

fn zip(parts: &[(&str, String)]) -> Result<Vec<u8>, Error> {
    let mut writer = ibis_ooxml::zip::Writer::new();
    for (name, xml) in parts {
        writer.add(name, ibis_ooxml::zip::EntryMeta::default(), xml.as_bytes()).map_err(|e| Error::new(format!("{e}")))?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new document opens as a Word document of one empty A4 page, and
    /// typing into it saves and reads back.
    #[test]
    fn blank_document_opens_and_saves() {
        use ibis_doc_model::{Block, Command, Document, FlowCommand, FlowPos};
        let data = blank_file(Kind::Document).unwrap();
        let opened = crate::open(data.clone()).unwrap();
        let flow = opened.presentation.flow.as_deref().expect("a flow document");
        assert_eq!(flow.page.size.width, 11906 * 635);
        assert_eq!(flow.page.size.height, 16838 * 635);
        assert_eq!(flow.body.len(), 1);
        let Block::Paragraph(p) = &flow.body[0] else { panic!("a paragraph") };
        let mut document = Document::new(opened.presentation.clone());
        document.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "新しい文書".into(), format: None })]).unwrap();
        let saved = opened.source.save(document.presentation(), crate::SaveOptions::default()).unwrap();
        let reread = crate::read(&saved).unwrap();
        let Block::Paragraph(p) = &reread.flow.as_deref().unwrap().body[0] else { panic!("a paragraph") };
        assert_eq!(p.text(), "新しい文書");
        // Saved unchanged: the same file.
        assert_eq!(opened.source.save(&opened.presentation, crate::SaveOptions::default()).unwrap(), data);
    }

    /// A new workbook opens with one empty sheet.
    #[test]
    fn blank_workbook_opens() {
        let opened = crate::open(blank_file(Kind::Workbook).unwrap()).unwrap();
        assert_eq!(opened.presentation.slides.len(), 1);
    }
}
