// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 68 (組版・禁則処理が Word と違う), the document (DOCX) side: where a line ends around the characters that may not start
//! or end a line, `――` `……`, dashes, no-break spaces and the closing marks that hang past the edge. Each test lays out a paragraph
//! of a line of exactly 46 full-width characters (MS Mincho 10.5 pt, whose advances are the typeface's own whatever is installed),
//! with the tested character at the 46th or 47th place as the issue's documents do.

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::typeset68_line_start_characters_follow_word", line_start_characters_follow_word),
        ("docx::typeset68_line_end_characters_follow_word", line_end_characters_follow_word),
        ("docx::typeset68_double_dash_and_ellipsis_are_not_split", double_dash_and_ellipsis_are_not_split),
        ("docx::typeset68_line_ends_after_a_dash", line_ends_after_a_dash),
        ("docx::typeset68_no_break_space_holds_its_neighbours", no_break_space_holds_its_neighbours),
        ("docx::typeset68_no_hanging_without_kinsoku", no_hanging_without_kinsoku),
        ("docx::typeset68_hanging_only_when_justified", hanging_only_when_justified),
        ("docx::typeset68_hung_mark_does_not_squeeze_the_line", hung_mark_does_not_squeeze_the_line),
        ("docx::typeset68_hanging_marks_are_words", hanging_marks_are_words),
        ("docx::typeset68_last_japanese_character_does_not_count_the_space_after_it", last_japanese_character_does_not_count_the_space_after_it),
        ("docx::typeset68_soft_hyphen_ends_a_line_as_a_hyphen", soft_hyphen_ends_a_line_as_a_hyphen),
    ]
}

/// Tools (not checks): `--ignored`.
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![("docx::typeset68_dump_attachment", dump_attachment)]
}

/// A one-paragraph document of the text of 46 full-width characters a line, `jc` the alignment and `ppr` more of the paragraph's properties.
fn make(name: &str, jc: &str, ppr: &str, text: &str, settings: &str) -> String {
    use ibis_pptx::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}">{settings}<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    // An optional hyphen is an element of its own between two texts.
    let text = text.replace('\u{ad}', r#"</w:t><w:softHyphen/><w:t xml:space="preserve">"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body><w:p><w:pPr>{ppr}<w:jc w:val="{jc}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="21"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="12540" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"# // i18n-ok（試験の文書の文字）
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("typeset68-{name}.docx"));
    std::fs::write(&file, z.finish().unwrap()).unwrap();
    format!("target/scenario-input/typeset68-{name}.docx")
}

/// The characters of each line with the x each starts at, top to bottom (page after page).
fn line_glyphs(pages: &Pagination) -> Vec<Vec<(char, f32)>> {
    let mut out: Vec<Vec<(char, f32)>> = Vec::new();
    for page in &pages.pages {
        let mut glyphs: Vec<(f32, f32, char)> = Vec::new();
        for item in &page.items {
            if let Item::Text { x, y, text, offsets, .. } = item {
                glyphs.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| (*y, x + dx, c)));
            }
        }
        glyphs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut last_y = f32::MIN;
        for (y, x, c) in glyphs {
            if (y - last_y).abs() > 1.0 {
                out.push(Vec::new());
                last_y = y;
            }
            out.last_mut().unwrap().push((c, x));
        }
    }
    out
}

