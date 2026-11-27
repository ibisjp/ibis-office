// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The Windows resources the build scripts of ibisOffice.exe and
// ibisOfficeUpdater.exe write (`include!`d by both build.rs): a resource
// file (.res) made here and handed to the linker, so no resource compiler is
// needed.

/// The Windows version in build-version.txt (at the top of the repository).
fn windows_version() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../build-version.txt");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter_map(|line| line.trim_end_matches('\r').split_once('\t'))
        .find(|(name, _)| name.trim() == "Windows")
        .map(|(_, version)| version.trim().to_string())
        .unwrap_or_else(|| panic!("build-version.txt has no Windows line"))
}

/// Append a resource with a numeric type and name to a .res file: its
/// header, then its data padded to 4 bytes. Type and name 0 make the empty
/// resource a .res file starts with.
fn push_resource(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    const HEADER_SIZE: u32 = 32;
    // Movable, pure and discardable, as resource compilers mark icons.
    const MEMORY_FLAGS: u16 = 0x1030;
    const LANG_NEUTRAL: u16 = 0;
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    for v in [0xffff, kind, 0xffff, name] {
        res.extend_from_slice(&v.to_le_bytes());
    }
    res.extend_from_slice(&0u32.to_le_bytes()); // data version
    let flags = if kind == 0 { 0 } else { MEMORY_FLAGS };
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&LANG_NEUTRAL.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // version
    res.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    res.extend_from_slice(data);
    res.resize(res.len().next_multiple_of(4), 0);
}

/// Append the version resource (RT_VERSION 1, what the 詳細 tab of the
/// file's properties shows): `version` (as 0.8.2) as the file's and the
/// product's, and the strings, in US English.
fn push_version_resource(res: &mut Vec<u8>, version: &str, description: &str, file_name: &str) {
    const RT_VERSION: u16 = 16;
    // A block of VS_VERSIONINFO: its length, its value's length, its type
    // (1: text), its key, its value and its children, each on 4 bytes.
    fn block(key: &str, value: &[u8], value_length: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 4];
        b[2..4].copy_from_slice(&value_length.to_le_bytes());
        b.extend_from_slice(&u16::from(text).to_le_bytes());
        for unit in key.encode_utf16().chain([0]) {
            b.extend_from_slice(&unit.to_le_bytes());
        }
        b.resize(b.len().next_multiple_of(4), 0);
        b.extend_from_slice(value);
        for child in children {
            b.resize(b.len().next_multiple_of(4), 0);
            b.extend_from_slice(child);
        }
        let length = b.len() as u16;
        b[0..2].copy_from_slice(&length.to_le_bytes());
        b
    }
    let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect() };
    let string = |key: &str, value: &str| block(key, &utf16(value), value.encode_utf16().count() as u16 + 1, true, &[]);

    let mut parts = version.split('.').map(|p| p.parse::<u16>().unwrap_or_else(|_| panic!("build-version.txt: {version} is not numbers")));
    let mut part = || u32::from(parts.next().unwrap_or(0));
    let (ms, ls) = (part() << 16 | part(), part() << 16 | part());
    let mut fixed = Vec::new();
    for v in [
        0xfeef04bd, // signature
        0x0001_0000, // structure version
        ms, ls,      // file version
        ms, ls,      // product version
        0x3f,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0, 0, 0,     // subtype, date
    ] {
        fixed.extend_from_slice(&u32::to_le_bytes(v));
    }
    let file_version = format!("{}.{}.{}.{}", ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff);
    let strings = block(
        "040904b0", // US English, Unicode
        &[],
        0,
        true,
        &[
            string("CompanyName", "ibis inc."),
            string("FileDescription", description),
            string("FileVersion", &file_version),
            string("InternalName", file_name.trim_end_matches(".exe")),
            string("LegalCopyright", "Copyright (c) ibis inc."),
            string("OriginalFilename", file_name),
            string("ProductName", "ibisOffice"),
            string("ProductVersion", version),
        ],
    );
    let translation = [0x0409u16.to_le_bytes(), 1200u16.to_le_bytes()].concat();
    let info = block(
        "VS_VERSION_INFO",
        &fixed,
        fixed.len() as u16,
        false,
        &[
            block("StringFileInfo", &[], 0, true, &[strings]),
            block("VarFileInfo", &[], 0, true, &[block("Translation", &translation, 4, false, &[])]),
        ],
    );
    push_resource(res, RT_VERSION, 1, &info);
}
