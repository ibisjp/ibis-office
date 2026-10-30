// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The paper a worksheet prints on when its `pageSetup` names none.
//!
//! ECMA-376's default `paperSize` is 1 (Letter), but Excel for Windows prints such a sheet on the paper the machine is set
//! to: the Windows region's default paper (`LOCALE_IPAPERSIZE`: 1 Letter, 9 A4, ...). Elsewhere (macOS, Linux, the web) and
//! when Windows does not say, the format's default, Letter.

/// The `paperSize` number for a sheet that names none: the Windows region's, else 1.
pub fn default_paper_size() -> i64 {
    use std::sync::OnceLock;
    static SIZE: OnceLock<i64> = OnceLock::new();
    *SIZE.get_or_init(region_paper_size)
}

#[cfg(windows)]
fn region_paper_size() -> i64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocaleInfoEx(locale_name: *const u16, lc_type: u32, data: *mut u16, count: i32) -> i32;
    }
    const LOCALE_IPAPERSIZE: u32 = 0x0000_100A;
    const LOCALE_RETURN_NUMBER: u32 = 0x2000_0000;
    let mut value = [0u16; 2];
    // SAFETY: a null name asks for the user's locale; the buffer holds the DWORD LOCALE_RETURN_NUMBER writes (two u16).
    let written = unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_IPAPERSIZE | LOCALE_RETURN_NUMBER, value.as_mut_ptr(), 2) };
    let size = value[0] as i64 | ((value[1] as i64) << 16);
    if written > 0 && size > 0 { size } else { 1 }
}

#[cfg(not(windows))]
fn region_paper_size() -> i64 {
    1
}
