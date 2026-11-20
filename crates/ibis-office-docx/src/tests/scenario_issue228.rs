// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #228: a footnote or endnote whose reference has a mark of its own (`w:customMarkFollows`, Word's 脚注の参照記号 *, †) shows that mark only, takes no
//! automatic number and is not counted by the others: Word's 1, *, 2 (not 1, 2*, 3), and † 1 2 for a mark first; the same for endnotes (i, *, ii).
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue228_a_footnote_with_a_mark_of_its_own_takes_no_number", footnotes),
        ("docx::issue228_an_endnote_with_a_mark_of_its_own_takes_no_number", endnotes),
        ("docx::issue228_the_mark_of_its_own_survives_a_save", round_trip),
        ("docx::issue228_marks_of_their_own_with_sections_restarting_the_numbers", sections),
    ]
}

/// A document of three paragraphs MAIN_A, MAIN_B, MAIN_C with a note each; `marks[i]` is the mark of its own of the note, if it has one.
fn make(name: &str, endnotes: bool, marks: [Option<&str>; 3]) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let (part, element, reference, mark_ref) = if endnotes { ("endnotes", "endnote", "endnoteReference", "endnoteRef") } else { ("footnotes", "footnote", "footnoteReference", "footnoteRef") };
    let ct = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/{part}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{part}+xml"/></Types>"#);
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{part}" Target="{part}.xml"/></Relationships>"#);
    let mut body = String::new();
    let mut notes = format!(r#"<w:{element} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{element}><w:{element} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{element}>"#);
    for (i, mark) in marks.iter().enumerate() {
        let (id, name) = (i + 1, ["A", "B", "C"][i]);
        let custom = if mark.is_some() { r#" w:customMarkFollows="1""# } else { "" };
        let after = mark.map(|m| format!("<w:t>{m}</w:t>")).unwrap_or_default();
        body.push_str(&format!(r#"<w:p><w:r><w:t>MAIN_{name}</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:{reference}{custom} w:id="{id}"/>{after}</w:r></w:p>"#));
        let note_mark = match mark {
            Some(m) => format!("<w:r><w:t>{m}</w:t></w:r>"),
            None => format!("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{mark_ref}/></w:r>"),
        };
        notes.push_str(&format!(r#"<w:{element} w:id="{id}"><w:p>{note_mark}<w:r><w:t xml:space="preserve"> NOTE_{name}</w:t></w:r></w:p></w:{element}>"#));
    }
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    let notes_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{part} xmlns:w="{w}">{notes}</w:{part}>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    z.add(&format!("word/{part}.xml"), EntryMeta::new(), notes_xml.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue228-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue228-{name}.docx")
}

/// The text of the page's lines, in order.
fn texts(file: &str) -> Vec<String> {
    let mut s = Scenario::open(file);
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    super::issue191_tests::lines(&pages, 0).into_iter().map(|l| l.1).collect()
}

/// What the page shows: each paragraph's text and its note's mark (the mark of its own, else the next automatic number of `numbers`), then each note at the foot (a note's mark,
/// then its text). Footnotes and endnotes go at the foot of the page or the end of the text in the same order.
fn expected(marks: [Option<&str>; 3], numbers: [&str; 3]) -> Vec<String> {
    let mut auto = 0;
    let shown: Vec<String> = marks
        .iter()
        .map(|m| {
            m.map_or_else(
                || {
                    auto += 1;
                    numbers[auto - 1].to_string()
                },
                str::to_string,
            )
        })
        .collect();
    let mut out = Vec::new();
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        out.push(format!("MAIN_{name}"));
        out.push(shown[i].clone());
    }
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        // A custom mark is text of the note's paragraph, one line with the note's text; an automatic one is a superscript of its own.
        if marks[i].is_some() {
            out.push(format!("{} NOTE_{name}", shown[i]));
        } else {
            out.push(shown[i].clone());
            out.push(format!(" NOTE_{name}"));
        }
    }
    out
}

fn footnotes() {
    for (name, marks) in [
        ("middle", [None, Some("*"), None]),
        ("first", [Some("\u{2020}"), None, None]),
        ("middle2", [None, Some("**"), None]),
        ("auto", [None, None, None]),
    ] {
        assert_eq!(texts(&make(&format!("foot-{name}"), false, marks)), expected(marks, ["1", "2", "3"]), "{name}");
    }
}

fn endnotes() {
    for (name, marks) in [("middle", [None, Some("*"), None]), ("first", [Some("\u{2020}"), None, None])] {
        assert_eq!(texts(&make(&format!("end-{name}"), true, marks)), expected(marks, ["i", "ii", "iii"]), "{name}");
    }
}

/// Saved and read again, the reference still has its mark of its own (the file is kept as it was read).
fn round_trip() {
    let file = make("round-trip", false, [None, Some("*"), None]);
    let root = crate::shell::scenario::repo_root();
    let bytes = std::fs::read(root.join(&file)).unwrap();
    let (opened, _) = ibis_formats::open_progressive(bytes.clone(), 100_000).unwrap();
    let flow = opened.presentation.flow.as_ref().expect("a document");
    assert_eq!(flow.footnotes.iter().map(|n| n.custom_mark).collect::<Vec<_>>(), [false, true, false], "read");
    let saved = opened.source.save(&opened.presentation, Default::default()).unwrap();
    let again = ibis_formats::read(&saved).unwrap();
    assert_eq!(again.flow.as_ref().unwrap().footnotes.iter().map(|n| n.custom_mark).collect::<Vec<_>>(), [false, true, false], "saved and read again");
}

/// A document of one paragraph per entry of `marks` with a note each (`Some` is the mark of its own of the note); the paragraph numbered `section_end` (0-based) ends the first section.
/// Every section restarts the numbering where `restart` is "eachSect" and is vertical (`tbRl`) where `vertical` is.
fn make_sections(name: &str, marks: &[Option<&str>], section_end: usize, restart: &str, vertical: bool) -> String {
    let endnotes = false;
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let (part, element, reference, mark_ref) = if endnotes { ("endnotes", "endnote", "endnoteReference", "endnoteRef") } else { ("footnotes", "footnote", "footnoteReference", "footnoteRef") };
    let ct = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/{part}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{part}+xml"/></Types>"#);
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{part}" Target="{part}.xml"/></Relationships>"#);
    let direction = if vertical { r#"<w:textDirection w:val="tbRl"/>"# } else { "" };
    let sect = format!(r#"<w:sectPr><w:footnotePr><w:numRestart w:val="{restart}"/></w:footnotePr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{direction}</w:sectPr>"#);
    let mut body = String::new();
    let mut notes = format!(r#"<w:{element} w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:{element}><w:{element} w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{element}>"#);
    for (i, mark) in marks.iter().enumerate() {
        let (id, name) = (i + 1, format!("N{i}"));
        let ppr = if i == section_end { format!("<w:pPr>{sect}</w:pPr>") } else { String::new() };
        let custom = if mark.is_some() { r#" w:customMarkFollows="1""# } else { "" };
        let after = mark.map(|m| format!("<w:t>{m}</w:t>")).unwrap_or_default();
        body.push_str(&format!(r#"<w:p>{ppr}<w:r><w:t>MAIN_{name}</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:{reference}{custom} w:id="{id}"/>{after}</w:r></w:p>"#));
        let note_mark = match mark {
            Some(m) => format!("<w:r><w:t>{m}</w:t></w:r>"),
            None => format!("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{mark_ref}/></w:r>"),
        };
        notes.push_str(&format!(r#"<w:{element} w:id="{id}"><w:p>{note_mark}<w:r><w:t xml:space="preserve"> NOTE_{name}</w:t></w:r></w:p></w:{element}>"#));
    }
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}{sect}</w:body></w:document>"#);
    let notes_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{part} xmlns:w="{w}">{notes}</w:{part}>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    z.add(&format!("word/{part}.xml"), EntryMeta::new(), notes_xml.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue228-s-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue228-s-{name}.docx")
}

/// Two sections of three paragraphs, each with notes 自動, 独自記号, 自動, the numbering restarting at each section, across and in vertical writing: every section reads 1, *, 2
/// (a mark of its own is neither numbered nor counted, and a section's first note that has one does not hold the restart back); without the restart the count goes on: 1, *, 2, 3, *, 4.
fn sections() {
    let marks = [None, Some("*"), None, None, Some("*"), None];
    for (vertical, restart, numbers) in [(false, "eachSect", ["1", "2", "1", "2"]), (true, "eachSect", ["1", "2", "1", "2"]), (false, "continuous", ["1", "2", "3", "4"]), (true, "continuous", ["1", "2", "3", "4"])] {
        let file = make_sections(&format!("{restart}-{vertical}"), &marks, 2, restart, vertical);
        let mut s = Scenario::open(&file);
        let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
        let shown = s.with(|e, _| super::flow_layout::note_numbers_with_pages(e.flow().expect("a document"), &pages.pages));
        let got: Vec<String> = (1..=6).map(|id| shown.get(&(false, id)).cloned().unwrap_or_default()).collect();
        let want = [numbers[0], "", numbers[1], numbers[2], "", numbers[3]];
        assert_eq!(got, want, "vertical {vertical}, {restart}");
    }
}
