// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A password-protected old workbook (RC4, and RC4 CryptoAPI; the fixtures are a fictional workbook protected with
//! the password `pass`, checked against msoffcrypto-tool's decryption) is asked for its password and opened with it.
use ibis_doc_model::sheet_calc::sheet_of_slide;

#[test]
fn protected_old_workbooks_open_with_their_password() {
    for name in ["protected_rc4.xls", "protected_cryptoapi.xls"] {
        let data = std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(ibis_formats::is_encrypted(&data), "{name}");
        assert!(ibis_formats::read(&data).is_err(), "{name}: not without the password");
        assert_eq!(ibis_formats::decrypt(&data, "wrong").err(), Some(ibis_formats::DecryptError::WrongPassword), "{name}");
        let book = ibis_formats::decrypt(&data, "pass").unwrap();
        let p = ibis_formats::read(&book).unwrap();
        assert_eq!(p.slides.len(), 2, "{name}");
        let sheet = sheet_of_slide(&p.slides[0]).unwrap();
        assert_eq!(sheet.cell(0, 0).unwrap().text, "Title", "{name}");
        assert_eq!(sheet.cell(1, 1).unwrap().text, "1,234.50", "{name}");
    }
}

/// A workbook Excel protected with a password (test/editing/xls-password.xls, password ibis-test-1234): asked for it,
/// the wrong one refused, the right one opens it.
#[test]
fn an_excel_protected_old_workbook_opens_with_its_password() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xls-password.xls")).unwrap();
    assert!(ibis_formats::is_encrypted(&data));
    assert_eq!(ibis_formats::decrypt(&data, "wrong").err(), Some(ibis_formats::DecryptError::WrongPassword));
    let book = ibis_formats::decrypt(&data, "ibis-test-1234").unwrap();
    let p = ibis_formats::read(&book).unwrap();
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    assert_eq!(sheet.cell(0, 0).unwrap().value, ibis_doc_model::CellValue::Text("品名".into()));
    assert_eq!(sheet.cell(2, 1).unwrap().value, ibis_doc_model::CellValue::Number(20.0));
}
