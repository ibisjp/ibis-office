// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A query over a table of the sheet, as the クエリ エディター has it: the
//! source table and a list of steps (columns chosen, rows filtered and
//! sorted, columns split, merged, pivoted, grouped, tables merged and
//! appended, columns added), each worked out on the table the step before
//! it gave. The result is written to a sheet as cells; the steps are kept
//! (and shown as the M code of Power Query) so that they can be edited.
//!
//! This is its own engine: it does not run M. The names of the steps and the
//! M text follow Power Query's, so that the steps read as Excel's do.
use std::collections::HashMap;

use ibis_calc::input::{parse_date_in_order, DateOrder};
use ibis_calc::{CellError, CellValue};

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod arrow_ipc_source;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use arrow_ipc_source::read_arrow_ipc;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod avro_source;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use avro_source::read_avro;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod sqlite_source;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use sqlite_source::{read_sqlite_table, sqlite_tables};
mod book;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod parquet_source;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod json_lines_source;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use json_lines_source::{json_array_choices, read_json_array, read_json_lines};
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use parquet_source::{parquet_columns, parquet_files_columns, parquet_folder_files, read_parquet_folder, read_parquet, read_parquet_columns, read_parquet_files, ParquetColumn};
mod codec;
mod cond;
mod expr;
mod m_parse;
mod m_value;
mod text_m;

pub use book::{read_source, result_from_source, result_of, set_workbook_loader, BookResolver};
pub use codec::{decode_queries, encode_queries};
pub use cond::{Cond, DatePart, DateWhen, DateUnit, DateFilter, Missing, Transform};
pub use expr::Expr;
pub use m_parse::{CodeError, m_to_query};
pub use m_value::{MEvalError, eval_m, parse_run_error, qtable_to_value, value_to_qtable};
pub use text_m::{english_step_name, query_to_m, query_to_m_named};

/// The type of a column (`Table.TransformColumnTypes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum QType {
    /// Any (`type any`): no conversion.
    #[default]
    Any,
    Text,
    /// A whole number (`Int64.Type`).
    Whole,
    /// A decimal number (`type number`).
    Decimal,
    Date,
    DateTime,
    Time,
    Logical,
    /// A fixed decimal number of four decimals (`Currency.Type`).
    Currency,
    /// A number shown as a percentage (`Percentage.Type`).
    Percentage,
    /// A 32-bit whole number (`Int32.Type`).
    Int32,
    /// A date and time with a time zone (`type datetimezone`); the zone is not kept (the values are serial numbers).
    DateTimeZone,
    /// A length of time (`type duration`), in days as a number.
    Duration,
}

impl QType {
    pub const ALL: [QType; 13] = [
        Self::Any,
        Self::Text,
        Self::Whole,
        Self::Decimal,
        Self::Date,
        Self::DateTime,
        Self::Time,
        Self::Logical,
        Self::Currency,
        Self::Percentage,
        Self::Int32,
        Self::DateTimeZone,
        Self::Duration,
    ];

    /// The type an M type expression names: the names `m_name` writes, the `X.Type` spellings of the same types, and `type nullable X`
    /// (nullable is not kept: every column may hold null here).
    pub fn from_m(text: &str) -> Option<QType> {
        let words: Vec<&str> = text.split_whitespace().filter(|w| *w != "nullable").collect();
        let name = match &words[..] {
            ["type", name] => *name,
            [name] => *name,
            _ => return None,
        };
        Some(match name {
            "any" | "Any.Type" => Self::Any,
            "text" | "Text.Type" => Self::Text,
            "Int64.Type" => Self::Whole,
            "number" | "Number.Type" | "Double.Type" | "Decimal.Type" | "Single.Type" => Self::Decimal,
            "date" | "Date.Type" => Self::Date,
            "datetime" | "DateTime.Type" => Self::DateTime,
            "time" | "Time.Type" => Self::Time,
            "logical" | "Logical.Type" => Self::Logical,
            "Currency.Type" => Self::Currency,
            "Percentage.Type" => Self::Percentage,
            "Int32.Type" => Self::Int32,
            "datetimezone" | "DateTimeZone.Type" => Self::DateTimeZone,
            "duration" | "Duration.Type" => Self::Duration,
            _ => return None,
        })
    }

    /// The type's M name.
    pub fn m_name(self) -> &'static str {
        match self {
            Self::Any => "type any",
            Self::Text => "type text",
            Self::Whole => "Int64.Type",
            Self::Decimal => "type number",
            Self::Date => "type date",
            Self::DateTime => "type datetime",
            Self::Time => "type time",
            Self::Logical => "type logical",
            Self::Currency => "Currency.Type",
            Self::Percentage => "Percentage.Type",
            Self::Int32 => "Int32.Type",
            Self::DateTimeZone => "type datetimezone",
            Self::Duration => "type duration",
        }
    }

    /// The number format a column of this type shows its numbers in on a sheet.
    pub fn number_format(self) -> Option<&'static str> {
        match self {
            Self::Date => Some("yyyy/m/d"),
            Self::DateTime => Some("yyyy/m/d h:mm:ss"),
            Self::Time => Some("h:mm:ss"),
            Self::DateTimeZone => Some("yyyy/m/d h:mm:ss"),
            Self::Duration => Some("[h]:mm:ss"),
            Self::Percentage => Some("0%"),
            _ => None,
        }
    }
}

/// The culture that reads and shows text as numbers and dates (the locale of 型の変更 using the locale).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Culture {
    /// The invariant reading: `1,234.5`, dates in year/month/day.
    #[default]
    Invariant,
    EnglishUs,
    Japanese,
    German,
    French,
    /// Chinese (simplified, China): numbers and dates written as in Japanese.
    ChineseChina,
    /// English (United Kingdom): `1,234.5`, dates day first.
    EnglishGb,
    /// Thai (Thailand): `1,234.5`, dates day first, the year in the Buddhist calendar (Gregorian + 543).
    Thai,
}

impl Culture {
    pub const ALL: [Culture; 8] = [Self::Invariant, Self::EnglishUs, Self::Japanese, Self::German, Self::French, Self::ChineseChina, Self::EnglishGb, Self::Thai];

    /// The cultures the locale dialog lists (`Table.TransformColumnTypes(..., culture)`): the ones the M interpreter holds the data of,
    /// without the invariant one (the dialog's list has no empty name).
    pub const LOCALES: [Culture; 7] = [Self::Japanese, Self::EnglishUs, Self::ChineseChina, Self::German, Self::French, Self::EnglishGb, Self::Thai];

    /// The culture's name (`en-US`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Invariant => "",
            Self::EnglishUs => "en-US",
            Self::Japanese => "ja-JP",
            Self::German => "de-DE",
            Self::French => "fr-FR",
            Self::ChineseChina => "zh-CN",
            Self::EnglishGb => "en-GB",
            Self::Thai => "th-TH",
        }
    }

    fn decimal_separator(self) -> char {
        match self {
            Self::German | Self::French => ',',
            _ => '.',
        }
    }

    fn group_separators(self) -> &'static [char] {
        match self {
            Self::German => &['.'],
            Self::French => &[' ', '\u{a0}', '\u{202f}'],
            _ => &[','],
        }
    }

    fn date_order(self) -> DateOrder {
        match self {
            Self::EnglishUs => DateOrder::Mdy,
            Self::German | Self::French | Self::EnglishGb | Self::Thai => DateOrder::Dmy,
            _ => DateOrder::Ymd,
        }
    }
}

