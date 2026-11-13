// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word's carrying over of a fallback typeface from the character before (#M98, tasks/doc/m98-word-fallback-carry.md), in the document (DOCX) layout.
//! The text is in `hAnsi` Century, which has neither of the characters (‒ ⅓), by the table of the Windows font file (`office_has_glyph`; a Mac's Century is another
//! version): ‒ is Courier New when it stands apart, ⅓ Cambria Math, and Courier New has both. Courier New is the one of the fallback typefaces that a Mac has too, so the
//! cases look at it (Cambria Math, where it is not installed, leaves the character in the run's typeface, and a line whose first character is cut from the carrying
//! over is told by that).

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::fallback_carry_follows_the_character_before", follows_the_character_before),
        ("docx::fallback_carry_is_cut_by_a_space_and_by_bold", is_cut_by_a_space_and_by_bold),
        ("docx::fallback_carry_goes_over_a_run_boundary_of_colour_and_size", goes_over_a_run_boundary_of_colour_and_size),
        ("docx::fallback_carry_is_cut_at_the_start_of_a_wrapped_line", is_cut_at_the_start_of_a_wrapped_line),
        ("docx::fallback_carry_passes_from_emoji_to_arrows_and_shapes", passes_from_emoji_to_arrows_and_shapes),
        ("docx::fallback_carry_comparison_documents_are_laid_out", comparison_documents_are_laid_out),
    ]
}

/// A one-paragraph document of the runs `runs`.
fn make(name: &str, runs: &str) -> String {
    use ibis_pptx::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body><w:p>{runs}</w:p><w:sectPr><w:pgSz w:w="12540" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"# // i18n-ok（試験の文書の文字）
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("fallback-carry-{name}.docx"));
    std::fs::write(&file, z.finish().unwrap()).unwrap();
    format!("target/scenario-input/fallback-carry-{name}.docx")
}


const CENTURY: &str = r#"<w:rFonts w:ascii="Century" w:hAnsi="Century" w:eastAsia="ＭＳ 明朝"/>"#; // i18n-ok（試験の文書の文字）

fn run(props: &str, text: &str) -> String {
    format!(r#"<w:r><w:rPr>{CENTURY}{props}<w:sz w:val="21"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// The lines of the document as (character, typeface) from the top, left to right.
fn drawn(file: &str) -> Vec<Vec<(char, String)>> {
    drawn_pages(file, 1).into_iter().next().unwrap()
}

/// The lines of the first `n` pages.
fn drawn_pages(file: &str, n: usize) -> Vec<Vec<Vec<(char, String)>>> {
    let mut s = Scenario::open(file);
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    pages.pages.iter().take(n).map(|page| lines_of(&page.items)).collect()
}

fn lines_of(items: &[Item]) -> Vec<Vec<(char, String)>> {
    let mut glyphs: Vec<(f32, f32, char, String)> = Vec::new();
    for item in items {
        if let Item::Text { x, y, text, offsets, font, .. } = item {
            glyphs.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| (*y, x + dx, c, font.family.to_string())));
        }
    }
    glyphs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<Vec<(char, String)>> = Vec::new();
    let mut last_y = f32::MIN;
    for (y, _, c, family) in glyphs {
        if (y - last_y).abs() > 1.0 {
            out.push(Vec::new());
            last_y = y;
        }
        out.last_mut().unwrap().push((c, family));
    }
    out
}

/// Tool (not a check): `--ignored`. The documents listed in the file `$IBIS_CARRY_LIST` (one path to a .docx a line, from the repository's root or absolute), laid out
/// in three ways: with Word's carrying over of a fallback typeface (as it is), without it, and with the installed typeface's own cmap asked first (as it was before
/// `office_has_glyph` was). One line a document: the number, the characters, the characters whose typeface differs without the carrying over, those that differ with the
/// installed typeface asked first. A document that cannot be opened (encrypted) is told as `skipped`.
pub(crate) fn count_corpus() {
    use std::sync::atomic::Ordering;
    let list = std::fs::read_to_string(std::env::var("IBIS_CARRY_LIST").expect("IBIS_CARRY_LIST")).expect("the list");
    let flat = |pages: Vec<Vec<Vec<(char, String)>>>| pages.into_iter().flatten().flatten().collect::<Vec<_>>();
    let differing = |a: &[(char, String)], b: &[(char, String)]| a.iter().zip(b).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len());
    for (number, path) in list.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        if path.contains("encrypted") {
            println!("carry-count\t{number}\tskipped");
            continue;
        }
        let normal = flat(drawn_pages(path, usize::MAX));
        super::flow_layout::CARRY_OFF.store(true, Ordering::Relaxed);
        let without = flat(drawn_pages(path, usize::MAX));
        super::flow_layout::CARRY_OFF.store(false, Ordering::Relaxed);
        super::flow_layout::INSTALLED_FIRST.store(true, Ordering::Relaxed);
        let installed = flat(drawn_pages(path, usize::MAX));
        super::flow_layout::INSTALLED_FIRST.store(false, Ordering::Relaxed);
        println!("carry-count\t{number}\t{}\t{}\t{}", normal.len(), differing(&normal, &without), differing(&normal, &installed));
    }
}

fn family_of(line: &[(char, String)], c: char) -> String {
    line.iter().find(|g| g.0 == c).map(|g| g.1.clone()).unwrap_or_else(|| panic!("{c} is not drawn: {line:?}"))
}

