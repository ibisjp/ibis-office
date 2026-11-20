// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How far reading a file has got, for the progress bar of the loading box.
//!
//! One file is read at a time per process as far as this is concerned: the
//! numbers are shared, [`reset`] starts again at 0. The value only goes
//! forward. Opening a large file reports in stages, each a part (`lo`..`hi`,
//! in thousandths) of the whole; a CSV file's conversion to a workbook takes
//! the first part and reading that workbook the rest.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

static VALUE: AtomicU32 = AtomicU32::new(0);
static LO: AtomicU32 = AtomicU32::new(0);
static HI: AtomicU32 = AtomicU32::new(1000);
static ROWS_TOTAL: AtomicU64 = AtomicU64::new(0);
static ROWS_DONE: AtomicU64 = AtomicU64::new(0);
static ROWS_FROM: AtomicU32 = AtomicU32::new(0);
static ROWS_TO: AtomicU32 = AtomicU32::new(1000);

/// Start a new file: 0, and the whole range for the next stages.
pub fn reset() {
    VALUE.store(0, Relaxed);
    LO.store(0, Relaxed);
    HI.store(1000, Relaxed);
    ROWS_TOTAL.store(0, Relaxed);
    ROWS_DONE.store(0, Relaxed);
}

/// The part of the whole (thousandths) the stages after this report in.
#[allow(dead_code)]
pub(crate) fn span(lo: u32, hi: u32) {
    LO.store(lo, Relaxed);
    HI.store(hi, Relaxed);
}

/// The part of the current span (`lo`..`hi` as given to [`span`]) a stage
/// ends at, `fraction` of the way (0 to 1) through it.
pub(crate) fn fraction(fraction: f64) {
    let (lo, hi) = (LO.load(Relaxed), HI.load(Relaxed));
    let at = lo as f64 + (hi.saturating_sub(lo)) as f64 * fraction.clamp(0.0, 1.0);
    VALUE.fetch_max(at as u32, Relaxed);
}

/// Sheets' XML of `total` bytes in all are about to be read, taking the part `from`..`to` (0 to 1) of the stage.
pub(crate) fn rows_begin(total: u64, from: f64, to: f64) {
    ROWS_FROM.store((from * 1000.0) as u32, Relaxed);
    ROWS_TO.store((to * 1000.0) as u32, Relaxed);
    ROWS_TOTAL.store(total.max(1), Relaxed);
    ROWS_DONE.store(0, Relaxed);
}

/// `bytes` more of the sheets' XML were read.
pub(crate) fn rows_done(bytes: u64) {
    let (from, to) = (ROWS_FROM.load(Relaxed) as f64 / 1000.0, ROWS_TO.load(Relaxed) as f64 / 1000.0);
    let done = ROWS_DONE.fetch_add(bytes, Relaxed) + bytes;
    let total = ROWS_TOTAL.load(Relaxed).max(1);
    fraction(from + (to - from) * (done as f64 / total as f64).min(1.0));
}

/// How far reading has got: 0 to 1.
pub fn current() -> f32 {
    VALUE.load(Relaxed) as f32 / 1000.0
}
