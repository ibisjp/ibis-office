// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Sets `cfg(headless_export)` where the export is built: with the `export` feature, on macOS, Windows and Linux, where GPUI
//! draws without a window (the same rule as ibis-office-app's build.rs).

fn main() {
    let target = |name: &str| std::env::var(name).unwrap_or_default();
    let os = target("CARGO_CFG_TARGET_OS");
    println!("cargo::rustc-check-cfg=cfg(headless_export)");
    if std::env::var_os("CARGO_FEATURE_EXPORT").is_some() && (os == "macos" || os == "windows" || os == "linux") {
        println!("cargo::rustc-cfg=headless_export");
    }
}
