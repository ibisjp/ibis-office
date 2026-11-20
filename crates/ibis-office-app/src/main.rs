// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Native build of the ibisOffice presentation editor, for development.

// On Windows, a GUI app: no console window opens beside it.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(all(not(target_family = "wasm"), not(target_os = "android"), not(target_os = "ios")))]
    ibis_office_app::run_native();
}
