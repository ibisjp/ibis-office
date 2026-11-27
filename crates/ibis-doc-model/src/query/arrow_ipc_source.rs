// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/263)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{QColumn, QTable, QType};
use crate::CellValue;
use arrow_array::*;
use arrow_ipc::reader::{FileReader, StreamReader, read_footer_length};
use arrow_schema::{DataType, Schema, TimeUnit};
use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;
const MAX_ROWS: usize = 1_048_575;
const MAX_COLUMNS: usize = 16_384;
const MAX_CELLS: usize = 5_000_000;

/// Import a local Arrow IPC file or stream into a bounded spreadsheet table.
pub fn read_arrow_ipc(path: &Path) -> Result<QTable, String> {
    if !path.is_file() {
        return Err("Select a local Arrow IPC file".into());
    }
    // Decode the same bytes we validate, even if another process replaces the source file.
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Arrow IPC source exceeds the 64 MiB limit".into());
    }
    read_bytes(&bytes).map_err(|error| {
        format!(
            "{}: {error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn read_bytes(bytes: &[u8]) -> Result<QTable, String> {
    let mut budget = MAX_DECODED_BYTES;
    if bytes.starts_with(b"ARROW1") {
        let (data, trailer) = bytes
            .split_last_chunk::<10>()
            .ok_or("Missing Arrow IPC footer")?;
        let footer_len = read_footer_length(*trailer).map_err(|error| error.to_string())?;
        let start = data
            .len()
            .checked_sub(footer_len)
            .ok_or("Invalid Arrow IPC footer length")?;
        let footer =
            arrow_ipc::root_as_footer(&data[start..]).map_err(|error| error.to_string())?;
        if footer
            .dictionaries()
            .is_some_and(|blocks| !blocks.is_empty())
        {
            return Err("Arrow IPC dictionary encoding is unsupported".into());
        }
        let blocks = footer
            .recordBatches()
            .ok_or("Missing Arrow IPC record batches")?;
        let mut previous_end = 8;
        for block in blocks {
            let offset =
                usize::try_from(block.offset()).map_err(|_| "Invalid Arrow IPC block offset")?;
            let metadata = usize::try_from(block.metaDataLength())
                .map_err(|_| "Invalid Arrow IPC metadata length")?;
            let body =
                usize::try_from(block.bodyLength()).map_err(|_| "Invalid Arrow IPC body length")?;
            let end = offset
                .checked_add(metadata)
                .and_then(|end| end.checked_add(body))
                .filter(|end| *end <= start)
                .ok_or("Arrow IPC block exceeds the file bounds")?;
            if offset < previous_end {
                return Err("Overlapping or unordered Arrow IPC file blocks".into());
            }
            previous_end = end;
            let (message, remaining) = inspect_message(&bytes[offset..end], &mut budget)?;
            if message.header_as_record_batch().is_none() || !remaining.is_empty() {
                return Err("Invalid Arrow IPC file block".into());
            }
            if usize::try_from(message.bodyLength()).ok() != Some(body) {
                return Err("Arrow IPC file block lengths do not match its message".into());
            }
        }
        let reader =
            FileReader::try_new(Cursor::new(bytes), None).map_err(|error| error.to_string())?;
        collect_batches(&reader.schema(), reader)
    } else {
        let (schema, mut rest) = inspect_message(bytes, &mut budget)?;
        if schema.header_as_schema().is_none() {
            return Err("Expected an Arrow IPC file or stream".into());
        }
        while !matches!(rest, [] | [0, 0, 0, 0] | [255, 255, 255, 255, 0, 0, 0, 0]) {
            let (message, remaining) = inspect_message(rest, &mut budget)?;
            if message.header_as_record_batch().is_none() {
                return Err(
                    "Expected an Arrow IPC record batch without dictionary encoding".into(),
                );
            }
            rest = remaining;
        }
        let reader = StreamReader::try_new(bytes, None).map_err(|error| error.to_string())?;
        collect_batches(&reader.schema(), reader)
    }
}

/// Validate envelope and buffer ranges before handing the data to Arrow's decoder.
fn inspect_message<'a>(
    bytes: &'a [u8],
    budget: &mut usize,
) -> Result<(arrow_ipc::Message<'a>, &'a [u8]), String> {
    let prefix = if bytes.starts_with(&[255; 4]) {
        &bytes[4..]
    } else {
        bytes
    };
    let (len, rest) = prefix
        .split_first_chunk::<4>()
        .ok_or("Truncated Arrow IPC message length")?;
    let len = usize::try_from(i32::from_le_bytes(*len))
        .map_err(|_| "Invalid Arrow IPC message length")?;
    let (metadata, rest) = rest
        .split_at_checked(len)
        .ok_or("Truncated Arrow IPC metadata")?;
    let message = arrow_ipc::root_as_message(metadata).map_err(|error| error.to_string())?;
    let body_len =
        usize::try_from(message.bodyLength()).map_err(|_| "Invalid Arrow IPC body length")?;
    let (body, rest) = rest
        .split_at_checked(body_len)
        .ok_or("Truncated Arrow IPC body")?;
    if let Some(batch) = message.header_as_record_batch() {
        if usize::try_from(batch.length()).map_or(true, |rows| rows > MAX_ROWS) {
            return Err("Arrow IPC record batch exceeds the row limit".into());
        }
        if let Some(buffers) = batch.buffers() {
            for buffer in buffers {
                let offset = usize::try_from(buffer.offset())
                    .map_err(|_| "Invalid Arrow IPC buffer offset")?;
                let length = usize::try_from(buffer.length())
                    .map_err(|_| "Invalid Arrow IPC buffer length")?;
                if offset > body.len() || length > body.len() - offset {
                    return Err("Arrow IPC buffer exceeds the message body".into());
                }
                let decoded = if batch.compression().is_some() && length != 0 {
                    let prefix = body[offset..offset + length]
                        .first_chunk::<8>()
                        .ok_or("Missing Arrow IPC compression length")?;
                    match i64::from_le_bytes(*prefix) {
                        -1 => length - 8,
                        size => usize::try_from(size)
                            .map_err(|_| "Invalid Arrow IPC compression length")?,
                    }
                } else {
                    length
                };
                *budget = budget
                    .checked_sub(decoded)
                    .ok_or("Arrow IPC decoded buffers exceed the 128 MiB limit")?;
            }
        }
    }
    Ok((message, rest))
}

fn collect_batches(
    schema: &Schema,
    batches: impl Iterator<Item = Result<RecordBatch, arrow_schema::ArrowError>>,
) -> Result<QTable, String> {
    if schema.fields().is_empty() || schema.fields().len() > MAX_COLUMNS {
        return Err("Arrow IPC source has an unsupported number of columns".into());
    }
    let mut seen = HashSet::new();
    let mut columns = Vec::new();
    for field in schema.fields() {
        if !seen.insert(field.name()) {
            return Err(format!("Duplicate Arrow IPC column: {}", field.name()));
        }
        let ty = match field.data_type() {
            DataType::Null => QType::Any,
            DataType::Boolean => QType::Logical,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => QType::Text,
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => QType::Whole,
            DataType::Float32 | DataType::Float64 => QType::Decimal,
            DataType::Date32 | DataType::Date64 => QType::Date,
            DataType::Timestamp(_, _) => QType::DateTime,
            other => {
                return Err(format!(
                    "Unsupported Arrow IPC column {}: {other}",
                    field.name()
                ));
            }
        };
        check_text(field.name())?;
        columns.push(QColumn {
            name: field.name().clone(),
            ty,
        });
    }
    let mut table = QTable {
        columns,
        rows: Vec::new(),
    };
    let width = table.columns.len();
    let mut text_budget = MAX_DECODED_BYTES;
    for batch in batches {
        let batch = batch.map_err(|error| error.to_string())?;
        let height = table.rows.len().saturating_add(batch.num_rows());
        if height > MAX_ROWS || width.saturating_mul(height) > MAX_CELLS {
            return Err("Arrow IPC source exceeds the query editor's table size limit".into());
        }
        for row in 0..batch.num_rows() {
            let values = batch
                .columns()
                .iter()
                .zip(&table.columns)
                .map(|(array, column)| {
                    cell(array.as_ref(), row, column.ty, &mut text_budget)
                        .map_err(|error| format!("{}: {error}", column.name))
                })
                .collect::<Result<Vec<_>, _>>()?;
            table.rows.push(values);
        }
    }
    Ok(table)
}

fn check_text(text: &str) -> Result<(), String> {
    if text.encode_utf16().take(32_768).count() > 32_767 {
        return Err("Text exceeds the spreadsheet cell limit".into());
    }
    Ok(())
}

fn cell(
    array: &dyn Array,
    row: usize,
    ty: QType,
    text_budget: &mut usize,
) -> Result<CellValue, String> {
    if array.is_null(row) || matches!(array.data_type(), DataType::Null) {
        return Ok(CellValue::Empty);
    }
    macro_rules! value {
        ($array:ty) => {
            array
                .as_any()
                .downcast_ref::<$array>()
                .ok_or("Arrow IPC value does not match its type")?
                .value(row)
        };
    }
    let number = match array.data_type() {
        DataType::Boolean => return Ok(CellValue::Bool(value!(BooleanArray))),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            let text = match array.data_type() {
                DataType::Utf8 => value!(StringArray),
                DataType::LargeUtf8 => value!(LargeStringArray),
                _ => value!(StringViewArray),
            };
            check_text(text)?;
            *text_budget = text_budget
                .checked_sub(text.len())
                .ok_or("Arrow IPC text exceeds the 128 MiB limit")?;
            return Ok(CellValue::Text(text.into()));
        }
        DataType::Int8 => f64::from(value!(Int8Array)),
        DataType::Int16 => f64::from(value!(Int16Array)),
        DataType::Int32 => f64::from(value!(Int32Array)),
        DataType::Int64 => value!(Int64Array) as f64,
        DataType::UInt8 => f64::from(value!(UInt8Array)),
        DataType::UInt16 => f64::from(value!(UInt16Array)),
        DataType::UInt32 => f64::from(value!(UInt32Array)),
        DataType::UInt64 => value!(UInt64Array) as f64,
        DataType::Float32 => f64::from(value!(Float32Array)),
        DataType::Float64 => value!(Float64Array),
        DataType::Date32 => f64::from(value!(Date32Array)) + 25_569.,
        DataType::Date64 => value!(Date64Array) as f64 / 86_400_000. + 25_569.,
        DataType::Timestamp(unit, _) => {
            let seconds = match unit {
                TimeUnit::Second => value!(TimestampSecondArray) as f64,
                TimeUnit::Millisecond => value!(TimestampMillisecondArray) as f64 / 1_000.,
                TimeUnit::Microsecond => value!(TimestampMicrosecondArray) as f64 / 1_000_000.,
                TimeUnit::Nanosecond => value!(TimestampNanosecondArray) as f64 / 1_000_000_000.,
            };
            seconds / 86_400. + 25_569.
        }
        _ => return Err("Unsupported Arrow IPC value".into()),
    };
    if !number.is_finite() || ty == QType::Whole && number.abs() >= (1u64 << 53) as f64 {
        return Err("Number exceeds spreadsheet precision".into());
    }
    Ok(CellValue::Number(number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_ipc::{
        CompressionType,
        writer::{FileWriter, IpcWriteOptions, StreamWriter},
    };
    use std::sync::Arc;

    #[test]
    fn imports_file_and_stream_with_multiple_batches_and_compression() {
        let batch = RecordBatch::try_from_iter([
            (
                "code",
                Arc::new(StringArray::from(vec![Some("001"), None])) as ArrayRef,
            ),
            ("day", Arc::new(Date32Array::from(vec![0, 1])) as ArrayRef),
            (
                "amount",
                Arc::new(Int32Array::from(vec![37, 49])) as ArrayRef,
            ),
        ])
        .unwrap();
        for compression in [
            None,
            Some(CompressionType::LZ4_FRAME),
            Some(CompressionType::ZSTD),
        ] {
            let options = IpcWriteOptions::default()
                .try_with_compression(compression)
                .unwrap();
            let mut file = Vec::new();
            {
                let mut writer =
                    FileWriter::try_new_with_options(&mut file, &batch.schema(), options.clone())
                        .unwrap();
                writer.write(&batch).unwrap();
                writer.write(&batch).unwrap();
                writer.finish().unwrap();
            }
            let mut stream = Vec::new();
            {
                let mut writer =
                    StreamWriter::try_new_with_options(&mut stream, &batch.schema(), options)
                        .unwrap();
                writer.write(&batch).unwrap();
                writer.write(&batch).unwrap();
                writer.finish().unwrap();
            }
            let table = read_bytes(&file).unwrap();
            assert_eq!(table, read_bytes(&stream).unwrap());
            assert_eq!(table.names(), ["code", "day", "amount"]);
            assert_eq!(table.columns[1].ty, QType::Date);
            assert_eq!(table.rows.len(), 4);
            assert_eq!(
                table.rows[0],
                [
                    CellValue::Text("001".into()),
                    CellValue::Number(25_569.),
                    CellValue::Number(37.)
                ]
            );
            assert_eq!(table.rows[3][0], CellValue::Empty);
            assert!(read_bytes(&file[..file.len() - 1]).is_err());
            assert!(read_bytes(&stream[..stream.len() - 1]).is_err());
        }
    }

    #[test]
    fn refuses_lossy_integers() {
        let array = UInt64Array::from(vec![9_007_199_254_740_993]);
        let mut text_budget = MAX_DECODED_BYTES;
        assert!(cell(&array, 0, QType::Whole, &mut text_budget).is_err());
    }
}
