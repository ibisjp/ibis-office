// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #235: the report of the signatures of a PDF, an XPS and an OFD (the information bar's button, the words of the document screens' report), and the text of
//! an XPS and an OFD picked by dragging and copied as a PDF's is. The files are made here (synthetic: a signature in each).
use crate::shell::i18n::{self, Language};
use crate::shell::infobar::InfoBarKind;
use crate::shell::scenario::Scenario;
use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue235_a_signed_pdf_has_the_bar_and_the_report", signed_pdf),
        ("shell::issue235_a_signed_ofd_has_the_bar_and_the_report", signed_ofd),
        ("shell::issue235_a_signed_xps_has_the_bar_and_the_report", signed_xps),
        ("shell::issue235_text_of_an_xps_is_picked_and_copied", xps_text),
        ("shell::issue235_text_of_an_ofd_is_picked_and_copied", ofd_text),
    ]
}

fn write(name: &str, bytes: &[u8]) -> String {
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue235-viewer-src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
    format!("target/scenario/issue235-viewer-src/{name}")
}

/// A one page PDF with text and a signature field (the signature's byte range is a made one: the report says what the file is, whatever it finds).
fn signed_pdf_bytes() -> Vec<u8> {
    let page = "BT /F1 24 Tf 72 720 Td (Signed PDF text) Tj ET";
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] /SigFlags 3 >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len()),
        "<< /FT /Sig /T (Signature1) /V 6 0 R >>".into(),
        "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 100 200 300] /Contents <00> /Name (ibisOffice Test Signer) /Reason (Approval) >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1));
    for o in &offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1));
    out.into_bytes()
}

