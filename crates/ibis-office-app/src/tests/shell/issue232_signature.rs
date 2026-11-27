// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Digital signatures of the open file (GitHub issue #232): the information bar, the report of what the signatures are, and the
//! question before a save or an export that makes them invalid. The signed files are synthetic (`scripts/gen-signed-fixtures.py`,
//! `test/signature/`); the changed one is made here from one of them.
use crate::shell::actions::Save;
use crate::shell::i18n::{self, Language};
use crate::shell::infobar::InfoBarKind;
use crate::shell::scenario::Scenario;
use ibis_formats::ibis_ooxml::signature::{Inspection, Status, inspect};
use ibis_formats::ibis_ooxml::Package;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue232_signed_files_show_the_bar_and_the_report", issue232_signed_files_show_the_bar_and_the_report),
        ("shell::issue232_a_changed_file_is_reported_as_not_matching", issue232_a_changed_file_is_reported_as_not_matching),
        ("shell::issue232_saving_an_edited_signed_file_asks_and_cancel_writes_nothing", issue232_saving_an_edited_signed_file_asks_and_cancel_writes_nothing),
        ("shell::issue232_exports_of_a_signed_file_ask", issue232_exports_of_a_signed_file_ask),
        ("shell::issue232_unsigned_files_ask_nothing", issue232_unsigned_files_ask_nothing),
    ]
}

const SIGNED: [&str; 3] = ["test/signature/signed-docx-paste-source.docx", "test/signature/signed-xlsx-sales.xlsx", "test/signature/signed-text-outline-glow.pptx"];

fn alert_text(s: &mut Scenario) -> String {
    s.update(|e, _, cx| e.show_signature_report(cx));
    s.settle(); // the system's judgment of the certificate is made in the background
    s.with(|e, _| e.alert.as_ref().map(|(t, m)| format!("{t}\n{m}"))).expect("the report is shown")
}

/// One edit of the file, in the way each screen takes one.
fn edit(s: &mut Scenario, path: &str) {
    if path.ends_with(".xlsx") {
        s.type_text("x");
        s.key("enter");
    } else if path.ends_with(".docx") {
        s.type_text("x");
    } else {
        s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetExtendedFeatures { enabled: true }], cx));
    }
    s.settle();
    assert!(s.with(|e, _| e.is_dirty()), "{path}: edited");
}

fn issue232_signed_files_show_the_bar_and_the_report() {
    for path in SIGNED {
        let mut s = Scenario::open(path);
        assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::DigitalSignature), "{path}");
        assert!(s.update(|e, _, cx| e.render_infobar(cx)).is_some(), "{path}");
        i18n::show(Language::Japanese);
        let ja = alert_text(&mut s);
        assert!(ja.contains("\u{96fb}\u{5b50}\u{7f72}\u{540d}\u{306e}\u{78ba}\u{8a8d}") && ja.contains("\u{7f72}\u{540d}\u{5024}\u{ff1a}\u{4e00}\u{81f4}") && ja.contains("ibisOffice Test Signer"), "{path}: {ja}");
        assert!(ja.contains("\u{4e00}\u{81f4}\u{3057}\u{3066}\u{3044}\u{307e}\u{3059}") && ja.contains("\u{8a3c}\u{660e}\u{66f8}\u{ff1a}") && ja.contains("\u{30bf}\u{30a4}\u{30e0}\u{30b9}\u{30bf}\u{30f3}\u{30d7}\u{3068}\u{3001}\u{7f72}\u{540d}\u{304c}\u{6587}\u{66f8}\u{5168}\u{4f53}"), "{path}: {ja}");
        // The same in English, from the tables.
        s.update(|e, _, cx| e.dismiss_alert(cx));
        i18n::show(Language::English);
        let en = alert_text(&mut s);
        i18n::show(Language::Japanese);
        assert!(en.contains("Check Digital Signatures") && en.contains("Signature value: matches") && en.contains("Certificate: ") && en.contains("Time stamps and whether") && en.contains("were not checked"), "{path}: {en}");
        assert!(!en.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c)), "{path}: no Japanese in the English report: {en}");
    }
}

fn issue232_a_changed_file_is_reported_as_not_matching() {
    let dir = crate::shell::scenario::repo_root().join("target/signature-test");
    std::fs::create_dir_all(&dir).unwrap();
    let mut package = Package::open(std::fs::read(crate::shell::scenario::repo_root().join(SIGNED[0])).unwrap()).unwrap();
    let mut bytes = package.read("word/styles.xml").unwrap();
    bytes.extend_from_slice(b"\n");
    package.set_bytes("word/styles.xml", bytes);
    let changed = dir.join("changed-signed.docx");
    std::fs::write(&changed, package.save(Default::default()).unwrap()).unwrap();
    let mut s = Scenario::open("target/signature-test/changed-signed.docx");
    assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::DigitalSignature));
    i18n::show(Language::Japanese);
    let ja = alert_text(&mut s);
    assert!(ja.contains("\u{4e00}\u{81f4}\u{3057}\u{306a}\u{3044}\u{90e8}\u{5206}\u{304c}\u{3042}\u{308a}\u{307e}\u{3059}") && ja.contains("word/styles.xml\u{ff1a}\u{4e00}\u{81f4}\u{3057}\u{307e}\u{305b}\u{3093}") && ja.contains("\u{7f72}\u{540d}\u{5024}\u{ff1a}\u{4e00}\u{81f4}"), "{ja}");
    assert!(ja.contains("\u{5bfe}\u{8c61}\u{90e8}\u{5206}\u{306e}\u{30c0}\u{30a4}\u{30b8}\u{30a7}\u{30b9}\u{30c8}\u{ff1a}9 \u{4ef6}\u{4e2d} 8 \u{4ef6}\u{304c}\u{4e00}\u{81f4}"), "{ja}");
    let _ = std::fs::remove_file(&changed);
}

