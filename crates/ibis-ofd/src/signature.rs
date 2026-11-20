// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OFD digital signature verification (SM3 reference digests and SM2withSM3).

use base64::Engine as _;
use ibis_ooxml::zip::Archive;
use roxmltree::Document as XmlDoc;
use sm2::dsa::VerifyingKey;
use sm2::dsa::signature::Verifier as _;
use sm2::elliptic_curve::pkcs8::DecodePublicKey as _;
use sm3::Digest as _;

use crate::{child_text, is_tag, parent_dir, read_entry_bytes, read_entry_utf8, resolve_path};

const SM2_WITH_SM3_OID: &str = "1.2.156.10197.1.501";
const DEFAULT_DIST_ID: &str = "1234567812345678";
const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];

/// Cryptographic signature verification outcome for one OFD signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfdSignatureCryptoResult {
    Valid,
    Invalid,
    NoPublicKey,
    Malformed(String),
    Unsupported(String),
}

/// Verification result for one `<ofd:Signature>` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfdSignatureVerification {
    pub id: String,
    pub references_ok: usize,
    pub references_total: usize,
    pub crypto: OfdSignatureCryptoResult,
    pub sig_algorithm: String,
    pub check_algorithm: String,
    pub subject_cn: Option<String>,
    pub issuer_cn: Option<String>,
}

impl OfdSignatureVerification {
    pub fn all_references_ok(&self) -> bool {
        self.references_total > 0 && self.references_ok == self.references_total
    }
}

/// Verify all signatures listed in `sig_path` (`Signatures.xml`).
pub fn verify_signatures(archive: &Archive, sig_path: &str) -> Vec<OfdSignatureVerification> {
    let Ok(sigs_xml) = read_entry_utf8(archive, sig_path) else {
        return Vec::new();
    };
    let Ok(sigs_doc) = XmlDoc::parse(&sigs_xml) else {
        return Vec::new();
    };
    let sigs_dir = parent_dir(sig_path);

    let mut out = Vec::new();
    for sig_node in sigs_doc.descendants().filter(|n| is_tag(*n, "Signature")) {
        let id = sig_node.attribute("ID").unwrap_or("").to_string();
        let Some(base_loc) = sig_node.attribute("BaseLoc") else {
            continue;
        };
        let Ok(sig_xml_path) = resolve_path(sigs_dir, base_loc) else {
            out.push(malformed_entry(id, "invalid Signature BaseLoc"));
            continue;
        };
        out.push(verify_single_signature(archive, id, &sig_xml_path));
    }
    out
}

fn malformed_entry(id: String, msg: &str) -> OfdSignatureVerification {
    OfdSignatureVerification {
        id,
        references_ok: 0,
        references_total: 0,
        crypto: OfdSignatureCryptoResult::Malformed(msg.to_string()),
        sig_algorithm: "-".into(),
        check_algorithm: "-".into(),
        subject_cn: None,
        issuer_cn: None,
    }
}

