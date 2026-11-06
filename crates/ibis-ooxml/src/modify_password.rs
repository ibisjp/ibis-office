// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 書き込みパスワード (the password to modify, `p:modifyVerifier`): never kept as the
//! password, but as a hash. The hash is that of the salt and the password
//! (UTF-16LE), then `spinCount` times of the hash and the round's number (4
//! bytes, little-endian), ECMA-376 Part 4, 14.8.1 (the same way as a protected
//! sheet's). SHA-1 (RustCrypto's `sha1`; older files), SHA-256, SHA-384 and SHA-512 (`sha2`).

use sha2::{Digest, Sha256, Sha384, Sha512};

/// The hash a password is made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    /// `cryptAlgorithmSid`: 4 SHA-1, 12 SHA-256, 13 SHA-384, 14 SHA-512 (the others, MD2 to RIPEMD, are not supported).
    pub fn from_sid(sid: u32) -> Option<Self> {
        match sid {
            4 => Some(Self::Sha1),
            12 => Some(Self::Sha256),
            13 => Some(Self::Sha384),
            14 => Some(Self::Sha512),
            _ => None,
        }
    }

    /// `algorithmName`: `SHA-1`, `SHA-256`, `SHA-384`, `SHA-512`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "SHA-1" | "SHA1" => Some(Self::Sha1),
            "SHA-256" | "SHA256" => Some(Self::Sha256),
            "SHA-384" | "SHA384" => Some(Self::Sha384),
            "SHA-512" | "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }

    pub fn sid(self) -> u32 {
        match self {
            Self::Sha1 => 4,
            Self::Sha256 => 12,
            Self::Sha384 => 13,
            Self::Sha512 => 14,
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => sha1::Sha1::digest(data).to_vec(),
            Self::Sha256 => Sha256::digest(data).to_vec(),
            Self::Sha384 => Sha384::digest(data).to_vec(),
            Self::Sha512 => Sha512::digest(data).to_vec(),
        }
    }
}

/// The hash of `password` with `salt`, `spin` rounds.
pub fn hash(algorithm: Algorithm, salt: &[u8], password: &str, spin: u32) -> Vec<u8> {
    let mut input = salt.to_vec();
    input.extend(password.encode_utf16().flat_map(|u| u.to_le_bytes()));
    let mut h = algorithm.digest(&input);
    for round in 0..spin {
        let mut next = h.clone();
        next.extend_from_slice(&round.to_le_bytes());
        h = algorithm.digest(&next);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_spun_one() {
        let salt = [1u8, 2, 3, 4];
        // No rounds: the hash of the salt and the UTF-16LE password.
        let mut input = salt.to_vec();
        input.extend([b'a', 0, b'b', 0]);
        assert_eq!(hash(Algorithm::Sha512, &salt, "ab", 0), Sha512::digest(&input).to_vec());
        // One round: the hash of that and 0u32.
        let once = Sha512::digest(&input).to_vec();
        let mut next = once;
        next.extend([0, 0, 0, 0]);
        assert_eq!(hash(Algorithm::Sha512, &salt, "ab", 1), Sha512::digest(&next).to_vec());
        // Another password, another salt or another number of rounds: another hash.
        assert_ne!(hash(Algorithm::Sha512, &salt, "ab", 5), hash(Algorithm::Sha512, &salt, "abc", 5));
        assert_ne!(hash(Algorithm::Sha512, &salt, "ab", 5), hash(Algorithm::Sha512, &[9], "ab", 5));
        assert_ne!(hash(Algorithm::Sha512, &salt, "ab", 5), hash(Algorithm::Sha512, &salt, "ab", 6));
        // Japanese passwords are the UTF-16 code units.
        assert_eq!(hash(Algorithm::Sha256, &salt, "あ", 0).len(), 32);
        assert_eq!(Algorithm::from_sid(14), Some(Algorithm::Sha512));
        assert_eq!(Algorithm::from_name("SHA-384").map(Algorithm::sid), Some(13));
        assert_eq!(Algorithm::from_sid(4), Some(Algorithm::Sha1));
        assert_eq!(Algorithm::from_name("SHA-1").map(Algorithm::sid), Some(4));
        assert_eq!(Algorithm::from_sid(3), None);
        assert_eq!(hash(Algorithm::Sha1, &salt, "ab", 0).len(), 20);
    }
}
