// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The time, for everything in the app: the only place that reads the
//! clock. `std::time::Instant::now` and `SystemTime::now` panic on
//! wasm32-unknown-unknown (opening a .docx in the web version did, until
//! 2026-10-06), so the web version reads the browser's clock (`Date.now()`)
//! instead. Calling them anywhere else is refused (clippy.toml's
//! disallowed-methods, and tests/clock_rule.rs).
#![allow(clippy::disallowed_methods)]

use std::time::Duration;
mod local_date;
pub(crate) use local_date::format_local_datetime;

/// Milliseconds since 1970-01-01 UTC.
pub(crate) fn unix_millis() -> u64 {
    #[cfg(not(target_family = "wasm"))]
    return std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    #[cfg(target_family = "wasm")]
    return js_sys::Date::now().max(0.0) as u64;
}

/// The local calendar now (the computer's time zone): what `DATE` and
/// `TIME` fields show.
pub(crate) fn local_now() -> ibis_doc_model::date_field::LocalTime {
    let seconds = (unix_millis() / 1000) as i64;
    ibis_doc_model::date_field::LocalTime::from_seconds(seconds + local_offset(seconds))
}

/// The clock for the calculator (`NOW`, `TODAY` and the year of `9/30` are
/// the local time): seconds since 1970 (UTC) and the time zone's offset.
pub(crate) fn calc_clock() -> (i64, i64) {
    let seconds = (unix_millis() / 1000) as i64;
    (seconds, local_offset(seconds))
}

/// Seconds east of UTC of the computer's time zone at `seconds` after 1970.
fn local_offset(seconds: i64) -> i64 {
    #[cfg(target_family = "wasm")]
    {
        // The browser's offset, in minutes west of UTC.
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.0));
        -(date.get_timezone_offset() as i64) * 60
    }
    #[cfg(all(unix, not(target_family = "wasm")))]
    {
        // The C library's localtime_r (of the system: no dependency).
        #[repr(C)]
        struct Tm {
            fields: [std::ffi::c_int; 9],
            gmtoff: std::ffi::c_long,
            zone: *const std::ffi::c_char,
        }
        unsafe extern "C" {
            fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
        }
        let mut tm = Tm { fields: [0; 9], gmtoff: 0, zone: std::ptr::null() };
        // SAFETY: localtime_r writes only into `tm`, a `struct tm` of this layout.
        let ok = unsafe { !localtime_r(&seconds, &mut tm).is_null() };
        if ok { tm.gmtoff as i64 } else { 0 }
    }
    #[cfg(windows)]
    {
        #[repr(C)]
        struct SystemTime([u16; 8]);
        #[repr(C)]
        struct TimeZoneInformation {
            bias: i32,
            standard_name: [u16; 32],
            standard_date: SystemTime,
            standard_bias: i32,
            daylight_name: [u16; 32],
            daylight_date: SystemTime,
            daylight_bias: i32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetTimeZoneInformation(out: *mut TimeZoneInformation) -> u32;
        }
        let _ = seconds;
        let mut info = TimeZoneInformation { bias: 0, standard_name: [0; 32], standard_date: SystemTime([0; 8]), standard_bias: 0, daylight_name: [0; 32], daylight_date: SystemTime([0; 8]), daylight_bias: 0 };
        // SAFETY: GetTimeZoneInformation writes only into `info`, a TIME_ZONE_INFORMATION.
        let id = unsafe { GetTimeZoneInformation(&mut info) };
        let bias = info.bias + match id {
            1 => info.standard_bias,
            2 => info.daylight_bias,
            _ => 0,
        };
        -(bias as i64) * 60
    }
}

/// The time now as `SystemTime` (the desktop app only: `--timing` compares
/// it with when the process started).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn system_now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}

/// A moment, to measure the time since (as `std::time::Instant`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Instant {
    #[cfg(not(target_family = "wasm"))]
    at: std::time::Instant,
    /// The browser's milliseconds since 1970 (to a millisecond).
    #[cfg(target_family = "wasm")]
    at: f64,
}

impl Instant {
    pub(crate) fn now() -> Self {
        #[cfg(not(target_family = "wasm"))]
        return Self { at: std::time::Instant::now() };
        #[cfg(target_family = "wasm")]
        return Self { at: js_sys::Date::now() };
    }

    pub(crate) fn elapsed(&self) -> Duration {
        #[cfg(not(target_family = "wasm"))]
        return self.at.elapsed();
        #[cfg(target_family = "wasm")]
        return Duration::from_secs_f64(((js_sys::Date::now() - self.at) / 1000.0).max(0.0));
    }
}

/// Times taken, printed when `IBIS_DEBUG_TIMING` is set; the clock is read
/// only then (and the browser has no environment: never there).
pub(crate) mod debug_timing {
    pub(crate) struct Start(super::Instant);

    /// The time now if timing is on.
    pub(crate) fn start() -> Option<Start> {
        #[cfg(not(target_family = "wasm"))]
        if std::env::var_os("IBIS_DEBUG_TIMING").is_some() {
            return Some(Start(super::Instant::now()));
        }
        None
    }

    /// Print how long `what` took since `start`, if timing is on.
    pub(crate) fn report(what: &str, start: Option<Start>) {
        if let Some(Start(at)) = start {
            eprintln!("{what} {:.1} ms", at.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
