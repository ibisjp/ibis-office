// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Password-protected files (パスワードを使用して暗号化): an OOXML package
//! (`.pptx`, `.xlsx`, `.docx`) kept encrypted in a compound file, as Office 2010 and later
//! write it (MS-OFFCRYPTO, "Agile Encryption": AES in CBC mode, a key made of the
//! password by SHA-2 hashed over `spinCount` rounds, the package in 4096-byte segments).
//! Office 2007's Standard Encryption (version 3.2 / 4.2: AES in ECB mode, a key made of
//! the password by SHA-1, MS-OFFCRYPTO 2.3.4.5-2.3.4.9) is read too, in [`standard`]. Saving
//! always writes Agile encryption, so a Standard file is upgraded when it is saved.
//! The RC4 methods are not read: the message says so, and the file is left as it is.

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::cfb;

/// Why a protected file was not opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecryptError {
    /// The password is not the one the file was protected with.
    WrongPassword,
    /// A method of protection this program does not read (the message names it).
    Unsupported(String),
    /// The file is damaged.
    Damaged(String),
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecryptError::WrongPassword => f.write_str("パスワードが違います"),
            DecryptError::Unsupported(what) => write!(f, "このファイルの暗号化の方式（{what}）には対応していません"),
            DecryptError::Damaged(what) => write!(f, "暗号化されたファイルを読めません（{what}）"),
        }
    }
}

impl std::error::Error for DecryptError {}

impl From<DecryptError> for crate::Error {
    fn from(e: DecryptError) -> Self {
        crate::Error::new(e.to_string())
    }
}

/// Whether `data` is a package kept encrypted in a compound file.
pub fn is_encrypted(data: &[u8]) -> bool {
    cfb::is_cfb(data) && cfb::stream(data, "EncryptionInfo").is_ok() && cfb::stream(data, "EncryptedPackage").is_ok()
}

const BLOCK_VERIFIER_INPUT: [u8; 8] = [0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79];
const BLOCK_VERIFIER_VALUE: [u8; 8] = [0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e];
const BLOCK_KEY_VALUE: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const BLOCK_HMAC_KEY: [u8; 8] = [0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6];
const BLOCK_HMAC_VALUE: [u8; 8] = [0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33];
/// A segment of the package.
const SEGMENT: usize = 4096;
/// Office's rounds of hashing the password.
const SPIN_COUNT: u32 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hash {
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    fn named(name: &str) -> Option<Self> {
        match name {
            "SHA256" => Some(Hash::Sha256),
            "SHA384" => Some(Hash::Sha384),
            "SHA512" => Some(Hash::Sha512),
            _ => None,
        }
    }

    fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut d = D::new();
            for p in parts {
                d.update(p);
            }
            d.finalize().to_vec()
        }
        match self {
            Hash::Sha256 => run::<Sha256>(parts),
            Hash::Sha384 => run::<Sha384>(parts),
            Hash::Sha512 => run::<Sha512>(parts),
        }
    }
}

/// The attribute `name` of the first element called `tag` in `xml`.
fn attribute(xml: &str, tag: &str, name: &str) -> Option<String> {
    let start = xml.find(&format!("<{tag} ")).or_else(|| xml.find(&format!(":{tag} ")).map(|i| i.saturating_sub(0)))?;
    let end = start + xml[start..].find('>')?;
    let element = &xml[start..end];
    let pattern = format!(" {name}=\"");
    let at = element.find(&pattern)? + pattern.len();
    let len = element[at..].find('"')?;
    Some(element[at..at + len].to_string())
}

struct Params {
    salt: Vec<u8>,
    block_size: usize,
    key_bits: usize,
    hash: Hash,
}