/// A column: its name and type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QColumn {
    pub name: String,
    pub ty: QType,
}

/// A table the steps work on: columns and rows of values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QTable {
    pub columns: Vec<QColumn>,
    pub rows: Vec<Vec<CellValue>>,
}

impl QTable {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    fn need(&self, name: &str) -> Result<usize, String> {
        self.column_index(name).ok_or_else(|| format!("列 {name} がありません"))
    }

    pub fn names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }

    /// A table of the given names and rows, the types guessed from the values (numbers, text).
    pub fn from_rows(names: Vec<String>, rows: Vec<Vec<CellValue>>) -> Self {
        let columns = names.into_iter().map(|name| QColumn { name, ty: QType::Any }).collect();
        Self { columns, rows }
    }
}

/// A comparison a row filter or a conditional column makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Equals,
    NotEquals,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Contains,
    DoesNotContain,
    BeginsWith,
    EndsWith,
    IsNull,
    IsNotNull,
    DoesNotBeginWith,
    DoesNotEndWith,
}

impl Op {
    pub const ALL: [Op; 14] = [
        Self::Equals,
        Self::NotEquals,
        Self::Greater,
        Self::GreaterOrEqual,
        Self::Less,
        Self::LessOrEqual,
        Self::Contains,
        Self::DoesNotContain,
        Self::BeginsWith,
        Self::EndsWith,
        Self::IsNull,
        Self::IsNotNull,
        Self::DoesNotBeginWith,
        Self::DoesNotEndWith,
    ];

    /// Whether the comparison needs a value to compare with.
    pub fn takes_value(self) -> bool {
        !matches!(self, Self::IsNull | Self::IsNotNull)
    }
}

/// How the values of a group, or of the cells a pivot puts together, are made one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Agg {
    Sum,
    Average,
    Min,
    Max,
    /// The number of rows.
    Count,
    CountDistinct,
    Median,
    /// The first value (Pivot's 集計しない).
    First,
}

impl Agg {
    pub const ALL: [Agg; 8] = [Self::Sum, Self::Average, Self::Min, Self::Max, Self::Count, Self::CountDistinct, Self::Median, Self::First];
}

/// The kind of a merge of two tables (the join).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JoinKind {
    LeftOuter,
    RightOuter,
    FullOuter,
    Inner,
    LeftAnti,
    RightAnti,
}

impl JoinKind {
    pub const ALL: [JoinKind; 6] = [Self::LeftOuter, Self::RightOuter, Self::FullOuter, Self::Inner, Self::LeftAnti, Self::RightAnti];
}

/// How a column is split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SplitBy {
    /// At each occurrence of the text.
    Delimiter(String),
    /// Every n characters.
    Length(usize),
    /// At each occurrence of any of the texts (`Splitter.SplitTextByAnyDelimiter`).
    AnyDelimiter(Vec<String>),
    /// At the first occurrence of the first text, then of the second in what is left, and so on (`Splitter.SplitTextByEachDelimiter`); from
    /// the end, the last occurrences, the last text of the text first.
    EachDelimiter { delimiters: Vec<String>, from_end: bool },
    /// At the positions (offsets from the start, or from the end), each piece to the next position and the last to the end
    /// (`Splitter.SplitTextByPositions`).
    Positions { positions: Vec<usize>, from_end: bool },
    /// Pieces of the lengths one after another, what is left over dropped (`Splitter.SplitTextByLengths`).
    Lengths { lengths: Vec<usize>, from_end: bool },
    /// Two pieces, the first `length` characters (or the last, from the end) and the rest: what Power Query writes for 文字数 with 1 回
    /// (`Splitter.SplitTextByPositions({0, n}, false)`; the flag is written even when it is false).
    Once { length: usize, from_end: bool },
    /// Where a character of one kind is followed by one of another (`Splitter.SplitTextByCharacterTransition`).
    Transition { from: CharClass, to: CharClass },
}

/// A kind of character the split at a transition tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    /// `"a".."z"`.
    Lower,
    /// `"A".."Z"`.
    Upper,
    /// `"0".."9"`.
    Digit,
    /// Anything but `"0".."9"`.
    NonDigit,
}

impl CharClass {
    pub fn contains(self, c: char) -> bool {
        match self {
            Self::Lower => c.is_ascii_lowercase(),
            Self::Upper => c.is_ascii_uppercase(),
            Self::Digit => c.is_ascii_digit(),
            Self::NonDigit => !c.is_ascii_digit(),
        }
    }
}

/// A step. Each is worked out on the table of the step before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// The first row made the column names (`Table.PromoteHeaders`).
    PromoteHeaders,
    /// The column names made the first row (`Table.DemoteHeaders`).
    DemoteHeaders,
    /// A column's type changed. Excel writes the changes that come one after another, of columns the step before does not hold, in one
    /// `Table.TransformColumnTypes`; `apart` is a change written as a step of its own although it could have been added to that one.
    ChangeType { column: String, ty: QType, culture: Culture, apart: bool },
    SelectColumns(Vec<String>),
    RemoveColumns(Vec<String>),
    /// Columns renamed at once, `(old name, new name)` each (`Table.RenameColumns`): every old name is looked up in the table before the step, so
    /// names can be exchanged.
    RenameColumns(Vec<(String, String)>),
    ReorderColumns(Vec<String>),
    FilterRows { column: String, op: Op, value: String },
    KeepTopRows(usize),
    KeepBottomRows(usize),
    RemoveTopRows(usize),
    RemoveBottomRows(usize),
    RemoveDuplicates(Vec<String>),
    RemoveBlankRows,
    RemoveErrors(Vec<String>),
    ReplaceErrors { columns: Vec<String>, with: String },
    Sort(Vec<(String, bool)>),
    SplitColumn { column: String, by: SplitBy, columns: usize },
    MergeColumns { columns: Vec<String>, separator: String, name: String },
    ReplaceValue { column: String, find: String, with: String, whole_cell: bool },
    ReplaceNull { columns: Vec<String>, with: String },
    FillDown(Vec<String>),
    FillUp(Vec<String>),
    Transpose,
    ReverseRows,
    Trim(Vec<String>),
    Upper(Vec<String>),
    Lower(Vec<String>),
    /// The values of the column `attribute` become columns, holding the values of `values`, put together by `agg`.
    Pivot { attribute: String, values: String, agg: Agg },
    /// The columns become rows of Attribute and Value.
    Unpivot { columns: Vec<String>, attribute: String, value: String },
    /// All the columns but these become rows of Attribute and Value.
    UnpivotOther { keep: Vec<String>, attribute: String, value: String },
    GroupBy { keys: Vec<String>, aggregations: Vec<(String, Agg, Option<String>)> },
    /// A merge with another table (a query or a table of the book by name), its columns `expand`ed.
    Merge { right: String, left_keys: Vec<String>, right_keys: Vec<String>, kind: JoinKind, expand: Vec<String> },
    /// Another table's rows added below (`Table.Combine`).
    Append { other: String },
    /// A column worked out by an expression (`Table.AddColumn`).
    AddColumn { name: String, formula: String, ty: Option<QType> },
    /// A column of the first matching clause's output (`if … then … else`).
    AddConditionalColumn { name: String, clauses: Vec<Clause>, otherwise: String },
    AddIndexColumn { name: String, start: i64, step: i64 },
    /// Rows kept by a condition of several comparisons joined by and/or, a list of values, a part of a date or a window of dates
    /// (`Table.SelectRows` with a longer condition).
    FilterCondition(Cond),
    /// A step of the columns named in it (select, remove, rename, reorder, change type) where a column that is not there is ignored or made
    /// of nulls (the `MissingField.Ignore` / `MissingField.UseNull` argument).
    WithMissing { inner: Box<Step>, missing: Missing },
    /// One function applied to the values of a column (`Table.TransformColumns`).
    Transform { column: String, function: Transform },
    /// Rows of the groups made of the neighbouring rows of the same keys (`Table.Group` with `GroupKind.Local`).
    GroupByLocal { keys: Vec<String>, aggregations: Vec<(String, Agg, Option<String>)> },
    /// The first `offset` rows, then `skip` rows dropped and `take` rows kept again and again (`Table.AlternateRows`).
    AlternateRows { offset: usize, skip: usize, take: usize },
    /// The rows from `offset` (0 = the first), `count` of them or all the rest (`Table.Range`).
    KeepRange { offset: usize, count: Option<usize> },
    /// A copy of a column, put after the others (`Table.DuplicateColumn`).
    DuplicateColumn { column: String, new_name: String },
    /// M the screen's steps cannot hold (K0039): the rest of the code from the first binding that is not one of the steps above, kept as
    /// it was written and run by the M interpreter. It is always the last step. `names` are the names of the bindings of the steps before
    /// it (the source first, as the code spells them); `bindings` are the lines after them, `result` the name after `in`.
    Code { names: Vec<String>, bindings: String, result: String },
}