fn verify_single_signature(
    archive: &Archive,
    id: String,
    sig_xml_path: &str,
) -> OfdSignatureVerification {
    let Ok(sig_xml_bytes) = read_entry_bytes(archive, sig_xml_path) else {
        return malformed_entry(id, "Signature.xml not found");
    };
    let Ok(sig_xml) = std::str::from_utf8(&sig_xml_bytes) else {
        return malformed_entry(id, "Signature.xml not UTF-8");
    };
    let Ok(doc) = XmlDoc::parse(sig_xml) else {
        return malformed_entry(id, "Signature.xml parse error");
    };

    let root = doc.root_element();
    let signed_info = root.children().find(|n| is_tag(*n, "SignedInfo"));
    let sig_method = signed_info
        .and_then(|si| child_text(si, "SignatureMethod"))
        .unwrap_or("")
        .to_string();

    let refs_node = signed_info.and_then(|si| si.children().find(|n| is_tag(*n, "References")));
    let check_method = refs_node
        .and_then(|rn| rn.attribute("CheckMethod"))
        .unwrap_or("")
        .to_string();

    let sig_dir = parent_dir(sig_xml_path);
    let mut references_total = 0usize;
    let mut references_ok = 0usize;

    if let Some(rn) = refs_node {
        for r in rn.children().filter(|n| is_tag(*n, "Reference")) {
            let Some(file_ref) = r.attribute("FileRef") else {
                continue;
            };
            references_total += 1;
            let Some(expected_b64) = child_text(r, "CheckValue") else {
                continue;
            };
            let Ok(target_path) = resolve_path(sig_dir, file_ref) else {
                continue;
            };
            let Ok(file_bytes) = read_entry_bytes(archive, &target_path) else {
                continue;
            };
            if verify_sm3_digest(&file_bytes, expected_b64) {
                references_ok += 1;
            }
        }
    }

    let Some(signed_value_rel) = child_text(root, "SignedValue") else {
        return OfdSignatureVerification {
            id,
            references_ok,
            references_total,
            crypto: OfdSignatureCryptoResult::Malformed("SignedValue missing".into()),
            sig_algorithm: sig_method,
            check_algorithm: check_method,
            subject_cn: None,
            issuer_cn: None,
        };
    };

    let Ok(signed_value_path) = resolve_path(sig_dir, signed_value_rel) else {
        return OfdSignatureVerification {
            id,
            references_ok,
            references_total,
            crypto: OfdSignatureCryptoResult::Malformed("invalid SignedValue path".into()),
            sig_algorithm: sig_method,
            check_algorithm: check_method,
            subject_cn: None,
            issuer_cn: None,
        };
    };

    let Ok(signed_value_bytes) = read_entry_bytes(archive, &signed_value_path) else {
        return OfdSignatureVerification {
            id,
            references_ok,
            references_total,
            crypto: OfdSignatureCryptoResult::Malformed("SignedValue.dat not found".into()),
            sig_algorithm: sig_method,
            check_algorithm: check_method,
            subject_cn: None,
            issuer_cn: None,
        };
    };

    let (crypto, subject_cn, issuer_cn) =
        verify_crypto(&sig_method, &sig_xml_bytes, &signed_value_bytes);

    OfdSignatureVerification {
        id,
        references_ok,
        references_total,
        crypto,
        sig_algorithm: sig_method,
        check_algorithm: check_method,
        subject_cn,
        issuer_cn,
    }
}

pub fn sm3_digest(bytes: &[u8]) -> [u8; 32] {
    sm3::Sm3::digest(bytes).into()
}

fn verify_sm3_digest(bytes: &[u8], expected_b64: &str) -> bool {
    let Ok(expected) = base64::engine::general_purpose::STANDARD.decode(expected_b64.trim()) else {
        return false;
    };
    expected.as_slice() == sm3_digest(bytes)
}

fn verify_crypto(
    sig_method: &str,
    signature_xml_bytes: &[u8],
    signed_value_bytes: &[u8],
) -> (OfdSignatureCryptoResult, Option<String>, Option<String>) {
    let trimmed = sig_method.trim();
    if trimmed != SM2_WITH_SM3_OID && !trimmed.eq_ignore_ascii_case("sm2-with-sm3") {
        return (
            OfdSignatureCryptoResult::Unsupported(trimmed.to_string()),
            None,
            None,
        );
    }
    if signed_value_bytes.len() <= 72 {
        return (OfdSignatureCryptoResult::NoPublicKey, None, None);
    }

    if let Some((spki_der, sig_der, signed_data, cert_der)) =
        extract_gbt38540(signed_value_bytes)
    {
        let (subject_cn, issuer_cn) = parse_cert_cns(&cert_der);
        let result = run_sm2_verify(&spki_der, &signed_data, &sig_der);
        return (result, subject_cn, issuer_cn);
    }

    if let Some((cert_der, spki_der)) = extract_cms_first_cert(&signed_value_bytes) {
        let (subject_cn, issuer_cn) = parse_cert_cns(&cert_der);
        let result = run_sm2_verify(&spki_der, signature_xml_bytes, signed_value_bytes);
        return (result, subject_cn, issuer_cn);
    }

    (OfdSignatureCryptoResult::NoPublicKey, None, None)
}