fn params(xml: &str, tag: &str) -> Result<Params, DecryptError> {
    let get = |name: &str| attribute(xml, tag, name).ok_or_else(|| DecryptError::Damaged(format!("{tag}/{name}")));
    if get("cipherAlgorithm")? != "AES" {
        return Err(DecryptError::Unsupported(format!("暗号 {}", get("cipherAlgorithm")?)));
    }
    if get("cipherChaining")? != "ChainingModeCBC" {
        return Err(DecryptError::Unsupported(format!("連鎖 {}", get("cipherChaining")?)));
    }
    let hash_name = get("hashAlgorithm")?;
    let hash = Hash::named(&hash_name).ok_or_else(|| DecryptError::Unsupported(format!("ハッシュ {hash_name}")))?;
    let salt = crate::base64::decode(&get("saltValue")?).ok_or_else(|| DecryptError::Damaged("saltValue".into()))?;
    let block_size: usize = get("blockSize")?.parse().map_err(|_| DecryptError::Damaged("blockSize".into()))?;
    let key_bits: usize = get("keyBits")?.parse().map_err(|_| DecryptError::Damaged("keyBits".into()))?;
    if block_size != 16 || !matches!(key_bits, 128 | 192 | 256) {
        return Err(DecryptError::Unsupported(format!("鍵 {key_bits} ビット")));
    }
    Ok(Params { salt, block_size, key_bits, hash })
}

/// `data` made of whole blocks, decrypted with `key` in CBC mode, in place.
fn decrypt_cbc(key: &[u8], iv: &[u8], data: &mut [u8]) -> Result<(), DecryptError> {
    let bad = || DecryptError::Damaged("AES".into());
    if data.len() % 16 != 0 {
        return Err(bad());
    }
    match key.len() {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv).map_err(|_| bad())?.decrypt_padded_mut::<NoPadding>(data).map(|_| ()).map_err(|_| bad()),
        24 => cbc::Decryptor::<aes::Aes192>::new_from_slices(key, iv).map_err(|_| bad())?.decrypt_padded_mut::<NoPadding>(data).map(|_| ()).map_err(|_| bad()),
        32 => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv).map_err(|_| bad())?.decrypt_padded_mut::<NoPadding>(data).map(|_| ()).map_err(|_| bad()),
        _ => Err(bad()),
    }
}

/// `data` (zero-padded to whole blocks) encrypted with `key` in CBC mode.
fn encrypt_cbc(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    let mut buffer = data.to_vec();
    buffer.resize(data.len().next_multiple_of(16), 0);
    let len = buffer.len();
    let _ = match key.len() {
        16 => cbc::Encryptor::<aes::Aes128>::new_from_slices(key, iv).expect("key and iv").encrypt_padded_mut::<NoPadding>(&mut buffer, len).map(|_| ()),
        24 => cbc::Encryptor::<aes::Aes192>::new_from_slices(key, iv).expect("key and iv").encrypt_padded_mut::<NoPadding>(&mut buffer, len).map(|_| ()),
        _ => cbc::Encryptor::<aes::Aes256>::new_from_slices(key, iv).expect("key and iv").encrypt_padded_mut::<NoPadding>(&mut buffer, len).map(|_| ()),
    };
    buffer
}

/// `bytes` cut or padded (with 0x36) to `len`.
fn fitted(mut bytes: Vec<u8>, len: usize) -> Vec<u8> {
    bytes.resize(len, 0x36);
    bytes
}

/// The key the password gives for `block` (MS-OFFCRYPTO 2.3.4.11).
fn password_key(password: &str, salt: &[u8], spin: u32, hash: Hash, key_bits: usize, block: &[u8; 8]) -> Vec<u8> {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut h = hash.digest(&[salt, &utf16]);
    for i in 0..spin {
        h = hash.digest(&[&i.to_le_bytes(), &h]);
    }
    fitted(hash.digest(&[&h, block]), key_bits / 8)
}

