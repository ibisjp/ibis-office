// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The password of a protected sheet, as Excel keeps it: never the
//! password itself, but a hash: SHA-512 of a random salt and the password
//! (UTF-16LE), then again `spinCount` times with the round's number
//! (4 bytes, little-endian) after it, Base64 (`algorithmName`,
//! `hashValue`, `saltValue`, `spinCount` of `sheetProtection`). Older files
//! keep a 16-bit hash (`password`), checked too. SHA-512 and Base64 are
//! written here (FIPS 180-4, RFC 4648) rather than added as dependencies.

use ibis_doc_model::{PasswordHash, SheetProtection};

/// Excel's rounds.
pub(crate) const SPIN_COUNT: u32 = 100_000;

const K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2, 0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65, 0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df, 0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec, 0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// SHA-512 of `data`.
pub(crate) fn sha512(data: &[u8]) -> [u8; 64] {
    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1, 0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 128 != 112 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u128) * 8).to_be_bytes());
    for block in message.chunks(128) {
        let mut w = [0u64; 80];
        for (i, word) in block.chunks(8).enumerate() {
            w[i] = u64::from_be_bytes(word.try_into().expect("8 bytes"));
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..80 {
            let s1 = v[4].rotate_right(14) ^ v[4].rotate_right(18) ^ v[4].rotate_right(41);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(28) ^ v[0].rotate_right(34) ^ v[0].rotate_right(39);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 64];
    for (i, word) in h.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&word.to_be_bytes());
    }
    out
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut count = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = BASE64.iter().position(|&b| b == c)? as u32;
        bits = bits << 6 | v;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count & 0xFF) as u8);
        }
    }
    Some(out)
}

/// The hash of `password` with `salt`, `spin_count` rounds.
pub(crate) fn hash(password: &str, salt: &[u8], spin_count: u32) -> [u8; 64] {
    let mut data = salt.to_vec();
    for unit in password.encode_utf16() {
        data.extend_from_slice(&unit.to_le_bytes());
    }
    let mut h = sha512(&data);
    let mut round = [0u8; 68];
    for i in 0..spin_count {
        round[..64].copy_from_slice(&h);
        round[64..].copy_from_slice(&i.to_le_bytes());
        h = sha512(&round);
    }
    h
}

/// 16 random bytes for a salt (not secret: it only keeps the same password
/// from giving the same hash).
fn salt() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for (i, part) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(crate::clock::unix_millis() as u64 ^ (i as u64) << 32);
        part.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

/// The hash Excel keeps for `password` (none for an empty one).
pub fn password_hash(password: &str) -> Option<PasswordHash> {
    if password.is_empty() {
        return None;
    }
    let salt = salt();
    Some(PasswordHash { algorithm: "SHA-512".into(), hash: base64(&hash(password, &salt, SPIN_COUNT)), salt: base64(&salt), spin_count: SPIN_COUNT })
}

/// Excel's old 16-bit hash, as 4 hex digits.
pub(crate) fn legacy_hash(password: &str) -> String {
    let bytes: Vec<u8> = password.chars().take(15).map(|c| (c as u32 & 0xFF) as u8).collect();
    if bytes.is_empty() {
        return "0000".into();
    }
    let mut h: u16 = 0;
    for &b in bytes.iter().rev() {
        h = ((h >> 14) & 1) | ((h << 1) & 0x7FFF);
        h ^= b as u16;
    }
    h = ((h >> 14) & 1) | ((h << 1) & 0x7FFF);
    h ^= bytes.len() as u16;
    h ^= 0xCE4B;
    format!("{h:04X}")
}

/// Whether `password` unprotects the sheet (no password: any, even empty).
pub fn opens(protection: &SheetProtection, password: &str) -> bool {
    if let Some(h) = &protection.hash {
        if !h.algorithm.eq_ignore_ascii_case("SHA-512") {
            // Another algorithm (SHA-1, …): not checked here.
            return false;
        }
        let (Some(salt), Some(expected)) = (unbase64(&h.salt), unbase64(&h.hash)) else { return false };
        return hash(password, &salt, h.spin_count) == expected.as_slice();
    }
    match &protection.legacy {
        Some(legacy) => legacy.eq_ignore_ascii_case(&legacy_hash(password)),
        None => true,
    }
}

/// Whether `password` unprotects the workbook (no password: any, even empty).
pub fn opens_workbook(protection: &ibis_doc_model::WorkbookProtection, password: &str) -> bool {
    if let Some(h) = &protection.hash {
        if !h.algorithm.eq_ignore_ascii_case("SHA-512") {
            return false;
        }
        let (Some(salt), Some(expected)) = (unbase64(&h.salt), unbase64(&h.hash)) else { return false };
        return hash(password, &salt, h.spin_count) == expected.as_slice();
    }
    match &protection.legacy {
        Some(legacy) => legacy.eq_ignore_ascii_case(&legacy_hash(password)),
        None => true,
    }
}

/// Whether `password` unprotects the range (no password: any, even empty).
pub fn opens_range(range: &ibis_doc_model::ProtectedRange, password: &str) -> bool {
    if let Some(h) = &range.hash {
        if !h.algorithm.eq_ignore_ascii_case("SHA-512") {
            return false;
        }
        let (Some(salt), Some(expected)) = (unbase64(&h.salt), unbase64(&h.hash)) else { return false };
        return hash(password, &salt, h.spin_count) == expected.as_slice();
    }
    match &range.legacy {
        Some(legacy) => legacy.eq_ignore_ascii_case(&legacy_hash(password)),
        None => true,
    }
}


#[cfg(test)]
mod tests {
    use super::{SPIN_COUNT, SheetProtection, base64, legacy_hash, opens, opens_workbook, password_hash, sha512, unbase64};

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn sha512_vectors() {
        assert_eq!(hex(&sha512(b"abc")), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
        assert_eq!(&hex(&sha512(b""))[..16], "cf83e1357eefb8bd");
        // Two blocks.
        let long = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        assert_eq!(&hex(&sha512(long))[..16], "8e959b75dae313da");
    }

    #[test]
    fn base64_round_trip() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(unbase64("TWE=").unwrap(), b"Ma");
    }

    #[test]
    fn hashes_check_the_password() {
        let p = SheetProtection { hash: password_hash("secret"), ..Default::default() };
        assert!(opens(&p, "secret"));
        assert!(!opens(&p, "Secret"));
        assert_eq!(p.hash.as_ref().unwrap().spin_count, SPIN_COUNT);
        let legacy = SheetProtection { legacy: Some(legacy_hash("password")), ..Default::default() };
        assert!(opens(&legacy, "password") && !opens(&legacy, "passw0rd"));
        assert_eq!(legacy_hash("password"), "83AF");
        assert!(opens(&SheetProtection::default(), ""));

        let wb_p = ibis_doc_model::WorkbookProtection { hash: password_hash("wb_secret"), lock_structure: true, ..Default::default() };
        assert!(opens_workbook(&wb_p, "wb_secret"));
        assert!(!opens_workbook(&wb_p, "wrong"));
        assert!(opens_workbook(&ibis_doc_model::WorkbookProtection::default(), ""));
    }
}
