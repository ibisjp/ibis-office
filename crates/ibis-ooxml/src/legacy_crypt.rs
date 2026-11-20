// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The RC4 encryption of the old binary formats (`.xls`, `.ppt`; [MS-OFFCRYPTO] 2.3.6 and 2.3.5, [MS-XLS]
//! 2.2.10): the password turned into a key, the check of the password against the verifier the file
//! holds, and the cipher that works on a block (of 1024 bytes in a workbook) of the stream. Written from the
//! specifications.
//!
//! Two methods are there:
//! - RC4 ("Office 97/2000 compatible": the header has a salt and a verifier, MD5);
//! - RC4 CryptoAPI ("Office 2003": an encryption header and a verifier, SHA-1, 40 to 128 bits).
//!
//! What the stream of a format does with the blocks (which bytes are not encrypted, how the blocks are
//! numbered) is its own: a workbook's is in `ibis-formats` (`xls`).

use md5::Md5;
use sha1::{Digest, Sha1};

use crate::encryption::DecryptError;

/// The RC4 cipher (a stream cipher of a key of 5 to 16 bytes).
#[derive(Clone)]
pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Self {
        let mut s = [0u8; 256];
        for (k, v) in s.iter_mut().enumerate() {
            *v = k as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        Self { s, i: 0, j: 0 }
    }

    /// The next byte of the key stream.
    pub fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[self.i as usize]);
        self.s.swap(self.i as usize, self.j as usize);
        let k = self.s[(self.s[self.i as usize].wrapping_add(self.s[self.j as usize])) as usize];
        k
    }

    /// `data` encrypted or decrypted (the same thing) from where the key stream is.
    pub fn apply(&mut self, data: &mut [u8]) {
        for b in data {
            *b ^= self.next_byte();
        }
    }

    /// The key stream passed over `n` bytes.
    pub fn skip(&mut self, n: usize) {
        for _ in 0..n {
            self.next_byte();
        }
    }
}

/// How the key is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rc4Method {
    /// MD5, a 40-bit key ("Office 97/2000 compatible").
    Standard,
    /// SHA-1 (RC4 CryptoAPI, "Office 2003").
    CryptoApi,
}

/// What the file holds to check the password and to make the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rc4Params {
    pub method: Rc4Method,
    pub salt: [u8; 16],
    pub encrypted_verifier: [u8; 16],
    /// 16 bytes (MD5) or 20 (SHA-1).
    pub encrypted_verifier_hash: Vec<u8>,
    /// The key size in bits (40 for the standard method).
    pub key_bits: u32,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn damaged(what: &str) -> DecryptError {
    DecryptError::Damaged(what.to_string())
}

/// The parameters from an `EncryptionInfo` of the RC4 methods: its version (major, minor) and what follows (the
/// bytes after `wEncryptionType` of a workbook's `FilePass`; a presentation's `CryptSession10Container` is the
/// same from its version on). The XOR obfuscation and the AES methods are `Unsupported`.
pub fn parse_rc4(info: &[u8]) -> Result<Rc4Params, DecryptError> {
    let (major, minor) = (u16_at(info, 0).ok_or_else(|| damaged("EncryptionInfo"))?, u16_at(info, 2).ok_or_else(|| damaged("EncryptionInfo"))?);
    match (major, minor) {
        (1, 1) => {
            let take = |at: usize, n: usize| info.get(at..at + n).ok_or_else(|| damaged("RC4 のヘッダー"));
            Ok(Rc4Params {
                method: Rc4Method::Standard,
                salt: take(4, 16)?.try_into().map_err(|_| damaged("salt"))?,
                encrypted_verifier: take(20, 16)?.try_into().map_err(|_| damaged("verifier"))?,
                encrypted_verifier_hash: take(36, 16)?.to_vec(),
                key_bits: 40,
            })
        }
        (2..=4, 2) => {
            let flags = u32_at(info, 4).ok_or_else(|| damaged("flags"))?;
            // fCryptoAPI (0x04) must be set; the AES methods (algID 0x660E..) are the Standard encryption of OOXML.
            if flags & 0x04 == 0 {
                return Err(DecryptError::Unsupported("AES".into()));
            }
            let header_size = u32_at(info, 8).ok_or_else(|| damaged("header size"))? as usize;
            let header = info.get(12..12 + header_size).ok_or_else(|| damaged("EncryptionHeader"))?;
            let alg_id = u32_at(header, 8).ok_or_else(|| damaged("algID"))?;
            let hash_id = u32_at(header, 12).ok_or_else(|| damaged("algIDHash"))?;
            // 0x6801 RC4; 0x8004 SHA-1 (0 means the default, SHA-1).
            if alg_id != 0x6801 && alg_id != 0 {
                return Err(DecryptError::Unsupported(format!("暗号 0x{alg_id:04x}")));
            }
            if hash_id != 0x8004 && hash_id != 0 {
                return Err(DecryptError::Unsupported(format!("ハッシュ 0x{hash_id:04x}")));
            }
            let mut key_bits = u32_at(header, 16).ok_or_else(|| damaged("keySize"))?;
            if key_bits == 0 {
                key_bits = 40;
            }
            let v = 12 + header_size;
            let salt_size = u32_at(info, v).ok_or_else(|| damaged("saltSize"))? as usize;
            if salt_size != 16 {
                return Err(damaged("salt"));
            }
            let salt: [u8; 16] = info.get(v + 4..v + 20).ok_or_else(|| damaged("salt"))?.try_into().map_err(|_| damaged("salt"))?;
            let encrypted_verifier: [u8; 16] = info.get(v + 20..v + 36).ok_or_else(|| damaged("verifier"))?.try_into().map_err(|_| damaged("verifier"))?;
            let hash_size = u32_at(info, v + 36).ok_or_else(|| damaged("verifierHashSize"))? as usize;
            let encrypted_verifier_hash = info.get(v + 40..v + 40 + hash_size.min(32)).ok_or_else(|| damaged("verifier hash"))?.to_vec();
            if encrypted_verifier_hash.len() < 20 {
                return Err(damaged("verifier hash"));
            }
            Ok(Rc4Params { method: Rc4Method::CryptoApi, salt, encrypted_verifier, encrypted_verifier_hash, key_bits })
        }
        _ => Err(DecryptError::Unsupported(format!("暗号化のバージョン {major}.{minor}"))),
    }
}

