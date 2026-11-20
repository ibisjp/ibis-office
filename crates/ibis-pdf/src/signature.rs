// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PDF digital signature field inspection, `/ByteRange` revision verification,
//! and digest computation.

use sha2::{Digest, Sha256};

use crate::container::PdfContainer;
use crate::cos::{CosDict, CosObj, ObjectId};
use crate::{Error, Result};

/// PDF `/SubFilter` values for digital signatures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubFilter {
    AdbePkcs7Detached,
    AdbePkcs7Sha1,
    EtsiCadesDetached,
    EtsiRfc3161,
    Other(String),
}

impl SubFilter {
    fn from_name(bytes: &[u8]) -> Self {
        match bytes {
            b"adbe.pkcs7.detached" => Self::AdbePkcs7Detached,
            b"adbe.pkcs7.sha1" => Self::AdbePkcs7Sha1,
            b"ETSI.CAdES.detached" => Self::EtsiCadesDetached,
            b"ETSI.RFC3161" => Self::EtsiRfc3161,
            other => Self::Other(String::from_utf8_lossy(other).into_owned()),
        }
    }
}

/// Structural integrity of a signature's `/ByteRange` against the PDF's revision history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ByteRangeStatus {
    /// Covers the entire PDF file up to the final `%%EOF`.
    CoversWholeFile,
    /// Covers revision `rev` cleanly, followed by incremental updates touching `changed_objects`.
    CoversRevisionWithSubsequentUpdates {
        revision: usize,
        changed_objects: Vec<u32>,
    },
    /// ByteRange is malformed, out of bounds, or does not align with any revision boundary.
    InvalidOrUnaligned,
}

/// Metadata extracted from a PDF signature field (`/FT /Sig`) and its `/V` dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureRecord {
    pub field_obj: ObjectId,
    pub sig_obj: ObjectId,
    pub field_name: String,
    pub signer_name: Option<String>,
    pub reason: Option<String>,
    pub byte_range: [usize; 4],
    pub sub_filter: Option<SubFilter>,
    pub cms_bytes: Vec<u8>,
    pub status: ByteRangeStatus,
}

/// Collect all signature records reachable from `/Root -> /AcroForm -> /Fields`.
pub fn inspect_signatures(container: &mut PdfContainer) -> Result<Vec<SignatureRecord>> {
    let (_, catalog) = container.catalog()?;
    let Some(acro_obj) = catalog.get(b"AcroForm").cloned() else {
        return Ok(Vec::new());
    };
    let acro_resolved = container.deref(&acro_obj)?;
    let Some(acro_dict) = acro_resolved.as_dict() else {
        return Ok(Vec::new());
    };
    let Some(fields) = acro_dict.get_array(b"Fields").map(|a| a.to_vec()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for field_ref in fields {
        if let Some(field_id) = field_ref.as_ref() {
            collect_field_signatures(container, field_id, "", &mut out)?;
        }
    }
    Ok(out)
}

fn collect_field_signatures(
    container: &mut PdfContainer,
    field_id: ObjectId,
    parent_name: &str,
    out: &mut Vec<SignatureRecord>,
) -> Result<()> {
    let field_obj = container.resolve(field_id)?;
    let Some(dict) = field_obj.as_dict().cloned() else {
        return Ok(());
    };

    let local_name = match dict.get(b"T") {
        Some(CosObj::Str(s)) => s.to_text_lossy(),
        _ => String::new(),
    };
    let full_name = match (parent_name.is_empty(), local_name.is_empty()) {
        (true, _) => local_name,
        (false, true) => parent_name.to_string(),
        (false, false) => format!("{parent_name}.{local_name}"),
    };

    if dict.get_name(b"FT") == Some(b"Sig")
        && let Some(sig_id) = dict.get_ref(b"V")
        && let Ok(sig_val) = container.resolve(sig_id)
        && let Some(sig_dict) = sig_val.as_dict()
        && let Some(rec) = parse_sig_dict(container, field_id, sig_id, &full_name, sig_dict)
    {
        out.push(rec);
    }

    if let Some(kids) = dict.get_array(b"Kids").map(|k| k.to_vec()) {
        for kid in kids {
            if let Some(kid_id) = kid.as_ref() {
                collect_field_signatures(container, kid_id, &full_name, out)?;
            }
        }
    }
    Ok(())
}

fn parse_sig_dict(
    container: &PdfContainer,
    field_obj: ObjectId,
    sig_obj: ObjectId,
    field_name: &str,
    sig_dict: &CosDict,
) -> Option<SignatureRecord> {
    let br_arr = sig_dict.get_array(b"ByteRange")?;
    if br_arr.len() != 4 {
        return None;
    }
    let byte_range = [
        br_arr[0].as_int()?.max(0) as usize,
        br_arr[1].as_int()?.max(0) as usize,
        br_arr[2].as_int()?.max(0) as usize,
        br_arr[3].as_int()?.max(0) as usize,
    ];
    let cms_bytes = match sig_dict.get(b"Contents") {
        Some(CosObj::Str(s)) => s.as_bytes().to_vec(),
        _ => Vec::new(),
    };
    let sub_filter = sig_dict.get_name(b"SubFilter").map(SubFilter::from_name);
    let signer_name = match sig_dict.get(b"Name") {
        Some(CosObj::Str(s)) => Some(s.to_text_lossy()),
        _ => None,
    };
    let reason = match sig_dict.get(b"Reason") {
        Some(CosObj::Str(s)) => Some(s.to_text_lossy()),
        _ => None,
    };

    let status = evaluate_byte_range(container, byte_range);

    Some(SignatureRecord {
        field_obj,
        sig_obj,
        field_name: field_name.to_string(),
        signer_name,
        reason,
        byte_range,
        sub_filter,
        cms_bytes,
        status,
    })
}

fn evaluate_byte_range(container: &PdfContainer, br: [usize; 4]) -> ByteRangeStatus {
    let raw_len = container.raw_bytes().len();
    let [off1, len1, off2, len2] = br;
    let Some(end1) = off1.checked_add(len1) else {
        return ByteRangeStatus::InvalidOrUnaligned;
    };
    let Some(end2) = off2.checked_add(len2) else {
        return ByteRangeStatus::InvalidOrUnaligned;
    };
    if off1 != 0 || end1 >= off2 || end2 > raw_len {
        return ByteRangeStatus::InvalidOrUnaligned;
    }

    let revs = container.revisions();
    let Some(rev_idx) = revs.iter().position(|r| r.eof_offset == end2) else {
        return ByteRangeStatus::InvalidOrUnaligned;
    };
    if rev_idx + 1 == revs.len() {
        ByteRangeStatus::CoversWholeFile
    } else {
        ByteRangeStatus::CoversRevisionWithSubsequentUpdates {
            revision: rev_idx,
            changed_objects: container.changed_objects_after(rev_idx),
        }
    }
}

/// Compute the SHA-256 digest over the two byte ranges specified in `record.byte_range`.
pub fn compute_byte_range_sha256(
    container: &PdfContainer,
    record: &SignatureRecord,
) -> Result<[u8; 32]> {
    let raw = container.raw_bytes();
    let [off1, len1, off2, len2] = record.byte_range;
    let slice1 = raw
        .get(off1..off1.saturating_add(len1))
        .ok_or_else(|| Error::new("ByteRange part 1 out of bounds"))?;
    let slice2 = raw
        .get(off2..off2.saturating_add(len2))
        .ok_or_else(|| Error::new("ByteRange part 2 out of bounds"))?;
    let mut hasher = Sha256::new();
    hasher.update(slice1);
    hasher.update(slice2);
    Ok(hasher.finalize().into())
}
