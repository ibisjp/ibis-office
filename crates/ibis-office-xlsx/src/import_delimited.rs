// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A CSV or TSV file read into a query table (the data side of `import_text.rs`: no field of a panel is read here).
use crate::import_text::ENCODINGS;
use ibis_doc_model::CellValue;
use ibis_formats::csv::ImportOptions;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub(crate) fn read_delimited_query(path: &std::path::Path, encoding: &str, delimiter: Option<char>, start_row: usize, headers: bool) -> Result<ibis_doc_model::query::QTable, String> {
    let encoding = ENCODINGS.iter().find(|(_, id)| *id == encoding)
        .map(|(encoding, _)| *encoding).ok_or("Unsupported text encoding")?;
    let data = read_delimited_bytes(path)?;
    delimited_table(&data, &ImportOptions { encoding, delimiter, start_row, fixed: None }, headers)
}

pub(crate) fn read_delimited_bytes(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if !path.is_file() {
        return Err("Select a local CSV or TSV file".into());
    }
    let mut data = Vec::new();
    std::fs::File::open(path).and_then(|file| file.take(64 * 1024 * 1024 + 1).read_to_end(&mut data))
        .map_err(|error| error.to_string())?;
    if data.len() > 64 * 1024 * 1024 {
        return Err("Delimited source exceeds the 64 MiB limit".into());
    }
    Ok(data)
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub(crate) fn delimited_table(data: &[u8], options: &ImportOptions, headers: bool) -> Result<ibis_doc_model::query::QTable, String> {
    use ibis_doc_model::query::{QType, table_of_cells};
    if data.len() > 64 * 1024 * 1024 {
        return Err("Delimited source exceeds the 64 MiB limit".into());
    }
    let text = ibis_formats::csv::decode_as(data, options.encoding).map_err(|error| error.to_string())?;
    let mut records = delimited_records(&text, options);
    let headings = headers.then(|| records.next()).flatten();
    let mut width = headings.as_ref().map_or(0, Vec::len);
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for record in records {
        width = width.max(record.len());
        let height = rows.len() + 1;
        if width > 16_384 || height > 1_048_575 || height.saturating_mul(width) > 5_000_000 {
            return Err("Delimited source exceeds the table size limit".into());
        }
        rows.push(record.into_iter().map(delimited_cell).collect());
    }
    if width == 0 || width > 16_384 {
        return Err("Delimited source is empty or exceeds the column limit".into());
    }
    if headings.iter().flatten().any(|field| field.encode_utf16().take(32_768).count() > 32_767)
        || rows.iter().flatten().any(|value| matches!(value, CellValue::Text(field) if field.encode_utf16().take(32_768).count() > 32_767))
    {
        return Err("Delimited text exceeds the spreadsheet cell limit".into());
    }
    let mut table = table_of_cells(rows, width);
    let mut used = std::collections::HashSet::new();
    for (index, column) in table.columns.iter_mut().enumerate() {
        let base = headings.as_ref().and_then(|row| row.get(index)).filter(|name| !name.is_empty()).unwrap_or(&column.name);
        let mut name = base.clone();
        let mut suffix = 2;
        while !used.insert(name.to_lowercase()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        column.name = name;
        let mut kinds = table.rows.iter().filter_map(|row| match &row[index] {
            CellValue::Empty => None,
            CellValue::Number(value) => Some(if value.fract() == 0.0 { QType::Whole } else { QType::Decimal }),
            CellValue::Bool(_) => Some(QType::Logical),
            _ => Some(QType::Text),
        });
        if let Some(first) = kinds.next() {
            column.ty = kinds.fold(first, |current, next| match (current, next) {
                (a, b) if a == b => a,
                (QType::Whole, QType::Decimal) | (QType::Decimal, QType::Whole) => QType::Decimal,
                _ => QType::Any,
            });
        }
    }
    Ok(table)
}

pub(crate) fn delimited_records<'a>(text: &'a str, options: &ImportOptions) -> impl Iterator<Item = Vec<String>> + 'a {
    let delimiter = options.delimiter.unwrap_or_else(|| ibis_formats::csv::detect_delimiter(text));
    ibis_formats::csv::CsvRecordIter::new(text, delimiter).skip(options.start_row.max(1) - 1)
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn delimited_cell(field: String) -> CellValue {
    if field.is_empty() {
        return CellValue::Empty;
    }
    // External data is never evaluated as a spreadsheet formula.
    let trimmed = field.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        return CellValue::Bool(trimmed.eq_ignore_ascii_case("true"));
    }
    let magnitude = trimmed.trim_start_matches(['+', '-']);
    let leading_zero = magnitude.starts_with('0') && magnitude.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
    match trimmed.parse::<f64>() {
        Ok(number) if !leading_zero && number.is_finite() && number.abs() < (1u64 << 53) as f64 =>
            CellValue::Number(number),
        _ => CellValue::Text(field),
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows", target_os = "linux")))]
#[::core::prelude::v1::test]
fn delimited_query_keeps_headers_and_does_not_evaluate_formulas() {
    let data = b"code\tamount\tamount\n001\t2.5\t=1+2\n";
    let table = delimited_table(data, &ImportOptions { delimiter: Some('\t'), ..Default::default() }, true).unwrap();
    assert_eq!(table.names(), ["code", "amount", "amount_2"]);
    assert_eq!(table.rows[0], [CellValue::Text("001".into()), CellValue::Number(2.5), CellValue::Text("=1+2".into())]);
}
