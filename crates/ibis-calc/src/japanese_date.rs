// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Conservative date candidates for sheet review. Input is never changed here.
//! Comparisons retain the precision and era bounds of partial dates.

use crate::eval::convert::days_in_month;

pub(crate) type Day = (i64, u32, u32);

pub(crate) struct DateRange {
    pub first: Day,
    pub last: Day,
    pub inferred: bool,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateReview {
    Candidate(String),
    /// The century was inferred using the documented 30-year pivot.
    Inferred(String),
    Unrecognized,
    Invalid,
}

/// Recognize complete Gregorian or Japanese-era dates. Incomplete dates are
/// not guessed. Two-digit years use
/// 00–29 = 2000–2029 and 30–99 = 1930–1999 and are marked as inferred.
pub fn review_date(input: &str) -> DateReview {
    let range = match parse_date(input, false) {
        Ok(range) => range,
        Err(error) => return error,
    };
    let (year, month, day) = range.first;
    let result = format!("{year:04}-{month:02}-{day:02}");
    if range.inferred {
        DateReview::Inferred(result)
    } else {
        DateReview::Candidate(result)
    }
}

pub(crate) fn parse_date(input: &str, partial: bool) -> Result<DateRange, DateReview> {
    let input = input.trim();
    let normalized: String = input
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            '／' => '/',
            '－' => '-',
            '．' => '.',
            _ => c,
        })
        .collect();
    let eras = [
        ("令和", 2018, (2019, 5, 1), None),
        ("R", 2018, (2019, 5, 1), None),
        ("r", 2018, (2019, 5, 1), None),
        ("平成", 1988, (1989, 1, 8), Some((2019, 4, 30))),
        ("H", 1988, (1989, 1, 8), Some((2019, 4, 30))),
        ("h", 1988, (1989, 1, 8), Some((2019, 4, 30))),
        ("昭和", 1925, (1926, 12, 25), Some((1989, 1, 7))),
        ("S", 1925, (1926, 12, 25), Some((1989, 1, 7))),
        ("s", 1925, (1926, 12, 25), Some((1989, 1, 7))),
        ("大正", 1911, (1912, 7, 30), Some((1926, 12, 24))),
        ("T", 1911, (1912, 7, 30), Some((1926, 12, 24))),
        ("t", 1911, (1912, 7, 30), Some((1926, 12, 24))),
        ("明治", 1867, (1873, 1, 1), Some((1912, 7, 29))),
        ("M", 1867, (1873, 1, 1), Some((1912, 7, 29))),
        ("m", 1867, (1873, 1, 1), Some((1912, 7, 29))),
    ];
    let era = eras
        .iter()
        .find_map(|(name, base, start, end)| normalized.strip_prefix(name).map(|rest| (rest, *base, *start, *end)));
    let text = era.map_or(normalized.as_str(), |v| v.0);
    let text = if era.is_some() && text.starts_with('元') {
        format!("1{}", &text['元'.len_utf8()..])
    } else {
        text.to_owned()
    };
    let parts: Vec<&str> = if let Some(day) = text.strip_suffix('日') {
        let (year, md) = day.split_once('年').ok_or(DateReview::Unrecognized)?;
        let (month, day) = md.split_once('月').ok_or(DateReview::Unrecognized)?;
        vec![year, month, day]
    } else if partial && let Some(month) = text.strip_suffix('月') {
        let (year, month) = month.split_once('年').ok_or(DateReview::Unrecognized)?;
        vec![year, month]
    } else if partial && let Some(year) = text.strip_suffix('年') {
        vec![year]
    } else {
        let delimiter = ['/', '-', '.'].into_iter().find(|&c| text.contains(c));
        match delimiter {
            Some(delimiter) => text.split(delimiter).collect(),
            None if partial => vec![&text],
            None => return Err(DateReview::Unrecognized),
        }
    };
    if parts.len() > 3 || (!partial && parts.len() != 3) || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(DateReview::Unrecognized);
    }
    if era.is_none() && !(parts[0].len() == 4 || (parts.len() == 3 && parts[0].len() == 2)) {
        return Err(DateReview::Unrecognized);
    }
    let inferred = era.is_none() && parts[0].len() == 2;
    let year: i64 = parts[0].parse().map_err(|_| DateReview::Invalid)?;
    if year == 0 && !inferred {
        return Err(DateReview::Invalid);
    }
    let year = match era {
        Some((_, base, _, _)) => year.checked_add(base).ok_or(DateReview::Invalid)?,
        None if inferred && year < 30 => 2000 + year,
        None if inferred => 1900 + year,
        None => year,
    };
    if year > 9999 {
        return Err(DateReview::Invalid);
    }
    let component = |index| parts.get(index).map(|s: &&str| s.parse::<u32>()).transpose().map_err(|_| DateReview::Invalid);
    let month = component(1)?;
    let day = component(2)?;
    let first_month = month.unwrap_or(1);
    let last_month = month.unwrap_or(12);
    if !(1..=12).contains(&first_month) {
        return Err(DateReview::Invalid);
    }
    let last_day = days_in_month(year, last_month);
    if day.is_some_and(|d| d == 0 || d > last_day) {
        return Err(DateReview::Invalid);
    }
    if era.is_some() && year < 1873 {
        return Err(DateReview::Unrecognized);
    }
    let mut first = (year, first_month, day.unwrap_or(1));
    let mut last = (year, last_month, day.unwrap_or(last_day));
    if let Some((_, _, start, end)) = era {
        first = first.max(start);
        if let Some(end) = end {
            last = last.min(end);
        }
    }
    if first > last {
        return Err(DateReview::Invalid);
    }
    Ok(DateRange {
        first,
        last,
        inferred,
        complete: day.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_dates_and_era_boundaries() {
        assert_eq!(review_date("令和元年5月1日"), DateReview::Candidate("2019-05-01".into()));
        assert_eq!(review_date("平成３１年４月３０日"), DateReview::Candidate("2019-04-30".into()));
        assert_eq!(review_date("S60.4.1"), DateReview::Candidate("1985-04-01".into()));
        assert_eq!(review_date("平成31年5月1日"), DateReview::Invalid);
        assert_eq!(review_date("令和元年4月30日"), DateReview::Invalid);
        assert_eq!(review_date("2024/2/29"), DateReview::Candidate("2024-02-29".into()));
        assert_eq!(review_date("明治5年1月1日"), DateReview::Unrecognized);
        assert_eq!(review_date("1900/2/29"), DateReview::Invalid);
        assert_eq!(review_date("24/2/29"), DateReview::Inferred("2024-02-29".into()));
        assert_eq!(review_date("00/1/1"), DateReview::Inferred("2000-01-01".into()));
        assert_eq!(review_date("29/12/31"), DateReview::Inferred("2029-12-31".into()));
        assert_eq!(review_date("30/1/1"), DateReview::Inferred("1930-01-01".into()));
        assert_eq!(review_date("99/1/1"), DateReview::Inferred("1999-01-01".into()));
        assert_eq!(review_date("令和5年2月"), DateReview::Unrecognized);
        assert_eq!(review_date("2024/1-2"), DateReview::Unrecognized);
    }
}