/// One clause of a conditional column: when the column compares so, the column has the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clause {
    pub column: String,
    pub op: Op,
    pub value: String,
    pub output: String,
}

/// A parameter of the workbook (`パラメーターの管理`): a name and a current value, used in the text of a
/// filter or a replacement as `{Name}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub value: String,
}

/// What a query starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    JsonArray { name: String, key: Option<String> },
    Delimited { name: String, encoding: String, delimiter: Option<char>, start_row: usize, headers: bool },
    ParquetFolder { name: String, columns: Vec<String> },
    /// A table in a local SQLite file. Only the display name and table are stored.
    Sqlite { name: String, table: String },
    /// A local Avro object container file. Only the display name is stored.
    Avro { name: String },
    /// A local Arrow IPC file. Only the display name is stored.
    ArrowIpc { name: String },
    /// A local JSON Lines file. Only the display name is stored.
    JsonLines { name: String },
    /// Local Parquet files selected by the user. Only the display names are stored.
    Parquet { names: Vec<String>, columns: Vec<String> },
    /// A blank query starting with an empty table (`#table({}, {})`).
    Blank,
    /// A table of the book, or a named range (`Excel.CurrentWorkbook(){[Name="Table1"]}[Content]`).
    Table(String),
    /// A range of a sheet, by its sheet and A1 text.
    Range { sheet: String, range: String },
    /// A text or CSV file (`Csv.Document(File.Contents("…"),[Delimiter=",", Columns=7, Encoding=65001, QuoteStyle=QuoteStyle.None])`).
    Csv { path: String, delimiter: String, columns: usize, encoding: u32, quote: String },
    /// A sheet or a table of a workbook file (`Excel.Workbook(File.Contents("…"), null, true)` and the navigation to the item).
    Workbook { path: String, item: String, sheet: bool },
}

/// A query: its name, source and steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub name: String,
    pub source: Source,
    pub steps: Vec<Step>,
    /// The sheet the result was loaded to (`閉じて読み込む`), by name.
    pub output: Option<String>,
}

/// Where the tables of other queries and of the book are read from, by name.
pub trait Resolver {
    fn table(&self, name: &str) -> Option<QTable>;
}

impl Resolver for HashMap<String, QTable> {
    fn table(&self, name: &str) -> Option<QTable> {
        self.get(name).cloned()
    }
}

/// The parameters' values written into a text (`{Name}`).
fn with_parameters(text: &str, parameters: &[Parameter]) -> String {
    let mut out = text.to_string();
    for p in parameters {
        out = out.replace(&format!("{{{}}}", p.name), &p.value);
    }
    out
}

// ---------------------------------------------------------------- values

fn is_null(v: &CellValue) -> bool {
    matches!(v, CellValue::Empty) || matches!(v, CellValue::Text(t) if t.is_empty() && false)
}

/// A value as the text of a cell.
pub fn value_text(v: &CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        CellValue::Number(n) => ibis_calc::number_to_xml(*n),
        CellValue::Text(t) => t.clone(),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Error(e) => e.as_str().to_string(),
    }
}

/// The order of values of different types in a sort: nothing first, then numbers, logical values, text, and errors (Excel: `null < number <
/// logical < text`, measured in `tasks/doc/requests/2026-11-17-k0039-m-semantics-result.md`, section 27).
fn rank(v: &CellValue) -> u8 {
    match v {
        CellValue::Empty => 0,
        CellValue::Number(_) => 1,
        CellValue::Bool(_) => 2,
        CellValue::Text(_) => 3,
        CellValue::Error(_) => 4,
    }
}

fn compare(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        // Text is compared by its UTF-16 code units, case and all (`"B" < "a"`): Excel's M compares text so (section 10).
        (CellValue::Text(x), CellValue::Text(y)) => x.encode_utf16().cmp(y.encode_utf16()),
        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

fn same(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x == y,
        _ => a == b,
    }
}

/// A key to group equal values by (numbers and text of the same look stay apart).
fn key_of(v: &CellValue) -> String {
    match v {
        CellValue::Empty => "\u{0}".into(),
        CellValue::Number(n) => format!("n{}", ibis_calc::number_to_xml(*n)),
        CellValue::Text(t) => format!("t{t}"),
        CellValue::Bool(b) => format!("b{b}"),
        CellValue::Error(e) => format!("e{}", e.as_str()),
    }
}

/// A text read as a number by a culture.
fn parse_number(text: &str, culture: Culture) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let mut cleaned = String::with_capacity(t.len());
    for c in t.chars() {
        if culture.group_separators().contains(&c) {
            continue;
        }
        if c == culture.decimal_separator() {
            cleaned.push('.');
        } else if c == '%' {
            return parse_number(&t[..t.len() - 1], culture).map(|n| n / 100.0);
        } else {
            cleaned.push(c);
        }
    }
    let n: f64 = cleaned.parse().ok()?;
    n.is_finite().then_some(n)
}

/// A date (and time) read from text by a culture, as a serial number.
fn parse_date_time(text: &str, culture: Culture) -> Option<f64> {
    let t = text.trim();
    // 2024年3月5日
    if t.contains('年') {
        let digits: Vec<i64> = t.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect();
        if digits.len() >= 3 {
            let serial = ibis_calc::eval::convert::days_from_civil(digits[0], digits[1], digits[2]) + 25569;
            return Some(serial as f64);
        }
    }
    let (date_part, time_part) = match t.split_once(|c: char| c == ' ' || c == 'T') {
        Some((d, tm)) if tm.contains(':') => (d, Some(tm)),
        _ => (t, None),
    };
    let mut serial = None;
    // An ISO date is year first whatever the culture.
    if date_part.len() >= 8 && date_part.as_bytes().get(4).is_some_and(|c| matches!(c, b'-' | b'/')) {
        serial = parse_date_in_order(date_part, DateOrder::Ymd, false);
    }
    if serial.is_none() {
        serial = if culture == Culture::Thai { parse_thai_date(date_part) } else { parse_date_in_order(date_part, culture.date_order(), false) };
    }
    let serial = match (serial, time_part) {
        (Some(s), _) => s,
        (None, _) if date_part.contains(':') => 0.0,
        _ => return None,
    };
    let time = match time_part {
        Some(tm) => parse_time_fraction(tm)?,
        None if date_part.contains(':') => parse_time_fraction(date_part)?,
        None => 0.0,
    };
    Some(serial + time)
}

