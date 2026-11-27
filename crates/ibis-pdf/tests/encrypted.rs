// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PDFs encrypted with the Standard security handler and the empty user password (synthetic fixtures made with
//! pymupdf: one page "Encrypted text 0190001", the title "Synthetic title").

use ibis_pdf::{CosObj, PdfContainer, SaveMode};

const FIXTURES: [(&str, &[u8]); 4] = [
    ("RC4 40-bit, R2", include_bytes!("fixtures/encrypted/rc4-40.pdf")),
    ("RC4 128-bit, R3", include_bytes!("fixtures/encrypted/rc4-128.pdf")),
    ("AES-128, R4", include_bytes!("fixtures/encrypted/aes-128.pdf")),
    ("AES-256, R6", include_bytes!("fixtures/encrypted/aes-256.pdf")),
];

#[test]
fn strings_of_an_encrypted_file_are_decrypted() {
    for (name, bytes) in FIXTURES {
        let mut container = PdfContainer::from_bytes(bytes.to_vec()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(container.is_encrypted(), "{name}");
        let trailer = container.revisions()[container.latest_revision_index()].trailer.clone();
        let info = container.deref(trailer.get(b"Info").expect("an /Info")).unwrap();
        let title = match info.as_dict().and_then(|d| d.get(b"Title")) {
            Some(CosObj::Str(s)) => s.to_text_lossy(),
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(title, "Synthetic title", "{name}");
    }
}

#[test]
fn streams_of_an_encrypted_file_are_decrypted() {
    for (name, bytes) in FIXTURES {
        let mut container = PdfContainer::from_bytes(bytes.to_vec()).unwrap();
        let pages = ibis_pdf::scene::collect_pages(&mut container).unwrap_or_else(|e| panic!("{name}: {e}"));
        let page = container.resolve(pages[0].object_id).unwrap();
        let mut contents = container.deref(page.as_dict().unwrap().get(b"Contents").unwrap()).unwrap();
        if let CosObj::Array(parts) = &contents {
            contents = container.deref(&parts[0]).unwrap();
        }
        let CosObj::Stream { dict, data } = contents else { panic!("{name}: contents not a stream") };
        let decoded = container.decode_stream(&dict, &data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = String::from_utf8_lossy(&decoded);
        assert!(text.contains("Tj") || text.contains("TJ"), "{name}: {text}");
    }
}

#[test]
fn a_file_that_needs_a_user_password_says_so() {
    let bytes = include_bytes!("fixtures/encrypted/aes-256-user-password.pdf");
    let error = PdfContainer::from_bytes(bytes.to_vec()).err().expect("an error");
    assert!(error.message().contains("password"), "{}", error.message());
}

#[test]
fn an_encrypted_file_is_not_updated_incrementally_but_can_be_rewritten_in_the_clear() {
    let (_, bytes) = FIXTURES[3];
    let mut container = PdfContainer::from_bytes(bytes.to_vec()).unwrap();
    let id = container.alloc_object(CosObj::Int(1));
    assert!(container.save(SaveMode::Incremental).is_err());
    let clear = container.save(SaveMode::FullRewrite).unwrap();
    assert!(!clear.windows(8).any(|w| w == b"/Encrypt"));
    let mut reopened = PdfContainer::from_bytes(clear).unwrap();
    assert!(!reopened.is_encrypted());
    assert_eq!(reopened.resolve(id).unwrap(), CosObj::Int(1));
}

#[cfg(feature = "interpret")]
#[test]
fn the_text_of_an_encrypted_file_is_read() {
    for (name, bytes) in FIXTURES {
        let mut container = PdfContainer::from_bytes(bytes.to_vec()).unwrap();
        let mut text = String::new();
        ibis_pdf::interpret::visit_text_scenes(&mut container, |scene| {
            text.push_str(&ibis_pdf::text_layout::page_text(&scene.text_index, ibis_pdf::text_layout::TextLayout::Reading));
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(text, "Encrypted text 0190001\n", "{name}");
    }
}
