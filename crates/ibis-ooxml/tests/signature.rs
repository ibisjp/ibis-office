// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Inspecting the digital signatures of a package (issue #232). The signed files in `test/signature/` are made by
//! `scripts/gen-signed-fixtures.py` (a throw-away self-signed RSA certificate, the hashed and signed bytes written by hand
//! from the Canonical XML rules); the changed files here are those with one part changed.

use ibis_ooxml::signature::{Check, Inspection, Malformed, Reason, Status, has_signature, inspect};
use ibis_ooxml::Package;

fn open(name: &str) -> Package {
    let path = format!("{}/../../test/{name}", env!("CARGO_MANIFEST_DIR"));
    Package::open(std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

const SIGNED: [(&str, &str); 3] = [
    ("signature/signed-docx-paste-source.docx", "word/document.xml"),
    ("signature/signed-xlsx-sales.xlsx", "xl/workbook.xml"),
    ("signature/signed-text-outline-glow.pptx", "ppt/presentation.xml"),
];

fn only_report(package: &Package) -> ibis_ooxml::signature::SignatureReport {
    match inspect(package) {
        Inspection::Signed(mut reports) => {
            assert_eq!(reports.len(), 1);
            reports.remove(0)
        }
        other => panic!("{other:?}"),
    }
}

fn edit_signature(package: &mut Package, from: &str, to: &str) {
    let text = String::from_utf8(package.read("_xmlsignatures/sig1.xml").unwrap()).unwrap();
    assert!(text.contains(from), "{from} is not in the signature");
    package.set_bytes("_xmlsignatures/sig1.xml", text.replacen(from, to, 1).into_bytes());
}

#[test]
fn unsigned_files_are_unsigned() {
    for name in ["editing/docx-paste-source.docx", "editing/xlsx-databar-2010.xlsx", "web-check/text-outline-glow.pptx"] {
        let package = open(name);
        assert!(!has_signature(&package), "{name}");
        assert_eq!(inspect(&package), Inspection::Unsigned, "{name}");
    }
}

#[test]
fn signed_files_are_consistent_in_all_three_formats() {
    for (name, _) in SIGNED {
        let package = open(name);
        assert!(has_signature(&package), "{name}");
        let report = only_report(&package);
        assert_eq!(report.part, "_xmlsignatures/sig1.xml", "{name}");
        assert_eq!(report.status, Status::Consistent, "{name}: {report:?}");
        assert_eq!(report.signature_value, Some(Check::Match), "{name}");
        assert_eq!(report.signer.as_deref(), Some("ibisOffice Test Signer"), "{name}");
        // The Object in SignedInfo, and every part in the Manifest (rels with the relationship transform, our own XML part with c14n).
        assert!(report.references.iter().any(|r| !r.in_manifest), "{name}");
        assert!(report.references.iter().filter(|r| r.in_manifest).count() >= 3, "{name}");
        assert!(report.references.iter().all(|r| r.check == Check::Match), "{name}: {:?}", report.references);
        assert!(report.references.iter().any(|r| r.uri.starts_with("/customXml/signed-data.xml?ContentType=")), "{name}");
    }
}

#[test]
fn a_changed_part_is_a_mismatch_of_that_part_only() {
    for (name, part) in SIGNED {
        let mut package = open(name);
        let mut bytes = package.read(part).unwrap();
        bytes.extend_from_slice(b"\n");
        package.set_bytes(part, bytes);
        let report = only_report(&package);
        assert_eq!(report.status, Status::Mismatch, "{name}");
        assert_eq!(report.signature_value, Some(Check::Match), "{name}");
        let bad: Vec<&str> = report.references.iter().filter(|r| r.check == Check::Mismatch).map(|r| r.uri.as_str()).collect();
        assert_eq!(bad.len(), 1, "{name}: {bad:?}");
        assert!(bad[0].starts_with(&format!("/{part}?")), "{name}: {bad:?}");
    }
}

#[test]
fn a_removed_part_is_missing() {
    let mut package = open(SIGNED[0].0);
    package.remove("word/styles.xml");
    let report = only_report(&package);
    assert_eq!(report.status, Status::Mismatch);
    assert_eq!(report.references.iter().filter(|r| r.check == Check::PartMissing).count(), 1);
}

#[test]
fn a_changed_signature_value_is_a_mismatch() {
    let mut package = open(SIGNED[0].0);
    let text = String::from_utf8(package.read("_xmlsignatures/sig1.xml").unwrap()).unwrap();
    let start = text.find("<SignatureValue>").unwrap() + "<SignatureValue>".len();
    let flipped = if &text[start..start + 1] == "A" { "B" } else { "A" };
    package.set_bytes("_xmlsignatures/sig1.xml", format!("{}{}{}", &text[..start], flipped, &text[start + 1..]).into_bytes());
    let report = only_report(&package);
    assert_eq!(report.status, Status::Mismatch);
    assert_eq!(report.signature_value, Some(Check::Mismatch));
    assert!(report.references.iter().all(|r| r.check == Check::Match));
}

#[test]
fn a_changed_signed_info_breaks_the_signature_value() {
    let mut package = open(SIGNED[1].0);
    // Another hash for the Object: SignedInfo changed, so its signature no longer holds (and the digest is not the Object's).
    edit_signature(&mut package, "<DigestValue>", "<DigestValue>AAAA");
    let report = only_report(&package);
    assert_eq!(report.status, Status::Mismatch);
    assert_eq!(report.signature_value, Some(Check::Mismatch));
    assert_eq!(report.references.iter().filter(|r| r.check == Check::Mismatch).count(), 1);
}

#[test]
fn an_algorithm_that_is_not_handled_is_unsupported_not_a_mismatch() {
    let mut package = open(SIGNED[0].0);
    edit_signature(&mut package, "xmldsig-more#rsa-sha256", "xmldsig-more#ecdsa-sha256");
    let report = only_report(&package);
    assert_eq!(report.status, Status::Unsupported);
    assert_eq!(report.signature_value, Some(Check::Unsupported("http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256".to_string())));

    let mut package = open(SIGNED[0].0);
    // A digest method and a transform of the manifest that are not handled (the Object's digest then no longer agrees too).
    edit_signature(&mut package, "xmlenc#sha256\"></DigestMethod><DigestValue>", "xmldsig-more#md5\"></DigestMethod><DigestValue>");
    let report = only_report(&package);
    assert!(report.references.iter().any(|r| matches!(r.check, Check::Unsupported(_))), "{report:?}");

    let mut package = open(SIGNED[0].0);
    edit_signature(&mut package, "2001/REC-xml-c14n-20010315\"></Transform>", "2002/xslt-unknown\"></Transform>");
    let report = only_report(&package);
    assert!(report.references.iter().any(|r| matches!(&r.check, Check::Unsupported(a) if a.contains("xslt-unknown"))), "{report:?}");
}

#[test]
fn no_key_or_an_external_reference_cannot_be_inspected_and_nothing_is_fetched() {
    let mut package = open(SIGNED[0].0);
    let text = String::from_utf8(package.read("_xmlsignatures/sig1.xml").unwrap()).unwrap();
    let a = text.find("<KeyInfo>").unwrap();
    let b = text.find("</KeyInfo>").unwrap() + "</KeyInfo>".len();
    package.set_bytes("_xmlsignatures/sig1.xml", format!("{}{}", &text[..a], &text[b..]).into_bytes());
    let report = only_report(&package);
    assert_eq!(report.signature_value, Some(Check::Uninspectable(Reason::NoKey)));
    assert_eq!(report.status, Status::Uninspectable);

    let mut package = open(SIGNED[0].0);
    edit_signature(&mut package, "/word/styles.xml?", "http://example.invalid/styles.xml?");
    let report = only_report(&package);
    let external: Vec<_> = report.references.iter().filter(|r| r.check == Check::Uninspectable(Reason::External)).collect();
    assert_eq!(external.len(), 1, "{report:?}");
}

#[test]
fn broken_structures_are_told_apart() {
    // Not XML.
    let mut package = open(SIGNED[0].0);
    package.set_bytes("_xmlsignatures/sig1.xml", b"not xml <".to_vec());
    assert_eq!(only_report(&package).malformed, Some(Malformed::Xml));
    // XML but not a signature.
    let mut package = open(SIGNED[0].0);
    package.set_bytes("_xmlsignatures/sig1.xml", b"<a/>".to_vec());
    let report = only_report(&package);
    assert_eq!((report.status, report.malformed), (Status::Malformed, Some(Malformed::NotSignature)));
    // A signature without its value, and without references.
    let mut package = open(SIGNED[0].0);
    package.set_bytes("_xmlsignatures/sig1.xml", b"<Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><SignedInfo/></Signature>".to_vec());
    assert_eq!(only_report(&package).malformed, Some(Malformed::NoSignatureValue));
    let mut package = open(SIGNED[0].0);
    package.set_bytes("_xmlsignatures/sig1.xml", b"<Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><SignedInfo/><SignatureValue>AAAA</SignatureValue></Signature>".to_vec());
    assert_eq!(only_report(&package).malformed, Some(Malformed::NoReference));
    // The signature part the relationship names is gone.
    let mut package = open(SIGNED[0].0);
    package.remove("_xmlsignatures/sig1.xml");
    assert_eq!(inspect(&package), Inspection::Malformed(Malformed::SignaturePartMissing));
    assert!(has_signature(&package));
    // Signature parts nothing points to.
    let mut package = open(SIGNED[0].0);
    let rels = package.relationships("").unwrap();
    let origin = rels.iter().find(|r| r.kind.ends_with("digital-signature/origin")).unwrap();
    package.remove_relationship("", &origin.id).unwrap();
    assert_eq!(inspect(&package), Inspection::Malformed(Malformed::OriginMissing));
}

#[test]
fn the_signature_does_not_change_when_the_file_is_saved_untouched() {
    let package = open(SIGNED[0].0);
    let saved = package.save(Default::default()).unwrap();
    let again = Package::open(saved).unwrap();
    assert_eq!(only_report(&again).status, Status::Consistent);
}

#[test]
fn stripping_takes_the_signatures_out_and_nothing_else() {
    for (name, main) in SIGNED {
        let original = open(name);
        let bytes = {
            let path = format!("{}/../../test/{name}", env!("CARGO_MANIFEST_DIR"));
            std::fs::read(path).unwrap()
        };
        let stripped = ibis_ooxml::signature::strip_bytes(&bytes).unwrap().unwrap_or_else(|| panic!("{name}: has signatures"));
        let package = Package::open(stripped.clone()).unwrap();
        assert_eq!(inspect(&package), Inspection::Unsigned, "{name}");
        assert!(!has_signature(&package), "{name}");
        assert!(!package.names().any(|n| n.starts_with("_xmlsignatures/")), "{name}");
        let types = String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap();
        assert!(!types.contains("xmlsignature") && !types.contains("sigs"), "{name}: {types}");
        let rels = String::from_utf8(package.read("_rels/.rels").unwrap()).unwrap();
        assert!(!rels.contains("digital-signature") && rels.contains("officeDocument"), "{name}: {rels}");
        // Every other part is the same bytes as before, and none is lost.
        for part in original.names().filter(|n| !n.starts_with("_xmlsignatures/") && *n != "[Content_Types].xml" && *n != "_rels/.rels") {
            assert_eq!(package.read(part).unwrap(), original.read(part).unwrap(), "{name}: {part}");
        }
        assert!(package.contains(main), "{name}");
        // Unsigned: nothing to take out the second time.
        assert!(ibis_ooxml::signature::strip_bytes(&stripped).unwrap().is_none(), "{name}");
    }
}
