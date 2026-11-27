// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/263)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{QColumn, QTable, QType};
use crate::CellValue;
use apache_avro::{Reader, Schema, types::Value};
use std::{
    cell::Cell,
    fs::File,
    io::{self, Read},
    path::Path,
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 128 * 1024 * 1024;
const MAX_ROWS: usize = 1_048_575;
const MAX_COLUMNS: usize = 16_384;
const MAX_CELLS: usize = 5_000_000;

/// Import a local Avro object container file with a top-level record schema.
pub fn read_avro(path: &Path) -> Result<QTable, String> {
    if !path.is_file() {
        return Err("Select a local Avro file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Avro source exceeds the 64 MiB limit".into());
    }
    read_records(&bytes).map_err(|error| {
        format!(
            "{}: {error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

// Avro's reader treats EOF within a block-count varint as ordinary EOF. Track
// bytes consumed so an incomplete block header cannot yield partial success.
struct CountedInput<'a> {
    remaining: &'a [u8],
    position: &'a Cell<usize>,
}

impl Read for CountedInput<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.remaining.read(buffer)?;
        self.position.set(self.position.get() + count);
        Ok(count)
    }
}

fn read_records(bytes: &[u8]) -> Result<QTable, String> {
    // This model is the only consumer of apache-avro in the workspace. Bound
    // the library's individual allocations, including expanded codec blocks.
    apache_avro::util::max_allocation_bytes(MAX_BYTES);
    let position = Cell::new(0);
    let input = CountedInput {
        remaining: bytes,
        position: &position,
    };
    let mut reader = Reader::new(input).map_err(|error| error.to_string())?;
    let Schema::Record(schema) = reader.writer_schema() else {
        return Err("Avro source requires a top-level record schema".into());
    };
    let width = schema.fields.len();
    if width == 0 || width > MAX_COLUMNS {
        return Err("Avro source has an unsupported number of columns".into());
    }
    let mut table = QTable::default();
    for field in &schema.fields {
        check_text(&field.name)?;
        let ty = column_type(&field.schema)
            .ok_or_else(|| format!("Unsupported Avro type in column {}", field.name))?;
        table.columns.push(QColumn {
            name: field.name.clone(),
            ty,
        });
    }
    let mut text_budget = MAX_TEXT_BYTES;
    loop {
        let before = position.get();
        let Some(record) = reader.next() else {
            if position.get() != before || before != bytes.len() {
                return Err("Incomplete Avro block header".into());
            }
            break;
        };
        let row = table.rows.len() + 1;
        if row > MAX_ROWS || width.saturating_mul(row) > MAX_CELLS {
            return Err("Avro source exceeds the query editor's table size limit".into());
        }
        let Value::Record(fields) = record.map_err(|error| format!("Row {row}: {error}"))? else {
            return Err(format!("Row {row}: expected an Avro record"));
        };
        if fields.len() != width {
            return Err(format!("Row {row}: Avro record does not match its schema"));
        }
        let values = fields
            .into_iter()
            .zip(&table.columns)
            .map(|((name, value), column)| {
                if name != column.name {
                    return Err(format!(
                        "Row {row}: Avro field order does not match its schema"
                    ));
                }
                cell(value, &mut text_budget).map_err(|error| format!("Row {row}, {name}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        table.rows.push(values);
    }
    Ok(table)
}

fn column_type(schema: &Schema) -> Option<QType> {
    Some(match schema {
        Schema::Null => QType::Any,
        Schema::Boolean => QType::Logical,
        Schema::String | Schema::Enum(_) | Schema::Uuid(_) => QType::Text,
        Schema::Int | Schema::Long => QType::Whole,
        Schema::Float | Schema::Double => QType::Decimal,
        Schema::Date => QType::Date,
        Schema::TimeMillis | Schema::TimeMicros => QType::Time,
        Schema::TimestampMillis
        | Schema::TimestampMicros
        | Schema::TimestampNanos
        | Schema::LocalTimestampMillis
        | Schema::LocalTimestampMicros
        | Schema::LocalTimestampNanos => QType::DateTime,
        Schema::Union(union) => {
            let [left, right] = union.variants() else {
                return None;
            };
            return match (left, right) {
                (Schema::Null, value) | (value, Schema::Null) => column_type(value),
                _ => None,
            };
        }
        _ => return None,
    })
}

fn check_text(text: &str) -> Result<(), &'static str> {
    if text.encode_utf16().take(32_768).count() > 32_767 {
        return Err("Text exceeds the spreadsheet cell limit");
    }
    Ok(())
}

fn cell(value: Value, text_budget: &mut usize) -> Result<CellValue, &'static str> {
    let number = match value {
        Value::Null => return Ok(CellValue::Empty),
        Value::Boolean(value) => return Ok(CellValue::Bool(value)),
        Value::String(text) | Value::Enum(_, text) => return text_cell(text, text_budget),
        Value::Uuid(value) => return text_cell(value.to_string(), text_budget),
        Value::Union(_, value) => return cell(*value, text_budget),
        Value::Int(value) => f64::from(value),
        Value::Long(value) => {
            if value.unsigned_abs() >= 1 << 53 {
                return Err("Integer exceeds spreadsheet precision");
            }
            value as f64
        }
        Value::Float(value) => f64::from(value),
        Value::Double(value) => value,
        Value::Date(value) => f64::from(value) + 25_569.,
        Value::TimeMillis(value) => f64::from(value) / 86_400_000.,
        Value::TimeMicros(value) => value as f64 / 86_400_000_000.,
        Value::TimestampMillis(value) | Value::LocalTimestampMillis(value) => {
            value as f64 / 86_400_000. + 25_569.
        }
        Value::TimestampMicros(value) | Value::LocalTimestampMicros(value) => {
            value as f64 / 86_400_000_000. + 25_569.
        }
        Value::TimestampNanos(value) | Value::LocalTimestampNanos(value) => {
            value as f64 / 86_400_000_000_000. + 25_569.
        }
        _ => return Err("Unsupported Avro value"),
    };
    if !number.is_finite() {
        return Err("Non-finite Avro number");
    }
    Ok(CellValue::Number(number))
}

fn text_cell(text: String, budget: &mut usize) -> Result<CellValue, &'static str> {
    check_text(&text)?;
    *budget = budget
        .checked_sub(text.len())
        .ok_or("Avro text exceeds the 128 MiB limit")?;
    Ok(CellValue::Text(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apache_avro::{Codec, Writer};

    #[test]
    fn imports_records_with_nulls_and_logical_types_across_codecs() {
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"Reading","fields":[
            {"name":"code","type":["null","string"]},
            {"name":"count","type":"long"},
            {"name":"day","type":{"type":"int","logicalType":"date"}}
        ]}"#,
        )
        .unwrap();
        for codec in [
            Codec::Null,
            Codec::Deflate(Default::default()),
            Codec::Snappy,
        ] {
            let mut writer = Writer::builder()
                .schema(&schema)
                .writer(Vec::new())
                .codec(codec)
                .build()
                .unwrap();
            for code in [
                Value::Union(1, Box::new(Value::String("001".into()))),
                Value::Union(0, Box::new(Value::Null)),
            ] {
                writer
                    .append(Value::Record(vec![
                        ("code".into(), code),
                        ("count".into(), Value::Long(37)),
                        ("day".into(), Value::Date(0)),
                    ]))
                    .unwrap();
                writer.flush().unwrap();
            }
            let mut bytes = writer.into_inner().unwrap();
            let table = read_records(&bytes).unwrap();
            assert_eq!(table.names(), ["code", "count", "day"]);
            assert_eq!(table.columns[2].ty, QType::Date);
            assert_eq!(
                table.rows[0],
                [
                    CellValue::Text("001".into()),
                    CellValue::Number(37.),
                    CellValue::Number(25_569.)
                ]
            );
            assert_eq!(table.rows[1][0], CellValue::Empty);
            assert!(read_records(&bytes[..bytes.len() - 1]).is_err());
            bytes.push(0x80);
            assert!(
                read_records(&bytes)
                    .unwrap_err()
                    .contains("Incomplete Avro block header")
            );
        }
    }

    #[test]
    fn rejects_nested_columns_and_lossy_integers() {
        let schema = Schema::parse_str(r#"{"type":"array","items":"string"}"#).unwrap();
        assert!(column_type(&schema).is_none());
        let mut budget = MAX_TEXT_BYTES;
        assert!(cell(Value::Long(9_007_199_254_740_993), &mut budget).is_err());
    }
}
