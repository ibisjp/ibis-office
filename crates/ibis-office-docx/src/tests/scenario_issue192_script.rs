// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #192, the size of a superscript or subscript (`w:vertAlign`, `LayoutCtx::script_size`): Windows Word's XPS (`tasks/doc/requests/2026-11-14-180-192-rest-many-patterns-result.md`)
//! has it the body's size times 0.6 (ＭＳ 明朝, 游明朝, 游ゴシック, BIZ UD) or the typeface's `OS/2` ySuperscriptYSize / ySubscriptYSize (メイリオ, Arial, Times New Roman: 0.65), rounded to
//! half a point.

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue192_script_size_of_ms_mincho_is_0_6_in_half_points", ms_mincho as fn()),
        ("docx::issue192_script_size_of_yu_mincho_is_0_6_in_half_points", yu_mincho),
        ("docx::issue192_script_size_of_meiryo_is_0_65_in_half_points", meiryo),
        ("docx::issue192_script_size_of_latin_typefaces_is_0_65_in_half_points", latin),
        ("docx::issue192_superscript_of_ms_mincho_rises_by_the_body_less_the_script", ms_mincho_rise),
        ("docx::issue192_rise_of_a_typeface_the_pc_lacks_comes_from_the_table_of_office_typefaces", rise_from_the_table),
    ]
}

/// A paragraph of "あ", the run `vert` ("superscript" or "subscript") of "いう" and "え", all in `font` of `half_points`; the path of the file.
fn make(name: &str, font: &str, half_points: u32, vert: &str, text: (&str, &str, &str)) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body><w:p><w:r><w:t>{}</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="{vert}"/></w:rPr><w:t>{}</w:t></w:r><w:r><w:t>{}</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        text.0, text.1, text.2
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue192-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue192-{name}.docx")
}

/// The size of the drawn text item that has `c` (the superscript's).
fn size_of(file: &str, c: char) -> f32 {
    let mut s = Scenario::open(file);
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    pages.pages[0].items.iter().find_map(|it| match it {
        Item::Text { text, size, .. } if text.contains(c) => Some(*size),
        _ => None,
    }).unwrap_or_else(|| panic!("no {c} in {file}"))
}

fn check(font: &str, kana: (&str, &str, &str), c: char, rows: &[(u32, f32)]) {
    for vert in ["superscript", "subscript"] {
        for &(hp, want) in rows {
            let got = size_of(&make(&format!("{vert}-{font}-{hp}"), font, hp, vert, kana), c);
            assert!((got - want).abs() < 0.01, "{font} {} pt {vert}: {got} against Word's {want}", hp as f32 / 2.0);
        }
    }
}

/// Word's XPS (6, 8, 10.5, 12, 24, 36 pt): `round(0.6 x size, 0.5 pt)`: 3.5, 5.0, 6.5, 7.0, 14.5, 21.5.
fn ms_mincho() {
    check("ＭＳ 明朝", ("あ", "いう", "え"), 'い', &[(12, 3.5), (16, 5.0), (21, 6.5), (24, 7.0), (48, 14.5), (72, 21.5)]);
}

fn yu_mincho() {
    check("游明朝", ("あ", "いう", "え"), 'い', &[(12, 3.5), (16, 5.0), (21, 6.5), (24, 7.0), (48, 14.5), (72, 21.5)]);
}

/// メイリオ: 4.0, 5.0, 7.0, 8.0, 15.5, 23.5 (0.65).
fn meiryo() {
    check("メイリオ", ("あ", "いう", "え"), 'い', &[(12, 4.0), (16, 5.0), (21, 7.0), (24, 8.0), (48, 15.5), (72, 23.5)]);
}

/// Arial and Times New Roman Latin letters (0.65): 24 pt 15.5, 10.5 pt 7.0.
fn latin() {
    check("Arial", ("a", "xy", "b"), 'x', &[(21, 7.0), (48, 15.5)]);
    check("Times New Roman", ("a", "xy", "b"), 'x', &[(21, 7.0), (48, 15.5)]);
}

/// The baseline of the drawn text item that has `c`.
fn baseline_of(file: &str, c: char) -> f32 {
    let mut s = Scenario::open(file);
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    pages.pages[0].items.iter().find_map(|it| match it {
        Item::Text { text, y, .. } if text.contains(c) => Some(*y),
        _ => None,
    }).unwrap_or_else(|| panic!("no {c} in {file}"))
}

/// Word's XPS of ＭＳ 明朝: a superscript rises by the body less its size (10.5 pt: 10.5 - 6.5 = 4.0, 24 pt: 24 - 14.5 = 9.5, 12 pt: 12 - 7.0 = 5.0; the XPS 3.96, 9.48, 5.06).
fn ms_mincho_rise() {
    for &(hp, want) in &[(21u32, 4.0f32), (24, 5.0), (48, 9.5)] {
        let file = make(&format!("rise-ms-{hp}"), "ＭＳ 明朝", hp, "superscript", ("あ", "いう", "え"));
        let up = baseline_of(&file, 'あ') - baseline_of(&file, 'い');
        assert!((up - want).abs() < 0.13, "ＭＳ 明朝 {} pt: raised {up} against Word's {want}", hp as f32 / 2.0);
    }
}

/// Word's XPS (`tasks/doc/requests/2026-11-16-192-rise-result.md`, e2table): a 24 pt superscript rises 9.0 pt in Segoe UI and 8.0 in Gadugi (their GDI ascents differ at the sizes: Gadugi's is hinted),
/// a subscript falls 2.0 and 2.5. Where the PC has no such typeface the table of the typefaces Office ships gives the metrics (`font_metrics::office_font_face`); where it has, its file does: the same.
fn rise_from_the_table() {
    for (font, up, down) in [("Segoe UI", 9.0f32, 2.0f32), ("Gadugi", 8.0, 2.5)] {
        let file = make(&format!("rise-table-{font}"), font, 48, "superscript", ("a", "xy", "b"));
        let raised = baseline_of(&file, 'a') - baseline_of(&file, 'x');
        assert!((raised - up).abs() < 0.13, "{font}: raised {raised} against Word's {up}");
        let file = make(&format!("fall-table-{font}"), font, 48, "subscript", ("a", "xy", "b"));
        let lowered = baseline_of(&file, 'x') - baseline_of(&file, 'a');
        assert!((lowered - down).abs() < 0.13, "{font}: lowered {lowered} against Word's {down}");
    }
}
