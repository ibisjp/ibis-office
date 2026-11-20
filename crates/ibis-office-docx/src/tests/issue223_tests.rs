// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #223 (an even-page section break with the numbering restarted makes a blank page Word does not). The issue's documents
//! (`test/github-issue/223/x/docx/`, downloaded from the issue; not in the repository) are two one-line sections; the page counts are
//! those of Word's PDFs the issue reports.

use crate::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue223_section_start_with_restarted_numbering_page_counts", page_counts as fn()),
        ("docx::issue223_even_odd_section_start_by_the_continued_number_and_rounded_start", parity_rule),
    ]
}

const DIR: &str = "test/github-issue/223/x/docx/";

fn page_count(name: &str) -> usize {
    let mut s = Scenario::open(&format!("{DIR}{name}.docx"));
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources).pages.len())
}

fn page_counts() {
    if !crate::shell::scenario::repo_root().join(DIR).join("section-even-7.docx").exists() {
        return;
    }
    // (file, Word's page count)
    for (name, word) in [
        ("section-even-7", 2),
        ("section-even-7-mode15", 2),
        ("section-even-continuous", 2),
        ("section-next-7", 2),
        ("section-next-continuous", 2),
        ("section-odd-7", 2),
        ("section-odd-continuous", 3),
    ] {
        assert_eq!(page_count(name), word, "{name}");
    }
}

