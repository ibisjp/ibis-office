// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #180 (ruby, 割注, 組み文字, 囲い文字 and fitText differ from Word's), the document (DOCX) side. Each test lays out one left aligned
//! paragraph (MS Mincho 10.5 pt, A4 with 72 pt margins, compatibility mode 15) and measures where characters are; the values are the issue's
//! measurements of Word's PDFs (tolerance 0.25 pt; Word's PDFs have 0.12 pt steps).

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue180_ruby_longer_than_its_base_widens_the_box", ruby_longer_than_its_base_widens_the_box),
        ("docx::issue180_ruby_and_its_base_are_not_parted_by_a_line", ruby_and_its_base_are_not_parted_by_a_line),
        ("docx::issue180_fit_text_latin_letters_fill_the_width", fit_text_latin_letters_fill_the_width),
        ("docx::issue180_fit_text_is_not_parted_by_a_line", fit_text_is_not_parted_by_a_line),
        ("docx::issue180_stacked_pair_is_drawn_and_takes_room", stacked_pair_is_drawn_and_takes_room),
        ("docx::issue180_enclosed_character_is_on_the_baseline", enclosed_character_is_on_the_baseline),
        ("docx::issue180_combined_run_size_and_alignment", combined_run_size_and_alignment),
        ("docx::issue180_dump", dump),
    ]
}

