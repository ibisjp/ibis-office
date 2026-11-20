// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #54, the corpus's rest (Windows Word's XPS, `tasks/doc/requests/2026-11-16-54-corpus-result.md`): the advances of Meiryo and Meiryo UI are their
//! font files' (the typeface not installed), there is no quarter em between a full-width bracket and a Latin letter or digit, and `w:contextualSpacing` takes
//! away the space before of the first paragraph of a cell when the paragraph before it, the last of the cell before, has its style.

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue54_meiryo_advances_are_the_font_files", meiryo_advances as fn()),
        ("docx::issue54_no_quarter_em_beside_a_full_width_bracket", bracket_autospace),
        ("docx::issue54_contextual_spacing_takes_the_space_before_after_the_cell_before", contextual_across_cells),
    ]
}

/// A document of `body` (the paragraphs and tables XML) with the default font `font` (all four slots), 11 pt, A4 and 72 pt margins, `styles` extra styles' XML.
fn make(name: &str, font: &str, body: &str, styles: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>{styles}</w:styles>"#);
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue54-meiryo-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue54-meiryo-{name}.docx")
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// Every drawn character of the first page: (x, baseline y, character).
fn drawn(p: &Pagination) -> Vec<(f32, f32, char)> {
    let mut v = Vec::new();
    for it in &p.pages[0].items {
        if let Item::Text { x, y, text, offsets, .. } = it {
            for (c, o) in text.chars().zip(offsets.iter()) {
                v.push((x + o, *y, c));
            }
        }
    }
    v
}

fn para(text: &str) -> String {
    format!(r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

/// Meiryo UI's advances (units of 2048): 、 1360, 「 1024, ー 1672, あ 1672, 1 1272; Meiryo's: all 2048 but the Latin (1 1272, A 1388). 11 pt, the quarter em between Japanese and
/// Latin added (11 / 4 = 2.75 pt) where the line has it.
fn meiryo_advances() {
    let em = 11.0f32;
    let cases = [("Meiryo UI", "あい、「ー", [1672.0, 1672.0, 1360.0, 1024.0]), ("メイリオ", "あい、「ー", [2048.0, 2048.0, 2048.0, 2048.0])];
    for (n, (font, text, units)) in cases.iter().enumerate() {
        let d = drawn(&pages_of(&make(&format!("adv{n}"), font, &para(text), "")));
        assert!(d.len() >= 5, "{font}: {d:?}");
        for (k, u) in units.iter().enumerate() {
            near(d[k + 1].0 - d[k].0, u / 2048.0 * em, 0.05, &format!("{font}: the advance of {}", text.chars().nth(k).unwrap()));
        }
    }
    // Latin: 1 is 1272 and A 1388 in both, with the quarter em between Japanese and Latin.
    let d = drawn(&pages_of(&make("latin", "Meiryo UI", &para("あ1A"), "")));
    near(d[1].0 - d[0].0, 1672.0 / 2048.0 * em + em / 4.0, 0.05, "あ to 1: the advance and the quarter em");
    near(d[2].0 - d[1].0, 1272.0 / 2048.0 * em, 0.05, "1 to A");
}

/// None between ） and a Latin letter, or a Latin letter and （: the room of the Latin letter is its width and the quarter em on the side of the Japanese character that is
/// no bracket (the letters are not among the characters the page's items give a place to, so the difference of two lines' ends is what is measured).
fn bracket_autospace() {
    // Meiryo UI (not installed: the advances are the font file's, A 1388 and 1 1272 of 2048, the glyph squeezed nowhere).
    let (em, quarter, a_w, one_w) = (11.0f32, 11.0f32 / 4.0, 1388.0 / 2048.0 * 11.0, 1272.0 / 2048.0 * 11.0);
    let end = |name: &str, text: &str| {
        let d = drawn(&pages_of(&make(name, "Meiryo UI", &para(text), "")));
        d.last().unwrap().0
    };
    let _ = em;
    let base = end("b0", "あ）あ");
    // あ A ） あ: the quarter em after あ (before A), none between A and ）.
    near(end("b1", "あA）あ") - base, a_w + quarter, 0.05, "A before ）: its width and one quarter em");
    // あ ） A あ: none between ） and A, the quarter em after A.
    near(end("b2", "あ）Aあ") - base, a_w + quarter, 0.05, "A after ）: its width and one quarter em");
    // あ A あ against あ あ: a quarter em on each side.
    near(end("b4", "あAあ") - end("b3", "ああ"), a_w + 2.0 * quarter, 0.05, "A between kana: its width and two quarter ems");
    // あ （ 1 あ against あ （ あ: none between （ and 1.
    near(end("b5", "あ（1あ") - end("b6", "あ（あ"), one_w + quarter, 0.05, "1 after （: its width and the quarter em after it");
}

/// The first paragraph of the second cell loses its space before when it has `w:contextualSpacing` and the last paragraph of the first cell has its style; it keeps it
/// when that one has another style. The first cell's own paragraph keeps its space (nothing, but a table, is before it).
fn contextual_across_cells() {
    let styles = r#"<w:style w:type="paragraph" w:styleId="S1"><w:name w:val="S1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="360"/><w:contextualSpacing/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="S2"><w:name w:val="S2"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="360"/><w:contextualSpacing/></w:pPr></w:style>"#;
    let cell = |style: &str, text: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#);
    let table = |a: &str, b: &str| format!(r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#, cell(a, "あ"), cell(b, "い"));
    let same = drawn(&pages_of(&make("ctx-same", "ＭＳ 明朝", &format!("{}{}", table("S1", "S1"), para("う")), styles)));
    let other = drawn(&pages_of(&make("ctx-other", "ＭＳ 明朝", &format!("{}{}", table("S1", "S2"), para("う")), styles)));
    let y = |d: &[(f32, f32, char)], c: char| d.iter().find(|i| i.2 == c).unwrap().1;
    near(y(&other, 'い'), y(&other, 'あ'), 0.05, "another style: the space before stays, the two cells' lines are level");
    near(y(&same, 'あ') - y(&same, 'い'), 18.0, 0.1, "the same style: the second cell's line is the space before (360 twips = 18 pt) higher");
}