/// A section's sectPr: `kind` is "even", "odd" or "next" (how the section starts), `start` its `w:pgNumType w:start`.
fn sect_pr(kind: &str, start: Option<u32>) -> String {
    let ty = match kind {
        "even" => r#"<w:type w:val="evenPage"/>"#,
        "odd" => r#"<w:type w:val="oddPage"/>"#,
        _ => "",
    };
    let pg = start.map_or(String::new(), |n| format!(r#"<w:pgNumType w:start="{n}"/>"#));
    format!(r#"<w:sectPr>{ty}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/>{pg}</w:sectPr>"#)
}

/// A document of the sections (each: its pages, its start kind, its start number; the last one's sectPr is the body's), as the
/// Windows measurement of Word (tasks/doc/requests/2026-11-15-223-section-start-parity-result.md) builds it: A1.. with page breaks, A4, 1 inch margins.
fn make_sections(name: &str, sections: &[(usize, &str, Option<u32>)]) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut body = String::new();
    let last = sections.len() - 1;
    for (i, (pages, kind, start)) in sections.iter().enumerate() {
        let label = ["A", "B", "C"][i];
        for k in 1..=*pages {
            let br = if k > 1 { r#"<w:r><w:br w:type="page"/></w:r>"# } else { "" };
            let sect = if k == *pages && i < last { format!("<w:pPr>{}</w:pPr>", sect_pr(kind, *start)) } else { String::new() };
            body.push_str(&format!("<w:p>{sect}{br}<w:r><w:t>{label}{k}</w:t></w:r></w:p>"));
        }
    }
    let (_, kind, start) = sections[last];
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}{}</w:body></w:document>"#, sect_pr(kind, start));
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue223-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue223-{name}.docx")
}

/// (total pages, the number of the first page of each section after the first) of the document.
fn measure(name: &str, sections: &[(usize, &str, Option<u32>)]) -> (usize, Vec<u32>) {
    let file = make_sections(name, sections);
    let mut s = Scenario::open(&file);
    s.with(|e, _| {
        let p = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let firsts = (1..sections.len()).map(|sec| p.pages.iter().find(|pg| pg.section == sec && pg.first_in_section).map_or(0, |pg| pg.number)).collect();
        (p.pages.len(), firsts)
    })
}

/// Word's rule (Windows, 150 documents): the blank page of an even/odd-page section start is decided by the parity of the continued number
/// (the previous section's last number + 1), not of the physical page; a section with a start number gets no blank page, and its start is
/// rounded up to the parity (even 7 -> 8, 1 -> 2; odd 2 -> 3, 8 -> 9). Each row is one document of the measurement's table.
fn parity_rule() {
    // Two sections: (A pages, A start, B kind, B start) -> (pages, B's number).
    let two: &[(usize, Option<u32>, &str, Option<u32>, usize, u32)] = &[
        (1, None, "even", None, 2, 2),
        (1, None, "even", Some(1), 2, 2),
        (1, None, "even", Some(6), 2, 6),
        (1, None, "even", Some(7), 2, 8),
        (1, None, "even", Some(8), 2, 8),
        (1, None, "odd", None, 3, 3),
        (1, None, "odd", Some(1), 2, 1),
        (1, None, "odd", Some(2), 2, 3),
        (1, None, "odd", Some(6), 2, 7),
        (1, None, "odd", Some(8), 2, 9),
        (1, None, "next", Some(7), 2, 7),
        (2, None, "even", None, 4, 4),
        (2, None, "even", Some(1), 3, 2),
        (2, None, "odd", None, 3, 3),
        (2, None, "odd", Some(6), 3, 7),
        (3, None, "even", None, 4, 4),
        (3, None, "odd", None, 5, 5),
        (3, None, "odd", Some(8), 4, 9),
        // A starts at 4 or 5 (one page): the continued number is 5 or 6, not the physical 2.
        (1, Some(4), "even", None, 3, 6),
        (1, Some(4), "odd", None, 2, 5),
        (1, Some(4), "even", Some(7), 2, 8),
        (1, Some(5), "even", None, 2, 6),
        (1, Some(5), "odd", None, 3, 7),
        (2, Some(4), "even", None, 3, 6),
        (2, Some(4), "odd", None, 4, 7),
        (2, Some(5), "even", None, 4, 8),
        (2, Some(5), "odd", Some(2), 3, 3),
    ];
    for (i, &(a_pages, a_start, kind, start, pages, number)) in two.iter().enumerate() {
        let got = measure(&format!("two{i}"), &[(a_pages, "next", a_start), (1, kind, start)]);
        assert_eq!(got, (pages, vec![number]), "A {a_pages} pages start {a_start:?}, B {kind} start {start:?}");
    }
    // Three sections (A one or two pages, no start): (B kind, B start, C kind, C start) -> (pages, B's number, C's number).
    let three: &[(usize, &str, Option<u32>, &str, Option<u32>, usize, u32, u32)] = &[
        (1, "even", None, "even", None, 4, 2, 4),
        (1, "even", None, "even", Some(2), 3, 2, 2),
        (1, "even", None, "even", Some(7), 3, 2, 8),
        (1, "even", None, "odd", None, 3, 2, 3),
        (1, "even", Some(7), "even", None, 4, 8, 10),
        (1, "even", Some(7), "odd", None, 3, 8, 9),
        (1, "odd", None, "even", None, 4, 3, 4),
        (1, "odd", None, "odd", None, 5, 3, 5),
        (1, "odd", None, "odd", Some(2), 4, 3, 3),
        (1, "odd", Some(7), "odd", None, 4, 7, 9),
        (1, "odd", Some(7), "even", None, 3, 7, 8),
        (1, "next", Some(7), "odd", None, 4, 7, 9),
        (2, "even", None, "even", None, 6, 4, 6),
        (2, "even", Some(7), "even", None, 5, 8, 10),
        (2, "odd", None, "odd", None, 5, 3, 5),
        (2, "odd", Some(7), "odd", None, 5, 7, 9),
    ];
    for (i, &(a_pages, b_kind, b_start, c_kind, c_start, pages, b_number, c_number)) in three.iter().enumerate() {
        let got = measure(&format!("three{i}"), &[(a_pages, "next", None), (1, b_kind, b_start), (1, c_kind, c_start)]);
        assert_eq!(got, (pages, vec![b_number, c_number]), "A {a_pages} pages, B {b_kind} {b_start:?}, C {c_kind} {c_start:?}");
    }
}