/// A date as th-TH writes it: day first, the year in the Buddhist calendar (the Gregorian year + 543), as .NET's Thai calendar reads it.
fn parse_thai_date(text: &str) -> Option<f64> {
    let sep = text.find(['/', '-', '.'])?;
    let sep = text[sep..].chars().next()?;
    let parts: Vec<&str> = text.split(sep).collect();
    let year = parts.last()?.trim();
    if parts.len() != 3 || year.len() != 4 {
        return parse_date_in_order(text, Culture::Thai.date_order(), false);
    }
    let gregorian = year.parse::<i64>().ok()? - 543;
    parse_date_in_order(&format!("{}{sep}{}{sep}{gregorian}", parts[0], parts[1]), DateOrder::Dmy, false)
}

fn parse_time_fraction(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches('Z');
    let (t, pm) = match t.to_ascii_uppercase() {
        u if u.ends_with("PM") => (t[..t.len() - 2].trim().to_string(), Some(true)),
        u if u.ends_with("AM") => (t[..t.len() - 2].trim().to_string(), Some(false)),
        _ => (t.to_string(), None),
    };
    let mut parts = t.split(':');
    let mut h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    let s: f64 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    match pm {
        Some(true) if h < 12.0 => h += 12.0,
        Some(false) if h == 12.0 => h = 0.0,
        _ => {}
    }
    Some((h * 3600.0 + m * 60.0 + s) / 86400.0)
}

/// A value converted to a type; what cannot be converted is the error `#VALUE!` (Power Query's
/// `DataFormat.Error`).
pub fn convert(v: &CellValue, ty: QType, culture: Culture) -> CellValue {
    let bad = || CellValue::Error(CellError::Value);
    if matches!(v, CellValue::Empty | CellValue::Error(_)) {
        return v.clone();
    }
    match ty {
        QType::Any => v.clone(),
        QType::Text => CellValue::Text(match v {
            CellValue::Number(n) => ibis_calc::number_to_xml(*n),
            CellValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            other => value_text(other),
        }),
        QType::Whole | QType::Decimal | QType::Currency | QType::Percentage | QType::Int32 => {
            let n = match v {
                CellValue::Number(n) => *n,
                CellValue::Bool(b) => f64::from(*b),
                CellValue::Text(t) => {
                    // A currency may be written with its sign (`$1,234`, `¥500`).
                    let text = if ty == QType::Currency { t.trim().trim_start_matches(['$', '¥', '€', '£']) } else { t.as_str() };
                    match parse_number(text, culture) {
                        Some(n) => n,
                        None => return bad(),
                    }
                }
                _ => return bad(),
            };
            match ty {
                // As the M interpreter converts (what Excel was measured to do): halves go to the even number (2.5 -> 2, 0.5 -> 0).
                QType::Whole => CellValue::Number(ibis_m_lang::culture::round_scaled(n, 0, 4)),
                QType::Int32 => {
                    let r = ibis_m_lang::culture::round_scaled(n, 0, 4);
                    if r.abs() > f64::from(i32::MAX) { bad() } else { CellValue::Number(r) }
                }
                // A fixed decimal keeps four decimals.
                QType::Currency => CellValue::Number(ibis_m_lang::culture::round_scaled(n, 4, 4)),
                _ => CellValue::Number(n),
            }
        }
        QType::Date | QType::DateTime | QType::Time | QType::DateTimeZone => {
            let n = match v {
                CellValue::Number(n) => *n,
                CellValue::Text(t) => {
                    // The zone's offset (`+09:00`, `Z`) is dropped: the value keeps the clock time written.
                    let text = cond::without_zone(t);
                    match parse_date_time(text, culture).or_else(|| parse_number(text, Culture::Invariant)) {
                        Some(n) => n,
                        None => return bad(),
                    }
                }
                _ => return bad(),
            };
            CellValue::Number(match ty {
                QType::Date => n.floor(),
                QType::Time => n - n.floor(),
                _ => n,
            })
        }
        QType::Duration => match v {
            CellValue::Number(n) => CellValue::Number(*n),
            CellValue::Text(t) => match cond::parse_duration(t).or_else(|| parse_number(t, culture)) {
                Some(n) => CellValue::Number(n),
                None => bad(),
            },
            _ => bad(),
        },
        QType::Logical => match v {
            CellValue::Bool(_) => v.clone(),
            CellValue::Number(n) => CellValue::Bool(*n != 0.0),
            CellValue::Text(t) => match t.trim().to_lowercase().as_str() {
                "true" | "yes" | "1" => CellValue::Bool(true),
                "false" | "no" | "0" => CellValue::Bool(false),
                _ => bad(),
            },
            _ => bad(),
        },
    }
}

/// A text typed in a step read as a value: a number if it reads as one, else text.
fn literal(text: &str) -> CellValue {
    match parse_number(text, Culture::Invariant) {
        Some(n) if !text.trim().is_empty() => CellValue::Number(n),
        _ => CellValue::Text(text.to_string()),
    }
}

pub(crate) fn matches_op(v: &CellValue, op: Op, operand: &str) -> bool {
    use std::cmp::Ordering::*;
    match op {
        Op::IsNull => return is_null(v),
        Op::IsNotNull => return !is_null(v),
        _ => {}
    }
    let lit = match cond::date_literal(operand) {
        Some(serial) => CellValue::Number(serial),
        None => literal(operand),
    };
    let text = value_text(v);
    match op {
        Op::Contains => text.to_lowercase().contains(&operand.to_lowercase()),
        Op::DoesNotContain => !text.to_lowercase().contains(&operand.to_lowercase()),
        Op::BeginsWith => text.to_lowercase().starts_with(&operand.to_lowercase()),
        Op::EndsWith => text.to_lowercase().ends_with(&operand.to_lowercase()),
        Op::DoesNotBeginWith => !text.to_lowercase().starts_with(&operand.to_lowercase()),
        Op::DoesNotEndWith => !text.to_lowercase().ends_with(&operand.to_lowercase()),
        Op::Equals => {
            if rank(v) == rank(&lit) { compare(v, &lit) == Equal } else { text == operand }
        }
        Op::NotEquals => {
            if rank(v) == rank(&lit) { compare(v, &lit) != Equal } else { text != operand }
        }
        Op::Greater | Op::GreaterOrEqual | Op::Less | Op::LessOrEqual => {
            // A date written as `#date(…)` is compared with numbers only (a text is not a date).
            if is_null(v) || (cond::date_literal(operand).is_some() && !matches!(v, CellValue::Number(_))) {
                return false;
            }
            let ord = if rank(v) == rank(&lit) { compare(v, &lit) } else { text.cmp(&operand.to_string()) };
            match op {
                Op::Greater => ord == Greater,
                Op::GreaterOrEqual => ord != Less,
                Op::Less => ord == Less,
                _ => ord != Greater,
            }
        }
        Op::IsNull | Op::IsNotNull => unreachable!(),
    }
}

