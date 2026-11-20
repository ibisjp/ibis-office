// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use ibis_pdf::cos::{CosDict, CosObj, ObjectId, StreamData};
use ibis_pdf::scene::collect_pages;
use ibis_pdf::signature::{
    ByteRangeStatus, SubFilter, compute_byte_range_sha256, inspect_signatures,
};
use ibis_pdf::{PdfContainer, SaveMode};
use sha2::{Digest, Sha256};

fn build_synthetic_pdf(page_count: usize, content_text: &str) -> Vec<u8> {
    use std::io::Write as _;
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");

    let mut offsets: Vec<(u32, usize)> = Vec::new();

    // 1: Catalog
    offsets.push((1, out.len()));
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // 2: Pages (MediaBox inherited by kids)
    offsets.push((2, out.len()));
    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", 3 + i * 2))
        .collect();
    let _ = writeln!(
        out,
        "2 0 obj\n<< /Type /Pages /Count {page_count} /Kids [{}] /MediaBox [0 0 595 842] >>\nendobj",
        kids.join(" ")
    );

    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(content_text.as_bytes(), 6);
    for i in 0..page_count {
        let page_num = 3 + i * 2;
        let stm_num = page_num + 1;
        offsets.push((page_num as u32, out.len()));
        let _ = writeln!(
            out,
            "{page_num} 0 obj\n<< /Type /Page /Parent 2 0 R /Contents {stm_num} 0 R >>\nendobj"
        );
        offsets.push((stm_num as u32, out.len()));
        let _ = writeln!(
            out,
            "{stm_num} 0 obj\n<< /Filter /FlateDecode /Length {} >>\nstream",
            compressed.len()
        );
        out.extend_from_slice(&compressed);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }

    let size = 3 + page_count * 2;
    let xref_offset = out.len();
    let _ = writeln!(out, "xref\n0 {size}");
    out.extend_from_slice(b"0000000000 65535 f \n");
    for &(_, off) in &offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = writeln!(
        out,
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );
    out
}

fn build_signed_pdf() -> (Vec<u8>, [u8; 32]) {
    use std::io::Write as _;
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");

    let mut offsets = Vec::new();
    offsets.push((1, out.len()));
    out.extend_from_slice(
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>\nendobj\n",
    );
    offsets.push((2, out.len()));
    out.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 595 842] >>\nendobj\n",
    );
    offsets.push((3, out.len()));
    out.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n");
    offsets.push((4, out.len()));
    out.extend_from_slice(
        b"4 0 obj\n<< /FT /Sig /T (Signature1) /V 5 0 R >>\nendobj\n",
    );

    offsets.push((5, out.len()));
    // Fixed-width placeholder for ByteRange and Contents so we can patch offsets cleanly.
    let br_placeholder = b"[0000000000 0000000000 0000000000 0000000000]";
    let contents_hex = format!("<{}>", "AA".repeat(32));
    let _ = write!(
        out,
        "5 0 obj\n<< /Type /Sig /SubFilter /adbe.pkcs7.detached /Name (Masaharu) /Reason (Approval) /ByteRange "
    );
    let br_pos = out.len();
    out.extend_from_slice(br_placeholder);
    out.extend_from_slice(b" /Contents ");
    let contents_start = out.len();
    out.extend_from_slice(contents_hex.as_bytes());
    let contents_end = out.len();
    out.extend_from_slice(b" >>\nendobj\n");

    let xref_offset = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for off in offsets {
        let _ = writeln!(out, "{:010} 00000 n ", off.1);
    }
    let _ = writeln!(
        out,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );

    let br = [0usize, contents_start, contents_end, out.len() - contents_end];
    let br_formatted = format!("[{:010} {:010} {:010} {:010}]", br[0], br[1], br[2], br[3]);
    assert_eq!(br_formatted.len(), br_placeholder.len());
    out[br_pos..br_pos + br_placeholder.len()].copy_from_slice(br_formatted.as_bytes());

    let mut hasher = Sha256::new();
    hasher.update(&out[br[0]..br[0] + br[1]]);
    hasher.update(&out[br[2]..br[2] + br[3]]);
    let digest: [u8; 32] = hasher.finalize().into();
    (out, digest)
}

