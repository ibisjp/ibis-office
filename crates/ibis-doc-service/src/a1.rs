// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A1 references (`B2`, `B2:D10`, `A:A`): 0-based rows and columns.

use ibis_doc_model::sheet::CellRange;

use crate::error::ToolError;

/// The letters of a 0-based column (0 is `A`, 26 is `AA`).
pub fn column_name(mut column: u32) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    letters.iter().rev().collect()
}

fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.trim().replace('$', "");
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let column = letters.to_ascii_uppercase().bytes().fold(0u32, |n, b| n * 26 + (b - b'A') as u32 + 1) - 1;
    let row: u32 = digits.parse().ok().filter(|&r| r >= 1)?;
    (column < 16384 && row <= 1_048_576).then_some((row - 1, column))
}

/// `"B2"` or `"B2:D10"` as a range.
pub fn parse_range(text: &str) -> Result<CellRange, ToolError> {
    let bad = || ToolError::invalid(format!("not a range in A1 form: {text}"));
    let (a, b) = match text.split_once(':') {
        Some((a, b)) => (parse_cell(a).ok_or_else(bad)?, parse_cell(b).ok_or_else(bad)?),
        None => {
            let c = parse_cell(text).ok_or_else(bad)?;
            (c, c)
        }
    };
    Ok(CellRange { first_row: a.0.min(b.0), first_column: a.1.min(b.1), last_row: a.0.max(b.0), last_column: a.1.max(b.1) })
}

pub fn range_text(range: &CellRange) -> String {
    let first = format!("{}{}", column_name(range.first_column), range.first_row + 1);
    if range.first_row == range.last_row && range.first_column == range.last_column {
        first
    } else {
        format!("{first}:{}{}", column_name(range.last_column), range.last_row + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_and_ranges_round_trip() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        let r = parse_range("B2:D10").unwrap();
        assert_eq!((r.first_row, r.first_column, r.last_row, r.last_column), (1, 1, 9, 3));
        assert_eq!(range_text(&r), "B2:D10");
        assert_eq!(range_text(&parse_range("c3").unwrap()), "C3");
        assert!(parse_range("2B").is_err() && parse_range("A0").is_err() && parse_range("").is_err());
    }
}