fn aggregate(values: &[CellValue], agg: Agg) -> CellValue {
    let numbers: Vec<f64> = values.iter().filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).collect();
    match agg {
        Agg::Count => CellValue::Number(values.len() as f64),
        Agg::CountDistinct => {
            let mut keys: Vec<String> = values.iter().map(key_of).collect();
            keys.sort();
            keys.dedup();
            CellValue::Number(keys.len() as f64)
        }
        Agg::First => values.first().cloned().unwrap_or(CellValue::Empty),
        Agg::Sum => {
            if numbers.is_empty() { CellValue::Empty } else { CellValue::Number(numbers.iter().sum()) }
        }
        Agg::Average => {
            if numbers.is_empty() { CellValue::Empty } else { CellValue::Number(numbers.iter().sum::<f64>() / numbers.len() as f64) }
        }
        Agg::Min => numbers.iter().copied().reduce(f64::min).map_or(CellValue::Empty, CellValue::Number),
        Agg::Max => numbers.iter().copied().reduce(f64::max).map_or(CellValue::Empty, CellValue::Number),
        Agg::Median => {
            if numbers.is_empty() {
                return CellValue::Empty;
            }
            let mut s = numbers;
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = s.len() / 2;
            CellValue::Number(if s.len() % 2 == 1 { s[mid] } else { (s[mid - 1] + s[mid]) / 2.0 })
        }
    }
}

/// A name not yet among `names`: `Name`, then `Name.1`, `Name.2`.
fn unique_name(names: &[String], wanted: &str) -> String {
    if !names.iter().any(|n| n == wanted) {
        return wanted.to_string();
    }
    (1..).map(|i| format!("{wanted}.{i}")).find(|n| !names.contains(n)).unwrap_or_else(|| wanted.to_string())
}

// ----------------------------------------------------------------- steps

