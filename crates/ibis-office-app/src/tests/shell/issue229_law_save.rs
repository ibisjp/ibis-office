// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #229: a document opened as a statute (from .xml, .lawtext or .law.txt) offers Lawtext and 法令標準 XML among the formats of 名前を付けて保存 (design:
//! `tasks/doc/test-design/issue-229-save.md`; the numbers L1... are that document's). All are regression tests (listed in `all()`).
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue229_l1_a_statute_offers_lawtext_and_law_xml_to_save_as", l1_statute_offers_law_formats),
        ("shell::issue229_l2_a_document_that_is_not_a_statute_does_not_offer_them_and_its_list_is_unchanged", l2_other_documents),
        ("shell::issue229_l3_a_statute_saved_as_lawtext_or_law_xml_reads_again_with_its_title_and_articles", l3_round_trip),
        ("shell::issue229_l4_a_statute_saved_as_docx_still_works", l4_statute_as_docx),
    ]
}

const LAWTEXT: &str = "crates/ibis-formats/tests/lawtext/administrative-procedure-act.lawtext";
const LAW_XML: &str = "crates/ibis-formats/tests/lawtext/321CONSTITUTION.xml";

/// The extensions of the formats offered, in order.
fn extensions(s: &mut Scenario) -> Vec<String> {
    s.save_format_choices().into_iter().flat_map(|(_, e)| e).collect()
}

/// A copy of `source` under the name `name` in the scenarios' input folder (the same content under another name: `.law.txt`).
fn copy_as(source: &str, name: &str) -> String {
    let root = crate::shell::scenario::repo_root();
    let dir = root.join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(root.join(source), dir.join(name)).unwrap();
    format!("target/scenario-input/{name}")
}

/// The text of every paragraph of the body, in order.
fn paragraphs(s: &mut Scenario) -> Vec<String> {
    s.with(|e, _| {
        e.presentation()
            .flow
            .as_ref()
            .map(|flow| flow.body.iter().filter_map(|b| if let ibis_doc_model::flow::Block::Paragraph(p) = b { Some(p.text().to_string()) } else { None }).collect())
            .unwrap_or_default()
    })
}

/// L1: a document opened from .xml, .lawtext or .law.txt offers Lawtext (`lawtext`) and 法令標準 XML (`xml`) among the formats (the formats a document offers
/// as before are still there).
fn l1_statute_offers_law_formats() {
    let mut bad = Vec::new();
    for (name, file) in [("lawtext", LAWTEXT.to_string()), ("xml", LAW_XML.to_string()), ("law.txt", copy_as(LAWTEXT, "issue229-copy.law.txt"))] {
        let mut s = Scenario::open(&file);
        s.settle();
        let list = extensions(&mut s);
        for needed in ["lawtext", "xml"] {
            if !list.iter().any(|e| e == needed) {
                bad.push(format!("{name}: the save formats lack {needed}: {list:?}"));
            }
        }
        for before in ["docx", "dotx", "odt", "md"] {
            if !list.iter().any(|e| e == before) {
                bad.push(format!("{name}: {before} is no longer offered: {list:?}"));
            }
        }
        let choices = s.save_format_choices();
        if choices.iter().any(|(label, _)| label.is_empty() || label.starts_with("file.")) {
            bad.push(format!("{name}: a format has no name shown (an untranslated key): {choices:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// L2: a document that is not a statute offers no Lawtext or XML, and its list is what it was: a document `docx`, `dotx`, `odt`, `md`; a sheet and a slide show
/// their own (neither has Lawtext or XML).
fn l2_other_documents() {
    let expected: [(&str, &[&str]); 3] = [
        ("test/editing/docx-report.docx", &["docx", "dotx", "odt", "md"]),
        ("test/editing/xlsx-sales.xlsx", &["xlsx", "xltx", "xls", "ods", "csv", "tsv"]),
        ("test/editing/pptx-plan.pptx", &["pptx", "ppsx", "potx", "odp"]),
    ];
    for (file, want) in expected {
        let mut s = Scenario::open(file);
        s.settle();
        let list = extensions(&mut s);
        assert!(!list.iter().any(|e| e == "lawtext" || e == "xml"), "{file}: no Lawtext or XML: {list:?}");
        assert_eq!(list, want, "{file}: the list is as it was");
    }
    // A Markdown file that is not a statute is no statute either.
    let md = copy_as("test/editing/docx-report.docx", "issue229-not-law.docx");
    let mut s = Scenario::open(&md);
    s.settle();
    assert!(!extensions(&mut s).iter().any(|e| e == "lawtext"), "a document with another name: no Lawtext");
}

/// L3: saved as Lawtext or as 法令標準 XML, the statute reads again with its title and its articles (every article title of the source is in the file read again).
fn l3_round_trip() {
    let mut bad = Vec::new();
    for (source, title) in [(LAWTEXT, "行政手続法"), (LAW_XML, "日本国憲法")] {
        for extension in ["lawtext", "xml"] {
            // The statute opened, its article titles taken, and saved (one Scenario at a time).
            let mut s = Scenario::open(source);
            s.settle();
            let before = paragraphs(&mut s);
            let titles: Vec<String> = before.iter().map(|t| t.trim().to_string()).filter(|t| t.starts_with('第') && t.chars().count() < 14 && t.chars().take(8).any(|c| c == '条')).collect();
            assert!(!titles.is_empty(), "{source}: the source has article titles: {before:?}");
            // Saved outside the scenario's own folder (opening a file copies it into that folder, over itself).
            let folder = crate::shell::scenario::repo_root().join("target/scenario-input");
            std::fs::create_dir_all(&folder).unwrap();
            let saved = s.save_as(folder.join(format!("issue229-{}.{extension}", source.rsplit('/').next().unwrap().replace('.', "-"))).to_str().unwrap());
            drop(s);
            let size = std::fs::metadata(&saved).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                bad.push(format!("{source} as {extension}: the file written is empty ({size} bytes)"));
                continue;
            }
            let mut again = Scenario::open(saved.to_str().unwrap());
            again.settle();
            let after = paragraphs(&mut again).join("\n");
            drop(again);
            if !after.contains(title) {
                bad.push(format!("{source} as {extension}: the title {title} is lost ({size} bytes written)"));
            }
            let missing: Vec<&String> = titles.iter().filter(|t| !after.contains(t.as_str())).collect();
            if !missing.is_empty() {
                bad.push(format!("{source} as {extension}: {} of {} article titles are lost, first {:?}", missing.len(), titles.len(), missing.first()));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// L4: the other formats are not disturbed: a statute saved as .docx writes a file that reads again.
fn l4_statute_as_docx() {
    let mut s = Scenario::open(LAWTEXT);
    s.settle();
    let saved = s.save_as("issue229-as.docx");
    let data = std::fs::read(&saved).unwrap();
    let again = ibis_odf::read(&data).unwrap_or_else(|e| panic!("{}: {e}", saved.display()));
    assert!(again.flow.is_some(), "the saved .docx has a body");
}
