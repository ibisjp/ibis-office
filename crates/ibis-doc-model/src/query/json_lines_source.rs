// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/263)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{QColumn, QTable, QType};
use crate::CellValue;
use serde_json::{Value, value::RawValue};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_COLUMNS: usize = 16_384;
const MAX_DATA_ROWS: usize = 1_048_575;
const MAX_CELLS: usize = 5_000_000;
const MAX_TEXT_UNITS: usize = 32_767;

/// Read one JSON object per line. Absent fields and nulls become empty cells.
pub fn read_json_lines(path: &Path) -> Result<QTable, String> {
    if !path.is_file() {
        return Err("Select a local JSON Lines file".into());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    read_records(BufReader::new(file)).map_err(|error| {
        format!(
            "{}: {error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn read_records(mut reader: impl BufRead) -> Result<QTable, String> {
    let mut output = Records::default();
    let mut line = Vec::new();
    let mut bytes = 0;
    for line_number in 1.. {
        line.clear();
        let read = reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("Line {line_number}: {error}"))?;
        if read == 0 {
            break;
        }
        bytes += read;
        if read > MAX_LINE_BYTES || bytes > MAX_SOURCE_BYTES {
            return Err(format!(
                "Line {line_number}: JSON Lines source exceeds the byte limit"
            ));
        }
        let fields = serde_json::from_slice::<Fields>(&line)
            .map_err(|error| format!("Line {line_number}: expected one JSON object ({error})"))?
            .0;
        output.push(fields, &format!("Line {line_number}"))?;
    }
    output.finish()
}

#[derive(Default)]
struct Records {
    table: QTable,
    indices: HashMap<String, usize>,
    types: Vec<Option<QType>>,
}

impl Records {
    fn push(&mut self, fields: Vec<(String, &RawValue)>, location: &str) -> Result<(), String> {
        let width = self.indices.len()
            + fields
                .iter()
                .filter(|(key, _)| !self.indices.contains_key(key))
                .count();
        let height = self.table.rows.len() + 1;
        if width > MAX_COLUMNS || height > MAX_DATA_ROWS || width.saturating_mul(height) > MAX_CELLS
        {
            return Err(format!(
                "{location}: JSON source exceeds the table size limit"
            ));
        }
        let mut row = vec![CellValue::Empty; width];
        for (name, value) in fields {
            if exceeds_text_limit(&name) {
                return Err(format!(
                    "{location}: field name exceeds the spreadsheet cell limit"
                ));
            }
            let index = *self.indices.entry(name.clone()).or_insert_with(|| {
                let index = self.table.columns.len();
                self.table.columns.push(QColumn {
                    name: name.clone(),
                    ty: QType::Any,
                });
                self.types.push(None);
                index
            });
            let (cell, ty) = cell(value).map_err(|error| format!("{location}, {name}: {error}"))?;
            if let Some(ty) = ty {
                self.types[index] = Some(match (self.types[index], ty) {
                    (None, ty) => ty,
                    (Some(previous), ty) if previous == ty => previous,
                    (Some(QType::Whole), QType::Decimal) | (Some(QType::Decimal), QType::Whole) => {
                        QType::Decimal
                    }
                    _ => QType::Any,
                });
            }
            row[index] = cell;
        }
        self.table.rows.push(row);
        Ok(())
    }

    fn finish(mut self) -> Result<QTable, String> {
        if self.table.columns.is_empty() {
            return Err("JSON source contains no fields".into());
        }
        for (column, ty) in self.table.columns.iter_mut().zip(self.types) {
            column.ty = ty.unwrap_or(QType::Any);
        }
        for row in &mut self.table.rows {
            row.resize(self.table.columns.len(), CellValue::Empty);
        }
        Ok(self.table)
    }
}

fn json_bytes(path: &Path) -> Result<Vec<u8>, String> {
    if !path.is_file() {
        return Err("Select a local JSON file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| {
            file.take((MAX_SOURCE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("JSON source exceeds the byte limit".into());
    }
    Ok(bytes)
}

/// The root array, or the names of array-valued fields in the root object.
pub fn json_array_choices(path: &Path) -> Result<Vec<Option<String>>, String> {
    let bytes = json_bytes(path)?;
    let document: &RawValue = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if document.get().starts_with('[') {
        return Ok(vec![None]);
    }
    let fields: BTreeMap<String, &RawValue> =
        serde_json::from_str(document.get()).map_err(|error| error.to_string())?;
    let choices: Vec<_> = fields
        .into_iter()
        .filter(|(_, value)| value.get().starts_with('['))
        .map(|(key, _)| Some(key))
        .collect();
    if choices.len() > 1024 {
        return Err("JSON source has too many arrays to list".into());
    }
    Ok(choices)
}

pub fn read_json_array(path: &Path, key: Option<&str>) -> Result<QTable, String> {
    let bytes = json_bytes(path)?;
    records_from_array(&bytes, key)
}

fn records_from_array(bytes: &[u8], key: Option<&str>) -> Result<QTable, String> {
    let document: &RawValue = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let array = match key {
        Some(key) => {
            let fields: BTreeMap<String, &RawValue> =
                serde_json::from_str(document.get()).map_err(|error| error.to_string())?;
            *fields
                .get(key)
                .ok_or_else(|| format!("Missing JSON array: {key}"))?
        }
        None => document,
    };
    let mut remaining = array
        .get()
        .strip_prefix('[')
        .and_then(|text| text.strip_suffix(']'))
        .ok_or("Expected a JSON array")?
        .trim_start();
    let mut output = Records::default();
    // RawValue has already validated the array syntax. Read one record at a
    // time so a dense array cannot allocate a second array of references.
    while !remaining.is_empty() {
        let location = format!("Record {}", output.table.rows.len() + 1);
        let mut stream = serde_json::Deserializer::from_str(remaining).into_iter::<&RawValue>();
        let row = stream
            .next()
            .ok_or("Expected a JSON record")?
            .map_err(|error| format!("{location}: {error}"))?;
        let fields = serde_json::from_str::<Fields>(row.get())
            .map_err(|error| format!("{location}: expected an object ({error})"))?
            .0;
        output.push(fields, &location)?;
        remaining = remaining[stream.byte_offset()..].trim_start();
        if let Some(rest) = remaining.strip_prefix(',') {
            remaining = rest.trim_start();
        }
    }
    output.finish()
}

/// The fields of one JSON object in the order they are written (a map would sort the
/// names); a repeated name keeps its first place and its last value.
struct Fields<'a>(Vec<(String, &'a RawValue)>);

impl<'de> serde::Deserialize<'de> for Fields<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Fields<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Fields<'de>, A::Error> {
                let mut fields: Vec<(String, &'de RawValue)> = Vec::new();
                while let Some((name, value)) = map.next_entry::<String, &'de RawValue>()? {
                    match fields.iter_mut().find(|(known, _)| *known == name) {
                        Some(slot) => slot.1 = value,
                        None => fields.push((name, value)),
                    }
                }
                Ok(Fields(fields))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

fn cell(raw: &RawValue) -> Result<(CellValue, Option<QType>), String> {
    let text = raw.get();
    if text.starts_with(['[', '{']) {
        if exceeds_text_limit(text) {
            return Err("Text exceeds the spreadsheet cell limit".into());
        }
        return Ok((CellValue::Text(text.into()), Some(QType::Text)));
    }
    let value = serde_json::from_str::<Value>(text).map_err(|error| error.to_string())?;
    let (cell, ty) = match value {
        Value::Null => return Ok((CellValue::Empty, None)),
        Value::Bool(value) => (CellValue::Bool(value), QType::Logical),
        Value::String(value) => (CellValue::Text(value), QType::Text),
        Value::Number(value) => {
            let number = value
                .as_f64()
                .ok_or("Number exceeds spreadsheet precision")?;
            if number.abs() >= (1u64 << 53) as f64 && number.fract() == 0. {
                return Err("Number exceeds spreadsheet precision".into());
            }
            let ty = if value.is_i64() || value.is_u64() {
                QType::Whole
            } else {
                QType::Decimal
            };
            (CellValue::Number(number), ty)
        }
        Value::Array(_) | Value::Object(_) => return Err("Expected a scalar JSON value".into()),
    };
    if let CellValue::Text(text) = &cell {
        if exceeds_text_limit(text) {
            return Err("Text exceeds the spreadsheet cell limit".into());
        }
    }
    Ok((cell, Some(ty)))
}

fn exceeds_text_limit(text: &str) -> bool {
    text.encode_utf16().take(MAX_TEXT_UNITS + 1).count() > MAX_TEXT_UNITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_sparse_records_with_new_fields_and_mixed_types() {
        let table = read_records(&b"{\"amount\":2,\"code\":\"001\",\"tags\":[true]}\r\n{\"amount\":2.5,\"extra\":false,\"tags\":null}\n{\"code\":9,\"tags\":{\"a\":18446744073709551617}}"[..]).unwrap();
        assert_eq!(table.names(), ["amount", "code", "tags", "extra"]);
        let table2 = read_records(&b"{\"zeta\":1,\"alpha\":2,\"zeta\":3}
{\"mid\":4,\"alpha\":5}"[..]).unwrap();
        assert_eq!(table2.names(), ["zeta", "alpha", "mid"]);
        assert_eq!(table2.rows[0][0], CellValue::Number(3.));
        assert_eq!(
            table.columns.iter().map(|col| col.ty).collect::<Vec<_>>(),
            [QType::Decimal, QType::Any, QType::Text, QType::Logical]
        );
        assert_eq!(
            table.rows[0],
            [
                CellValue::Number(2.),
                CellValue::Text("001".into()),
                CellValue::Text("[true]".into()),
                CellValue::Empty
            ]
        );
        assert_eq!(
            table.rows[1],
            [
                CellValue::Number(2.5),
                CellValue::Empty,
                CellValue::Empty,
                CellValue::Bool(false)
            ]
        );
        assert_eq!(
            table.rows[2][2],
            CellValue::Text("{\"a\":18446744073709551617}".into())
        );
    }

    #[test]
    fn rejects_invalid_records_and_precision_loss_with_line_numbers() {
        for record in [
            "[]",
            "{bad}",
            "",
            "{\"n\":9007199254740993}",
            "{\"n\":18446744073709551617}",
        ] {
            let data = format!("{{\"n\":1}}\n{record}\n");
            assert!(
                read_records(data.as_bytes())
                    .unwrap_err()
                    .starts_with("Line 2")
            );
        }
        assert!(
            read_records(vec![b' '; MAX_LINE_BYTES + 1].as_slice())
                .unwrap_err()
                .contains("byte limit")
        );
        assert!(read_records(&b"{}"[..]).is_err());
    }
}

#[cfg(test)]
#[test]
fn chooses_json_array_and_preserves_record_types() {
    let bytes = br#"{"packages":[{"code":"001","n":2},{"n":2.5,"extra":true}],"items":[1]}"#;
    let table = records_from_array(bytes, Some("packages")).unwrap();
    assert_eq!(table.names(), ["code", "n", "extra"]);
    assert_eq!(table.rows[0][0], CellValue::Text("001".into()));
    assert_eq!(table.rows[1][0], CellValue::Empty);
    assert_eq!(table.columns[1].ty, QType::Decimal);
    assert!(records_from_array(bytes, Some("items")).is_err());
    assert!(records_from_array(br#"[{"n":1}]"#, None).is_ok());
}