impl Step {
    /// The step worked out on `table`.
    pub fn apply(&self, table: &QTable, parameters: &[Parameter], resolver: &dyn Resolver) -> Result<QTable, String> {
        let p = |t: &str| with_parameters(t, parameters);
        let mut t = table.clone();
        match self {
            // Run by `Query::run`, which has the tables of the steps before.
            Step::Code { .. } => return Err("code step".to_string()),
            Step::PromoteHeaders => {
                let Some(first) = t.rows.first().cloned() else { return Ok(t) };
                let mut names: Vec<String> = Vec::new();
                for (i, v) in first.iter().enumerate() {
                    let base = match v {
                        CellValue::Empty => format!("Column{}", i + 1),
                        other => value_text(other),
                    };
                    let name = unique_name(&names, &base);
                    names.push(name);
                }
                t.rows.remove(0);
                for (c, name) in t.columns.iter_mut().zip(names) {
                    c.name = name;
                    c.ty = QType::Any;
                }
            }
            Step::DemoteHeaders => {
                let header: Vec<CellValue> = t.columns.iter().map(|c| CellValue::Text(c.name.clone())).collect();
                t.rows.insert(0, header);
                for (i, c) in t.columns.iter_mut().enumerate() {
                    c.name = format!("Column{}", i + 1);
                    c.ty = QType::Any;
                }
            }
            Step::ChangeType { column, ty, culture, .. } => {
                let i = t.need(column)?;
                for row in &mut t.rows {
                    row[i] = convert(&row[i], *ty, *culture);
                }
                t.columns[i].ty = *ty;
            }
            Step::SelectColumns(names) => {
                let idx: Vec<usize> = names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                t = pick(&t, &idx);
            }
            Step::RemoveColumns(names) => {
                let gone: Vec<usize> = names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let keep: Vec<usize> = (0..t.columns.len()).filter(|i| !gone.contains(i)).collect();
                t = pick(&t, &keep);
            }
            Step::RenameColumns(renames) => {
                let at: Vec<usize> = renames.iter().map(|(from, _)| t.need(from)).collect::<Result<_, _>>()?;
                for (k, (_, to)) in renames.iter().enumerate() {
                    t.columns[at[k]].name = to.clone();
                }
                // The names after all the renames must be different from one another.
                for (i, c) in t.columns.iter().enumerate() {
                    if t.columns[..i].iter().any(|x| x.name == c.name) {
                        return Err(format!("列 {} は既にあります", c.name));
                    }
                }
            }
            Step::ReorderColumns(names) => {
                // The listed columns take, in the order listed, the places the listed columns have (Excel: `{"c","a"}` on a,b,c,d is
                // c,b,a,d; a list of all the columns, which is what Excel writes, is the new order).
                let listed: Vec<usize> = names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                if let Some(dup) = listed.iter().enumerate().find(|(k, i)| listed[..*k].contains(i)) {
                    return Err(format!("列 {} が重複しています", t.columns[*dup.1].name));
                }
                let mut slots = listed.clone();
                slots.sort_unstable();
                let mut idx: Vec<usize> = (0..t.columns.len()).collect();
                for (slot, col) in slots.iter().zip(&listed) {
                    idx[*slot] = *col;
                }
                t = pick(&t, &idx);
            }
            Step::FilterRows { column, op, value } => {
                let i = t.need(column)?;
                let operand = p(value);
                t.rows.retain(|r| matches_op(&r[i], *op, &operand));
            }
            Step::KeepTopRows(n) => t.rows.truncate(*n),
            Step::KeepBottomRows(n) => {
                let skip = t.rows.len().saturating_sub(*n);
                t.rows.drain(..skip);
            }
            Step::RemoveTopRows(n) => {
                let n = (*n).min(t.rows.len());
                t.rows.drain(..n);
            }
            Step::RemoveBottomRows(n) => {
                let keep = t.rows.len().saturating_sub(*n);
                t.rows.truncate(keep);
            }
            Step::RemoveDuplicates(names) => {
                let idx: Vec<usize> = if names.is_empty() { (0..t.columns.len()).collect() } else { names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()? };
                let mut seen = std::collections::HashSet::new();
                t.rows.retain(|r| seen.insert(idx.iter().map(|&i| key_of(&r[i])).collect::<Vec<_>>().join("\u{1}")));
            }
            Step::RemoveBlankRows => {
                t.rows.retain(|r| r.iter().any(|v| !matches!(v, CellValue::Empty) && !matches!(v, CellValue::Text(s) if s.trim().is_empty())));
            }
            Step::RemoveErrors(names) => {
                let idx: Vec<usize> = if names.is_empty() { (0..t.columns.len()).collect() } else { names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()? };
                t.rows.retain(|r| idx.iter().all(|&i| !matches!(r[i], CellValue::Error(_))));
            }
            Step::ReplaceErrors { columns, with } => {
                let idx: Vec<usize> = columns.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let value = literal(&p(with));
                for r in &mut t.rows {
                    for &i in &idx {
                        if matches!(r[i], CellValue::Error(_)) {
                            r[i] = value.clone();
                        }
                    }
                }
            }
            Step::Sort(keys) => {
                let idx: Vec<(usize, bool)> = keys.iter().map(|(n, asc)| t.need(n).map(|i| (i, *asc))).collect::<Result<_, _>>()?;
                // A stable sort, the last key first.
                t.rows.sort_by(|a, b| {
                    for &(i, asc) in &idx {
                        let ord = compare(&a[i], &b[i]);
                        let ord = if asc { ord } else { ord.reverse() };
                        if ord != std::cmp::Ordering::Equal {
                            return ord;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
            }
            Step::SplitColumn { column, by, columns: fixed } => {
                let i = t.need(column)?;
                let parts: Vec<Vec<String>> = t
                    .rows
                    .iter()
                    .map(|r| match &r[i] {
                        CellValue::Empty => Vec::new(),
                        v => {
                            cond::split_text(&value_text(v), by)
                        }
                    })
                    .collect();
                // The columns Power Query names in the call (`{"a.1", "a.2"}`) are all it makes, however many pieces there are.
                let width = if *fixed > 0 { *fixed } else { parts.iter().map(Vec::len).max().unwrap_or(1).max(1) };
                let mut columns = t.columns.clone();
                let mut names_so_far: Vec<String> = columns.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, c)| c.name.clone()).collect();
                let mut new_columns = Vec::new();
                for k in 0..width {
                    let name = unique_name(&names_so_far, &format!("{column}.{}", k + 1));
                    names_so_far.push(name.clone());
                    new_columns.push(QColumn { name, ty: QType::Text });
                }
                columns.splice(i..=i, new_columns);
                let rows: Vec<Vec<CellValue>> = t
                    .rows
                    .iter()
                    .zip(&parts)
                    .map(|(r, ps)| {
                        let mut out = r[..i].to_vec();
                        for k in 0..width {
                            // A piece that is empty is the empty text (`Text.Split("みかん","ん")` is `{"みか",""}`); a piece that is not there is null.
                            out.push(ps.get(k).map_or(CellValue::Empty, |s| CellValue::Text(s.clone())));
                        }
                        out.extend_from_slice(&r[i + 1..]);
                        out
                    })
                    .collect();
                t = QTable { columns, rows };
            }
            Step::MergeColumns { columns, separator, name } => {
                let idx: Vec<usize> = columns.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let first = *idx.first().ok_or("列を選んでください")?;
                let rest: Vec<String> = t.columns.iter().enumerate().filter(|(k, _)| !idx.contains(k)).map(|(_, c)| c.name.clone()).collect();
                let new_name = unique_name(&rest, name);
                for r in &mut t.rows {
                    let joined = idx.iter().map(|&i| value_text(&r[i])).collect::<Vec<_>>().join(separator);
                    r[first] = CellValue::Text(joined);
                }
                t.columns[first] = QColumn { name: new_name, ty: QType::Text };
                let keep: Vec<usize> = (0..t.columns.len()).filter(|k| *k == first || !idx.contains(k)).collect();
                t = pick(&t, &keep);
            }
            Step::ReplaceValue { column, find, with, whole_cell } => {
                let i = t.need(column)?;
                let (find, with) = (p(find), p(with));
                for r in &mut t.rows {
                    let text = value_text(&r[i]);
                    if matches!(r[i], CellValue::Empty) && !find.is_empty() {
                        continue;
                    }
                    if *whole_cell {
                        if text == find {
                            r[i] = literal_keep_text(&with);
                        }
                    } else if !find.is_empty() && text.contains(&find) {
                        r[i] = CellValue::Text(text.replace(&find, &with));
                    }
                }
            }
            Step::ReplaceNull { columns, with } => {
                let idx: Vec<usize> = columns.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let value = literal(&p(with));
                for r in &mut t.rows {
                    for &i in &idx {
                        if matches!(r[i], CellValue::Empty) {
                            r[i] = value.clone();
                        }
                    }
                }
            }
            Step::FillDown(names) | Step::FillUp(names) => {
                let idx: Vec<usize> = names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let up = matches!(self, Step::FillUp(_));
                for &i in &idx {
                    let order: Vec<usize> = if up { (0..t.rows.len()).rev().collect() } else { (0..t.rows.len()).collect() };
                    let mut last = CellValue::Empty;
                    for k in order {
                        if matches!(t.rows[k][i], CellValue::Empty) {
                            t.rows[k][i] = last.clone();
                        } else {
                            last = t.rows[k][i].clone();
                        }
                    }
                }
            }
            Step::Transpose => {
                let cols = t.columns.len();
                let rows: Vec<Vec<CellValue>> = (0..cols).map(|c| t.rows.iter().map(|r| r[c].clone()).collect()).collect();
                let columns = (0..t.rows.len()).map(|i| QColumn { name: format!("Column{}", i + 1), ty: QType::Any }).collect();
                t = QTable { columns, rows };
            }
            Step::ReverseRows => t.rows.reverse(),
            Step::Trim(names) | Step::Upper(names) | Step::Lower(names) => {
                let idx: Vec<usize> = names.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                for r in &mut t.rows {
                    for &i in &idx {
                        if let CellValue::Text(s) = &r[i] {
                            r[i] = CellValue::Text(match self {
                                Step::Trim(_) => s.trim().to_string(),
                                // One character to one character, as Excel's M does (`ß` stays, `İ` is `i`).
                                Step::Upper(_) => s.chars().map(ibis_m_lang::culture::simple_upper).collect(),
                                _ => s.chars().map(ibis_m_lang::culture::simple_lower).collect(),
                            });
                        }
                    }
                }
                // The columns are `type text` after these steps (what Excel writes in the step).
                for &i in &idx {
                    t.columns[i].ty = QType::Text;
                }
            }
            Step::FilterCondition(cond) => {
                for name in cond.columns() {
                    t.need(name)?;
                }
                let names = t.names();
                let today = ibis_calc::eval::convert::today_serial(false).unwrap_or(0);
                t.rows.retain(|r| cond.matches(&names, r, today, &p));
            }
            Step::WithMissing { inner, missing } => {
                let wanted = inner.named_columns();
                let absent: Vec<String> = wanted.into_iter().filter(|n| t.column_index(n).is_none()).collect();
                match missing {
                    Missing::Error => return inner.apply(&t, parameters, resolver),
                    Missing::UseNull => {
                        for name in absent {
                            if t.column_index(&name).is_none() {
                                t.columns.push(QColumn { name, ty: QType::Any });
                                for r in &mut t.rows {
                                    r.push(CellValue::Empty);
                                }
                            }
                        }
                        return inner.apply(&t, parameters, resolver);
                    }
                    Missing::Ignore => match inner.without_columns(&absent) {
                        Some(step) => return step.apply(&t, parameters, resolver),
                        None => return Ok(t),
                    },
                }
            }
            Step::Transform { column, function } => {
                let i = t.need(column)?;
                let function = function.with_parameters(&p);
                for r in &mut t.rows {
                    r[i] = function.apply(&r[i]);
                }
                // The type Excel writes with the function (`Number.Abs` leaves the column's own).
                t.columns[i].ty = match function {
                    Transform::Abs => t.columns[i].ty,
                    f => QType::from_m(f.result_type_m()).unwrap_or(QType::Any),
                };
            }
            Step::GroupByLocal { keys, aggregations } => t = group_by(&t, keys, aggregations, true)?,
            Step::AlternateRows { offset, skip, take } => {
                let mut rows = Vec::new();
                let (mut k, n) = (0usize, t.rows.len());
                while k < n.min(*offset) {
                    rows.push(t.rows[k].clone());
                    k += 1;
                }
                // `take` of 0 would not move: nothing more is kept.
                while k < n && *take > 0 {
                    k += skip;
                    for _ in 0..*take {
                        if k < n {
                            rows.push(t.rows[k].clone());
                        }
                        k += 1;
                    }
                }
                t.rows = rows;
            }
            Step::KeepRange { offset, count } => {
                let from = (*offset).min(t.rows.len());
                let to = count.map_or(t.rows.len(), |c| from.saturating_add(c).min(t.rows.len()));
                t.rows = t.rows[from..to].to_vec();
            }
            Step::DuplicateColumn { column, new_name } => {
                let i = t.need(column)?;
                if t.column_index(new_name).is_some() {
                    return Err(format!("列 {new_name} は既にあります"));
                }
                let ty = t.columns[i].ty;
                for r in &mut t.rows {
                    let v = r[i].clone();
                    r.push(v);
                }
                t.columns.push(QColumn { name: new_name.clone(), ty });
            }
            Step::Pivot { attribute, values, agg } => t = pivot(&t, attribute, values, *agg)?,
            Step::Unpivot { columns, attribute, value } => {
                let idx: Vec<usize> = columns.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                t = unpivot(&t, &idx, attribute, value);
            }
            Step::UnpivotOther { keep, attribute, value } => {
                let kept: Vec<usize> = keep.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
                let idx: Vec<usize> = (0..t.columns.len()).filter(|i| !kept.contains(i)).collect();
                t = unpivot(&t, &idx, attribute, value);
            }
            Step::GroupBy { keys, aggregations } => t = group_by(&t, keys, aggregations, false)?,
            Step::Merge { right, left_keys, right_keys, kind, expand } => {
                let other = resolver.table(right).ok_or_else(|| format!("テーブル {right} がありません"))?;
                t = merge(&t, &other, left_keys, right_keys, *kind, expand, right)?;
            }
            Step::Append { other } => {
                let other = resolver.table(other).ok_or_else(|| format!("テーブル {other} がありません"))?;
                let mut columns = t.columns.clone();
                for c in &other.columns {
                    if !columns.iter().any(|x| x.name == c.name) {
                        columns.push(c.clone());
                    }
                }
                let place = |src: &QTable, row: &Vec<CellValue>| -> Vec<CellValue> {
                    columns.iter().map(|c| src.column_index(&c.name).map_or(CellValue::Empty, |i| row[i].clone())).collect()
                };
                let mut rows: Vec<Vec<CellValue>> = t.rows.iter().map(|r| place(&t, r)).collect();
                rows.extend(other.rows.iter().map(|r| place(&other, r)));
                t = QTable { columns, rows };
            }
            Step::AddColumn { name, formula, ty } => {
                if t.column_index(name).is_some() {
                    return Err(format!("列 {name} は既にあります"));
                }
                let e = Expr::parse(&p(formula))?;
                let names = t.names();
                let mut out = Vec::with_capacity(t.rows.len());
                for r in &t.rows {
                    out.push(e.eval(&names, r));
                }
                for (r, v) in t.rows.iter_mut().zip(out) {
                    r.push(match ty {
                        Some(ty) => convert(&v, *ty, Culture::Invariant),
                        None => v,
                    });
                }
                t.columns.push(QColumn { name: name.clone(), ty: ty.unwrap_or(QType::Any) });
            }
            Step::AddConditionalColumn { name, clauses, otherwise } => {
                if t.column_index(name).is_some() {
                    return Err(format!("列 {name} は既にあります"));
                }
                let idx: Vec<usize> = clauses.iter().map(|c| t.need(&c.column)).collect::<Result<_, _>>()?;
                for r in &mut t.rows {
                    let hit = clauses.iter().zip(&idx).find(|(c, i)| matches_op(&r[**i], c.op, &p(&c.value)));
                    let out = match hit {
                        Some((c, _)) => literal(&p(&c.output)),
                        None => literal(&p(otherwise)),
                    };
                    r.push(out);
                }
                t.columns.push(QColumn { name: name.clone(), ty: QType::Any });
            }
            Step::AddIndexColumn { name, start, step } => {
                if t.column_index(name).is_some() {
                    return Err(format!("列 {name} は既にあります"));
                }
                for (k, r) in t.rows.iter_mut().enumerate() {
                    r.push(CellValue::Number((*start + *step * k as i64) as f64));
                }
                t.columns.push(QColumn { name: name.clone(), ty: QType::Whole });
            }
        }
        Ok(t)
    }
}

/// Text from a split or a replacement: kept as text (a leading zero stays).
fn literal_keep_text(s: &str) -> CellValue {
    if s.is_empty() { CellValue::Empty } else { CellValue::Text(s.to_string()) }
}

fn pick(t: &QTable, idx: &[usize]) -> QTable {
    QTable {
        columns: idx.iter().map(|&i| t.columns[i].clone()).collect(),
        rows: t.rows.iter().map(|r| idx.iter().map(|&i| r[i].clone()).collect()).collect(),
    }
}

fn pivot(t: &QTable, attribute: &str, values: &str, agg: Agg) -> Result<QTable, String> {
    let a = t.need(attribute)?;
    let v = t.need(values)?;
    let keep: Vec<usize> = (0..t.columns.len()).filter(|&i| i != a && i != v).collect();
    // The new columns, in the order the attribute values first appear.
    let mut attrs: Vec<String> = Vec::new();
    for r in &t.rows {
        let name = value_text(&r[a]);
        if !attrs.contains(&name) {
            attrs.push(name);
        }
    }
    let mut groups: Vec<(Vec<CellValue>, Vec<Vec<CellValue>>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in &t.rows {
        let key_values: Vec<CellValue> = keep.iter().map(|&i| r[i].clone()).collect();
        let key = key_values.iter().map(key_of).collect::<Vec<_>>().join("\u{1}");
        let g = *index.entry(key).or_insert_with(|| {
            groups.push((key_values, vec![Vec::new(); attrs.len()]));
            groups.len() - 1
        });
        let k = attrs.iter().position(|n| *n == value_text(&r[a])).unwrap_or(0);
        groups[g].1[k].push(r[v].clone());
    }
    let mut columns: Vec<QColumn> = keep.iter().map(|&i| t.columns[i].clone()).collect();
    let taken: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    for name in &attrs {
        columns.push(QColumn { name: unique_name(&taken, name), ty: QType::Any });
    }
    let rows = groups
        .into_iter()
        .map(|(mut key, cells)| {
            key.extend(cells.iter().map(|vals| if vals.is_empty() { CellValue::Empty } else { aggregate(vals, agg) }));
            key
        })
        .collect();
    Ok(QTable { columns, rows })
}

fn unpivot(t: &QTable, idx: &[usize], attribute: &str, value: &str) -> QTable {
    let keep: Vec<usize> = (0..t.columns.len()).filter(|i| !idx.contains(i)).collect();
    let mut columns: Vec<QColumn> = keep.iter().map(|&i| t.columns[i].clone()).collect();
    columns.push(QColumn { name: attribute.to_string(), ty: QType::Text });
    columns.push(QColumn { name: value.to_string(), ty: QType::Any });
    let mut rows = Vec::new();
    for r in &t.rows {
        for &i in idx {
            // Nothing in a cell makes no row (Power Query leaves null out).
            if matches!(r[i], CellValue::Empty) {
                continue;
            }
            let mut out: Vec<CellValue> = keep.iter().map(|&k| r[k].clone()).collect();
            out.push(CellValue::Text(t.columns[i].name.clone()));
            out.push(r[i].clone());
            rows.push(out);
        }
    }
    QTable { columns, rows }
}

fn group_by(t: &QTable, keys: &[String], aggregations: &[(String, Agg, Option<String>)], local: bool) -> Result<QTable, String> {
    let key_idx: Vec<usize> = keys.iter().map(|n| t.need(n)).collect::<Result<_, _>>()?;
    let agg_idx: Vec<Option<usize>> = aggregations.iter().map(|(_, _, c)| c.as_ref().map(|c| t.need(c)).transpose()).collect::<Result<_, _>>()?;
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, (Vec<CellValue>, Vec<&Vec<CellValue>>)> = HashMap::new();
    // A local grouping closes a group when the keys change: the same keys later start a group of their own.
    let mut run = 0usize;
    let mut last_key: Option<String> = None;
    for r in &t.rows {
        let mut key = key_idx.iter().map(|&i| key_of(&r[i])).collect::<Vec<_>>().join("\u{1}");
        if local {
            if last_key.as_ref().is_some_and(|k| *k != key) {
                run += 1;
            }
            last_key = Some(key.clone());
            key = format!("{run}\u{2}{key}");
        }
        let entry = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (key_idx.iter().map(|&i| r[i].clone()).collect(), Vec::new())
        });
        entry.1.push(r);
    }
    let mut columns: Vec<QColumn> = key_idx.iter().map(|&i| t.columns[i].clone()).collect();
    // The types Excel writes for the aggregations: a count is `Int64.Type`, the rest `type nullable number`.
    for (name, agg, _) in aggregations {
        let ty = match agg {
            Agg::Count | Agg::CountDistinct => QType::Whole,
            Agg::First => QType::Any,
            _ => QType::Decimal,
        };
        columns.push(QColumn { name: name.clone(), ty });
    }
    // The groups are in the order their keys first come (`Table.Group` of Excel: b, a, c for b, a, b, c, a; measured, section 14); a local
    // grouping gives them as they come too.
    let keyed: Vec<&(Vec<CellValue>, Vec<&Vec<CellValue>>)> = order.iter().map(|k| &groups[k]).collect();
    let rows = keyed
        .into_iter()
        .map(|(key, members)| {
            let mut out = key.clone();
            for ((_, agg, _), idx) in aggregations.iter().zip(&agg_idx) {
                let values: Vec<CellValue> = match idx {
                    Some(i) => members.iter().map(|r| r[*i].clone()).collect(),
                    None => members.iter().map(|_| CellValue::Number(1.0)).collect(),
                };
                out.push(aggregate(&values, *agg));
            }
            out
        })
        .collect();
    Ok(QTable { columns, rows })
}

fn merge(left: &QTable, right: &QTable, left_keys: &[String], right_keys: &[String], kind: JoinKind, expand: &[String], right_name: &str) -> Result<QTable, String> {
    if left_keys.is_empty() || left_keys.len() != right_keys.len() {
        return Err("結合する列の数が合いません".into());
    }
    let lk: Vec<usize> = left_keys.iter().map(|n| left.need(n)).collect::<Result<_, _>>()?;
    let rk: Vec<usize> = right_keys.iter().map(|n| right.need(n)).collect::<Result<_, _>>()?;
    let key = |row: &Vec<CellValue>, idx: &[usize]| -> Option<String> {
        // Nothing never matches nothing in Power Query's join? It does (null = null); keep that.
        Some(idx.iter().map(|&i| key_of(&row[i])).collect::<Vec<_>>().join("\u{1}"))
    };
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (j, r) in right.rows.iter().enumerate() {
        if let Some(k) = key(r, &rk) {
            by_key.entry(k).or_default().push(j);
        }
    }
    let expand_idx: Vec<usize> = expand.iter().map(|n| right.need(n)).collect::<Result<_, _>>()?;
    let left_names = left.names();
    let mut columns = left.columns.clone();
    if !matches!(kind, JoinKind::LeftAnti | JoinKind::RightAnti) {
        for &j in &expand_idx {
            let wanted = format!("{right_name}.{}", right.columns[j].name);
            let name = unique_name(&columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), &wanted);
            columns.push(QColumn { name, ty: right.columns[j].ty });
        }
    }
    let _ = left_names;
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut right_used = vec![false; right.rows.len()];
    for l in &left.rows {
        let matches = key(l, &lk).and_then(|k| by_key.get(&k)).cloned().unwrap_or_default();
        match kind {
            JoinKind::LeftAnti => {
                if matches.is_empty() {
                    rows.push(l.clone());
                }
            }
            JoinKind::RightAnti => {
                for &j in &matches {
                    right_used[j] = true;
                }
            }
            _ => {
                if matches.is_empty() {
                    if matches!(kind, JoinKind::LeftOuter | JoinKind::FullOuter) {
                        let mut out = l.clone();
                        out.extend(expand_idx.iter().map(|_| CellValue::Empty));
                        rows.push(out);
                    }
                } else {
                    for &j in &matches {
                        right_used[j] = true;
                        let mut out = l.clone();
                        out.extend(expand_idx.iter().map(|&c| right.rows[j][c].clone()));
                        rows.push(out);
                    }
                }
            }
        }
    }
    match kind {
        JoinKind::RightOuter | JoinKind::FullOuter => {
            for (j, used) in right_used.iter().enumerate() {
                if !used {
                    // The left columns empty, but the key columns take the right's values.
                    let mut out = vec![CellValue::Empty; left.columns.len()];
                    for (&l, &r) in lk.iter().zip(&rk) {
                        out[l] = right.rows[j][r].clone();
                    }
                    out.extend(expand_idx.iter().map(|&c| right.rows[j][c].clone()));
                    rows.push(out);
                }
            }
        }
        JoinKind::RightAnti => {
            // The right's rows that match nothing, in the right's columns.
            let mut columns = right.columns.clone();
            columns.shrink_to_fit();
            let rows = right.rows.iter().enumerate().filter(|(j, _)| !right_used[*j]).map(|(_, r)| r.clone()).collect();
            return Ok(QTable { columns, rows });
        }
        _ => {}
    }
    Ok(QTable { columns, rows })
}

