// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #263: a small sample of each external data format (made here, no outside data) is written to a file,
//! read by the same function the file picker calls, and worked through a query step into the table that is loaded to the sheet.

use super::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn text(s: &str) -> CellValue {
    CellValue::Text(s.into())
}

/// The sample every format holds: `code` (text, with a leading zero), `amount` (whole). Rows: 001/10, 002/20, 003/30.
fn assert_sample(table: QTable) {
    // JSON Lines keeps its fields in name order, so the columns are looked up by name and put in the order of the sample.
    let mut names = table.names();
    names.sort();
    assert_eq!(names, ["amount", "code"]);
    let at = |n: &str| table.names().iter().position(|x| x == n).unwrap();
    let (code, amount) = (at("code"), at("amount"));
    let table = QTable {
        columns: vec![table.columns[code].clone(), table.columns[amount].clone()],
        rows: table.rows.iter().map(|r| vec![r[code].clone(), r[amount].clone()]).collect(),
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0], [text("001"), CellValue::Number(10.)]);
    // A step on the table: keep the rows over 10, as the query editor does before the table is loaded to the sheet.
    let query = Query { name: "Q".into(), source: Source::Blank, steps: vec![Step::FilterRows { column: "amount".into(), op: Op::Greater, value: "10".into() }], output: None };
    let (tables, error) = query.run(table, &[], &HashMap::<String, QTable>::new());
    assert!(error.is_none(), "{error:?}");
    let result = tables.last().unwrap();
    assert_eq!(result.rows, [[text("002"), CellValue::Number(20.)], [text("003"), CellValue::Number(30.)]]);
}

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn put(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn json_lines_sample() {
    let d = dir();
    let path = put(d.path(), "sample.jsonl", b"{\"code\":\"001\",\"amount\":10}\n{\"code\":\"002\",\"amount\":20}\n{\"code\":\"003\",\"amount\":30}\n");
    assert_sample(read_json_lines(&path).unwrap());
}

#[test]
fn arrow_ipc_sample() {
    use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
    let batch = RecordBatch::try_from_iter([
        ("code", Arc::new(StringArray::from(vec!["001", "002", "003"])) as ArrayRef),
        ("amount", Arc::new(Int64Array::from(vec![10, 20, 30])) as ArrayRef),
    ])
    .unwrap();
    let mut bytes = Vec::new();
    {
        let mut writer = arrow_ipc::writer::FileWriter::try_new(&mut bytes, &batch.schema()).unwrap();
        writer.write(&batch).unwrap();
        writer.finish().unwrap();
    }
    let d = dir();
    assert_sample(read_arrow_ipc(&put(d.path(), "sample.arrow", &bytes)).unwrap());
}

#[test]
fn avro_sample() {
    use apache_avro::{types::Value, Schema, Writer};
    let schema = Schema::parse_str(r#"{"type":"record","name":"Row","fields":[{"name":"code","type":"string"},{"name":"amount","type":"long"}]}"#).unwrap();
    let mut writer = Writer::new(&schema, Vec::new()).unwrap();
    for (code, amount) in [("001", 10i64), ("002", 20), ("003", 30)] {
        writer.append(Value::Record(vec![("code".into(), Value::String(code.into())), ("amount".into(), Value::Long(amount))])).unwrap();
    }
    let bytes = writer.into_inner().unwrap();
    let d = dir();
    assert_sample(read_avro(&put(d.path(), "sample.avro", &bytes)).unwrap());
}

#[test]
fn parquet_sample() {
    use parquet::data_type::{ByteArray, ByteArrayType, Int64Type};
    use parquet::file::{properties::WriterProperties, writer::SerializedFileWriter};
    use parquet::schema::parser::parse_message_type;
    let d = dir();
    let path = d.path().join("sample.parquet");
    let schema = Arc::new(parse_message_type("message sample { REQUIRED BINARY code (UTF8); REQUIRED INT64 amount; }").unwrap());
    let mut writer = SerializedFileWriter::new(std::fs::File::create(&path).unwrap(), schema, Arc::new(WriterProperties::builder().build())).unwrap();
    let mut group = writer.next_row_group().unwrap();
    let mut code = group.next_column().unwrap().unwrap();
    code.typed::<ByteArrayType>().write_batch(&[ByteArray::from("001"), ByteArray::from("002"), ByteArray::from("003")], None, None).unwrap();
    code.close().unwrap();
    let mut amount = group.next_column().unwrap().unwrap();
    amount.typed::<Int64Type>().write_batch(&[10, 20, 30], None, None).unwrap();
    amount.close().unwrap();
    group.close().unwrap();
    writer.close().unwrap();
    assert_sample(read_parquet(&path).unwrap());
}

#[test]
fn sqlite_sample() {
    // The sample database written by the fork's author with SQLite itself (a table `sales "2026"`).
    let d = dir();
    let path = put(d.path(), "sample.sqlite3", include_bytes!("fixtures/external.sqlite3"));
    assert_eq!(sqlite_tables(&path).unwrap(), ["binary_data", "empty", "sales \"2026\""]);
    let table = read_sqlite_table(&path, "sales \"2026\"").unwrap();
    assert_eq!(table.names(), ["code", "amount", "remark"]);
    assert!(!table.rows.is_empty());
}