/// The package inside `data`, a protected file, opened with `password`.
pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>, DecryptError> {
    let info = cfb::stream(data, "EncryptionInfo").map_err(|_| DecryptError::Damaged("EncryptionInfo".into()))?;
    let package = cfb::stream(data, "EncryptedPackage").map_err(|_| DecryptError::Damaged("EncryptedPackage".into()))?;
    if info.len() < 8 {
        return Err(DecryptError::Damaged("EncryptionInfo".into()));
    }
    let (major, minor) = (u16::from_le_bytes([info[0], info[1]]), u16::from_le_bytes([info[2], info[3]]));
    if matches!((major, minor), (2..=4, 2)) {
        return standard::decrypt(&info, &package, password);
    }
    if (major, minor) != (4, 4) {
        let name = match (major, minor) {
            (1, 1) => "RC4".to_string(),
            _ => format!("版 {major}.{minor}"),
        };
        return Err(DecryptError::Unsupported(name));
    }
    let xml = String::from_utf8_lossy(&info[8..]).into_owned();
    let key_data = params(&xml, "keyData")?;
    let encryptor = params(&xml, "encryptedKey")?;
    let get = |name: &str| -> Result<Vec<u8>, DecryptError> {
        attribute(&xml, "encryptedKey", name).and_then(|v| crate::base64::decode(&v)).ok_or_else(|| DecryptError::Damaged(name.into()))
    };
    let spin: u32 = attribute(&xml, "encryptedKey", "spinCount").and_then(|v| v.parse().ok()).ok_or_else(|| DecryptError::Damaged("spinCount".into()))?;
    let iv = fitted(encryptor.salt.clone(), encryptor.block_size);
    let key_for = |block: &[u8; 8]| password_key(password, &encryptor.salt, spin, encryptor.hash, encryptor.key_bits, block);
    // The password is the right one when the hash of the verifier it decrypts is the hash it decrypts.
    let mut verifier = get("encryptedVerifierHashInput")?;
    decrypt_cbc(&key_for(&BLOCK_VERIFIER_INPUT), &iv, &mut verifier)?;
    let mut verifier_hash = get("encryptedVerifierHashValue")?;
    decrypt_cbc(&key_for(&BLOCK_VERIFIER_VALUE), &iv, &mut verifier_hash)?;
    let wanted = encryptor.hash.digest(&[&verifier[..encryptor.salt.len().min(verifier.len())]]);
    if verifier_hash.len() < wanted.len() || verifier_hash[..wanted.len()] != wanted[..] {
        return Err(DecryptError::WrongPassword);
    }
    let mut key = get("encryptedKeyValue")?;
    decrypt_cbc(&key_for(&BLOCK_KEY_VALUE), &iv, &mut key)?;
    key.truncate(key_data.key_bits / 8);
    // The package: its size, then 4096-byte segments, each with an IV of its own.
    if package.len() < 8 {
        return Err(DecryptError::Damaged("EncryptedPackage".into()));
    }
    let size = u64::from_le_bytes(package[..8].try_into().expect("8 bytes")) as usize;
    let mut out = Vec::with_capacity(package.len());
    for (i, segment) in package[8..].chunks(SEGMENT).enumerate() {
        let iv = fitted(key_data.hash.digest(&[&key_data.salt, &(i as u32).to_le_bytes()]), key_data.block_size);
        let mut bytes = segment.to_vec();
        bytes.truncate(bytes.len() / 16 * 16);
        decrypt_cbc(&key, &iv, &mut bytes)?;
        out.extend_from_slice(&bytes);
    }
    if size > out.len() {
        return Err(DecryptError::Damaged("size".into()));
    }
    out.truncate(size);
    // The integrity value: the HMAC of the encrypted stream, when the file has it.
    if let (Some(enc_key), Some(enc_value)) = (attribute(&xml, "dataIntegrity", "encryptedHmacKey").and_then(|v| crate::base64::decode(&v)), attribute(&xml, "dataIntegrity", "encryptedHmacValue").and_then(|v| crate::base64::decode(&v))) {
        let iv_for = |block: &[u8; 8]| fitted(key_data.hash.digest(&[&key_data.salt, block]), key_data.block_size);
        let (mut hmac_key, mut hmac_value) = (enc_key, enc_value);
        decrypt_cbc(&key, &iv_for(&BLOCK_HMAC_KEY), &mut hmac_key)?;
        decrypt_cbc(&key, &iv_for(&BLOCK_HMAC_VALUE), &mut hmac_value)?;
        let mac = hmac_of(key_data.hash, &hmac_key[..key_data.hash.digest(&[]).len().min(hmac_key.len())], &package);
        if hmac_value.len() < mac.len() || hmac_value[..mac.len()] != mac[..] {
            return Err(DecryptError::Damaged("改ざんか破損".into()));
        }
    }
    Ok(out)
}

