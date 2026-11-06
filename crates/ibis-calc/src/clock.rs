// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The calculator's only reading of the clock: today's date in UTC, for the
//! year a date typed without one takes. The app reads the time through its
//! own `clock` module, but this crate does not depend on the app or on
//! js-sys (it needs no browser otherwise), so it keeps this one function:
//! `SystemTime::now` panics on wasm32-unknown-unknown, so there it gives
//! `None`, and the web version sets the year itself
//! (`eval::convert::set_clock`, from the browser's clock).
#![allow(clippy::disallowed_methods)]

/// Days since 1970-01-01 (UTC), from the system's clock; `None` in the
/// browser.
pub(crate) fn unix_days() -> Option<i64> {
    #[cfg(not(target_family = "wasm"))]
    return std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| (d.as_secs() / 86_400) as i64);
    #[cfg(target_family = "wasm")]
    None
}

/// Nanoseconds since 1970 (UTC), from the system's clock, to seed random numbers; `None` in the browser.
pub(crate) fn unix_nanos() -> Option<u64> {
    #[cfg(not(target_family = "wasm"))]
    return std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_nanos() as u64);
    #[cfg(target_family = "wasm")]
    None
}

/// Nanoseconds on a monotonic clock since the first call, to time a calculation (tests, profiling); `None` in the browser.
pub fn monotonic_nanos() -> Option<u64> {
    #[cfg(not(target_family = "wasm"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let start = START.get_or_init(std::time::Instant::now);
        Some(start.elapsed().as_nanos() as u64)
    }
    #[cfg(target_family = "wasm")]
    None
}