fn run_sm2_verify(spki_der: &[u8], message: &[u8], sig_der: &[u8]) -> OfdSignatureCryptoResult {
    let Ok(public_key) = sm2::PublicKey::from_public_key_der(spki_der) else {
        return OfdSignatureCryptoResult::Malformed("malformed SPKI".into());
    };
    let Ok(verifying_key) = VerifyingKey::new(DEFAULT_DIST_ID, public_key) else {
        return OfdSignatureCryptoResult::Malformed("malformed verifying key".into());
    };
    let Some(raw64) = der_ecdsa_sig_to_raw64(sig_der) else {
        return OfdSignatureCryptoResult::Malformed("malformed DER signature".into());
    };
    let Ok(signature) = sm2::dsa::Signature::from_slice(&raw64) else {
        return OfdSignatureCryptoResult::Malformed("malformed SM2 signature".into());
    };
    match verifying_key.verify(message, &signature) {
        Ok(()) => OfdSignatureCryptoResult::Valid,
        Err(_) => OfdSignatureCryptoResult::Invalid,
    }
}

fn der_ecdsa_sig_to_raw64(sig_der: &[u8]) -> Option<[u8; 64]> {
    fn write_scalar(int_bytes: &[u8], slot: &mut [u8; 32]) -> Option<()> {
        let stripped = int_bytes.strip_prefix(&[0x00]).unwrap_or(int_bytes);
        if stripped.len() > 32 {
            return None;
        }
        slot[32 - stripped.len()..].copy_from_slice(stripped);
        Some(())
    }
    let (seq_body, _) = read_tlv(sig_der, 0x30)?;
    let (r_bytes, r_rest) = read_tlv(seq_body, 0x02)?;
    let (s_bytes, _) = read_tlv(r_rest, 0x02)?;
    let mut out = [0u8; 64];
    let (r_slot, s_slot) = out.split_at_mut(32);
    write_scalar(r_bytes, r_slot.try_into().ok()?)?;
    write_scalar(s_bytes, s_slot.try_into().ok()?)?;
    Some(out)
}

