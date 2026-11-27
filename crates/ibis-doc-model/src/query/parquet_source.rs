// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/263)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{QColumn, QTable, QType};
use crate::CellValue;
use parquet::basic::{ConvertedType, LogicalType, Repetition, Type as PhysicalType};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use parquet::schema::types::Type;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

fn number(value: f64, ty: QType) -> Result<(CellValue, QType), String> {
    if !value.is_finite() {
        return Err("Non-finite Parquet number".into());
    }
    Ok((CellValue::Number(value), ty))
}

fn integer(value: i128) -> Result<(CellValue, QType), String> {
    if value.unsigned_abs() >= 1 << 53 {
        return Err("Parquet integer exceeds spreadsheet precision".into());
    }
    number(value as f64, QType::Whole)
}

fn cell(field: Field) -> Result<(CellValue, QType), String> {
    match field {
        Field::Null => Ok((CellValue::Empty, QType::Any)),
        Field::Bool(v) => Ok((CellValue::Bool(v), QType::Logical)),
        Field::Str(v) => Ok((CellValue::Text(v), QType::Text)),
        Field::Byte(v) => integer(v.into()),
        Field::Short(v) => integer(v.into()),
        Field::Int(v) => integer(v.into()),
        Field::Long(v) => integer(v.into()),
        Field::UByte(v) => integer(v.into()),
        Field::UShort(v) => integer(v.into()),
        Field::UInt(v) => integer(v.into()),
        Field::ULong(v) => integer(v.into()),
        Field::Float16(v) => number(v.to_f64(), QType::Decimal),
        Field::Float(v) => number(v.into(), QType::Decimal),
        Field::Double(v) => number(v, QType::Decimal),
        Field::Date(v) => number(f64::from(v) + 25_569., QType::Date),
        Field::TimeMillis(v) => number(f64::from(v) / 86_400_000., QType::Time),
        Field::TimeMicros(v) => number(v as f64 / 86_400_000_000., QType::Time),
        Field::TimestampMillis(v) => number(v as f64 / 86_400_000. + 25_569., QType::DateTime),
        Field::TimestampMicros(v) => number(v as f64 / 86_400_000_000. + 25_569., QType::DateTime),
        _ => Err("Unsupported Parquet value type".into()),
    }
}

/// A top-level field and its spreadsheet type, if the reader can import it.
pub struct ParquetColumn {
    pub name: String,
    pub ty: Option<QType>,
}

fn column_type(field: &Type) -> Option<QType> {
    let info = field.get_basic_info();
    if !field.is_primitive() || info.repetition() == Repetition::REPEATED {
        return None;
    }
    use ConvertedType as C;
    use PhysicalType as P;
    Some(match (field.get_physical_type(), info.converted_type()) {
        (P::BOOLEAN, C::NONE) => QType::Logical,
        (P::FLOAT | P::DOUBLE, C::NONE) => QType::Decimal,
        (P::FIXED_LEN_BYTE_ARRAY, C::NONE)
            if info.logical_type_ref() == Some(&LogicalType::Float16) =>
        {
            QType::Decimal
        }
        (P::BYTE_ARRAY, C::UTF8 | C::ENUM | C::JSON) => QType::Text,
        (P::INT32, C::DATE) => QType::Date,
        (P::INT32, C::TIME_MILLIS) | (P::INT64, C::TIME_MICROS) => QType::Time,
        (P::INT96, C::NONE) | (P::INT64, C::TIMESTAMP_MILLIS | C::TIMESTAMP_MICROS) => {
            QType::DateTime
        }
        (P::INT32 | P::INT64, C::NONE) if info.logical_type_ref().is_none() => QType::Whole,
        (P::INT32, C::INT_8 | C::INT_16 | C::INT_32 | C::UINT_8 | C::UINT_16 | C::UINT_32)
        | (P::INT64, C::INT_64 | C::UINT_64) => QType::Whole,
        _ => return None,
    })
}

fn open(path: &Path) -> Result<SerializedFileReader<File>, String> {
    if !path.is_file() {
        return Err("Select a local Parquet file".into());
    }
    let file = File::open(path).map_err(|e| e.to_string())?;
    SerializedFileReader::new(file).map_err(|e| e.to_string())
}

/// Read only the schema, without decoding column data.
pub fn parquet_columns(path: &Path) -> Result<Vec<ParquetColumn>, String> {
    let reader = open(path)?;
    Ok(reader
        .metadata()
        .file_metadata()
        .schema()
        .get_fields()
        .iter()
        .map(|field| ParquetColumn {
            name: field.name().into(),
            ty: column_type(field),
        })
        .collect())
}

