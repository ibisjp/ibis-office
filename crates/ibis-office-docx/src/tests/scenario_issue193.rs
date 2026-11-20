// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #193: a justified list item (`w:jc="both"`) in a table cell. The number and the tab after it (`w:suff` tab, the default) are not justified: the tab ends at the text's start
//! (the paragraph's left indent, the hanging indent's end), and the text of the first line is spread over the room from there, so every full line of the item ends at the right margin
//! of the cell. The issue reports the first line ending 9.4 pt past the other lines (430.6 pt against 421.2 pt: the tab's room was spread over the text as well).
//! The document is the issue's synthetic one (a 7200-twip fixed cell, decimal list, left 800 hanging 440, MS Gothic 12 pt); the expectation is from the rule, not from a measurement.

use super::flow_layout::{Item, Pagination};
use super::issue191_tests::pages_of;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue193_justified_list_item_lines_end_together_in_a_cell", lines_end_together_in_a_cell),
        ("docx::issue193_justified_list_item_lines_end_together_in_the_body", lines_end_together_in_the_body),
    ]
}

const TEXT: &str = "これは試験用の文章です。表の中の文字送りを確かめる文章を繰り返します。これは試験用の文章です。表の中の文字送りを確かめる文章を繰り返します。";

fn make(name: &str, body: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ ゴシック" w:hAnsi="ＭＳ ゴシック" w:eastAsia="ＭＳ ゴシック" w:cs="ＭＳ ゴシック"/><w:sz w:val="24"/><w:szCs w:val="24"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let numbering = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{w}"><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:tabs><w:tab w:val="num" w:pos="1800"/></w:tabs><w:ind w:left="1800" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let mut z = Writer::new();
    for (n, d) in [("[Content_Types].xml", ct.to_string()), ("_rels/.rels", rels.to_string()), ("word/_rels/document.xml.rels", doc_rels.to_string()), ("word/settings.xml", settings), ("word/styles.xml", styles), ("word/numbering.xml", numbering), ("word/document.xml", document)] {
        z.add(n, EntryMeta::new(), d.as_bytes()).unwrap();
    }
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue193-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue193-{name}.docx")
}

fn item(jc: &str) -> String {
    format!(r#"<w:p><w:pPr><w:jc w:val="{jc}"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:ind w:left="800" w:hanging="440"/></w:pPr><w:r><w:t>{TEXT}</w:t></w:r></w:p>"#)
}

/// Per line (in order): where the first and the last text of the line start and where its text ends (the right end of the last Text item).
fn ends(p: &Pagination) -> Vec<(f32, f32)> {
    let mut v: Vec<(f32, f32)> = Vec::new();
    for it in &p.pages[0].items {
        if let Item::Text { x, y, offsets, .. } = it {
            let end = x + offsets.last().copied().unwrap_or(0.0);
            match v.last_mut() {
                Some(l) if (l.0 - y).abs() < 0.01 => l.1 = l.1.max(end),
                _ => v.push((*y, end)),
            }
        }
    }
    v
}

/// Where a plain justified paragraph (no number, left indent 40 pt) of the same text ends its full lines: the edge the list item's lines are compared with.
fn plain_end() -> f32 {
    let body = format!(r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:left="800"/></w:pPr><w:r><w:t>{TEXT}</w:t></w:r></w:p>"#);
    ends(&pages_of(&make("plain", &body)))[0].1
}

/// The ends of the full lines (all but the last), which must be one x: the first line, after the number and its tab, ends where the others do.
fn check(body: &str, name: &str) -> f32 {
    let pg = pages_of(&make(name, body));
    let lines = ends(&pg);
    assert!(lines.len() >= 2, "{name}: {} lines", lines.len());
    let first = lines[0].1;
    for (i, (_, end)) in lines[..lines.len() - 1].iter().enumerate() {
        assert!((end - first).abs() < 0.05, "{name}: line {i} ends at {end}, the first at {first}: {lines:?}");
    }
    first
}

/// The issue's cell (the lines end together).
fn lines_end_together_in_a_cell() {
    let tbl = format!(r#"<w:tbl><w:tblPr><w:tblW w:type="auto" w:w="0"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="7200"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:type="dxa" w:w="7200"/></w:tcPr>{}</w:tc></w:tr></w:tbl>"#, item("both"));
    check(&tbl, "cell");
}

/// The same item in the body: the first line ends where a plain justified paragraph of the text (no number, the same left indent) does.
fn lines_end_together_in_the_body() {
    let first = check(&item("both"), "body");
    let plain = format!(r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:left="800"/></w:pPr><w:r><w:t>{TEXT}</w:t></w:r></w:p>"#);
    let control = ends(&pages_of(&make("plain", &plain)))[0].1;
    assert!((first - control).abs() < 0.05, "the list item's first line ends at {first}, the plain paragraph's at {control}");
}