/// Standard Encryption (ECMA-376 first edition, Office 2007), written from MS-OFFCRYPTO 2.3.4.5 (EncryptionInfo),
/// 2.3.4.7 (key generation), 2.3.4.9 (password verification) and 2.3.4.4 (EncryptedPackage).
mod standard {
    use super::DecryptError;
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
    use sha1::{Digest, Sha1};

    const ALG_AES128: u32 = 0x660E;
    const ALG_AES192: u32 = 0x660F;
    const ALG_AES256: u32 = 0x6610;
    const HASH_SHA1: u32 = 0x8004;
    const F_CRYPTOAPI: u32 = 0x04;
    const F_EXTERNAL: u32 = 0x10;
    const F_AES: u32 = 0x20;
    /// Rounds of hashing the password (2.3.4.7).
    pub(super) const ITERATIONS: u32 = 50_000;

    fn damaged(what: &str) -> DecryptError {
        DecryptError::Damaged(what.to_string())
    }

    fn u32_at(d: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
    }

    fn sha1(parts: &[&[u8]]) -> [u8; 20] {
        let mut h = Sha1::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().into()
    }

    /// The key of `key_bytes` bytes the password gives (2.3.4.7).
    pub(super) fn derive_key(password: &str, salt: &[u8], key_bytes: usize) -> Vec<u8> {
        let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut h = sha1(&[salt, &utf16]);
        for i in 0..ITERATIONS {
            h = sha1(&[&i.to_le_bytes(), &h]);
        }
        let h_final = sha1(&[&h, &0u32.to_le_bytes()]);
        let mut inner = [0x36u8; 64];
        let mut outer = [0x5Cu8; 64];
        for (i, b) in h_final.iter().enumerate() {
            inner[i] ^= b;
            outer[i] ^= b;
        }
        let mut key = sha1(&[&inner]).to_vec();
        key.extend_from_slice(&sha1(&[&outer]));
        key.truncate(key_bytes);
        key
    }

    /// `data` (whole blocks) transformed block by block with AES in ECB mode.
    pub(super) fn ecb(key: &[u8], data: &mut [u8], decrypt: bool) -> Result<(), DecryptError> {
        fn run<C: BlockDecrypt + BlockEncrypt + KeyInit>(key: &[u8], data: &mut [u8], decrypt: bool) -> Result<(), DecryptError> {
            let cipher = C::new_from_slice(key).map_err(|_| damaged("AES"))?;
            for block in data.chunks_exact_mut(16) {
                let block = GenericArray::from_mut_slice(block);
                if decrypt {
                    cipher.decrypt_block(block);
                } else {
                    cipher.encrypt_block(block);
                }
            }
            Ok(())
        }
        if data.len() % 16 != 0 {
            return Err(damaged("AES"));
        }
        match key.len() {
            16 => run::<aes::Aes128>(key, data, decrypt),
            24 => run::<aes::Aes192>(key, data, decrypt),
            32 => run::<aes::Aes256>(key, data, decrypt),
            _ => Err(damaged("AES")),
        }
    }