/// A document of `body` (the XML of its paragraphs); the path of the file.
fn make(name: &str, body: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#
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
    std::fs::write(dir.join(format!("issue180-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue180-{name}.docx")
}

/// A left aligned paragraph of `before` (plain text), the run `middle` (XML), and `after`.
fn para(before: &str, middle: &str, after: &str) -> String {
    format!(r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t xml:space="preserve">{before}</w:t></w:r>{middle}<w:r><w:t xml:space="preserve">{after}</w:t></w:r></w:p>"#)
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// Every text item of the first page: (baseline, size, text, the x of each character).
fn texts(p: &Pagination) -> Vec<(f32, f32, String, Vec<f32>)> {
    p.pages[0].items.iter().filter_map(|it| match it {
        Item::Text { x, y, text, offsets, size, .. } => Some((*y, *size, text.to_string(), offsets.iter().map(|o| x + o).collect())),
        _ => None,
    }).collect()
}

/// The items whose text is exactly `text`.
fn of<'a>(t: &'a [(f32, f32, String, Vec<f32>)], text: &str) -> Vec<&'a (f32, f32, String, Vec<f32>)> {
    t.iter().filter(|i| i.2 == text).collect()
}

/// The x of the first character of the item that starts with `text`.
fn start_x(t: &[(f32, f32, String, Vec<f32>)], text: &str) -> f32 {
    t.iter().find(|i| i.2.starts_with(text)).unwrap_or_else(|| panic!("no item {text}: {t:?}")).3[0]
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

const AFTER: &str = "たちつえおはひふへ";

fn ruby(align: &str, ruby: &str, base: &str) -> String {
    format!(r#"<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="{align}"/><w:hps w:val="10"/><w:hpsRaise w:val="18"/><w:hpsBaseText w:val="21"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:rPr><w:sz w:val="10"/></w:rPr><w:t>{ruby}</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>{base}</w:t></w:r></w:rubyBase></w:ruby></w:r>"#)
}

/// Item 1 (D1a-D1c): a ruby longer than its base makes the box as wide as the ruby, the base in the middle of it.
fn ruby_longer_than_its_base_widens_the_box() {
    // D1a: a base of three characters under a ruby of two: the base's width.
    let t = texts(&pages_of(&make("d1a", &para("あいう", &ruby("center", "さし", "かきく"), AFTER))));
    near(start_x(&t, AFTER), 135.1, 0.25, "D1a: the text after");
    // D1b: one character under a ruby of four (20 pt).
    let t = texts(&pages_of(&make("d1b", &para("あいう", &ruby("center", "さしすせ", "か"), AFTER))));
    near(of(&t, "さしすせ")[0].3[0], 103.6, 0.25, "D1b: the ruby's left");
    near(of(&t, "か")[0].3[0], 108.3, 0.25, "D1b: the base");
    near(start_x(&t, AFTER), 123.6, 0.25, "D1b: the text after");
    // D1c: two characters under a ruby of five.
    let t = texts(&pages_of(&make("d1c", &para("あいう", &ruby("center", "さしすせそ", "かき"), AFTER))));
    near(of(&t, "かき")[0].3[0], 105.5, 0.25, "D1c: the base's first character");
    near(start_x(&t, AFTER), 128.5, 0.25, "D1c: the text after");
}

/// The 41 or 39 characters that fill a line (451.3 pt: 42 characters of 10.5 pt fit).
fn fill(n: usize) -> String {
    "あいうえおはひふへほまみむめもやゆよらりるれろわをんあいうえおはひふへほまみむめも".chars().take(n).collect()
}

/// Item 2 (D2a-D2c): a ruby's base is not parted by the end of a line, and the ruby is drawn once.
fn ruby_and_its_base_are_not_parted_by_a_line() {
    // D2a: a base of two under a ruby of four, one character of room: the whole base goes to the next line.
    let t = texts(&pages_of(&make("d2a", &para(&fill(41), &ruby("center", "さしすせ", "かき"), "たちつえおはひふへほま"))));
    assert_eq!(of(&t, "さしすせ").len(), 1, "the ruby is drawn once: {t:?}");
    let base = of(&t, "かき");
    assert_eq!(base.len(), 1, "the base is not parted: {t:?}");
    near(base[0].3[0], 72.0, 0.25, "D2a: the base at the start of the next line");
    // D2b: a base of four under a ruby of two, two characters of room: the same.
    let t = texts(&pages_of(&make("d2b", &para(&fill(40), &ruby("center", "さし", "かきくけ"), "たちつえおはひふへほま"))));
    assert_eq!(of(&t, "さし").len(), 1, "the ruby is drawn once: {t:?}");
    let base = of(&t, "かきくけ");
    assert_eq!(base.len(), 1, "the base is not parted: {t:?}");
    near(base[0].3[0], 72.0, 0.25, "D2b: the base at the start of the next line");
    // D2c: a base that fits exactly stays on the line.
    let t = texts(&pages_of(&make("d2c", &para(&fill(40), &ruby("center", "さしすせ", "かき"), "たちつえおはひふへほま"))));
    assert_eq!(of(&t, "さしすせ").len(), 1, "{t:?}");
    near(of(&t, "かき")[0].3[0], 492.0, 0.25, "D2c: the base ends the line");
}

fn fit_text(width: u32, id: u32, text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:fitText w:val="{width}" w:id="{id}"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// Item 4 (D4a-D4d): the letters of a fitted span are spread over its width, from its left, with no quarter em beside Japanese text.
fn fit_text_latin_letters_fill_the_width() {
    // D4a: three kana over 47.25 pt (the same as Word's).
    let t = texts(&pages_of(&make("d4a", &para("あいう", &fit_text(945, 101, "かきく"), AFTER))));
    near(of(&t, "かきく")[0].3[1], 121.9, 0.25, "D4a: the second kana");
    near(start_x(&t, AFTER), 150.9, 0.25, "D4a: the text after");
    // D4b: four letters over 63 pt.
    let t = texts(&pages_of(&make("d4b", &para("あいう", &fit_text(1260, 102, "xqzv"), AFTER))));
    for (got, want) in of(&t, "xqzv")[0].3.iter().zip([103.6, 122.9, 142.1, 161.4]) {
        near(*got, want, 0.25, "D4b: a letter");
    }
    near(start_x(&t, AFTER), 166.6, 0.25, "D4b: the text after");
    // D4c: three letters over 63 pt.
    let t = texts(&pages_of(&make("d4c", &para("あいう", &fit_text(1260, 103, "xqz"), AFTER))));
    for (got, want) in of(&t, "xqz")[0].3.iter().zip([103.6, 132.5, 161.4]) {
        near(*got, want, 0.25, "D4c: a letter");
    }
    near(start_x(&t, AFTER), 166.6, 0.25, "D4c: the text after");
    // D4d: four letters over 21 pt, their natural width: not squeezed.
    let t = texts(&pages_of(&make("d4d", &para("あいう", &fit_text(420, 104, "xqzv"), AFTER))));
    let sizes: Vec<f32> = t.iter().filter(|i| "xqzv".contains(&i.2)).map(|i| i.1).collect();
    assert!(sizes.iter().all(|s| (*s - 10.5).abs() < 0.01), "the letters keep their size: {t:?}");
    near(start_x(&t, AFTER), 124.6, 0.25, "D4d: the text after");
}

/// Item 4 (D4e): a fitted span that does not fit the rest of the line goes whole to the next.
fn fit_text_is_not_parted_by_a_line() {
    let t = texts(&pages_of(&make("d4e", &para(&fill(39), &fit_text(2520, 105, "かきくけこさしす"), "おはひふへほまみむめもや"))));
    let span: Vec<_> = t.iter().filter(|i| i.2.contains('か') && i.2.contains('す') || i.2.starts_with("かき")).collect();
    assert_eq!(span.len(), 1, "the span is one item on one line: {t:?}");
    assert_eq!(span[0].2, "かきくけこさしす", "{t:?}");
    near(span[0].3[0], 72.0, 0.25, "D4e: at the start of the next line");
}

fn stack(up: &str, down: &str) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">eq \o(\s\up 5(</w:instrText></w:r><w:r><w:rPr><w:sz w:val="10"/></w:rPr><w:instrText xml:space="preserve">{up}</w:instrText></w:r><w:r><w:instrText xml:space="preserve">),\s\do 2(</w:instrText></w:r><w:r><w:rPr><w:sz w:val="10"/></w:rPr><w:instrText xml:space="preserve">{down}</w:instrText></w:r><w:r><w:instrText xml:space="preserve">))</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

/// Item 5 (D5a-D5c): `eq \o(\s\up 5(上),\s\do 2(下))` draws its two lines (5 pt high, 5 pt up and 2 pt down from the baseline) in a cell as wide as the wider.
fn stacked_pair_is_drawn_and_takes_room() {
    let t = texts(&pages_of(&make("d5a", &para("あいう", &stack("ぱぴ", "ぷぺ"), AFTER))));
    let (up, down) = (of(&t, "ぱぴ"), of(&t, "ぷぺ"));
    assert!(up.len() == 1 && down.len() == 1, "both lines are drawn: {t:?}");
    near(up[0].1, 5.0, 0.1, "D5a: the size");
    let base = of(&t, "あいう")[0].0;
    near(base - up[0].0, 5.0, 0.1, "D5a: the upper line's rise");
    near(down[0].0 - base, 2.0, 0.1, "D5a: the lower line's drop");
    near(start_x(&t, AFTER), 113.5, 0.25, "D5a: the text after");
    let t = texts(&pages_of(&make("d5b", &para("あいう", &stack("ぱぴぷ", "ぺぽば"), AFTER))));
    assert_eq!(of(&t, "ぱぴぷ").len() + of(&t, "ぺぽば").len(), 2, "{t:?}");
    near(start_x(&t, AFTER), 118.6, 0.25, "D5b: the text after");
}

fn enclosed(ring: &str, ring_half_points: u32, inner: &str, inner_half_points: u32) -> String {
    format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> </w:instrText></w:r><w:r><w:instrText xml:space="preserve">eq \o\ac(</w:instrText></w:r><w:r><w:rPr><w:sz w:val="{ring_half_points}"/><w:szCs w:val="{ring_half_points}"/></w:rPr><w:instrText xml:space="preserve">{ring}</w:instrText></w:r><w:r><w:instrText xml:space="preserve">,</w:instrText></w:r><w:r><w:rPr><w:sz w:val="{inner_half_points}"/><w:szCs w:val="{inner_half_points}"/></w:rPr><w:instrText xml:space="preserve">{inner}</w:instrText></w:r><w:r><w:instrText xml:space="preserve">)</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
}

/// Item 6 (D6a-D6d): the character in an enclosure stands on the enclosure's baseline, whatever the two sizes.
fn enclosed_character_is_on_the_baseline() {
    for (name, ring, rs, inner, is, x) in [("d6a", "○", 21, "か", 15, 105.1), ("d6b", "□", 21, "か", 15, 105.1), ("d6c", "○", 21, "12", 15, 105.1), ("d6d", "○", 29, "か", 21, 105.6)] {
        let t = texts(&pages_of(&make(name, &para("あいう", &enclosed(ring, rs, inner, is), AFTER))));
        let (ring_item, inner_item) = (of(&t, ring)[0], of(&t, inner)[0]);
        near(inner_item.0, ring_item.0, 0.05, &format!("{name}: the baseline"));
        // (The digits' own widths come from the typeface: MS Mincho's half-width 3.75 pt at 7.5 pt where it is installed, a stand-in's elsewhere, so only
        // the kana's x is compared.)
        if inner == "か" {
            near(inner_item.3[0], x, 0.25, &format!("{name}: the character's x"));
        }
    }
}

fn combined(round: bool, text: &str) -> String {
    let brackets = if round { r#" w:combineBrackets="round""# } else { "" };
    format!(r#"<w:r><w:rPr><w:eastAsianLayout w:id="106" w:combine="1"{brackets}/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// Item 7 (D7a-D7d): a 割注's characters are half the size in whole half points (10.5 pt: 5.0), and the shorter line of an odd count starts at the block's left.
fn combined_run_size_and_alignment() {
    // D7a: four characters in round brackets.
    let t = texts(&pages_of(&make("d7a", &para("あいう", &combined(true, "ぱぴぷぺ"), AFTER))));
    near(of(&t, "ぱ")[0].1, 5.0, 0.05, "D7a: the size");
    near(of(&t, ")")[0].3[0], 118.8, 0.25, "D7a: the closing bracket");
    near(start_x(&t, AFTER), 124.1, 0.25, "D7a: the text after");
    // D7b: five in brackets (three over two): the lower line from the left.
    let t = texts(&pages_of(&make("d7b", &para("あいう", &combined(true, "ぱぴぷぺぽ"), AFTER))));
    near(of(&t, "ぺ")[0].3[0], 108.9, 0.25, "D7b: the lower line's left");
    near(start_x(&t, AFTER), 129.1, 0.25, "D7b: the text after");
    // D7c: five, no brackets.
    let t = texts(&pages_of(&make("d7c", &para("あいう", &combined(false, "ぱぴぷぺぽ"), AFTER))));
    near(of(&t, "ぺ")[0].3[0], 103.6, 0.25, "D7c: the lower line's left");
    near(start_x(&t, AFTER), 118.6, 0.25, "D7c: the text after");
    // D7d: seven in brackets (four over three).
    let t = texts(&pages_of(&make("d7d", &para("あいう", &combined(true, "ぱぴぷぺぽばび"), AFTER))));
    near(of(&t, ")")[0].3[0], 128.9, 0.25, "D7d: the closing bracket");
    near(start_x(&t, AFTER), 134.1, 0.25, "D7d: the text after");
    near(of(&t, "ぽ")[0].3[0], 108.9, 0.25, "D7d: the lower line's left");
}

/// Not a test: with `I180_FILE` set, prints the first pages of the file (the text items and where their characters are).
fn dump() {
    let Ok(file) = std::env::var("I180_FILE") else { return };
    let p = pages_of(&file);
    for (n, page) in p.pages.iter().enumerate() {
        println!("--- page {n}");
        for it in &page.items {
            if let Item::Text { x, y, text, offsets, size, .. } = it {
                let xs: Vec<String> = offsets.iter().map(|o| format!("{:.1}", x + o)).collect();
                println!("T y={y:.1} sz={size:.2} {text} [{}]", xs.join(","));
            }
        }
    }
}
