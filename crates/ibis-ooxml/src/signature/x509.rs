// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The little of an X.509 certificate (RFC 5280) and of RSA (RFC 8017, RSASSA-PKCS1-v1_5) that checking a signature value takes:
//! the subject's name and the public key out of the DER, and the verification. Nothing about the certificate is judged (not its
//! validity period, its chain, its revocation).

use num_bigint::BigUint;

struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
}

/// One DER element (single-byte tag) at the start of `data`, and what follows it.
fn read(data: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    let tag = *data.first()?;
    let first = *data.get(1)?;
    let (len, header) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = len << 8 | *data.get(2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let end = header.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    Some((Tlv { tag, content: &data[header..end] }, &data[end..]))
}

fn items(mut data: &[u8]) -> Option<Vec<Tlv<'_>>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let (item, rest) = read(data)?;
        out.push(item);
        data = rest;
    }
    Some(out)
}

fn oid(content: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut value = 0u64;
    for &b in content {
        value = value << 7 | (b & 0x7f) as u64;
        if b & 0x80 == 0 {
            if parts.is_empty() {
                let (first, second) = if value < 40 { (0, value) } else if value < 80 { (1, value - 40) } else { (2, value - 80) };
                parts.push(first.to_string());
                parts.push(second.to_string());
            } else {
                parts.push(value.to_string());
            }
            value = 0;
        }
    }
    parts.join(".")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    Rsa { modulus: Vec<u8>, exponent: Vec<u8> },
    /// Another kind of key: the algorithm's OID (`1.2.840.10045.2.1` for an elliptic-curve key).
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    /// The subject's common name (or, failing that, organization), as written in the certificate.
    pub subject: Option<String>,
    pub key: PublicKey,
}

const OID_RSA: &str = "1.2.840.113549.1.1.1";

pub fn parse_certificate(der: &[u8]) -> Option<Certificate> {
    let (cert, _) = read(der)?;
    if cert.tag != 0x30 {
        return None;
    }
    let (tbs, _) = read(cert.content)?;
    if tbs.tag != 0x30 {
        return None;
    }
    let mut fields = items(tbs.content)?.into_iter();
    let mut next = fields.next()?;
    if next.tag == 0xA0 {
        next = fields.next()?; // the version
    }
    if next.tag != 0x02 {
        return None;
    }
    let _sig_alg = fields.next()?;
    let _issuer = fields.next()?;
    let _validity = fields.next()?;
    let subject = fields.next()?;
    let spki = fields.next()?;
    if subject.tag != 0x30 || spki.tag != 0x30 {
        return None;
    }
    let spki_items = items(spki.content)?;
    let algorithm = items(spki_items.first()?.content)?;
    let algorithm_oid = oid(algorithm.first()?.content);
    let bits = spki_items.get(1)?;
    let key = if algorithm_oid == OID_RSA && bits.tag == 0x03 {
        let (rsa, _) = read(bits.content.get(1..)?)?;
        let parts = items(rsa.content)?;
        PublicKey::Rsa { modulus: strip_zeros(parts.first()?.content), exponent: strip_zeros(parts.get(1)?.content) }
    } else {
        PublicKey::Other(algorithm_oid)
    };
    Some(Certificate { subject: name(subject.content), key })
}

fn strip_zeros(bytes: &[u8]) -> Vec<u8> {
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    bytes[start..].to_vec()
}

/// The common name (2.5.4.3) of a Name, else the organization (2.5.4.10).
fn name(content: &[u8]) -> Option<String> {
    let mut common = None;
    let mut organization = None;
    for set in items(content)? {
        for pair in items(set.content)? {
            let parts = items(pair.content)?;
            let (Some(key), Some(value)) = (parts.first(), parts.get(1)) else { continue };
            let text = match value.tag {
                0x1e => {
                    let units: Vec<u16> = value.content.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                    String::from_utf16_lossy(&units)
                }
                _ => String::from_utf8_lossy(value.content).into_owned(),
            };
            match oid(key.content).as_str() {
                "2.5.4.3" => common = common.or(Some(text)),
                "2.5.4.10" => organization = organization.or(Some(text)),
                _ => {}
            }
        }
    }
    common.or(organization)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    pub fn digest(self, data: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        match self {
            Hash::Sha1 => sha1::Sha1::digest(data).to_vec(),
            Hash::Sha256 => sha2::Sha256::digest(data).to_vec(),
            Hash::Sha384 => sha2::Sha384::digest(data).to_vec(),
            Hash::Sha512 => sha2::Sha512::digest(data).to_vec(),
        }
    }

    /// The DER of the DigestInfo up to the hash value (RFC 8017, section 9.2 note 1).
    fn digest_info_prefix(self) -> &'static [u8] {
        match self {
            Hash::Sha1 => &[0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14],
            Hash::Sha256 => &[0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20],
            Hash::Sha384 => &[0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02, 0x05, 0x00, 0x04, 0x30],
            Hash::Sha512 => &[0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03, 0x05, 0x00, 0x04, 0x40],
        }
    }
}

/// RSASSA-PKCS1-v1_5 verification (RFC 8017, section 8.2.2) of `signature` over `message`.
pub fn rsa_pkcs1_v15_verify(modulus: &[u8], exponent: &[u8], hash: Hash, message: &[u8], signature: &[u8]) -> bool {
    let n = BigUint::from_bytes_be(modulus);
    let e = BigUint::from_bytes_be(exponent);
    let k = modulus.len();
    // A key too small to hold the encoded message, or an even or tiny modulus, is no RSA key to trust a result from.
    if k < 11 || signature.len() != k || n.bit_count_is_even_or_small() || e < BigUint::from(3u8) {
        return false;
    }
    let s = BigUint::from_bytes_be(signature);
    if s >= n {
        return false;
    }
    let m = s.modpow(&e, &n).to_bytes_be();
    let mut em = vec![0u8; k - m.len().min(k)];
    em.extend_from_slice(&m);
    let prefix = hash.digest_info_prefix();
    let digest = hash.digest(message);
    let t_len = prefix.len() + digest.len();
    if k < t_len + 11 {
        return false;
    }
    let mut expected = Vec::with_capacity(k);
    expected.extend_from_slice(&[0x00, 0x01]);
    expected.resize(k - t_len - 1, 0xff);
    expected.push(0x00);
    expected.extend_from_slice(prefix);
    expected.extend_from_slice(&digest);
    em == expected
}

trait ModulusCheck {
    fn bit_count_is_even_or_small(&self) -> bool;
}

impl ModulusCheck for BigUint {
    fn bit_count_is_even_or_small(&self) -> bool {
        self.bits() < 512 || !self.bit(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oid_dotted() {
        assert_eq!(oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01]), OID_RSA);
        assert_eq!(oid(&[0x55, 0x04, 0x03]), "2.5.4.3");
    }
}