    /// The package inside a file protected with Standard Encryption.
    pub(super) fn decrypt(info: &[u8], package: &[u8], password: &str) -> Result<Vec<u8>, DecryptError> {
        let info_flags = u32_at(info, 4).ok_or_else(|| damaged("EncryptionInfo"))?;
        if info_flags & F_EXTERNAL != 0 {
            return Err(DecryptError::Unsupported("外部の暗号化".into()));
        }
        if info_flags & F_CRYPTOAPI == 0 {
            return Err(DecryptError::Unsupported("標準暗号化（CryptoAPI なし）".into()));
        }
        let header_size = u32_at(info, 8).ok_or_else(|| damaged("EncryptionHeader"))? as usize;
        let header = info.get(12..12usize.saturating_add(header_size)).ok_or_else(|| damaged("EncryptionHeader"))?;
        let header_flags = u32_at(header, 0).ok_or_else(|| damaged("EncryptionHeader"))?;
        let alg_id = u32_at(header, 8).ok_or_else(|| damaged("algID"))?;
        let hash_id = u32_at(header, 12).ok_or_else(|| damaged("algIDHash"))?;
        let key_bits = u32_at(header, 16).ok_or_else(|| damaged("keySize"))?;
        if (info_flags | header_flags) & F_AES == 0 || !matches!(alg_id, ALG_AES128 | ALG_AES192 | ALG_AES256) {
            return Err(DecryptError::Unsupported(format!("標準暗号化の暗号 0x{alg_id:04x}")));
        }
        if hash_id != HASH_SHA1 && hash_id != 0 {
            return Err(DecryptError::Unsupported(format!("ハッシュ 0x{hash_id:04x}")));
        }
        let expected_bits = match alg_id {
            ALG_AES128 => 128,
            ALG_AES192 => 192,
            _ => 256,
        };
        if key_bits != expected_bits {
            return Err(damaged("keySize"));
        }
        // The verifier follows the header: salt, the encrypted verifier, the size of its hash, the encrypted hash.
        let v = 12 + header_size;
        let salt_size = u32_at(info, v).ok_or_else(|| damaged("EncryptionVerifier"))? as usize;
        if salt_size != 16 {
            return Err(damaged("saltSize"));
        }
        let take = |at: usize, n: usize| info.get(at..at + n).ok_or_else(|| damaged("EncryptionVerifier"));
        let salt = take(v + 4, 16)?;
        let mut verifier = take(v + 20, 16)?.to_vec();
        let hash_size = u32_at(info, v + 36).ok_or_else(|| damaged("EncryptionVerifier"))? as usize;
        let mut verifier_hash = take(v + 40, 32)?.to_vec();
        if hash_size != 20 {
            return Err(damaged("verifierHashSize"));
        }
        let key = derive_key(password, salt, (key_bits / 8) as usize);
        ecb(&key, &mut verifier, true)?;
        ecb(&key, &mut verifier_hash, true)?;
        if verifier_hash[..20] != sha1(&[&verifier])[..] {
            return Err(DecryptError::WrongPassword);
        }
        // The package: its size (8 bytes), then the data in whole blocks.
        if package.len() < 8 {
            return Err(damaged("EncryptedPackage"));
        }
        let size = u64::from_le_bytes(package[..8].try_into().expect("8 bytes"));
        let mut out = package[8..package.len() - (package.len() - 8) % 16].to_vec();
        ecb(&key, &mut out, true)?;
        if size > out.len() as u64 {
            return Err(damaged("size"));
        }
        out.truncate(size as usize);
        Ok(out)
    }
}