fn utf16le(password: &str) -> Vec<u8> {
    // The password is at most 255 characters.
    password.encode_utf16().take(255).flat_map(|u| u.to_le_bytes()).collect()
}

fn md5(parts: &[&[u8]]) -> [u8; 16] {
    let mut d = Md5::new();
    for p in parts {
        d.update(p);
    }
    d.finalize().into()
}

fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut d = Sha1::new();
    for p in parts {
        d.update(p);
    }
    d.finalize().into()
}

/// The keys made of a password that the file accepted: the cipher of each block of the stream.
#[derive(Clone)]
pub struct Rc4Keys {
    method: Rc4Method,
    key_bits: u32,
    /// MD5: the 16 bytes the block keys are made from (their first 5). SHA-1: the hash of the salt and the password.
    base: Vec<u8>,
}

impl Rc4Keys {
    /// The key of the password, checked against the file's verifier: `WrongPassword` if it does not agree.
    pub fn new(params: &Rc4Params, password: &str) -> Result<Self, DecryptError> {
        let pw = utf16le(password);
        let base: Vec<u8> = match params.method {
            Rc4Method::Standard => {
                let h0 = md5(&[&pw]);
                // The first 5 bytes of the hash and the salt, 16 times, hashed again.
                let mut data = Vec::with_capacity((5 + 16) * 16);
                for _ in 0..16 {
                    data.extend_from_slice(&h0[..5]);
                    data.extend_from_slice(&params.salt);
                }
                md5(&[&data]).to_vec()
            }
            Rc4Method::CryptoApi => sha1(&[&params.salt, &pw]).to_vec(),
        };
        let keys = Self { method: params.method, key_bits: params.key_bits, base };
        // The verifier and its hash are encrypted with the first block's cipher, one after the other.
        let mut cipher = keys.block_cipher(0);
        let mut verifier = params.encrypted_verifier;
        cipher.apply(&mut verifier);
        let mut hash = params.encrypted_verifier_hash.clone();
        cipher.apply(&mut hash);
        let expected: Vec<u8> = match params.method {
            Rc4Method::Standard => md5(&[&verifier]).to_vec(),
            Rc4Method::CryptoApi => sha1(&[&verifier]).to_vec(),
        };
        if hash.len() < expected.len() || hash[..expected.len()] != expected[..] {
            return Err(DecryptError::WrongPassword);
        }
        Ok(keys)
    }

    /// The cipher of block `block`, at its first byte.
    pub fn block_cipher(&self, block: u32) -> Rc4 {
        let b = block.to_le_bytes();
        match self.method {
            Rc4Method::Standard => Rc4::new(&md5(&[&self.base[..5], &b])),
            Rc4Method::CryptoApi => {
                let h = sha1(&[&self.base, &b]);
                let bytes = (self.key_bits / 8).clamp(5, 16) as usize;
                let mut key = [0u8; 16];
                key[..bytes].copy_from_slice(&h[..bytes]);
                // A 40-bit key is made 128 bits long with zeros.
                let len = if self.key_bits <= 40 { 16 } else { bytes };
                Rc4::new(&key[..len])
            }
        }
    }
}

