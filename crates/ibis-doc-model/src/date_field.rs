// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Dates and times as Word's fields write them (`DATE`, `TIME`, and the
//! picture of `\@`): the calendar of a moment, and the text of a picture
//! such as `yyyy'年'M'月'd'日'` or `ggge年M月d日（aaa）`.

/// A moment on the local calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 for Sunday to 6 for Saturday.
    pub weekday: u32,
}

impl LocalTime {
    /// The moment `seconds` after 1970-01-01 00:00 (local: the offset from
    /// UTC already added).
    pub fn from_seconds(seconds: i64) -> Self {
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400) as u32;
        // Howard Hinnant's days_from_civil, backwards.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        LocalTime { year, month, day, hour: rest / 3600, minute: rest / 60 % 60, second: rest % 60, weekday: (days + 4).rem_euclid(7) as u32 }
    }

    /// Converts this LocalTime to seconds since 1970-01-01 00:00:00.
    pub fn to_seconds(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 86_400
            + (self.hour as i64) * 3600
            + (self.minute as i64) * 60
            + (self.second as i64)
    }

    /// Adds `days` days to this LocalTime.
    pub fn add_days(&self, days: i64) -> Self {
        Self::from_seconds(self.to_seconds() + days * 86_400)
    }

    /// Formats as ISO 8601 UTC date string `YYYY-MM-DDTHH:MM:SSZ`.
    pub fn to_iso_date(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", self.year, self.month, self.day, self.hour, self.minute, self.second)
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const WEEKDAYS_JA: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

/// The Japanese era of a date: its name, its letter, and the year in it.
fn era(t: &LocalTime) -> (&'static str, &'static str, i64) {
    let ymd = (t.year, t.month, t.day);
    let (name, letter, first) = if ymd >= (2019, 5, 1) {
        ("令和", "R", 2019)
    } else if ymd >= (1989, 1, 8) {
        ("平成", "H", 1989)
    } else if ymd >= (1926, 12, 25) {
        ("昭和", "S", 1926)
    } else if ymd >= (1912, 7, 30) {
        ("大正", "T", 1912)
    } else {
        ("明治", "M", 1868)
    };
    (name, letter, t.year - first + 1)
}

/// `n` in kanji numerals (十月, 二十七日).
fn kanji_number(n: i64) -> String {
    const DIGITS: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n < 10 {
        return DIGITS[n.max(0) as usize].to_string();
    }
    let (tens, ones) = (n / 10, n % 10);
    format!("{}十{}", if tens == 1 { String::new() } else { kanji_number(tens) }, if ones == 0 { String::new() } else { DIGITS[ones as usize].to_string() })
}

/// The text of a presentation's date field of form `format` (`datetime1`
/// to `datetime13`, 0 for `datetime`, 14 for `datetimeyyyy`) at `t`: in
/// Japanese as PowerPoint shows them (test/editing/pptx-datetime-by-powerpoint.pptx,
/// ja-JP), else as ECMA-376 shows the forms (21.1.2.2.4, U.S. English).
pub fn slide_date_text(format: u8, east_asian: bool, t: &LocalTime) -> Option<String> {
    if east_asian {
        let picture = match format {
            0 | 1 => "yyyy/M/d",
            2 => "yyyy'年'M'月'd'日'(aaa)",
            3 => "ggge'年'M'月'd'日'",
            4 => "yyyy'年'M'月'd'日'",
            5 => "yyyy/MM/dd",
            6 => "ggge'年'M'月'",
            7 => {
                let (name, _, year) = era(t);
                return Some(format!("{name}{}年{}月{}日", kanji_number(year), kanji_number(t.month as i64), kanji_number(t.day as i64)));
            }
            8 => "yy/M/d H'時'mm'分'",
            9 => "yy/M/d H'時'mm'分'ss'秒'",
            10 => "H:mm",
            11 => "H:mm:ss",
            12 => "H'時'mm'分'",
            13 => "H'時'mm'分'ss'秒'",
            14 => "yyyy'年'",
            _ => return None,
        };
        return Some(format_picture(t, picture));
    }
    let picture = match format {
        0 | 1 => "M/d/yyyy",
        2 => "dddd, MMMM d, yyyy",
        3 => "d MMMM yyyy",
        4 => "MMMM d, yyyy",
        5 => "d-MMM-yy",
        6 => "MMMM yy",
        7 => "MMM-yy",
        8 => "M/d/yyyy h:mm AM/PM",
        9 => "M/d/yyyy h:mm:ss AM/PM",
        10 => "H:mm",
        11 => "H:mm:ss",
        12 => "h:mm AM/PM",
        13 => "h:mm:ss AM/PM",
        14 => "yyyy",
        _ => return None,
    };
    Some(format_picture(t, picture))
}

/// The text of date-and-time picture `picture` (a field's `\@`) for `t`:
/// `yyyy` `yy` (year), `M` `MM` `MMM` `MMMM` (month), `d` `dd` (day),
/// `ddd` `dddd` (weekday), `aaa` `aaaa` (曜日), `ggg` `gg` `g` and `e` `ee`
/// (和暦), `H` `HH` `h` `hh` (hour), `m` `mm` (minute), `s` `ss` (second),
/// `AM/PM` `am/pm`; text in `'` and anything else as it is.
pub fn format_picture(t: &LocalTime, picture: &str) -> String {
    let chars: Vec<char> = picture.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let run = |c: char, i: usize| chars[i..].iter().take_while(|&&x| x == c).count();
    while i < chars.len() {
        let c = chars[i];
        let n = run(c, i);
        let two = |v: u32| format!("{v:02}");
        let hour12 = if t.hour % 12 == 0 { 12 } else { t.hour % 12 };
        match c {
            '\'' => {
                let end = chars[i + 1..].iter().position(|&x| x == '\'').map_or(chars.len(), |p| i + 1 + p);
                out.extend(&chars[i + 1..end]);
                i = end + 1;
                continue;
            }
            'y' | 'Y' => out.push_str(&if n <= 2 { format!("{:02}", t.year.rem_euclid(100)) } else { t.year.to_string() }),
            'M' => out.push_str(&match n {
                1 => t.month.to_string(),
                2 => two(t.month),
                3 => MONTHS[t.month as usize - 1][..3].to_string(),
                _ => MONTHS[t.month as usize - 1].to_string(),
            }),
            'd' | 'D' => out.push_str(&match n {
                1 => t.day.to_string(),
                2 => two(t.day),
                3 => WEEKDAYS[t.weekday as usize][..3].to_string(),
                _ => WEEKDAYS[t.weekday as usize].to_string(),
            }),
            'a' if n >= 3 => out.push_str(&format!("{}{}", WEEKDAYS_JA[t.weekday as usize], if n >= 4 { "曜日" } else { "" })),
            'g' => {
                let (name, letter, _) = era(t);
                out.push_str(match n {
                    1 => letter,
                    2 => &name[..3],
                    _ => name,
                });
            }
            'e' => {
                let (_, _, y) = era(t);
                out.push_str(&if n >= 2 { format!("{y:02}") } else { y.to_string() });
            }
            'H' => out.push_str(&if n >= 2 { two(t.hour) } else { t.hour.to_string() }),
            'h' => out.push_str(&if n >= 2 { two(hour12) } else { hour12.to_string() }),
            'm' => out.push_str(&if n >= 2 { two(t.minute) } else { t.minute.to_string() }),
            's' | 'S' => out.push_str(&if n >= 2 { two(t.second) } else { t.second.to_string() }),
            'A' | 'a' if chars[i..].iter().take(5).collect::<String>().eq_ignore_ascii_case("am/pm") => {
                let pm = t.hour >= 12;
                let s = if pm { "PM" } else { "AM" };
                out.push_str(&if c == 'a' { s.to_lowercase() } else { s.to_string() });
                i += 5;
                continue;
            }
            _ => {
                out.push(c);
                i += 1;
                continue;
            }
        }
        i += n;
    }
    out
}

/// What field `instruction` shows at `t` when it is a `DATE`, `TIME`,
/// `CREATEDATE`, `SAVEDATE`, or `PRINTDATE` (its `\@` picture, or Word's
/// Japanese defaults `yyyy/MM/dd` and `H:mm`); `None` for other fields.
pub fn date_field_text(instruction: &str, t: &LocalTime) -> Option<String> {
    let trimmed = instruction.trim_start();
    let kind = trimmed.split_whitespace().next()?;
    let default = match kind.to_ascii_uppercase().as_str() {
        "DATE" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => "yyyy/MM/dd",
        "TIME" => "H:mm",
        _ => return None,
    };
    Some(format_picture(t, &picture(trimmed).unwrap_or_else(|| default.to_string())))
}

/// Parse an ISO 8601 / W3CDTF datetime string (e.g. `2026-10-08T12:00:00Z` or `2026-10-08`) into a [`LocalTime`].
pub fn parse_iso8601(s: &str) -> Option<LocalTime> {
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    if s.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    let month: u32 = s.get(5..7)?.parse().ok()?;
    if s.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let mut hour = 0;
    let mut minute = 0;
    let mut second = 0;
    if s.len() >= 19 && (s.as_bytes().get(10) == Some(&b'T') || s.as_bytes().get(10) == Some(&b' ')) {
        hour = s.get(11..13)?.parse().ok()?;
        minute = s.get(14..16)?.parse().ok()?;
        second = s.get(17..19)?.parse().ok()?;
    }
    let days = days_from_civil(year, month, day);
    let weekday = (days + 4).rem_euclid(7) as u32;
    Some(LocalTime { year, month, day, hour, minute, second, weekday })
}

fn days_from_civil(mut y: i64, m: u32, d: u32) -> i64 {
    y -= i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// The picture of a field's `\@` switch (in quotes, or one word).
pub fn picture(instruction: &str) -> Option<String> {
    let at = instruction.find("\\@")?;
    let rest = instruction[at + 2..].trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        Some(quoted[..quoted.find('"').unwrap_or(quoted.len())].to_string())
    } else {
        rest.split_whitespace().next().map(str::to_string)
    }
}

#[cfg(test)]
mod tests {

    /// A multi-byte character inside the date must give `None`, not a panic from slicing by byte position.
    #[test]
    fn iso8601_with_multibyte_character_is_none() {
        assert!(parse_iso8601("20\u{e9}6-10-08T12:00:00Z").is_none());
        assert!(parse_iso8601("2026-1\u{e9}-08").is_none());
        assert!(parse_iso8601("2026-10-08T12:\u{e9}0:00Z").is_none());
        assert!(parse_iso8601("2026-10-08T12:00:00Z").is_some());
    }
    use super::*;

    #[test]
    fn calendar_of_a_moment() {
        // 2026-10-07 14:05:09 is a Wednesday.
        let t = LocalTime::from_seconds(1_791_381_909);
        assert_eq!(t, LocalTime { year: 2026, month: 10, day: 7, hour: 14, minute: 5, second: 9, weekday: 3 });
        assert_eq!(LocalTime::from_seconds(0), LocalTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, weekday: 4 });
        assert_eq!(LocalTime::from_seconds(951_782_400).month, 2, "2000-02-29");
        assert_eq!(LocalTime::from_seconds(951_782_400).day, 29);
    }

    #[test]
    fn pictures_as_word_writes_them() {
        let t = LocalTime::from_seconds(1_791_381_909);
        assert_eq!(format_picture(&t, "yyyy/MM/dd"), "2026/10/07");
        assert_eq!(format_picture(&t, "yyyy'年'M'月'd'日'"), "2026年10月7日");
        assert_eq!(format_picture(&t, "yyyy年M月d日(aaa)"), "2026年10月7日(水)");
        assert_eq!(format_picture(&t, "ggge年M月d日"), "令和8年10月7日");
        assert_eq!(format_picture(&t, "gge"), "令8");
        assert_eq!(format_picture(&t, "dddd, MMMM d, yyyy"), "Wednesday, October 7, 2026");
        assert_eq!(format_picture(&t, "H:mm:ss"), "14:05:09");
        assert_eq!(format_picture(&t, "h:mm AM/PM"), "2:05 PM");
        assert_eq!(format_picture(&t, "aaaa"), "水曜日");
        assert_eq!(date_field_text(r#"DATE \@ "yyyy'年'M'月'd'日'" \* MERGEFORMAT"#, &t).as_deref(), Some("2026年10月7日"));
        assert_eq!(date_field_text("DATE", &t).as_deref(), Some("2026/10/07"));
        assert_eq!(date_field_text("TIME", &t).as_deref(), Some("14:05"));
        assert_eq!(date_field_text(r#"CREATEDATE \@ "yyyy'年'M'月'd'日'""#, &t).as_deref(), Some("2026年10月7日"));
        assert_eq!(date_field_text("SAVEDATE", &t).as_deref(), Some("2026/10/07"));
        assert_eq!(date_field_text("PRINTDATE", &t).as_deref(), Some("2026/10/07"));
        // 平成 ends on 2019-04-30.
        let heisei = LocalTime { year: 2019, month: 4, day: 30, hour: 0, minute: 0, second: 0, weekday: 2 };
        assert_eq!(format_picture(&heisei, "ggge年"), "平成31年");
    }

    #[test]
    fn parse_iso8601_datetime() {
        let t = parse_iso8601("2026-10-07T14:05:09Z").expect("valid iso datetime");
        assert_eq!(t, LocalTime { year: 2026, month: 10, day: 7, hour: 14, minute: 5, second: 9, weekday: 3 });
        let t2 = parse_iso8601("2026-10-08").expect("valid date");
        assert_eq!(t2, LocalTime { year: 2026, month: 10, day: 8, hour: 0, minute: 0, second: 0, weekday: 4 });
    }

    #[test]
    fn test_localtime_to_seconds_and_add_days() {
        let t = LocalTime { year: 2026, month: 10, day: 9, hour: 0, minute: 0, second: 0, weekday: 5 };
        let s = t.to_seconds();
        assert_eq!(LocalTime::from_seconds(s), t);
        let t_next = t.add_days(1);
        assert_eq!(t_next, LocalTime { year: 2026, month: 10, day: 10, hour: 0, minute: 0, second: 0, weekday: 6 });
        assert_eq!(t_next.to_iso_date(), "2026-10-10T00:00:00Z");
    }
}