fn xps_bytes() -> Vec<u8> {
    let mut w = Writer::new();
    let meta = EntryMeta::default();
    w.add("FixedDocSeq.fdseq", meta, br#"<?xml version="1.0" encoding="utf-8"?><FixedDocumentSequence xmlns="http://schemas.microsoft.com/xps/2005/06"><DocumentReference Source="Documents/1/FixedDoc.fdoc"/></FixedDocumentSequence>"#).unwrap();
    w.add("Documents/1/FixedDoc.fdoc", meta, br#"<?xml version="1.0" encoding="utf-8"?><FixedDocument xmlns="http://schemas.microsoft.com/xps/2005/06"><PageContent Source="Pages/1.fpage"/></FixedDocument>"#).unwrap();
    w.add("Documents/1/Pages/1.fpage", meta, r##"<?xml version="1.0" encoding="utf-8"?><FixedPage Width="793.7" Height="1122.5" xmlns="http://schemas.microsoft.com/xps/2005/06" xml:lang="ja-JP"><Glyphs OriginX="50" OriginY="80" FontRenderingEmSize="16" Fill="#FF112233" UnicodeString="XPSテスト"/></FixedPage>"##.as_bytes()).unwrap();
    w.add("_xmlsignatures/sig1.psdsxs", meta, b"<Signature/>").unwrap();
    w.finish().unwrap()
}

fn ofd_bytes() -> Vec<u8> {
    let mut w = Writer::new();
    let meta = EntryMeta::default();
    w.add("OFD.xml", meta, br#"<?xml version="1.0" encoding="UTF-8"?><ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.1"><ofd:DocBody><ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot><ofd:Signatures>Doc_0/Signs/Signatures.xml</ofd:Signatures></ofd:DocBody></ofd:OFD>"#).unwrap();
    w.add("Doc_0/Document.xml", meta, br#"<?xml version="1.0" encoding="UTF-8"?><ofd:Document xmlns:ofd="http://www.ofdspec.org/2016"><ofd:CommonData><ofd:PageArea><ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox></ofd:PageArea></ofd:CommonData><ofd:Pages><ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/></ofd:Pages></ofd:Document>"#).unwrap();
    w.add("Doc_0/Signs/Signatures.xml", meta, br#"<?xml version="1.0" encoding="UTF-8"?><ofd:Signatures xmlns:ofd="http://www.ofdspec.org/2016"><ofd:Signature ID="s1" BaseLoc="Sign_0/Signature.xml"/></ofd:Signatures>"#).unwrap();
    let page = r#"<?xml version="1.0" encoding="UTF-8"?><ofd:Page xmlns:ofd="http://www.ofdspec.org/2016"><ofd:Content><ofd:Layer ID="1"><ofd:TextObject ID="3" Boundary="15 15 80 10" Font="1" Size="4"><ofd:FillColor Value="0 0 0"/><ofd:TextCode X="0" Y="5">OFD Signed Text</ofd:TextCode></ofd:TextObject></ofd:Layer></ofd:Content></ofd:Page>"#.as_bytes();
    w.add("Doc_0/Pages/Page_0/Content.xml", meta, page).unwrap();
    // A digest that is not the page's: the reference does not match.
    let digest = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let sig = format!(r#"<?xml version="1.0" encoding="UTF-8"?><ofd:Signature xmlns:ofd="http://www.ofdspec.org/2016"><ofd:SignedInfo><ofd:SignatureMethod>1.2.156.10197.1.501</ofd:SignatureMethod><ofd:References CheckMethod="1.2.156.10197.1.401"><ofd:Reference FileRef="/Doc_0/Pages/Page_0/Content.xml"><ofd:CheckValue>{digest}</ofd:CheckValue></ofd:Reference></ofd:References></ofd:SignedInfo><ofd:SignedValue>SignedValue.dat</ofd:SignedValue></ofd:Signature>"#);
    w.add("Doc_0/Signs/Sign_0/Signature.xml", meta, sig.as_bytes()).unwrap();
    w.add("Doc_0/Signs/Sign_0/SignedValue.dat", meta, &[0u8; 64]).unwrap();
    w.finish().unwrap()
}

/// The report in the two languages of the tables; the bar is there and says it is only looked at.
fn report_in_both(file: &str, ja: &[&str], en: &[&str], no_ja_in_en: bool) {
    let mut s = Scenario::open(file);
    assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::DigitalSignature), "{file}: the bar");
    assert!(s.update(|e, _, cx| e.render_infobar(cx)).is_some(), "{file}");
    for (lang, words) in [(Language::Japanese, ja), (Language::English, en)] {
        i18n::show(lang);
        s.update(|e, _, cx| e.show_signature_report(cx));
        let shown = s.with(|e, _| e.alert.as_ref().map(|(t, m)| format!("{t}\n{m}"))).expect("the report is shown");
        for w in words {
            assert!(shown.contains(w), "{file} {lang:?}: {w} in {shown}");
        }
        if lang == Language::English && no_ja_in_en {
            assert!(!shown.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c)), "{file}: no Japanese in the English report: {shown}");
        }
        s.update(|e, _, cx| e.dismiss_alert(cx));
    }
    i18n::show(Language::Japanese);
}

fn signed_pdf() {
    let file = write("signed.pdf", &signed_pdf_bytes());
    report_in_both(&file, &["電子署名の確認", "Signature1", "ibisOffice Test Signer", "Approval", "adbe.pkcs7.detached", "署名値（証明書による署名）"], &["Check Digital Signatures", "Signature1", "ibisOffice Test Signer", "Approval", "adbe.pkcs7.detached", "were not checked"], true);
}

fn signed_ofd() {
    let file = write("signed.ofd", &ofd_bytes());
    // The reference's digest is not the page's: the report says it does not match, and how many digests do.
    report_in_both(&file, &["電子署名の確認", "s1", "一致しない部分があります", "1 件中 0 件が一致"], &["Check Digital Signatures", "s1", "something does not match", "0 of 1 match"], true);
}

fn signed_xps() {
    let file = write("signed.xps", &xps_bytes());
    report_in_both(&file, &["電子署名の確認", "電子署名が 1 件あります"], &["Check Digital Signatures", "has 1 digital signature(s)"], true);
}

fn clipboard(s: &mut Scenario) -> Option<String> {
    s.with_app(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn position(s: &mut Scenario, page: usize, index: usize) -> (f32, f32) {
    s.with(|e, _| ibis_office_pdf::editor::char_position(e, page, index)).expect("the page was drawn")
}

fn picks_and_copies(file: &str, first: &str, all: &str) {
    let mut s = Scenario::open(file);
    assert_eq!(s.with(|e, _| e.kind()), ibis_formats::Kind::Pdf);
    assert_eq!(s.menu_state("edit.copy").map(|m| m.0), Some(false), "{file}: nothing picked, the copy is grey");
    let (a, b) = (position(&mut s, 0, 0), position(&mut s, 0, first.chars().count() - 1));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::selected_text(e)), first, "{file}");
    s.menu("edit.copy");
    assert_eq!(clipboard(&mut s).as_deref(), Some(first), "{file}");
    s.menu("edit.select-all");
    s.menu("edit.copy");
    assert_eq!(clipboard(&mut s).as_deref(), Some(all), "{file}: all of it");
}

fn xps_text() {
    let file = write("text.xps", &xps_bytes());
    picks_and_copies(&file, "XPS", "XPSテスト");
}

fn ofd_text() {
    let file = write("text.ofd", &ofd_bytes());
    picks_and_copies(&file, "OFD", "OFD Signed Text");
}