impl Query {
    pub fn new(name: &str, source: Source) -> Self {
        Self { name: name.to_string(), source, steps: Vec::new(), output: None }
    }

    /// The table after each step (the first is the source itself), or the error of the first step that fails
    /// and the tables before it.
    pub fn run(&self, source: QTable, parameters: &[Parameter], resolver: &dyn Resolver) -> (Vec<QTable>, Option<(usize, String)>) {
        let mut tables = vec![source];
        for (k, step) in self.steps.iter().enumerate() {
            if let Step::Code { names, bindings, result } = step {
                // The first binding of the code is the line after `let` and the bindings of the steps before it.
                match m_value::run_code(names, bindings, result, &tables, names.len() + 2, resolver) {
                    Ok(t) => tables.push(t),
                    Err(e) => return (tables, Some((k, e))),
                }
                continue;
            }
            match step.apply(tables.last().expect("a table"), parameters, resolver) {
                Ok(t) => tables.push(t),
                Err(e) => return (tables, Some((k, e))),
            }
        }
        (tables, None)
    }
}

/// A table of a block of cells: the first row is not yet the header (`Column1`, `Column2`…), as a range
/// read in Power Query is.
pub fn table_of_cells(rows: Vec<Vec<CellValue>>, width: usize) -> QTable {
    let columns = (0..width).map(|i| QColumn { name: format!("Column{}", i + 1), ty: QType::Any }).collect();
    let rows = rows.into_iter().map(|mut r| {
        r.resize(width, CellValue::Empty);
        r
    });
    QTable { columns, rows: rows.collect() }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_excel;

#[cfg(test)]
mod tests_m;

#[cfg(all(test, any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod tests_external;
