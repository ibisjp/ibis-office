// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Decryption of PDFs encrypted with the Standard security handler (ISO 32000-2 7.6.4), opened with the empty user
//! password: the common case of a file anyone may open whose owner restricted printing or copying.
//!
//! RC4 (40 to 128 bits, revisions 2 to 4), AES-128 (AESV2) and AES-256 (AESV3, revisions 5 and 6), with the crypt
//! filters of `/CF`, `/StmF` and `/StrF`. A file that needs a user password reports so.

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::NoPadding};
use md5::{Digest as _, Md5};
use sha2::{Sha256, Sha384, Sha512};

use crate::cos::{CosDict, CosObj, ObjectId};
use crate::{Error, Result};

/// The padding string of Algorithm 2 (ISO 32000-2 7.6.4.3.2): the empty password, padded.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08, 0x2E, 0x2E, 0x00,
    0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// How strings or streams are encrypted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    Identity,
    Rc4,
    AesV2,
    AesV3,
}

/// The file key and the methods for the strings and the streams of an encrypted PDF.
#[derive(Clone, Debug)]
pub struct Decryptor {
    key: Vec<u8>,
    strings: Method,
    streams: Method,
    encrypt_metadata: bool,
}

impl Decryptor {
    /// The decryptor for the `/Encrypt` dictionary `encrypt` with the first `/ID` string `id0`, opened with the empty
    /// user password.
    pub fn new(encrypt: &CosDict, id0: &[u8]) -> Result<Self> {
        if encrypt.get_name(b"Filter") != Some(b"Standard".as_slice()) {
            return Err(Error::new("PDF encrypted with an unsupported security handler"));
        }
        let v = encrypt.get_int(b"V").unwrap_or(0);
        let r = encrypt.get_int(b"R").unwrap_or(0);
        let o = string(encrypt, b"O")?;
        let u = string(encrypt, b"U")?;
        let encrypt_metadata = !matches!(encrypt.get(b"EncryptMetadata"), Some(CosObj::Bool(false)));
        let (strings, streams) = match v {
            1 | 2 => (Method::Rc4, Method::Rc4),
            4 | 5 => {
                let filter = |key: &[u8]| -> Result<Method> {
                    let name = encrypt.get_name(key).unwrap_or(b"Identity");
                    if name == b"Identity" {
                        return Ok(Method::Identity);
                    }
                    let cfm = encrypt
                        .get(b"CF")
                        .and_then(CosObj::as_dict)
                        .and_then(|cf| cf.get(name))
                        .and_then(CosObj::as_dict)
                        .and_then(|f| f.get_name(b"CFM"))
                        .unwrap_or(b"None");
                    Ok(match cfm {
                        b"V2" => Method::Rc4,
                        b"AESV2" => Method::AesV2,
                        b"AESV3" => Method::AesV3,
                        b"None" => Method::Identity,
                        other => return Err(Error::new(format!("unsupported crypt filter method {}", String::from_utf8_lossy(other)))),
                    })
                };
                (filter(b"StrF")?, filter(b"StmF")?)
            }
            other => return Err(Error::new(format!("unsupported PDF encryption version {other}"))),
        };
        let key = match r {
            2..=4 => {
                let p = encrypt.get_int(b"P").unwrap_or(0) as i32;
                let length = match r {
                    2 => 5,
                    _ => (encrypt.get_int(b"Length").unwrap_or(40).clamp(40, 128) / 8) as usize,
                };
                let key = key_rev234(&o, p, id0, r, length, encrypt_metadata);
                if !user_password_matches_rev234(&key, &u, id0, r) {
                    return Err(Error::new("PDF requires a password to open"));
                }
                key
            }
            5 | 6 => {
                let ue = string(encrypt, b"UE")?;
                if u.len() < 48 || ue.len() < 32 {
                    return Err(Error::new("PDF encryption dictionary is malformed"));
                }
                let hash = |salt: &[u8]| if r == 5 { Sha256::digest(salt).to_vec() } else { hash_rev6(b"", salt, b"") };
                if hash(&u[32..40])[..32] != u[..32] {
                    return Err(Error::new("PDF requires a password to open"));
                }
                let intermediate = hash(&u[40..48]);
                let mut key = ue[..32].to_vec();
                cbc::Decryptor::<aes::Aes256>::new_from_slices(&intermediate[..32], &[0; 16])
                    .map_err(|_| Error::new("PDF encryption key is malformed"))?
                    .decrypt_padded_mut::<NoPadding>(&mut key)
                    .map_err(|_| Error::new("PDF encryption key is malformed"))?;
                key
            }
            other => return Err(Error::new(format!("unsupported PDF encryption revision {other}"))),
        };
        Ok(Self { key, strings, streams, encrypt_metadata })
    }