#[test]
fn parses_pages_and_decodes_flate_stream() {
    let pdf = build_synthetic_pdf(2, "BT /F1 12 Tf 72 720 Td (Hello) Tj ET");
    let mut container = PdfContainer::from_bytes(pdf).expect("valid PDF");
    assert_eq!(container.revisions().len(), 1);

    let pages = collect_pages(&mut container).expect("pages");
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].media_box.width(), 595.0);
    assert_eq!(pages[1].media_box.height(), 842.0);

    let page0 = container.resolve(pages[0].object_id).expect("page 0");
    let stm_id = page0.as_dict().unwrap().get_ref(b"Contents").unwrap();
    let CosObj::Stream { dict, data } = container.resolve(stm_id).unwrap() else {
        panic!("expected stream");
    };
    let decoded = container.decode_stream(&dict, &data).unwrap();
    assert_eq!(decoded, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET");
}

#[test]
fn incremental_update_preserves_original_bytes_and_revisions() {
    let original = build_synthetic_pdf(1, "BT ET");
    let mut container = PdfContainer::from_bytes(original.clone()).unwrap();

    // Add a new annotation-like dictionary object and attach it to Page 1.
    let mut annot = CosDict::new();
    annot.set(b"Type".to_vec(), CosObj::Name(b"Annot".to_vec()));
    annot.set(b"Subtype".to_vec(), CosObj::Name(b"Text".to_vec()));
    let annot_id = container.alloc_object(CosObj::Dict(annot));

    let page_id = ObjectId::new(3, 0);
    let mut page_dict = container.resolve(page_id).unwrap().as_dict().unwrap().clone();
    page_dict.set(b"Annots".to_vec(), CosObj::Array(vec![CosObj::Ref(annot_id)]));
    container.set_object(page_id, CosObj::Dict(page_dict));

    let updated = container.save(SaveMode::Incremental).unwrap();
    assert!(
        updated.starts_with(&original),
        "incremental update must not alter original bytes"
    );

    let mut reloaded = PdfContainer::from_bytes(updated).unwrap();
    assert_eq!(reloaded.revisions().len(), 2);

    // Revision 0 has no /Annots; Revision 1 has /Annots.
    let rev0_page = reloaded.resolve_at(page_id, 0).unwrap();
    assert!(rev0_page.as_dict().unwrap().get(b"Annots").is_none());

    let rev1_page = reloaded.resolve_at(page_id, 1).unwrap();
    let annots = rev1_page.as_dict().unwrap().get_array(b"Annots").unwrap();
    assert_eq!(annots, &[CosObj::Ref(annot_id)]);
    assert_eq!(reloaded.changed_objects_after(0), vec![3, annot_id.num]);
}

#[test]
fn signature_byte_range_survives_incremental_update() {
    let (signed_pdf, expected_digest) = build_signed_pdf();
    let mut container = PdfContainer::from_bytes(signed_pdf).unwrap();

    let sigs = inspect_signatures(&mut container).unwrap();
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].field_name, "Signature1");
    assert_eq!(sigs[0].signer_name.as_deref(), Some("Masaharu"));
    assert_eq!(sigs[0].sub_filter, Some(SubFilter::AdbePkcs7Detached));
    assert_eq!(sigs[0].status, ByteRangeStatus::CoversWholeFile);
    assert_eq!(
        compute_byte_range_sha256(&container, &sigs[0]).unwrap(),
        expected_digest
    );

    // Append a new object in an incremental update.
    let note_id = container.alloc_object(CosObj::Stream {
        dict: CosDict::new(),
        data: StreamData::Owned {
            bytes: b"post-sign note".to_vec(),
            encoded: false,
        },
    });
    let updated_pdf = container.save(SaveMode::Incremental).unwrap();

    let mut reloaded = PdfContainer::from_bytes(updated_pdf).unwrap();
    let sigs_after = inspect_signatures(&mut reloaded).unwrap();
    assert_eq!(sigs_after.len(), 1);
    assert_eq!(
        sigs_after[0].status,
        ByteRangeStatus::CoversRevisionWithSubsequentUpdates {
            revision: 0,
            changed_objects: vec![note_id.num],
        }
    );
    // The signed byte range digest itself must remain completely intact.
    assert_eq!(
        compute_byte_range_sha256(&reloaded, &sigs_after[0]).unwrap(),
        expected_digest
    );
}