const COURIER: &str = "Courier New";

/// ‒ is Courier New set apart; ⅓ is Cambria Math set apart, but after ‒ it is drawn in Courier New, which has it (#M98: ‒⅓).
fn follows_the_character_before() {
    let lines = drawn(&make("follows", &run("", "‒⅓⅓⅓")));
    assert_eq!(lines[0].iter().map(|g| g.1.as_str()).collect::<Vec<_>>(), [COURIER; 4], "{lines:?}");
    // ⅓ alone is not in Courier New (Cambria Math, which is not installed here, or the run's typeface).
    let alone = drawn(&make("alone", &run("", "⅓")));
    assert_ne!(family_of(&alone[0], '⅓'), COURIER, "{alone:?}");
}

/// A space cuts the carrying over, and so does a run that is bold where the one before is not (#M98 (c) 2).
fn is_cut_by_a_space_and_by_bold() {
    let spaced = drawn(&make("space", &run("", "‒ ⅓")));
    assert_eq!(family_of(&spaced[0], '‒'), COURIER);
    assert_ne!(family_of(&spaced[0], '⅓'), COURIER, "{spaced:?}");
    let bold = drawn(&make("bold", &format!("{}{}", run("", "‒"), run("<w:b/>", "⅓"))));
    assert_ne!(family_of(&bold[0], '⅓'), COURIER, "{bold:?}");
}

/// A run boundary where only the colour, the size or the underline differ does not cut it.
fn goes_over_a_run_boundary_of_colour_and_size() {
    for (n, props) in ["<w:color w:val=\"FF0000\"/>", "<w:u w:val=\"single\"/>", "<w:lang w:val=\"en-US\"/>"].iter().enumerate() {
        let lines = drawn(&make(&format!("boundary{n}"), &format!("{}{}", run("", "‒"), run(props, "⅓"))));
        assert_eq!(family_of(&lines[0], '⅓'), COURIER, "{props}: {lines:?}");
    }
}

/// The first character of a wrapped line is drawn as set apart (#M98 (c) 3): a long run of ⅓ after ‒ is in Courier New up to the line's end and not after it. The lines
/// are broken by the widths the carrying over gave (#M102), not again by the widths of the typefaces the lines after the first are drawn in: the second line holds as many
/// characters as the first, though ⅓ is narrower in the typeface it is drawn in there.
fn is_cut_at_the_start_of_a_wrapped_line() {
    let lines = drawn(&make("wrapped", &run("", &format!("‒{}", "⅓".repeat(200)))));
    assert!(lines.len() >= 3, "{} lines", lines.len());
    assert!(lines[0].iter().all(|g| g.1 == COURIER), "{:?}", lines[0]);
    assert!(lines[1].iter().all(|g| g.1 != COURIER), "{:?}", lines[1]);
    assert_eq!(lines[1].len(), lines[0].len(), "the lines are not broken again");
}

/// Segoe UI Emoji has all the arrows (2190 - 21FF) and shapes (25A0 - 25FF): after an emoji (set apart: Segoe UI Emoji) an arrow Century lacks and a shape Century lacks
/// (set apart: Cambria Math) are carried in it (#M102), from the cmaps of the Windows fonts.
fn passes_from_emoji_to_arrows_and_shapes() {
    use ibis_doc_model::flow::{CarryInput, FallbackCarry, FontSlot};
    use crate::render::font_metrics::office_has_glyph;
    let draw = |carry: &mut FallbackCarry, c: char| {
        let has_glyph = office_has_glyph("Century", c).unwrap_or(true);
        carry.next(CarryInput { c, slot: FontSlot::HAnsi, typeface: "Century", bold: false, italic: false, line_start: false, has_glyph }, office_has_glyph).fallback
    };
    let mut carry = FallbackCarry::default();
    assert_eq!(draw(&mut carry, '\u{2614}'), Some("Segoe UI Emoji"));
    assert_eq!(draw(&mut carry, '\u{21a6}'), Some("Segoe UI Emoji"), "an arrow after the emoji");
    assert_eq!(draw(&mut carry, '\u{25c6}'), Some("Segoe UI Emoji"), "a shape after the arrow");
    // The same characters standing apart.
    for c in ['\u{21a6}', '\u{25c6}'] {
        assert_eq!(draw(&mut FallbackCarry::default(), c), Some("Cambria Math"), "{c:?}");
    }
}

/// `test/editing/fallback-carry-century.docx` (made by `scripts/make-fallback-carry-docx.py`, for Word to draw and be compared with) is laid out: its third case, ‒⅓, is in
/// Courier New, and the one with a space between ‐ and ‒ is cut.
fn comparison_documents_are_laid_out() {
    let lines = drawn("test/editing/fallback-carry-century.docx");
    let case = |n: usize| lines.iter().find(|l| l.iter().map(|g| g.0).collect::<String>().starts_with(&format!("Q{n} "))).unwrap_or_else(|| panic!("case {n}: {lines:?}")).clone();
    let third = case(3);
    assert_eq!(family_of(&third, '\u{2012}'), COURIER, "{third:?}");
    assert_eq!(family_of(&third, '\u{2153}'), COURIER, "{third:?}");
    let spaced = case(8);
    assert_eq!(family_of(&spaced, '\u{2012}'), COURIER, "{spaced:?}");
    assert!(drawn("test/editing/fallback-carry-calibri.docx").len() >= 7);
}