    /// Whether the stream with dictionary `dict` is encrypted: not a cross-reference stream, not metadata the file
    /// leaves in the clear.
    pub fn encrypts_stream(&self, dict: &CosDict) -> bool {
        match dict.get_name(b"Type") {
            Some(b"XRef") => false,
            Some(b"Metadata") => self.encrypt_metadata,
            _ => true,
        }
    }

    /// The plain bytes of a string of object `id`.
    pub fn decrypt_string(&self, id: ObjectId, data: &[u8]) -> Vec<u8> {
        self.decrypt(self.strings, id, data)
    }

    /// The plain bytes (still filtered) of a stream of object `id`.
    pub fn decrypt_stream(&self, id: ObjectId, data: &[u8]) -> Vec<u8> {
        self.decrypt(self.streams, id, data)
    }

    fn decrypt(&self, method: Method, id: ObjectId, data: &[u8]) -> Vec<u8> {
        match method {
            Method::Identity => data.to_vec(),
            Method::Rc4 => rc4(&self.object_key(id, false), data),
            Method::AesV2 => aes_cbc(&self.object_key(id, true), data),
            Method::AesV3 => aes_cbc(&self.key, data),
        }
    }

    /// Algorithm 1: the key of one object, from the file key, the object number and the generation.
    fn object_key(&self, id: ObjectId, aes: bool) -> Vec<u8> {
        let mut md5 = Md5::new();
        md5.update(&self.key);
        md5.update(&id.num.to_le_bytes()[..3]);
        md5.update(&id.generation.to_le_bytes()[..2]);
        if aes {
            md5.update(b"sAlT");
        }
        md5.finalize()[..(self.key.len() + 5).min(16)].to_vec()
    }
}

/// Recursively decrypt the strings of `obj`, an object `id` holds (a stream's bytes are left to the caller).
pub fn decrypt_strings(decryptor: &Decryptor, id: ObjectId, obj: &mut CosObj) {
    match obj {
        CosObj::Str(s) => {
            let plain = decryptor.decrypt_string(id, s.as_bytes());
            *s = crate::cos::CosString::Literal(plain);
        }
        CosObj::Array(items) => items.iter_mut().for_each(|item| decrypt_strings(decryptor, id, item)),
        CosObj::Dict(dict) | CosObj::Stream { dict, .. } => {
            for value in dict.values_mut() {
                decrypt_strings(decryptor, id, value);
            }
        }
        _ => {}
    }
}

fn string(dict: &CosDict, key: &[u8]) -> Result<Vec<u8>> {
    match dict.get(key) {
        Some(CosObj::Str(s)) => Ok(s.as_bytes().to_vec()),
        _ => Err(Error::new(format!("PDF encryption dictionary lacks /{}", String::from_utf8_lossy(key)))),
    }
}