/// An `EncryptionInfo` of the standard RC4 method for `password` with a salt and a verifier given (for writing
/// the files the tests read, and for a format that writes them).
pub fn make_standard_info(password: &str, salt: [u8; 16], verifier: [u8; 16]) -> Vec<u8> {
    let params = Rc4Params { method: Rc4Method::Standard, salt, encrypted_verifier: verifier, encrypted_verifier_hash: Vec::new(), key_bits: 40 };
    let pw = utf16le(password);
    let h0 = md5(&[&pw]);
    let mut data = Vec::new();
    for _ in 0..16 {
        data.extend_from_slice(&h0[..5]);
        data.extend_from_slice(&params.salt);
    }
    let keys = Rc4Keys { method: Rc4Method::Standard, key_bits: 40, base: md5(&[&data]).to_vec() };
    let mut cipher = keys.block_cipher(0);
    let mut ev = verifier;
    cipher.apply(&mut ev);
    let mut eh = md5(&[&verifier]);
    cipher.apply(&mut eh);
    let mut out = vec![1, 0, 1, 0];
    out.extend_from_slice(&salt);
    out.extend_from_slice(&ev);
    out.extend_from_slice(&eh);
    out
}

/// An `EncryptionInfo` of the RC4 CryptoAPI method (`key_bits` 40 to 128).
pub fn make_cryptoapi_info(password: &str, salt: [u8; 16], verifier: [u8; 16], key_bits: u32) -> Vec<u8> {
    let pw = utf16le(password);
    let keys = Rc4Keys { method: Rc4Method::CryptoApi, key_bits, base: sha1(&[&salt, &pw]).to_vec() };
    let mut cipher = keys.block_cipher(0);
    let mut ev = verifier;
    cipher.apply(&mut ev);
    let mut eh = sha1(&[&verifier]);
    cipher.apply(&mut eh);
    // The header: flags, sizeExtra, algID (RC4), algIDHash (SHA-1), keySize, providerType, two reserved, the CSP name.
    let csp: Vec<u8> = "Microsoft Enhanced Cryptographic Provider v1.0".encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect();
    let mut header = Vec::new();
    for v in [0x04u32, 0, 0x6801, 0x8004, key_bits, 1, 0, 0] {
        header.extend_from_slice(&v.to_le_bytes());
    }
    header.extend_from_slice(&csp);
    let mut out = Vec::new();
    out.extend_from_slice(&[2, 0, 2, 0]);
    out.extend_from_slice(&0x04u32.to_le_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&ev);
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&eh);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_gives_the_known_key_stream() {
        // The widely published test vector: key "Key", plaintext "Plaintext".
        let mut data = *b"Plaintext";
        Rc4::new(b"Key").apply(&mut data);
        assert_eq!(data, [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
        // Skipping is the same as spending.
        let (mut a, mut b) = (Rc4::new(b"Secret"), Rc4::new(b"Secret"));
        a.skip(7);
        let mut x = [0u8; 7];
        b.apply(&mut x);
        assert_eq!(a.next_byte(), b.next_byte());
    }

    #[test]
    fn both_methods_accept_their_password_and_refuse_another() {
        let (salt, verifier) = ([7u8; 16], [9u8; 16]);
        for (info, name) in [(make_standard_info("パスワード", salt, verifier), "standard"), (make_cryptoapi_info("パスワード", salt, verifier, 128), "cryptoapi"), (make_cryptoapi_info("パスワード", salt, verifier, 40), "cryptoapi 40")] {
            let params = parse_rc4(&info).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(Rc4Keys::new(&params, "パスワード").is_ok(), "{name}");
            assert_eq!(Rc4Keys::new(&params, "違う").err(), Some(DecryptError::WrongPassword), "{name}");
        }
    }

    #[test]
    fn blocks_have_their_own_keys() {
        let params = parse_rc4(&make_cryptoapi_info("x", [1; 16], [2; 16], 128)).unwrap();
        let keys = Rc4Keys::new(&params, "x").unwrap();
        let (mut a, mut b) = (keys.block_cipher(0), keys.block_cipher(1));
        assert_ne!(a.next_byte(), b.next_byte());
        assert_eq!(keys.block_cipher(5).next_byte(), keys.block_cipher(5).next_byte());
    }
}
