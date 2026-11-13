// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Office 2007's Standard Encryption (MS-OFFCRYPTO 2.3.4.5-2.3.4.9). The test files are made here by an
//! encryptor written from the specification (the program itself only reads this method).

use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use ibis_ooxml::{cfb, encryption, Package};
use sha1::{Digest, Sha1};

fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn ecb_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.resize(data.len().next_multiple_of(16), 0);
    fn run<C: BlockEncrypt + KeyInit>(key: &[u8], out: &mut [u8]) {
        let c = C::new_from_slice(key).unwrap();
        for b in out.chunks_exact_mut(16) {
            c.encrypt_block(GenericArray::from_mut_slice(b));
        }
    }
    match key.len() {
        16 => run::<aes::Aes128>(key, &mut out),
        24 => run::<aes::Aes192>(key, &mut out),
        _ => run::<aes::Aes256>(key, &mut out),
    }
    out
}

/// 2.3.4.7, written out step by step.
fn key_for(password: &str, salt: &[u8], key_bytes: usize) -> Vec<u8> {
    let pw: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut h = sha1(&[salt, &pw]);
    for i in 0u32..50_000 {
        h = sha1(&[&i.to_le_bytes(), &h]);
    }
    let hfinal = sha1(&[&h, &[0, 0, 0, 0]]);
    let (mut b1, mut b2) = (vec![0x36u8; 64], vec![0x5Cu8; 64]);
    for i in 0..20 {
        b1[i] ^= hfinal[i];
        b2[i] ^= hfinal[i];
    }
    let mut x3 = sha1(&[&b1]).to_vec();
    x3.extend(sha1(&[&b2]));
    x3[..key_bytes].to_vec()
}

fn protect(package: &[u8], password: &str, key_bits: u32) -> Vec<u8> {
    let salt: Vec<u8> = (0..16u8).map(|i| i.wrapping_mul(37).wrapping_add(5)).collect();
    let verifier: Vec<u8> = (0..16u8).map(|i| i.wrapping_mul(11).wrapping_add(3)).collect();
    let key = key_for(password, &salt, (key_bits / 8) as usize);
    let alg = match key_bits {
        128 => 0x660Eu32,
        192 => 0x660F,
        _ => 0x6610,
    };
    let mut header = Vec::new();
    for v in [0x24u32, 0, alg, 0x8004, key_bits, 0x18, 0, 0] {
        header.extend(v.to_le_bytes());
    }
    header.extend("Microsoft Enhanced RSA and AES Cryptographic Provider".encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()));
    let mut info = vec![4, 0, 2, 0];
    info.extend(0x24u32.to_le_bytes());
    info.extend((header.len() as u32).to_le_bytes());
    info.extend(&header);
    info.extend(16u32.to_le_bytes());
    info.extend(&salt);
    info.extend(ecb_encrypt(&key, &verifier));
    info.extend(20u32.to_le_bytes());
    info.extend(ecb_encrypt(&key, &sha1(&[&verifier])));
    let mut encrypted = (package.len() as u64).to_le_bytes().to_vec();
    encrypted.extend(ecb_encrypt(&key, package));
    cfb::write(&[cfb::Item::stream("EncryptionInfo", info), cfb::Item::stream("EncryptedPackage", encrypted)])
}

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn standard_files_open_for_every_key_size() {
    for (name, main_part) in [("xlsx-encrypted.xlsx", "xl/workbook.xml"), ("docx-encrypted.docx", "word/document.xml")] {
        let package = encryption::decrypt(&file(name), "ibis123").unwrap();
        for bits in [128, 192, 256] {
            let locked = protect(&package, "ibis123", bits);
            assert!(encryption::is_encrypted(&locked));
            let opened = Package::open(encryption::decrypt(&locked, "ibis123").unwrap_or_else(|e| panic!("{name} {bits}: {e}"))).unwrap();
            assert!(opened.contains(main_part), "{name} {bits}");
        }
    }
}

#[test]
fn standard_wrong_password_is_told_and_unicode_works() {
    let locked = protect(b"PK payload", "right", 128);
    assert_eq!(encryption::decrypt(&locked, "wrong"), Err(encryption::DecryptError::WrongPassword));
    assert_eq!(encryption::decrypt(&locked, ""), Err(encryption::DecryptError::WrongPassword));
    let locked = protect(b"PK payload", "\u{30d1}\u{30b9}\u{1F512}", 256);
    assert_eq!(encryption::decrypt(&locked, "\u{30d1}\u{30b9}\u{1F512}").unwrap(), b"PK payload");
    assert_eq!(encryption::decrypt(&locked, "\u{30d1}\u{30b9}"), Err(encryption::DecryptError::WrongPassword));
}

#[test]
fn standard_damage_is_not_a_wrong_password() {
    let locked = protect(b"PK payload", "pw", 128);
    // Cut the verifier short: damaged, not a wrong password.
    let mut info = cfb::stream(&locked, "EncryptionInfo").unwrap();
    info.truncate(info.len() - 20);
    let package = cfb::stream(&locked, "EncryptedPackage").unwrap();
    let cut = cfb::write(&[cfb::Item::stream("EncryptionInfo", info), cfb::Item::stream("EncryptedPackage", package.clone())]);
    assert!(matches!(encryption::decrypt(&cut, "pw"), Err(encryption::DecryptError::Damaged(_))));
    // A package size larger than the data.
    let mut bad = package;
    bad[..8].copy_from_slice(&u64::MAX.to_le_bytes());
    let info = cfb::stream(&locked, "EncryptionInfo").unwrap();
    let file = cfb::write(&[cfb::Item::stream("EncryptionInfo", info), cfb::Item::stream("EncryptedPackage", bad)]);
    assert!(matches!(encryption::decrypt(&file, "pw"), Err(encryption::DecryptError::Damaged(_))));
}