/// List the first file's fields, allowing only names with a common type across the selection.
pub fn parquet_files_columns(paths: &[PathBuf]) -> Result<Vec<ParquetColumn>, String> {
    let mut paths = paths.iter();
    let first = paths.next().ok_or("Select at least one Parquet file")?;
    let mut columns = parquet_columns(first)?;
    for path in paths {
        let fields =
            parquet_columns(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut by_name = std::collections::HashMap::new();
        for field in fields {
            by_name
                .entry(field.name)
                .and_modify(|ty| *ty = None)
                .or_insert(field.ty);
        }
        for column in &mut columns {
            if by_name.get(&column.name).copied().flatten() != column.ty {
                column.ty = None;
            }
        }
    }
    Ok(columns)
}

/// Read a set of files into a single table with a common projected schema.
pub fn read_parquet_files(paths: &[PathBuf], selected: &[String]) -> Result<QTable, String> {
    if paths.is_empty() {
        return Err("Select at least one Parquet file".into());
    }
    let mut table = QTable::default();
    let mut seen = std::collections::HashSet::new();
    for (index, path) in paths.iter().enumerate() {
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !seen.insert(canonical) {
            return Err(format!("Duplicate Parquet file: {}", path.display()));
        }
        let expected = (index != 0).then_some(table.columns.as_slice());
        let mut part = read_columns(path, selected, table.rows.len(), expected)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if index == 0 {
            table.columns = part.columns;
        }
        table.rows.append(&mut part.rows);
    }
    Ok(table)
}

/// Locate Parquet files below a selected folder without following symlinks.
pub fn parquet_folder_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut visited = 0;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            visited += 1;
            if visited > 16_384 {
                return Err("Parquet folder contains too many entries".into());
            }
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with(['.', '_']) {
                continue;
            }
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("parquet"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    if files.is_empty() {
        return Err("No Parquet files found in the selected folder".into());
    }
    Ok(files)
}

fn hive_partition(value: &str) -> Result<String, String> {
    let mut chars = value.bytes();
    let mut result = Vec::with_capacity(value.len());
    while let Some(ch) = chars.next() {
        if ch == b'%' {
            let digit = |value: Option<u8>| value.and_then(|value| char::from(value).to_digit(16));
            let hi = digit(chars.next()).ok_or("Invalid escape in Parquet partition name")?;
            let lo = digit(chars.next()).ok_or("Invalid escape in Parquet partition name")?;
            result.push(((hi << 4) | lo) as u8);
        } else {
            result.push(ch);
        }
    }
    String::from_utf8(result).map_err(|error| error.to_string())
}

fn hive_values(
    root: &Path,
    file: &Path,
) -> Result<std::collections::BTreeMap<String, CellValue>, String> {
    let mut values = std::collections::BTreeMap::new();
    let relative = file.strip_prefix(root).map_err(|error| error.to_string())?;
    for component in relative.parent().unwrap_or(Path::new("")).components() {
        let component = component
            .as_os_str()
            .to_str()
            .ok_or("Invalid UTF-8 in Parquet folder name")?;
        let Some((key, value)) = component.split_once('=') else {
            continue;
        };
        let key = hive_partition(key)?;
        let value = hive_partition(value)?;
        if key.is_empty() || values.contains_key(&key) {
            return Err(format!("Invalid or duplicate Parquet partition key: {key}"));
        }
        values.insert(
            key,
            if value == "__HIVE_DEFAULT_PARTITION__" {
                CellValue::Empty
            } else {
                CellValue::Text(value)
            },
        );
    }
    Ok(values)
}

/// Read common data columns and append the Hive partition columns as text.
pub fn read_parquet_folder(root: &Path, selected: &[String]) -> Result<QTable, String> {
    let files = parquet_folder_files(root)?;
    let partitions = files
        .iter()
        .map(|file| hive_values(root, file))
        .collect::<Result<Vec<_>, _>>()?;
    let keys: std::collections::BTreeSet<_> = partitions
        .iter()
        .flat_map(|values| values.keys().cloned())
        .collect();
    let mut result = QTable::default();
    let mut data_width = 0;
    for (index, (file, mut values)) in files.iter().zip(partitions).enumerate() {
        let expected = (index != 0).then_some(&result.columns[..data_width]);
        let mut part = read_columns(file, selected, result.rows.len(), expected)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        if index == 0 {
            data_width = part.columns.len();
            if part
                .columns
                .iter()
                .any(|column| keys.contains(&column.name))
            {
                return Err("A Parquet partition column also occurs in the file schema".into());
            }
            result.columns = part.columns;
            result
                .columns
                .extend(keys.iter().cloned().map(|name| QColumn {
                    name,
                    ty: QType::Text,
                }));
        }
        let width = result.columns.len();
        if width > 16_384 || width.saturating_mul(result.rows.len() + part.rows.len()) > 5_000_000 {
            return Err("Parquet folder exceeds the query editor's size limit".into());
        }
        let partition: Vec<_> = keys
            .iter()
            .map(|key| values.remove(key).unwrap_or(CellValue::Empty))
            .collect();
        for row in &mut part.rows {
            row.extend(partition.iter().cloned());
        }
        result.rows.append(&mut part.rows);
    }
    Ok(result)
}

pub fn read_parquet(path: &Path) -> Result<QTable, String> {
    read_parquet_columns(path, &[])
}

/// Materialize selected top-level fields. An empty selection means the legacy whole-table source.
pub fn read_parquet_columns(path: &Path, selected: &[String]) -> Result<QTable, String> {
    read_columns(path, selected, 0, None)
}

fn read_columns(
    path: &Path,
    selected: &[String],
    rows_loaded: usize,
    expected: Option<&[QColumn]>,
) -> Result<QTable, String> {
    let reader = open(path)?;
    let meta = reader.metadata().file_metadata();
    let schema = meta.schema();
    let fields = schema.get_fields();
    let projected = if selected.is_empty() {
        fields.to_vec()
    } else {
        let mut by_name = std::collections::HashMap::new();
        for field in fields {
            by_name
                .entry(field.name())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(field));
        }
        selected
            .iter()
            .map(|name| match by_name.remove(name.as_str()) {
                Some(Some(field)) => Ok(field.clone()),
                Some(None) => Err(format!("Ambiguous Parquet column: {name}")),
                None => Err(format!("Missing or duplicate Parquet column: {name}")),
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let columns = projected
        .iter()
        .map(|field| {
            let name = field.name().to_string();
            let ty =
                column_type(field).ok_or_else(|| format!("Unsupported Parquet column: {name}"))?;
            Ok(QColumn { name, ty })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if expected.is_some_and(|expected| expected != columns) {
        return Err("Selected columns differ from the first Parquet file".into());
    }
    let width = columns.len();
    let rows = (rows_loaded as u64).saturating_add(meta.num_rows() as u64);
    if width > 16_384 || rows >= 1_048_576 || (width as u64).saturating_mul(rows) > 5_000_000 {
        return Err("Parquet source exceeds the query editor's size limit".into());
    }
    let mut table = QTable {
        columns,
        rows: Vec::new(),
    };
    let projection = Type::group_type_builder(schema.name())
        .with_fields(projected)
        .build()
        .map_err(|e| e.to_string())?;
    for row in reader
        .get_row_iter(Some(projection))
        .map_err(|e| e.to_string())?
    {
        let mut values = Vec::with_capacity(width);
        for (name, field) in row.map_err(|e| e.to_string())?.into_columns() {
            let (value, _) = cell(field).map_err(|e| format!("{name}: {e}"))?;
            values.push(value);
        }
        table.rows.push(values);
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_snappy_table() {
        assert_reads_compressed_table(parquet::basic::Compression::SNAPPY, "snappy", false);
    }

    #[test]
    fn reads_a_zstandard_table() {
        assert_reads_compressed_table(
            parquet::basic::Compression::ZSTD(Default::default()),
            "zstd",
            false,
        );
    }

    #[test]
    fn projects_scalar_columns_around_nested_data() {
        assert_reads_compressed_table(parquet::basic::Compression::SNAPPY, "nested", true);
    }

    fn write_table(
        compression: parquet::basic::Compression,
        fixture: &str,
        nested: bool,
    ) -> PathBuf {
        use parquet::data_type::{ByteArray, ByteArrayType, Int32Type, Int64Type};
        use parquet::file::{properties::WriterProperties, writer::SerializedFileWriter};
        use parquet::schema::parser::parse_message_type;
        use std::sync::Arc;

        let file = std::env::temp_dir().join(format!(
            "ibis-parquet-{fixture}-{}.parquet",
            std::process::id()
        ));
        let output = File::options()
            .write(true)
            .create_new(true)
            .open(&file)
            .unwrap();
        let extra = if nested {
            " OPTIONAL GROUP extra { REQUIRED INT32 item; }"
        } else {
            ""
        };
        let schema = Arc::new(
            parse_message_type(&format!(
                "message example {{ REQUIRED BINARY code (UTF8); OPTIONAL INT64 amount;{extra} }}"
            ))
            .unwrap(),
        );
        let properties = WriterProperties::builder()
            .set_compression(compression)
            .build();
        let mut writer = SerializedFileWriter::new(output, schema, Arc::new(properties)).unwrap();
        let mut group = writer.next_row_group().unwrap();
        let mut code = group.next_column().unwrap().unwrap();
        code.typed::<ByteArrayType>()
            .write_batch(
                &[ByteArray::from("01202"), ByteArray::from("00001")],
                None,
                None,
            )
            .unwrap();
        code.close().unwrap();
        let mut amount = group.next_column().unwrap().unwrap();
        amount
            .typed::<Int64Type>()
            .write_batch(&[7], Some(&[1, 0]), None)
            .unwrap();
        amount.close().unwrap();
        if nested {
            let mut extra = group.next_column().unwrap().unwrap();
            extra
                .typed::<Int32Type>()
                .write_batch(&[8], Some(&[1, 0]), None)
                .unwrap();
            extra.close().unwrap();
        }
        group.close().unwrap();
        writer.close().unwrap();

        file
    }

    #[test]
    fn imports_a_partitioned_folder_in_relative_path_order() {
        let directory = tempfile::tempdir().unwrap();
        let fixture = write_table(parquet::basic::Compression::SNAPPY, "folder-test", false);
        for month in ["02", "01"] {
            let path = directory
                .path()
                .join("year=2026")
                .join(format!("month={month}"));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::copy(&fixture, path.join("part.parquet")).unwrap();
        }
        std::fs::remove_file(fixture).unwrap();
        let loaded = read_parquet_folder(directory.path(), &["code".into()]).unwrap();
        assert_eq!(loaded.names(), ["code", "month", "year"]);
        assert_eq!(loaded.rows.len(), 4);
        assert_eq!(loaded.rows[0][1], CellValue::Text("01".into()));
        assert_eq!(loaded.rows[2][1], CellValue::Text("02".into()));
    }

    #[test]
    fn appends_files_and_validates_projected_schema_and_limits() {
        let first = write_table(parquet::basic::Compression::SNAPPY, "part-01", false);
        let second = write_table(
            parquet::basic::Compression::ZSTD(Default::default()),
            "part-02",
            true,
        );
        let paths = [second.clone(), first.clone()];
        let schema = parquet_files_columns(&paths).unwrap();
        assert_eq!(
            schema.iter().map(|c| c.ty).collect::<Vec<_>>(),
            [Some(QType::Text), Some(QType::Whole), None]
        );
        let columns = vec!["amount".into(), "code".into()];
        let combined = read_parquet_files(&paths, &columns).unwrap();
        assert_eq!(combined.names(), ["amount", "code"]);
        assert_eq!(combined.rows.len(), 4);
        assert_eq!(
            combined.rows[0],
            [CellValue::Number(7.), CellValue::Text("01202".into())]
        );
        assert_eq!(combined.rows[..2], combined.rows[2..]);
        assert!(
            read_parquet_files(&[first.clone(), first.clone()], &columns)
                .unwrap_err()
                .contains("Duplicate")
        );
        assert!(
            read_columns(&first, &columns, 1_048_574, None)
                .unwrap_err()
                .contains("size limit")
        );
        let mut incompatible = combined.columns;
        incompatible[0].ty = QType::Text;
        assert!(
            read_columns(&second, &columns, 0, Some(&incompatible))
                .unwrap_err()
                .contains("differ")
        );
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
    }

    fn assert_reads_compressed_table(
        compression: parquet::basic::Compression,
        fixture: &str,
        nested: bool,
    ) {
        let file = write_table(compression, fixture, nested);
        let result = if nested {
            let schema = parquet_columns(&file).unwrap();
            assert_eq!(schema[0].ty, Some(QType::Text));
            assert_eq!(schema[1].ty, Some(QType::Whole));
            assert_eq!(schema[2].name, "extra");
            assert_eq!(schema[2].ty, None);
            assert!(read_parquet_columns(&file, &["extra".into()]).is_err());
            assert!(read_parquet_columns(&file, &["missing".into()]).is_err());
            let reordered = read_parquet_columns(&file, &["amount".into(), "code".into()]).unwrap();
            assert_eq!(reordered.names(), ["amount", "code"]);
            assert_eq!(
                reordered.rows[0],
                [CellValue::Number(7.), CellValue::Text("01202".into())]
            );
            read_parquet_columns(&file, &["code".into(), "amount".into()])
        } else {
            read_parquet(&file)
        };
        std::fs::remove_file(file).unwrap();
        let table = result.unwrap();
        assert_eq!(table.names(), ["code", "amount"]);
        assert_eq!(
            table.rows,
            [
                vec![CellValue::Text("01202".into()), CellValue::Number(7.)],
                vec![CellValue::Text("00001".into()), CellValue::Empty],
            ]
        );
    }

    #[test]
    fn preserves_codes_and_missing_values() {
        assert_eq!(
            cell(Field::Str("01202".into())).unwrap(),
            (CellValue::Text("01202".into()), QType::Text)
        );
        assert_eq!(cell(Field::Null).unwrap(), (CellValue::Empty, QType::Any));
        assert_eq!(
            cell(Field::Long(1202)).unwrap(),
            (CellValue::Number(1202.), QType::Whole)
        );
        assert_eq!(
            cell(Field::Date(0)).unwrap(),
            (CellValue::Number(25_569.), QType::Date)
        );
        assert!(cell(Field::Long(i64::MAX)).is_err());
    }
}
