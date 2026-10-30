// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Display metadata using the operating system's locale and time zone.

fn timestamp(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if !raw.is_ascii() {
        return None;
    }
    let time = ibis_doc_model::date_field::parse_iso8601(raw)?;
    if !(1..=12).contains(&time.month)
        || !(1..=31).contains(&time.day)
        || time.hour > 23
        || time.minute > 59
        || time.second > 59
    {
        return None;
    }
    let mut seconds = time.to_seconds();
    // OOXML timestamps default to UTC. Honour an explicit ISO 8601 offset.
    if let Some(start) = raw
        .get(19..)
        .and_then(|tail| tail.find(['+', '-']).map(|i| i + 19))
    {
        let tail = &raw[start..];
        let hour: i64 = tail.get(1..3)?.parse().ok()?;
        let minute: i64 = tail.get(4..6)?.parse().ok()?;
        if tail.as_bytes().get(3) != Some(&b':') || hour > 23 || minute > 59 {
            return None;
        }
        seconds -= (if tail.starts_with('-') { -1 } else { 1 }) * (hour * 3600 + minute * 60);
    }
    Some(seconds)
}

pub(crate) fn format_local_datetime(raw: &str) -> String {
    let Some(seconds) = timestamp(raw) else {
        return raw.to_string();
    };
    system_format(seconds).unwrap_or_else(|| raw.to_string())
}

#[cfg(target_os = "macos")]
fn system_format(seconds: i64) -> Option<String> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    type Id = *mut Object;
    // Foundation owns the locale/calendar/time-zone preferences, including the user's overrides.
    unsafe {
        let pool: Id = msg_send![class!(NSAutoreleasePool), new];
        let date: Id = msg_send![class!(NSDate), dateWithTimeIntervalSince1970: seconds as f64];
        let formatter: Id = msg_send![class!(NSDateFormatter), new];
        let locale: Id = msg_send![class!(NSLocale), currentLocale];
        let zone: Id = msg_send![class!(NSTimeZone), localTimeZone];
        let _: () = msg_send![formatter, setLocale: locale];
        let _: () = msg_send![formatter, setTimeZone: zone];
        let _: () = msg_send![formatter, setDateStyle: 1usize];
        let _: () = msg_send![formatter, setTimeStyle: 2usize];
        let rendered: Id = msg_send![formatter, stringFromDate: date];
        let bytes: *const std::ffi::c_char = msg_send![rendered, UTF8String];
        let result = (!bytes.is_null()).then(|| {
            std::ffi::CStr::from_ptr(bytes)
                .to_string_lossy()
                .into_owned()
        });
        let _: () = msg_send![formatter, release];
        let _: () = msg_send![pool, drain];
        result
    }
}

#[cfg(target_family = "wasm")]
fn system_format(seconds: i64) -> Option<String> {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.));
    Some(
        date.to_locale_string("default", &wasm_bindgen::JsValue::UNDEFINED)
            .into(),
    )
}

#[cfg(target_os = "linux")]
fn system_format(seconds: i64) -> Option<String> {
    use std::ffi::{c_char, c_int, c_long, c_void};
    #[repr(C)]
    struct Tm {
        fields: [c_int; 9],
        offset: c_long,
        zone: *const c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
        fn newlocale(mask: c_int, locale: *const c_char, base: *mut c_void) -> *mut c_void;
        fn strftime_l(
            out: *mut c_char,
            len: usize,
            format: *const c_char,
            tm: *const Tm,
            locale: *mut c_void,
        ) -> usize;
        fn freelocale(locale: *mut c_void);
    }
    unsafe {
        let mut tm = Tm {
            fields: [0; 9],
            offset: 0,
            zone: std::ptr::null(),
        };
        if localtime_r(&seconds, &mut tm).is_null() {
            return None;
        }
        let locale = newlocale(1 << 2, c"".as_ptr(), std::ptr::null_mut());
        if locale.is_null() {
            return None;
        }
        let mut out = [0u8; 256];
        let n = strftime_l(
            out.as_mut_ptr().cast(),
            out.len(),
            c"%x %X".as_ptr(),
            &tm,
            locale,
        );
        freelocale(locale);
        (n > 0).then(|| String::from_utf8_lossy(&out[..n]).into_owned())
    }
}

#[cfg(windows)]
fn system_format(seconds: i64) -> Option<String> {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        weekday: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const std::ffi::c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
        fn GetDateFormatEx(
            locale: *const u16,
            flags: u32,
            time: *const SystemTime,
            format: *const u16,
            out: *mut u16,
            len: i32,
            calendar: *const u16,
        ) -> i32;
        fn GetTimeFormatEx(
            locale: *const u16,
            flags: u32,
            time: *const SystemTime,
            format: *const u16,
            out: *mut u16,
            len: i32,
        ) -> i32;
    }
    let t = ibis_doc_model::date_field::LocalTime::from_seconds(seconds);
    let utc = SystemTime {
        year: t.year as u16,
        month: t.month as u16,
        weekday: t.weekday as u16,
        day: t.day as u16,
        hour: t.hour as u16,
        minute: t.minute as u16,
        second: t.second as u16,
        millis: 0,
    };
    unsafe {
        let mut local = std::mem::zeroed();
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
            return None;
        }
        let mut date = [0u16; 128];
        let mut time = [0u16; 128];
        let d = GetDateFormatEx(
            std::ptr::null(),
            1,
            &local,
            std::ptr::null(),
            date.as_mut_ptr(),
            128,
            std::ptr::null(),
        );
        let t = GetTimeFormatEx(
            std::ptr::null(),
            0,
            &local,
            std::ptr::null(),
            time.as_mut_ptr(),
            128,
        );
        (d > 0 && t > 0).then(|| {
            format!(
                "{} {}",
                String::from_utf16_lossy(&date[..d as usize - 1]),
                String::from_utf16_lossy(&time[..t as usize - 1])
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_offsets_represent_the_same_instant() {
        assert_eq!(
            timestamp("2026-10-24T09:00:00+09:00"),
            timestamp("2026-10-24T00:00:00Z")
        );
        assert_eq!(
            timestamp("2026-10-23T19:00:00-05:00"),
            timestamp("2026-10-24T00:00:00Z")
        );
        assert_eq!(format_local_datetime(""), "");
        assert_eq!(format_local_datetime("unknown"), "unknown");
    }
}
