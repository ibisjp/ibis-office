// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The scenario tests (`src/shell/scenario.rs`): every registered scenario,
//! run in turn on the main thread.
//!
//!   cargo test --release -p ibis-office-app --test scenarios [-- <part of a name> | !<part of a name to leave out>]

// `run_scenarios` exists only where the scenario driver is built (src/lib.rs):
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(ibis_office_app::run_scenarios(&args));
}

// Elsewhere (Windows) there is no driver: the test target is empty (it has no harness, so
// it needs a `main`).
#[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios")))]
fn main() {}