fn issue232_saving_an_edited_signed_file_asks_and_cancel_writes_nothing() {
    for path in SIGNED {
        let mut s = Scenario::open(path);
        let file = s.with(|e, _| e.path.clone()).expect("the copy's path");
        // Saved with no edit: no question, and the saved file is the original, so its signature still agrees.
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none()), "{path}: no edit, no question");
        let saved = std::fs::read(&file).unwrap();
        let status = |bytes: &[u8]| match inspect(&Package::open(bytes.to_vec()).unwrap()) {
            Inspection::Signed(reports) => reports[0].status,
            other => panic!("{other:?}"),
        };
        assert_eq!(status(&saved), Status::Consistent, "{path}: the untouched save keeps the signature");
        let original = saved;

        edit(&mut s, path);
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_some()), "{path}: the edited file asks");
        assert_eq!(std::fs::read(&file).unwrap(), original, "{path}: nothing is written while it asks");
        // \u{30ad}\u{30e3}\u{30f3}\u{30bb}\u{30eb}: nothing written, and it asks again next time.
        s.update(|e, _, cx| e.answer_ask(false, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none() && e.is_dirty()), "{path}");
        assert_eq!(std::fs::read(&file).unwrap(), original, "{path}: cancelled");
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_some()), "{path}: asks again after a cancel");
        // OK: the save goes on, and the saved file has no signature any more (Office takes them out too); the bar goes.
        s.update(|e, _, cx| e.answer_ask(true, cx));
        s.settle();
        let (asking, dirty, status_text) = s.with(|e, _| (e.ask.is_some(), e.is_dirty(), e.status.clone()));
        assert!(!asking && !dirty, "{path}: saved: asking {asking}, dirty {dirty}, status {status_text:?}");
        let written = std::fs::read(&file).unwrap();
        assert_ne!(written, original, "{path}");
        let package = Package::open(written.clone()).unwrap();
        assert_eq!(inspect(&package), Inspection::Unsigned, "{path}: the signatures are gone");
        assert!(!package.names().any(|n| n.starts_with("_xmlsignatures/")), "{path}: no signature part is left");
        let content_types = String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap();
        assert!(!content_types.contains("xmlsignature") && !content_types.contains("sigs"), "{path}: {content_types}");
        let rels = String::from_utf8(package.read("_rels/.rels").unwrap()).unwrap();
        assert!(!rels.contains("digital-signature") && rels.contains("officeDocument"), "{path}: {rels}");
        // What is left hangs together (opens in Office): every internal target of the package's relationships and every Override of
        // [Content_Types].xml is a part of the file.
        for r in package.relationships("").unwrap().iter().filter(|r| !r.external) {
            assert!(package.contains(r.target.trim_start_matches('/')), "{path}: {} has no part", r.target);
        }
        for name in package.names().filter(|n| !n.ends_with('/')) {
            let kind = package.content_type(name).unwrap();
            assert!(kind.is_some(), "{path}: {name} has no content type");
        }
        for part in Package::open(original.clone()).unwrap().names().filter(|n| n.starts_with("xl/worksheets/") || n.starts_with("word/") || n.starts_with("ppt/slides/")) {
            assert!(package.contains(part), "{path}: {part} lost");
        }
        assert!(!s.with(|e, _| e.has_digital_signature()) && s.with(|e, _| e.infobar_kind()) != Some(InfoBarKind::DigitalSignature), "{path}: the bar is gone");
        // The saved file opens again, unsigned, with nothing to ask.
        let reopened = ibis_formats::open(written.clone()).unwrap_or_else(|e| panic!("{path}: opens again: {e}"));
        assert!(!reopened.source.has_signature(), "{path}: opened again: unsigned");
        // Edited again, saved without the question.
        edit(&mut s, path);
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none() && !e.is_dirty()), "{path}: not asked twice");
    }
}

fn issue232_exports_of_a_signed_file_ask() {
    for path in SIGNED {
        let mut s = Scenario::open(path);
        // An export never carries the signatures, edited or not: the question comes before the file window; cancel ends it.
        s.update(|e, w, cx| e.export_pdf(w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.as_ref().is_some_and(|a| a.message.contains("\u{66f8}\u{304d}\u{51fa}\u{3057}\u{305f}"))), "{path}: export asks");
        s.update(|e, _, cx| e.answer_ask(false, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none()), "{path}");
        // A copy asks only when edited.
        s.update(|e, w, cx| e.save_copy(w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none()), "{path}: an unedited copy is the original");
    }
}

fn issue232_unsigned_files_ask_nothing() {
    for path in ["test/editing/docx-paste-source.docx", "test/editing/ibis-src.xlsx", "test/editing/88-review-order.pptx"] {
        let mut s = Scenario::open(path);
        assert_ne!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::DigitalSignature), "{path}");
        assert!(!s.with(|e, _| e.has_digital_signature()), "{path}");
        edit(&mut s, path);
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        let (asked, dirty, msg) = s.with(|e, _| (e.ask.is_some(), e.is_dirty(), e.status.clone()));
        assert!(!asked && !dirty, "{path}: saved with no question: asked {asked} ({msg:?}), dirty {dirty}");
        // An export of an unsigned file asks nothing either.
        s.update(|e, w, cx| e.export_pdf(w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_none()), "{path}: the export asks nothing");
    }
}