/// Algorithm 2 with the empty password.
fn key_rev234(o: &[u8], p: i32, id0: &[u8], r: i64, length: usize, encrypt_metadata: bool) -> Vec<u8> {
    let mut md5 = Md5::new();
    md5.update(PAD);
    md5.update(&o[..o.len().min(32)]);
    md5.update(p.to_le_bytes());
    md5.update(id0);
    if r >= 4 && !encrypt_metadata {
        md5.update([0xFF; 4]);
    }
    let mut digest = md5.finalize().to_vec();
    if r >= 3 {
        for _ in 0..50 {
            digest = Md5::digest(&digest[..length]).to_vec();
        }
    }
    digest.truncate(length);
    digest
}

/// Algorithms 4 and 5: whether `key` (from the empty password) yields the file's `/U`.
fn user_password_matches_rev234(key: &[u8], u: &[u8], id0: &[u8], r: i64) -> bool {
    if r == 2 {
        return u.len() >= 32 && rc4(key, &PAD) == u[..32];
    }
    let mut md5 = Md5::new();
    md5.update(PAD);
    md5.update(id0);
    let mut x = rc4(key, &md5.finalize());
    for i in 1..=19u8 {
        let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
        x = rc4(&k, &x);
    }
    u.len() >= 16 && x[..16] == u[..16]
}

/// Algorithm 2.B (revision 6): the hash of `password` with `salt` and `udata`.
fn hash_rev6(password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut k = Sha256::new().chain_update(password).chain_update(salt).chain_update(udata).finalize().to_vec();
    let mut round = 0u32;
    loop {
        let block: Vec<u8> = [password, &k, udata].concat();
        let mut e: Vec<u8> = block.iter().copied().cycle().take(block.len() * 64).collect();
        let len = e.len();
        cbc::Encryptor::<aes::Aes128>::new_from_slices(&k[..16], &k[16..32])
            .expect("key and iv")
            .encrypt_padded_mut::<NoPadding>(&mut e, len)
            .expect("a multiple of the block size");
        k = match e[..16].iter().map(|&b| u32::from(b)).sum::<u32>() % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        if round >= 64 && u32::from(*e.last().expect("not empty")) <= round - 32 {
            break;
        }
    }
    k.truncate(32);
    k
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|&b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            b ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        })
        .collect()
}

/// AES-CBC with the 16-byte IV in front, PKCS#7 padding removed where it is well formed.
fn aes_cbc(key: &[u8], data: &[u8]) -> Vec<u8> {
    if data.len() < 32 {
        return Vec::new();
    }
    let (iv, body) = data.split_at(16);
    let mut plain = body[..body.len() / 16 * 16].to_vec();
    let ok = match key.len() {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv).ok().and_then(|d| d.decrypt_padded_mut::<NoPadding>(&mut plain).ok().map(|_| ())),
        32 => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv).ok().and_then(|d| d.decrypt_padded_mut::<NoPadding>(&mut plain).ok().map(|_| ())),
        _ => None,
    };
    if ok.is_none() {
        return Vec::new();
    }
    if let Some(&pad) = plain.last()
        && (1..=16).contains(&pad)
        && plain.len() >= pad as usize
        && plain[plain.len() - pad as usize..].iter().all(|&b| b == pad)
    {
        plain.truncate(plain.len() - pad as usize);
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_matches_the_published_test_vector() {
        // RFC 6229 is for longer keys; this is the classic "Key"/"Plaintext" vector.
        assert_eq!(rc4(b"Key", b"Plaintext"), [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
    }

    #[test]
    fn aes_cbc_strips_pkcs7_padding_and_takes_the_iv_in_front() {
        let key = [7u8; 16];
        let iv = [9u8; 16];
        let mut block = b"hello world\x05\x05\x05\x05\x05".to_vec();
        let len = block.len();
        cbc::Encryptor::<aes::Aes128>::new_from_slices(&key, &iv).unwrap().encrypt_padded_mut::<NoPadding>(&mut block, len).unwrap();
        let data = [iv.as_slice(), &block].concat();
        assert_eq!(aes_cbc(&key, &data), b"hello world");
    }
}