fn extract_gbt38540(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> {
    let (body, _) = read_tlv(data, 0x30)?;
    let (e0_tlv, after_e0) = skip_tlv(body)?;
    let (cert_der, after_e1) = read_tlv(after_e0, 0x04)?;
    let (_, after_e2) = skip_tlv(after_e1)?;
    let (bit_value, _) = read_tlv(after_e2, 0x03)?;
    if bit_value.is_empty() {
        return None;
    }
    let sig_der = bit_value[1..].to_vec();
    let (cert_body, _) = read_tlv(cert_der, 0x30)?;
    let spki_der = extract_spki_from_cert_body(cert_body)?;
    Some((spki_der, sig_der, e0_tlv.to_vec(), cert_der.to_vec()))
}

fn extract_cms_first_cert(cms_der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let (_, content_info_body) = read_tlv(cms_der, 0x30)?;
    let (_, after_oid) = skip_tlv(content_info_body)?;
    let (signed_data_explicit, _) = read_tlv(after_oid, 0xA0)?;
    let (signed_data_body, _) = read_tlv(signed_data_explicit, 0x30)?;
    let (_, after_version) = skip_tlv(signed_data_body)?;
    let (_, after_digalgs) = skip_tlv(after_version)?;
    let (_, after_eci) = skip_tlv(after_digalgs)?;
    let (certs, _) = read_tlv(after_eci, 0xA0)?;
    let cert_len = tlv_total_length(certs)?;
    let cert_der = certs.get(..cert_len)?.to_vec();
    let (cert_body, _) = read_tlv(&cert_der, 0x30)?;
    let spki_der = extract_spki_from_cert_body(cert_body)?;
    Some((cert_der, spki_der))
}

fn extract_spki_from_cert_body(cert_body: &[u8]) -> Option<Vec<u8>> {
    let (tbs_body, _) = read_tlv(cert_body, 0x30)?;
    let mut rest = tbs_body;
    if rest.first() == Some(&0xA0) {
        let (_, r) = skip_tlv(rest)?;
        rest = r;
    }
    for _ in 0..5 {
        let (_, r) = skip_tlv(rest)?;
        rest = r;
    }
    let spki_len = tlv_total_length(rest)?;
    Some(rest.get(..spki_len)?.to_vec())
}

fn parse_cert_cns(cert_der: &[u8]) -> (Option<String>, Option<String>) {
    let Some((cert_body, _)) = read_tlv(cert_der, 0x30) else {
        return (None, None);
    };
    let Some((tbs_body, _)) = read_tlv(cert_body, 0x30) else {
        return (None, None);
    };
    let mut rest = tbs_body;
    if rest.first() == Some(&0xA0) {
        let Some((_, r)) = skip_tlv(rest) else {
            return (None, None);
        };
        rest = r;
    }
    for _ in 0..2 {
        let Some((_, r)) = skip_tlv(rest) else {
            return (None, None);
        };
        rest = r;
    }
    let Some((issuer_val, r)) = read_tlv(rest, 0x30) else {
        return (None, None);
    };
    let issuer_cn = extract_cn_from_name(issuer_val);
    let Some((_, r)) = skip_tlv(r) else {
        return (None, issuer_cn);
    };
    let Some((subject_val, _)) = read_tlv(r, 0x30) else {
        return (None, issuer_cn);
    };
    (extract_cn_from_name(subject_val), issuer_cn)
}

fn extract_cn_from_name(name_body: &[u8]) -> Option<String> {
    let mut rest = name_body;
    while !rest.is_empty() {
        let (set_body, after_set) = read_tlv(rest, 0x31).or_else(|| read_tlv(rest, 0x30))?;
        rest = after_set;
        let mut set_rest = set_body;
        while !set_rest.is_empty() {
            let Some((atv_body, after_atv)) = read_tlv(set_rest, 0x30) else {
                break;
            };
            set_rest = after_atv;
            let Some((oid_value, after_oid)) = read_tlv(atv_body, 0x06) else {
                continue;
            };
            if oid_value == OID_COMMON_NAME
                && let Some(&tag) = after_oid.first()
                && matches!(tag, 0x0C | 0x13 | 0x16 | 0x1E)
                && let Some((s_bytes, _)) = read_tlv(after_oid, tag)
                && let Ok(s) = std::str::from_utf8(s_bytes)
            {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn read_tlv(data: &[u8], expected_tag: u8) -> Option<(&[u8], &[u8])> {
    if data.first() != Some(&expected_tag) {
        return None;
    }
    let (len, header_len) = decode_der_length(data.get(1..)?)?;
    let end = 1usize.checked_add(header_len)?.checked_add(len)?;
    if data.len() < end {
        return None;
    }
    Some((&data[1 + header_len..end], &data[end..]))
}

fn skip_tlv(data: &[u8]) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let (len, header_len) = decode_der_length(data.get(1..)?)?;
    let end = 1usize.checked_add(header_len)?.checked_add(len)?;
    if data.len() < end {
        return None;
    }
    Some((&data[..end], &data[end..]))
}

fn tlv_total_length(data: &[u8]) -> Option<usize> {
    if data.is_empty() {
        return None;
    }
    let (len, header_len) = decode_der_length(data.get(1..)?)?;
    1usize.checked_add(header_len)?.checked_add(len)
}

fn decode_der_length(data: &[u8]) -> Option<(usize, usize)> {
    let first = *data.first()?;
    if first < 0x80 {
        Some((first as usize, 1))
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 || data.len() < 1 + n {
            return None;
        }
        let mut len = 0usize;
        for &b in &data[1..1 + n] {
            len = (len << 8) | (b as usize);
        }
        Some((len, 1 + n))
    }
}

/// What the signatures of an OFD, taken together, were found to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureOutcome {
    /// A reference digest or a signature value does not agree with the file.
    Mismatch,
    /// Every reference digest (SM3) agrees and every signature value (SM2) is valid.
    Verified,
    /// Every reference digest agrees; the signature value was not (or could not be) verified.
    DigestsMatch,
}

/// The number of signatures and what they were found to be, for the status bar (the words are the screen's: the caller translates).
pub fn summarize_signatures(verifications: &[OfdSignatureVerification]) -> Option<(usize, SignatureOutcome)> {
    if verifications.is_empty() {
        return None;
    }
    let n = verifications.len();
    let any_tampered = verifications.iter().any(|v| !v.all_references_ok() || matches!(v.crypto, OfdSignatureCryptoResult::Invalid));
    if any_tampered {
        return Some((n, SignatureOutcome::Mismatch));
    }
    let all_crypto_valid = verifications.iter().all(|v| v.all_references_ok() && v.crypto == OfdSignatureCryptoResult::Valid);
    Some((n, if all_crypto_valid { SignatureOutcome::Verified } else { SignatureOutcome::DigestsMatch }))
}
