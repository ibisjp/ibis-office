// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embeds the bundled icons (`icons/material/*.svg` and `icons/ibis/*.svg` of this crate, where the app's
//! resources are kept: tasks/doc/crate-split-plan.md, 12.4) into the shell, so it serves them from memory on every
//! platform (see `icons.rs`).
//!
//! It also gives the version the app shows (`IBIS_VERSION`, from version.txt) and sets `cfg(headless_export)` where the
//! export is built: with the `export` feature, on macOS, Windows and Linux, where GPUI draws without a window (the same
//! rule as ibis-office-app's and ibis-office-render's build.rs).

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    app_version();
    let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
    let dir = resources.join("material");
    println!("cargo:rerun-if-changed={}", dir.display());
    // The icons drawn for ibisOffice itself (icons/ibis/*.svg: line samples for the border styles, the merge menu) are
    // named "ibis/<file>".
    let own = resources.join("ibis");
    println!("cargo:rerun-if-changed={}", own.display());
    let list = |dir: &Path, prefix: &str| -> Vec<(String, std::path::PathBuf)> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".svg")).map(str::to_string))
            .collect();
        names.sort();
        names.into_iter().map(|n| (format!("{prefix}{n}"), dir.join(format!("{n}.svg")))).collect()
    };
    let mut out = String::from("/// The bundled icons, by name (file name without `.svg`; the ones drawn for ibisOffice: `ibis/<file name>`).\npub(crate) static MATERIAL_ICONS: &[(&str, &[u8])] = &[\n");
    for (name, path) in list(&dir, "").into_iter().chain(list(&own, "ibis/")) {
        writeln!(out, "    ({name:?}, include_bytes!({:?})),", path.display().to_string()).unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("material_icons.rs");
    std::fs::write(dest, out).unwrap();

    let target = |name: &str| std::env::var(name).unwrap_or_default();
    let os = target("CARGO_CFG_TARGET_OS");
    println!("cargo::rustc-check-cfg=cfg(headless_export)");
    if std::env::var_os("CARGO_FEATURE_EXPORT").is_some() && (os == "macos" || os == "windows" || os == "linux") {
        println!("cargo::rustc-cfg=headless_export");
    }
}

/// The version the app shows (`IBIS_VERSION`), the one version.txt (at the top of the repository: a platform, a tab and its
/// version, per line) gives the platform built for: Windows, macOS, Linux or wasm. The same as ibis-office-app's build.rs.
fn app_version() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../version.txt");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let target = |name: &str| std::env::var(name).unwrap_or_default();
    let platform = match (target("CARGO_CFG_TARGET_OS").as_str(), target("CARGO_CFG_TARGET_FAMILY").as_str()) {
        ("windows", _) => "Windows",
        ("macos", _) => "macOS",
        ("linux", _) => "Linux",
        (_, "wasm") => "wasm",
        (os, _) => panic!("version.txt has no line for {os}"),
    };
    let version = text
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').split_once('\t'))
        .find(|(name, _)| name.trim() == platform)
        .map(|(_, version)| version.trim().to_string())
        .unwrap_or_else(|| panic!("version.txt has no {platform} line"));
    println!("cargo:rustc-env=IBIS_VERSION={version}");
}
