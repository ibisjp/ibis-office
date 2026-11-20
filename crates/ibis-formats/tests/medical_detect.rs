// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The medical readers (FHIR, CDA, HL7 v2, DICOM, DPC E/F, RECEIPTC.UKE) take a file by what is in it, so an ordinary JSON, CSV, TSV, XML or text file must
//! not be taken for one of them; and the documents they make are in the screen's language.
use ibis_formats::{dicom, dpc, fhir, uke};

const FHIR_BUNDLE: &str = include_str!("fixtures/synthetic_fhir_3doc6info.json");
const UKE: &[u8] = include_bytes!("fixtures/synthetic_receiptc.uke");

fn fhir_md(text: &str) -> bool {
    fhir::input_to_markdown(text.as_bytes()).expect("no error").is_some()
}

#[test]
fn ordinary_json_is_not_fhir() {
    for text in [
        r#"{"name": "app", "version": "1.0.0", "dependencies": {"a": "^1"}}"#,
        r#"{"resourceType": "Spreadsheet", "rows": []}"#,
        r#"{"note": "the resourceType Patient and Bundle are FHIR words", "items": [1, 2, 3]}"#,
        r#"{"data": {"resourceType": "Patient", "id": "1"}}"#,
        r#"["resourceType", "Patient", "Bundle"]"#,
        r#"{"resourceType": 12}"#,
        r#"{"resourceType": "patient"}"#,
        "{}",
        "{not json at all, resourceType Patient",
    ] {
        assert!(!fhir_md(text), "taken for FHIR: {text}");
    }
}

#[test]
fn fhir_json_xml_cda_and_hl7_v2_are_taken() {
    assert!(fhir_md(FHIR_BUNDLE));
    assert!(fhir_md(r#"{"resourceType": "Patient", "id": "p1", "gender": "male"}"#));
    assert!(fhir_md(r#"<Bundle xmlns="http://hl7.org/fhir"><id value="b"/></Bundle>"#));
    assert!(fhir_md(r#"<ClinicalDocument xmlns="urn:hl7-org:v3"><title>T</title></ClinicalDocument>"#));
    assert!(fhir_md("MSH|^~\\&|A|B|C|D|20261101||ADT^A01|MSG1|P|2.5\rPID|1||123\r"));
}

#[test]
fn ordinary_xml_and_text_are_not_medical() {
    for text in [
        r#"<?xml version="1.0"?><root><Bundle>a bundle of sticks</Bundle></root>"#,
        r#"<Bundle>not in the FHIR namespace</Bundle>"#,
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>see http://hl7.org/fhir for the Bundle and Composition</body></html>"#,
        r#"<note xmlns="http://hl7.org/fhir"><to>x</to></note>"#,
        r#"<ClinicalDocument>no namespace</ClinicalDocument>"#,
        r#"<Other xmlns="urn:hl7-org:v3"/>"#,
        "MSH|only a line starting like a message",
        "MSH|^~\\&|a|b\nnot a segment",
        "MSH is a name; MSH|^~\\&| in the middle",
        "plain text",
    ] {
        assert!(!fhir_md(text), "taken for FHIR, CDA or HL7: {text}");
    }
}

#[test]
fn ordinary_csv_and_tsv_are_not_dpc_or_uke() {
    for data in [
        "a,b,c\n1,2,3\n",
        "id\tname\tdate\n1\tx\t2026-11-01\n",
        "IR,1,2\n",
        "IR,1,13,1,0123456,,clinic,50810,03\nXX,1,2,3,4,5\n",
        "IR,1,13,1,0123456,,clinic,50810,03\nIR,again,2,3,4,5\n",
        "name,age,city,country,phone,mail\nA,1,x,y,z,w\n",
        "RE,00001,1112,50810,x,1,4550512,,100\n",
        "\u{65bd}\u{8a2d}\u{30b3}\u{30fc}\u{30c9}\t\u{30c7}\u{30fc}\u{30bf}\u{8b58}\u{5225}\u{756a}\u{53f7}\n1\t2\n",
        "\u{65bd}\u{8a2d}\u{30b3}\u{30fc}\u{30c9}\t\u{9000}\u{9662}\u{5e74}\u{6708}\u{65e5}\t\u{5165}\u{9662}\u{5e74}\u{6708}\u{65e5}\n1\t2\t3\n",
    ] {
        assert!(!dpc::is_dpc_ef(data.as_bytes()), "taken for DPC E/F: {data:?}");
        assert!(!uke::is_uke(data.as_bytes()), "taken for UKE: {data:?}");
    }
}

#[test]
fn the_real_forms_are_taken() {
    assert!(uke::is_uke(UKE));
    assert!(!dpc::is_dpc_ef(UKE));
    let dpc_text = "\u{65bd}\u{8a2d}\u{30b3}\u{30fc}\u{30c9}\t\u{30c7}\u{30fc}\u{30bf}\u{8b58}\u{5225}\u{756a}\u{53f7}\t\u{9000}\u{9662}\u{5e74}\u{6708}\u{65e5}\t\u{5165}\u{9662}\u{5e74}\u{6708}\u{65e5}\t\u{30c7}\u{30fc}\u{30bf}\u{533a}\u{5206}\n0123456\t0000123456\t20261110\t20261101\t11\n";
    assert!(dpc::is_dpc_ef(dpc_text.as_bytes()));
    assert!(!uke::is_uke(dpc_text.as_bytes()));
}

#[test]
fn dicom_needs_its_prefix() {
    assert!(!dicom::is_dicom(b"DICM"));
    assert!(!dicom::is_dicom(&[0u8; 200]));
    let mut data = vec![0u8; 132];
    data[128..132].copy_from_slice(b"DICM");
    assert!(dicom::is_dicom(&data));
}

/// The headings, sheet names and notes of the documents are in the language of the screen (Japanese by default, English after `set_english_names`).
#[test]
fn the_documents_follow_the_language() {
    let japanese = fhir::input_to_markdown(FHIR_BUNDLE.as_bytes()).unwrap().unwrap();
    assert!(japanese.contains("## \u{60a3}\u{8005}\u{60c5}\u{5831}"), "{japanese}");
    ibis_formats::set_english_names(true);
    let english = fhir::input_to_markdown(FHIR_BUNDLE.as_bytes()).unwrap().unwrap();
    let opened = uke::open_uke(UKE).unwrap();
    ibis_formats::set_english_names(false);
    assert!(english.contains("## Patient information"), "{english}");
    assert!(english.contains("| Patient ID |"), "{english}");
    assert!(!english.contains("\u{60a3}\u{8005}\u{60c5}\u{5831}"));
    let names: Vec<String> = opened.presentation.workbook.as_ref().unwrap().sheets.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names, vec!["Receipt details".to_string(), "Receipt summary".to_string()]);
    // A document made in English is read back as FHIR in either language.
    let docx = ibis_formats::markdown::open_markdown(english.into_bytes()).unwrap();
    let json = fhir::flow_to_fhir_bundle_json(&docx.presentation).expect("a bundle");
    assert!(json.contains("\"Patient\""), "{json}");
}