fn lines(pages: &Pagination) -> Vec<String> {
    line_glyphs(pages).into_iter().map(|l| l.into_iter().map(|(c, _)| c).collect()).collect()
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

fn laid_out(file: &str) -> Vec<String> {
    lines(&pages_of(file))
}

fn kana(n: usize) -> String {
    "あ".repeat(n) // i18n-ok（試験の文書の文字）
}

/// D6 (standard level): ℃ ° ′ ″ ｣ ･ ﾞ do not start a line; ― … ‥ do. 高レベル (`strictFirstAndLastChars`) also keeps ｯ ｰ off the line start.
fn line_start_characters_follow_word() {
    for (n, c) in "℃°′″｣･ﾞ".chars().enumerate() { // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("start{n}"), "both", "", &format!("{}{c}{}", kana(46), kana(3)), ""));
        assert_eq!(l.len(), 2, "{c}: {l:?}");
        assert!(!l[1].starts_with(c), "{c} starts the second line: {l:?}");
        assert_eq!(l[0].chars().count(), 45, "{c}: the character before goes down with it: {l:?}");
    }
    for (n, c) in "―…‥".chars().enumerate() { // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("startok{n}"), "both", "", &format!("{}{c}{}", kana(46), kana(3)), ""));
        assert!(l[1].starts_with(c), "{c} may start a line: {l:?}");
    }
    // High level: a half-width ｯ and ｰ stay off the start of a line, which the standard level lets start one.
    let std_lines = laid_out(&make("start-std", "both", "", &format!("{}ｯ{}", kana(46), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(std_lines[1].starts_with('ｯ'), "{std_lines:?}"); // i18n-ok（試験の文書の文字）
    let strict = laid_out(&make("start-strict", "both", "", &format!("{}ｯ{}", kana(46), kana(3)), "<w:strictFirstAndLastChars/>")); // i18n-ok（試験の文書の文字）
    assert!(!strict[1].starts_with('ｯ'), "{strict:?}"); // i18n-ok（試験の文書の文字）
}

/// D7: ＄ ￥ $ ¥ £ ｢ do not end a line.
fn line_end_characters_follow_word() {
    for (n, c) in "＄￥$¥£｢".chars().enumerate() { // i18n-ok（試験の文書の文字）
        // The character at the end of the line (the 46th place, or just before the edge for a half-width one).
        let w = if c.is_ascii() || matches!(c, '¥' | '£' | '｢') { 1 } else { 0 }; // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("end{n}"), "left", "", &format!("{}{c}{}", kana(45), kana(3 + w)), ""));
        assert!(!l[0].ends_with(c), "{c} ends the first line: {l:?}");
    }
}

/// D8: `――` and `……` are not split, but the line may end before them.
fn double_dash_and_ellipsis_are_not_split() {
    for (n, pair) in ["――", "……"].iter().enumerate() { // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("pair{n}"), "left", "", &format!("{}{pair}{}", kana(45), kana(3)), ""));
        assert!(l[1].starts_with(pair), "{pair} goes down whole, and the character before it stays: {l:?}");
        assert_eq!(l[0].chars().count(), 45, "{l:?}");
    }
}

/// D9: a line may end after an em dash and an en dash inside Latin text.
fn line_ends_after_a_dash() {
    // 43 full-width characters and "abc—def": "abc—" fills the line up to the 45.5th place.
    let l = laid_out(&make("dash-em", "left", "", &format!("{}abc—def{}", kana(43), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(l[0].ends_with("abc—"), "{l:?}"); // i18n-ok（試験の文書の文字）
    let l = laid_out(&make("dash-en", "left", "", &format!("{}abc–def{}", kana(43), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(l[0].ends_with("abc–"), "{l:?}"); // i18n-ok（試験の文書の文字）
}

/// P8 / D-side: no break after (or before) a no-break space.
fn no_break_space_holds_its_neighbours() {
    // "x" and 44 kana, then "10\u{a0}kg": the line would end at the no-break space.
    let l = laid_out(&make("nbsp", "left", "", &format!("{}10\u{a0}kg{}", kana(44), kana(2)), ""));
    assert!(!l[0].ends_with("10\u{a0}") && !l[0].ends_with("10"), "{l:?}");
    let l = laid_out(&make("nnbsp", "left", "", &format!("{}10\u{202f}kg{}", kana(44), kana(2)), ""));
    assert!(!l[0].ends_with("10\u{202f}") && !l[0].ends_with("10"), "{l:?}");
}

/// D10: a paragraph without 禁則処理 (`w:kinsoku w:val="0"`) does not hang a closing mark either: it starts the next line.
fn no_hanging_without_kinsoku() {
    let l = laid_out(&make("nokinsoku", "both", r#"<w:kinsoku w:val="0"/>"#, &format!("{}」{}", kana(46), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(l.len() == 2 && l[1].starts_with('」'), "{l:?}"); // i18n-ok（試験の文書の文字）
    let l = laid_out(&make("kinsoku", "both", "", &format!("{}」{}", kana(46), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(l[0].ends_with('」'), "with 禁則処理 it hangs: {l:?}"); // i18n-ok（試験の文書の文字）
}

/// D11: closing marks hang only in a justified or distributed line; a left aligned (or centred, or right aligned) one takes the character before it down.
fn hanging_only_when_justified() {
    for (jc, n) in [("left", 0), ("center", 1), ("right", 2)] {
        let l = laid_out(&make(&format!("hang{n}"), jc, "", &format!("{}。{}", kana(46), kana(3)), "")); // i18n-ok（試験の文書の文字）
        assert!(!l[0].ends_with('。'), "{jc}: {l:?}"); // i18n-ok（試験の文書の文字）
        assert!(l[1].starts_with("あ。"), "{jc}: {l:?}"); // i18n-ok（試験の文書の文字）
    }
    let l = laid_out(&make("hang-both", "both", "", &format!("{}。{}", kana(46), kana(3)), "")); // i18n-ok（試験の文書の文字）
    assert!(l[0].ends_with('。'), "{l:?}"); // i18n-ok（試験の文書の文字）
}

/// D4: a hung closing mark is outside the line's room: the line's characters are not squeezed to take it in (they keep their natural advances).
fn hung_mark_does_not_squeeze_the_line() {
    for (n, c) in "。、」".chars().enumerate() { // i18n-ok（試験の文書の文字）
        let pages = pages_of(&make(&format!("squeeze{n}"), "both", "", &format!("{}{c}{}", kana(46), kana(3)), ""));
        let first = &line_glyphs(&pages)[0];
        assert_eq!(first.len(), 47, "{c}: the mark hangs on the first line");
        let step = (first[45].1 - first[0].1) / 45.0;
        assert!((step - 10.5).abs() < 0.05, "{c}: the advance of a character is {step}, not 10.5");
    }
}

/// D5: Word hangs 、。，．）」』】］｝ , . ’ ” and the half-width ｡ ､ ; 〕 〉 》 do not hang (with the character before them they go down).
fn hanging_marks_are_words() {
    for (n, c) in "，．）』】］｝｡､".chars().enumerate() { // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("hangset{n}"), "both", "", &format!("{}{c}{}", kana(46), kana(3)), ""));
        assert!(l[0].ends_with(c), "{c} hangs: {l:?}");
    }
    for (n, c) in "〕〉》".chars().enumerate() { // i18n-ok（試験の文書の文字）
        let l = laid_out(&make(&format!("nohang{n}"), "both", "", &format!("{}{c}{}", kana(46), kana(3)), ""));
        assert!(!l[0].ends_with(c) && l[1].starts_with(&format!("あ{c}")), "{c} does not hang: {l:?}"); // i18n-ok（試験の文書の文字）
    }
}

/// D3: the space between Japanese and Latin letters after the line's last Japanese character is not counted: a character that fits without it stays.
fn last_japanese_character_does_not_count_the_space_after_it() {
    let l = laid_out(&make("autospace", "left", "", &format!("{}ん{}", kana(45), "well-known"), "")); // i18n-ok（試験の文書の文字）
    assert_eq!(l[0].chars().count(), 46, "{l:?}");
    assert!(l[1].starts_with("well"), "{l:?}");
}

/// D12: a line may end at an optional hyphen (`w:softHyphen`), which is shown as a hyphen there.
fn soft_hyphen_ends_a_line_as_a_hyphen() {
    let l = laid_out(&make("softhyphen", "left", "", &format!("{}abc\u{ad}defghijkl", kana(43)), ""));
    assert!(l[0].ends_with("abc-"), "{l:?}");
    assert!(l[1].starts_with("def"), "{l:?}");
}

/// A tool: the lines of the issue's attachment, to `target/typeset-68/lines-docx.txt` (`TYPESET68_OUT` names another file).
fn dump_attachment() {
    let file = "test/github-issue/68/typeset-docx.docx";
    let l = laid_out(file);
    let out = std::env::var("TYPESET68_OUT").unwrap_or_else(|_| "target/typeset-68/lines-docx.txt".to_string());
    let path = crate::shell::scenario::repo_root().join(out);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, l.join("\n")).unwrap();
}