fn hmac_of(hash: Hash, key: &[u8], data: &[u8]) -> Vec<u8> {
    match hash {
        Hash::Sha256 => {
            let mut m = Hmac::<Sha256>::new_from_slice(key).expect("any key length");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
        Hash::Sha384 => {
            let mut m = Hmac::<Sha384>::new_from_slice(key).expect("any key length");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
        Hash::Sha512 => {
            let mut m = Hmac::<Sha512>::new_from_slice(key).expect("any key length");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
    }
}

/// `package` protected with `password` (AES-256, SHA-512, 100 000 rounds), as a compound
/// file; `random` fills the bytes it is given with random ones.
pub fn encrypt(package: &[u8], password: &str, random: &mut dyn FnMut(&mut [u8])) -> Vec<u8> {
    let hash = Hash::Sha512;
    let mut bytes = |n: usize| {
        let mut v = vec![0u8; n];
        random(&mut v);
        v
    };
    let (key_salt, pass_salt, key, verifier, hmac_key) = (bytes(16), bytes(16), bytes(32), bytes(16), bytes(64));
    // The package in segments.
    let mut encrypted = (package.len() as u64).to_le_bytes().to_vec();
    for (i, segment) in package.chunks(SEGMENT).enumerate() {
        let iv = fitted(hash.digest(&[&key_salt, &(i as u32).to_le_bytes()]), 16);
        encrypted.extend(encrypt_cbc(&key, &iv, segment));
    }
    if package.is_empty() {
        // Nothing to protect: no segments.
    }
    // The key, the verifier, and the integrity values.
    let key_for = |block: &[u8; 8]| password_key(password, &pass_salt, SPIN_COUNT, hash, 256, block);
    let iv = fitted(pass_salt.clone(), 16);
    let enc_verifier = encrypt_cbc(&key_for(&BLOCK_VERIFIER_INPUT), &iv, &verifier);
    let enc_verifier_hash = encrypt_cbc(&key_for(&BLOCK_VERIFIER_VALUE), &iv, &hash.digest(&[&verifier]));
    let enc_key = encrypt_cbc(&key_for(&BLOCK_KEY_VALUE), &iv, &key);
    let iv_for = |block: &[u8; 8]| fitted(hash.digest(&[&key_salt, block]), 16);
    let enc_hmac_key = encrypt_cbc(&key, &iv_for(&BLOCK_HMAC_KEY), &hmac_key);
    let enc_hmac_value = encrypt_cbc(&key, &iv_for(&BLOCK_HMAC_VALUE), &hmac_of(hash, &hmac_key, &encrypted));
    let b64 = crate::base64::encode;
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<encryption xmlns=\"http://schemas.microsoft.com/office/2006/encryption\" xmlns:p=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\" xmlns:c=\"http://schemas.microsoft.com/office/2006/keyEncryptor/certificate\"><keyData saltSize=\"16\" blockSize=\"16\" keyBits=\"256\" hashSize=\"64\" cipherAlgorithm=\"AES\" cipherChaining=\"ChainingModeCBC\" hashAlgorithm=\"SHA512\" saltValue=\"{}\"/><dataIntegrity encryptedHmacKey=\"{}\" encryptedHmacValue=\"{}\"/><keyEncryptors><keyEncryptor uri=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\"><p:encryptedKey spinCount=\"{SPIN_COUNT}\" saltSize=\"16\" blockSize=\"16\" keyBits=\"256\" hashSize=\"64\" cipherAlgorithm=\"AES\" cipherChaining=\"ChainingModeCBC\" hashAlgorithm=\"SHA512\" saltValue=\"{}\" encryptedVerifierHashInput=\"{}\" encryptedVerifierHashValue=\"{}\" encryptedKeyValue=\"{}\"/></keyEncryptor></keyEncryptors></encryption>",
        b64(&key_salt), b64(&enc_hmac_key), b64(&enc_hmac_value), b64(&pass_salt), b64(&enc_verifier), b64(&enc_verifier_hash), b64(&enc_key)
    );
    let mut info = vec![4, 0, 4, 0, 0x40, 0, 0, 0];
    info.extend_from_slice(xml.as_bytes());
    cfb::write(&[
        cfb::Item::stream("EncryptionInfo", info),
        cfb::Item::stream("EncryptedPackage", encrypted),
        cfb::Item::storage("\u{6}DataSpaces"),
        cfb::Item::stream("\u{6}DataSpaces/Version", data_spaces::VERSION.to_vec()),
        cfb::Item::stream("\u{6}DataSpaces/DataSpaceMap", data_spaces::MAP.to_vec()),
        cfb::Item::storage("\u{6}DataSpaces/DataSpaceInfo"),
        cfb::Item::stream("\u{6}DataSpaces/DataSpaceInfo/StrongEncryptionDataSpace", data_spaces::INFO.to_vec()),
        cfb::Item::storage("\u{6}DataSpaces/TransformInfo"),
        cfb::Item::storage("\u{6}DataSpaces/TransformInfo/StrongEncryptionTransform"),
        cfb::Item::stream("\u{6}DataSpaces/TransformInfo/StrongEncryptionTransform/\u{6}Primary", data_spaces::TRANSFORM.to_vec()),
    ])
}

/// The data space streams Office writes for Agile encryption (MS-OFFCRYPTO 2.3.4.1–2.3.4.4): the same in every file.
mod data_spaces {
    fn lp(text: &str) -> Vec<u8> {
        // UNICODE-LP-P4: a length in bytes, the UTF-16 text, zeros to a multiple of 4.
        let units: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut out = (units.len() as u32).to_le_bytes().to_vec();
        out.extend(&units);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    pub(super) static VERSION: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        let mut v = lp("Microsoft.Container.DataSpaces");
        for _ in 0..3 {
            v.extend_from_slice(&[1, 0, 0, 0]);
        }
        v
    });
    pub(super) static MAP: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        let (package, space) = (lp("EncryptedPackage"), lp("StrongEncryptionDataSpace"));
        let entry_len = 4 + 4 + 4 + 4 + package.len() - 4 + space.len();
        let mut v = vec![8, 0, 0, 0, 1, 0, 0, 0];
        v.extend_from_slice(&(entry_len as u32).to_le_bytes());
        v.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        v.extend(package);
        v.extend(space);
        v
    });
    pub(super) static INFO: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        let mut v = vec![8, 0, 0, 0, 1, 0, 0, 0];
        v.extend(lp("StrongEncryptionTransform"));
        v
    });
    pub(super) static TRANSFORM: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        // IRMDSTransformInfo: the header (its length, the type 1, the transform's id and name, three versions),
        // then EncryptionTransformInfo as Office writes it for Agile encryption (no name, block size 0, cipher mode 0, reserved 4).
        let (id, name) = (lp("{FF9A3F03-56EF-4613-BDD5-5A41C1D07246}"), lp("Microsoft.Container.EncryptionTransform"));
        let mut v = 88u32.to_le_bytes().to_vec();
        v.extend_from_slice(&[1, 0, 0, 0]);
        v.extend(id);
        v.extend(name);
        for _ in 0..3 {
            v.extend_from_slice(&[1, 0, 0, 0]);
        }
        v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0]);
        v
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random(buffer: &mut [u8]) {
        // Not random: a repeatable sequence, for tests.
        let mut x = 0x1234_5678u32;
        for b in buffer {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (x >> 24) as u8;
        }
    }

    #[test]
    fn what_is_encrypted_is_opened_with_its_password() {
        for size in [0usize, 1, 15, 16, 4095, 4096, 4097, 10_000, 100_000] {
            let package: Vec<u8> = (0..size).map(|i| (i * 31 % 251) as u8).collect();
            let file = encrypt(&package, "ibis123", &mut random);
            assert!(is_encrypted(&file), "{size}");
            assert_eq!(decrypt(&file, "ibis123").unwrap(), package, "{size}");
        }
    }

    #[test]
    fn a_wrong_password_is_told() {
        let file = encrypt(b"PK secret", "right", &mut random);
        assert_eq!(decrypt(&file, "wrong"), Err(DecryptError::WrongPassword));
        assert_eq!(decrypt(&file, ""), Err(DecryptError::WrongPassword));
        // Japanese passwords (UTF-16).
        let file = encrypt(b"PK secret", "パスワード", &mut random);
        assert!(decrypt(&file, "パスワード").is_ok());
        assert_eq!(decrypt(&file, "ぱすわーど"), Err(DecryptError::WrongPassword));
    }

    #[test]
    fn damage_is_noticed() {
        let mut file = encrypt(&vec![7u8; 5000], "ibis123", &mut random);
        // A byte of the encrypted package changed: the integrity value says so.
        let package = cfb::stream(&file, "EncryptedPackage").unwrap();
        let at = file.windows(64).position(|w| w == &package[100..164]).expect("the package in the file");
        file[at + 5] ^= 0xFF;
        assert!(matches!(decrypt(&file, "ibis123"), Err(DecryptError::Damaged(_))));
    }

    #[test]
    fn other_methods_are_named() {
        // A file that is not protected at all.
        assert!(!is_encrypted(b"PK\x03\x04"));
        // An unknown version.
        let mut info = vec![9, 0, 9, 0, 0x24, 0, 0, 0];
        info.extend_from_slice(&[0; 40]);
        let file = cfb::write(&[cfb::Item::stream("EncryptionInfo", info), cfb::Item::stream("EncryptedPackage", vec![0; 16])]);
        assert!(matches!(decrypt(&file, "x"), Err(DecryptError::Unsupported(_))));
    }
}
