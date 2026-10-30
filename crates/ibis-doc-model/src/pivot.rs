// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pivot tables (ピボットテーブル): a table's definition
//! (`xl/pivotTables/pivotTable*.xml`, ECMA-376 Part 1 §18.10) with what its
//! cache's definition (`pivotCacheDefinition`) says of its source and of
//! each field. The table's cells are the sheet's own; the definition says
//! where it is, how its rows and columns are laid out (for its style), and
//! is what 更新 builds the cells again from.

use std::collections::HashMap;

use crate::chart::{
    Axis, AxisKind, AxisSourceKind, Chart, ChartGroup, ChartText, ChartTitle, ChartType, Legend,
    LegendPosition, Number, Series, Side,
};
use crate::drawing::{Color, Fill, Stroke};
use crate::Rgb;
use crate::sheet::{CellError, CellRange, CellValue};

/// A pivot table on a sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotTable {
    /// Its name (`name`), as ピボットテーブル分析 shows it.
    pub name: String,
    /// The file's part it was read from (`xl/pivotTables/pivotTable1.xml`),
    /// for the save to find it; `None` for one made in the app.
    pub part: Option<String>,
    /// The cache's definition part it uses.
    pub cache_part: Option<String>,
    /// The cells it covers (`location/@ref`), without its page fields.
    pub location: CellRange,
    /// `location`'s offsets: the first row of headers, of data, and the
    /// first column of data, from the location's corner.
    pub first_header_row: u32,
    pub first_data_row: u32,
    pub first_data_col: u32,
    /// Where its data comes from.
    pub source: PivotSource,
    /// The cache's fields, in order, with how the table uses each.
    pub fields: Vec<PivotField>,
    /// The fields on the rows and on the columns, in order (indexes into
    /// `fields`; `-2` is the values' place when there are several).
    pub row_fields: Vec<i32>,
    pub col_fields: Vec<i32>,
    /// Report filters (ページ フィールド).
    pub page_fields: Vec<PivotPageField>,
    /// The values (値).
    pub data_fields: Vec<PivotDataField>,
    /// The table's lines, as its rows and columns are (`rowItems`,
    /// `colItems`): what each row (column) of the body is.
    pub row_items: Vec<PivotLine>,
    pub col_items: Vec<PivotLine>,
    /// 総計 of the rows (a last row) and of the columns (a last column).
    pub row_grand_totals: bool,
    pub col_grand_totals: bool,
    /// The layout's defaults: コンパクト (`compact`) puts the row fields in
    /// one column; アウトライン (`outline`) puts each in its own column with
    /// its subtotals at the top; neither is 表形式.
    pub compact: bool,
    pub outline: bool,
    /// The texts the table writes when the file names its own (else the
    /// app's language's: 行ラベル, 列ラベル, 総計, 値).
    pub row_header_caption: Option<String>,
    pub col_header_caption: Option<String>,
    pub grand_total_caption: Option<String>,
    pub data_caption: Option<String>,
    /// Shown for an empty value (`missingCaption`, with `showMissing`).
    pub missing_caption: Option<String>,
    /// `showMissing` off: Excel then shows 0 for a value with no data.
    pub missing_zero: bool,
    /// Shown for a value that is an error (`errorCaption`, with `showError`).
    pub error_caption: Option<String>,
    /// 項目ラベルをすべて繰り返す (`fillDownLabelsDefault`): the label of an
    /// outer item on each row of its group, in the outline and tabular layouts.
    pub fill_down_labels: bool,
    /// ラベル フィルター, 値フィルター and トップテン (`filters`).
    pub filters: Vec<PivotFilter>,
    /// Its style (`pivotTableStyleInfo`).
    pub style: Option<PivotStyleInfo>,
    /// The values shown on the rows (`dataOnRows`) when there are several.
    pub data_on_rows: bool,
    /// Built again in the app (更新): the save asks Excel to build its
    /// cache again on opening (`refreshOnLoad`).
    pub refreshed: bool,
    /// The file asks for it to be built on opening (its cache's
    /// `refreshOnLoad`), as Excel does.
    pub refresh_on_load: bool,
    /// The cells with a pivot button (▼: the captions and the report
    /// filters' values) as (row, column), when built in the app; the corner
    /// caption (合計 / 金額 over the row labels) is `corner`.
    pub buttons: Vec<(u32, u32)>,
    pub corner: Option<(u32, u32)>,
    /// セルを結合して中央揃えにする (`mergeItem`).
    pub merge_item: bool,
    /// 更新時に列幅を自動調整する (`applyWidthHeightFormats`).
    pub auto_fit_columns: bool,
    /// 更新時にセルの書式を保持する (`useAutoFormatting`).
    pub preserve_formatting: bool,
    /// フィールドごとに複数のフィルターを許可する (`multipleFieldFilters`).
    pub multiple_field_filters: bool,
    /// 集計アイテム / 計算アイテム (`calculatedItems`, ECMA-376 Part 4 §3.10.1.8).
    pub calculated_items: Vec<PivotCalculatedItem>,
    /// ドリル ボタン（+/- ボタン）の表示 (`showDrill`).
    pub show_drill: bool,
}

/// A calculated item within a pivot field (集計アイテム / 計算アイテム, ECMA-376 Part 4 §3.10.1.8 `calculatedItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotCalculatedItem {
    /// The index of the field this item belongs to (`field`).
    pub field: u32,
    /// The name of this calculated item.
    pub name: String,
    /// The formula (e.g. "= 関東 + 関西").
    pub formula: String,
}

/// A filter of a field's items by their labels or by a value (`filter`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotFilter {
    /// The field it filters (an index into `fields`).
    pub field: u32,
    /// Its type (`type`): `captionEqual`, `captionBeginsWith`, …, `valueGreaterThan`, …,
    /// `count`, `percent`, `sum` (トップテン).
    pub kind: String,
    /// The value of a value filter (`iMeasureFld`, an index into `data_fields`).
    pub measure: u32,
    /// What it compares with: the value typed, without the wildcards a
    /// 始まる / 終わる / 含む filter is written with (トップテン: the number).
    pub value: String,
    /// The upper end of 指定の範囲内 / 範囲外.
    pub value2: Option<String>,
    /// トップテン: the top items, or the bottom.
    pub top: bool,
}

impl PivotFilter {
    /// Whether it filters by the items' own labels.
    pub fn is_label(&self) -> bool {
        self.kind.starts_with("caption")
    }

    /// Whether the label `text` stays.
    pub fn keeps_label(&self, text: &str) -> bool {
        let (a, b) = (text.to_lowercase(), self.value.to_lowercase());
        let compare = |x: &str, y: &str| match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
            (Ok(p), Ok(q)) => p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal),
            _ => x.cmp(y),
        };
        let upper = self.value2.as_deref().unwrap_or("").to_lowercase();
        match self.kind.as_str() {
            "captionEqual" => a == b,
            "captionNotEqual" => a != b,
            "captionBeginsWith" => a.starts_with(&b),
            "captionNotBeginsWith" => !a.starts_with(&b),
            "captionEndsWith" => a.ends_with(&b),
            "captionNotEndsWith" => !a.ends_with(&b),
            "captionContains" => a.contains(&b),
            "captionNotContains" => !a.contains(&b),
            "captionGreaterThan" => compare(&a, &b).is_gt(),
            "captionGreaterThanOrEqual" => compare(&a, &b).is_ge(),
            "captionLessThan" => compare(&a, &b).is_lt(),
            "captionLessThanOrEqual" => compare(&a, &b).is_le(),
            "captionBetween" => compare(&a, &b).is_ge() && compare(&a, &upper).is_le(),
            "captionNotBetween" => compare(&a, &b).is_lt() || compare(&a, &upper).is_gt(),
            _ => true,
        }
    }

    /// Which of the items, with `values` (the value of each; `None`: no data), a value filter keeps.
    pub fn keeps_values(&self, values: &[Option<f64>]) -> Vec<bool> {
        let number = |s: &str| s.trim().parse::<f64>().ok();
        let (low, high) = (number(&self.value), self.value2.as_deref().and_then(number));
        let single = |f: &dyn Fn(f64, f64) -> bool| -> Vec<bool> { values.iter().map(|v| matches!((v, low), (Some(v), Some(l)) if f(*v, l))).collect() };
        match self.kind.as_str() {
            "valueEqual" => single(&|v, l| v == l),
            "valueNotEqual" => values.iter().map(|v| v.is_some_and(|v| low.is_some_and(|l| v != l))).collect(),
            "valueGreaterThan" => single(&|v, l| v > l),
            "valueGreaterThanOrEqual" => single(&|v, l| v >= l),
            "valueLessThan" => single(&|v, l| v < l),
            "valueLessThanOrEqual" => single(&|v, l| v <= l),
            "valueBetween" => values.iter().map(|v| matches!((v, low, high), (Some(v), Some(l), Some(h)) if *v >= l && *v <= h)).collect(),
            "valueNotBetween" => values.iter().map(|v| matches!((v, low, high), (Some(v), Some(l), Some(h)) if *v < l || *v > h)).collect(),
            "count" | "percent" | "sum" => {
                // The items from the top (bottom) down: N of them, or until their values make N per cent (N) of the total.
                let n = low.unwrap_or(10.0);
                let mut order: Vec<usize> = (0..values.len()).filter(|&i| values[i].is_some()).collect();
                order.sort_by(|&a, &b| {
                    let (x, y) = (values[a].unwrap_or(0.0), values[b].unwrap_or(0.0));
                    if self.top { y.partial_cmp(&x) } else { x.partial_cmp(&y) }.unwrap_or(std::cmp::Ordering::Equal)
                });
                let total: f64 = order.iter().map(|&i| values[i].unwrap_or(0.0)).sum();
                let mut keep = vec![false; values.len()];
                let mut running = 0.0;
                for (k, &i) in order.iter().enumerate() {
                    let reached = match self.kind.as_str() {
                        "count" => k as f64 >= n,
                        "percent" => total != 0.0 && running / total * 100.0 >= n,
                        _ => running >= n,
                    };
                    if reached {
                        break;
                    }
                    keep[i] = true;
                    running += values[i].unwrap_or(0.0);
                }
                keep
            }
            _ => vec![true; values.len()],
        }
    }
}

/// Where a pivot table's data comes from (`cacheSource/worksheetSource`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotSource {
    /// The sheet of `range` (its name).
    pub sheet: Option<String>,
    pub range: Option<CellRange>,
    /// A table's or a defined name's name instead of a range.
    pub name: Option<String>,
}

/// Where a field is placed (`pivotField/@axis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotAxis {
    Row,
    Column,
    Page,
    Values,
}

/// The unit by which dates in a pivot field are grouped (ECMA-376 Part 1 §18.18.35 `ST_GroupBy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateGroupBy {
    Years,
    Quarters,
    Months,
    Days,
}

impl DateGroupBy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Years => "years",
            Self::Quarters => "quarters",
            Self::Months => "months",
            Self::Days => "days",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "years" => Some(Self::Years),
            "quarters" => Some(Self::Quarters),
            "months" => Some(Self::Months),
            "days" => Some(Self::Days),
            _ => None,
        }
    }
}

/// How a pivot field's items are grouped (日付・数値のグループ化, ECMA-376 Part 1 §18.10.1.34 `fieldGroup`).
#[derive(Clone, Debug)]
pub enum PivotFieldGrouping {
    /// Numeric range grouping: e.g. start: 10, end: 90, interval: 20
    Number {
        start: f64,
        end: f64,
        interval: f64,
        auto_start: bool,
        auto_end: bool,
    },
    /// Date grouping: years, quarters, months, days
    Date {
        by: DateGroupBy,
        start: Option<f64>,
        end: Option<f64>,
        auto_start: bool,
        auto_end: bool,
    },
    /// Manual text item grouping (手動グループ化): e.g. mapping "北海道" -> "グループ1", "東北" -> "グループ1"
    Manual {
        groups: HashMap<String, String>,
    },
}

impl PartialEq for PivotFieldGrouping {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Number { start: s1, end: e1, interval: i1, auto_start: as1, auto_end: ae1 },
                Self::Number { start: s2, end: e2, interval: i2, auto_start: as2, auto_end: ae2 },
            ) => s1.to_bits() == s2.to_bits() && e1.to_bits() == e2.to_bits() && i1.to_bits() == i2.to_bits() && as1 == as2 && ae1 == ae2,
            (
                Self::Date { by: b1, start: s1, end: e1, auto_start: as1, auto_end: ae1 },
                Self::Date { by: b2, start: s2, end: e2, auto_start: as2, auto_end: ae2 },
            ) => b1 == b2 && s1.map(f64::to_bits) == s2.map(f64::to_bits) && e1.map(f64::to_bits) == e2.map(f64::to_bits) && as1 == as2 && ae1 == ae2,
            (
                Self::Manual { groups: g1 },
                Self::Manual { groups: g2 },
            ) => g1 == g2,
            _ => false,
        }
    }
}

impl Eq for PivotFieldGrouping {}

/// A field of the cache, as the table uses it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotField {
    /// Its name (the source's heading).
    pub name: String,
    /// Formula if this is a calculated field (計算フィールド: e.g. "売上 * 0.1").
    pub formula: Option<String>,
    pub axis: Option<PivotAxis>,
    /// The cache's items of the field (`sharedItems`), which `items` and
    /// the lines point into.
    pub shared: Vec<CellValue>,
    /// The table's items of the field, in the order shown (`items/item`).
    pub items: Vec<PivotItem>,
    /// Each field's own layout (both default to the table's).
    pub compact: bool,
    pub outline: bool,
    /// Subtotals at the top of each group (`subtotalTop`, the default).
    pub subtotal_top: bool,
    /// The automatic subtotal (`defaultSubtotal`, the default).
    pub default_subtotal: bool,
    /// The items sorted (`sortType`): ascending, descending, or as listed.
    pub sort: PivotSort,
    /// When sorted by a value/data field (ECMA-376 `dataField`), the index into `data_fields`.
    pub sort_data_field: Option<usize>,
    /// Grouping of items (日付・数値のグループ化).
    pub grouping: Option<PivotFieldGrouping>,
    /// 各項目の後に空行を入れる (`insertBlankRow`, ECMA-376 Part 1 §18.10.1.69).
    pub insert_blank_row: bool,
    /// Per-field repeat item labels (`x14:pivotField/@fillDownLabels`); `None` inherits `PivotTable::fill_down_labels`.
    pub fill_down_labels: Option<bool>,
}

/// The order of a field's items.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PivotSort {
    #[default]
    Manual,
    Ascending,
    Descending,
}

/// An item of a field (`item`): a shared item (`x`) or a subtotal's place
/// (`t`, as `default`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotItem {
    pub x: Option<u32>,
    /// Hidden by the field's filter (`h`).
    pub hidden: bool,
    /// Its type (`t`): `None` for a data item, else `default`, `sum`, ….
    pub kind: Option<String>,
}

/// A report filter: its field, and the one item it keeps (`item`), or all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotPageField {
    pub field: u32,
    pub item: Option<u32>,
}

/// A value of the table (`dataField`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotDataField {
    /// Its name (合計 / 金額).
    pub name: String,
    /// The field it sums (an index into `fields`).
    pub field: u32,
    /// How (`subtotal`): `sum` (the default), `count`, `average`, `max`,
    /// `min`, `product`, `countNums`, `stdDev`, `stdDevp`, `var`, `varp`.
    pub function: String,
    /// 計算の種類 (`showDataAs`), when not the plain value.
    pub show_as: Option<String>,
    /// Its number format (`numFmtId`, as the format's code).
    pub number_format: Option<String>,
}

/// A line of a table's rows or columns (`rowItems/i`, `colItems/i`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotLine {
    /// Its type (`t`): `None` for a line of data, else `default` (a
    /// subtotal), `grand`, `blank`, `sum`, ….
    pub kind: Option<String>,
    /// How many of the fields' items it repeats from the line before (`r`):
    /// its first item is of the field at that depth.
    pub repeat: u32,
    /// The items of its fields from `repeat` on (`x/@v`, each an index into
    /// the field's `items`).
    pub items: Vec<u32>,
    /// The value it is of, when there are several (`i`).
    pub data: u32,
}

/// A pivot table's style (`pivotTableStyleInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PivotStyleInfo {
    pub name: String,
    pub row_headers: bool,
    pub col_headers: bool,
    pub row_stripes: bool,
    pub col_stripes: bool,
    pub last_column: bool,
}

/// What a pivot button (▼) is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotButton {
    /// 行ラベル: the row fields (one chosen in its panel).
    Rows,
    /// 列ラベル: the column fields.
    Columns,
    /// One field's name (アウトライン・表形式).
    Field(usize),
    /// A report filter (its place among them).
    Page(usize),
}

/// The source's data: its headings and its records (rows), as values.
#[derive(Clone, Debug, Default)]
pub struct PivotData {
    pub headings: Vec<String>,
    pub records: Vec<Vec<CellValue>>,
}

/// What a cell of a built table is (for its format and its style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotRole {
    /// A caption of the header (合計 / 金額, 行ラベル, 列ラベル, a field's
    /// name): Excel gives it a pivot button.
    Caption,
    /// An item of a column field, or a value's name over its column.
    ColumnLabel,
    /// An item of a row field, at its depth in the column it is in.
    RowLabel { indent: u32 },
    /// A subtotal's label (北海道 集計) or 総計.
    TotalLabel,
    /// A value.
    Value,
    /// A report filter's name and what it keeps.
    PageLabel,
    PageValue,
}

/// What a body row (column) of a built table is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotLineKind {
    /// An item of the field at `depth` (0: the first field): a header of
    /// its group when it has fields under it, `subtotal` when its values
    /// are its group's.
    Item { depth: u32, last: bool, subtotal: bool },
    /// A group's subtotal (北海道 集計), of the field at `depth`.
    Subtotal { depth: u32 },
    /// 総計.
    Grand,
    /// 各アイテムの後の空行 (`insertBlankRow`).
    Blank,
}

/// A cell of a built table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotCell {
    pub row: u32,
    pub column: u32,
    pub value: CellValue,
    pub role: PivotRole,
}

/// A table built from its data: its cells, where it is, and what its rows
/// and columns are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotBuilt {
    pub cells: Vec<PivotCell>,
    /// The table's range (without its report filters), as `location`.
    pub location: CellRange,
    pub first_header_row: u32,
    pub first_data_row: u32,
    pub first_data_col: u32,
    /// The report filters' range, above the table.
    pub page_range: Option<CellRange>,
    /// Each body row's kind, top to bottom, and each data column's.
    pub rows: Vec<PivotLineKind>,
    pub columns: Vec<PivotLineKind>,
    /// The columns of the row fields' labels.
    pub label_columns: u32,
    /// The rows' and columns' lines, as the file writes them (`rowItems`,
    /// `colItems`; each item an index among the field's items shown).
    pub row_items: Vec<PivotLine>,
    pub col_items: Vec<PivotLine>,
}

/// Excel's (Japanese) captions, when the table names none.
const ROW_LABELS: &str = "行ラベル";
const COLUMN_LABELS: &str = "列ラベル";
const GRAND_TOTAL: &str = "総計";
const VALUES: &str = "値";
const BLANK_ITEM: &str = "(空白)";
const ALL_ITEMS: &str = "(すべて)";

/// Two values as one item: text without regard to case, as Excel groups them.
fn same_item(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Text(x), CellValue::Text(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => a == b,
    }
}

fn format_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn format_date_grouped(serial: f64, by: DateGroupBy) -> String {
    let (year, month, day) = crate::chart::serial_to_date(serial);
    match by {
        DateGroupBy::Years => format!("{year}"),
        DateGroupBy::Quarters => format!("第{}四半期", (month.saturating_sub(1)) / 3 + 1),
        DateGroupBy::Months => format!("{month}月"),
        DateGroupBy::Days => format!("{year}/{month:02}/{day:02}"),
    }
}

impl PivotField {
    /// Map a raw data value to its grouped bucket label (or itself if not grouped).
    pub fn map_value_to_group(&self, val: &CellValue) -> CellValue {
        let Some(grouping) = &self.grouping else {
            return val.clone();
        };
        match grouping {
            PivotFieldGrouping::Number { start, end, interval, .. } => {
                let n = match val {
                    CellValue::Number(n) => Some(*n),
                    CellValue::Text(t) => t.trim().parse::<f64>().ok(),
                    _ => None,
                };
                let Some(n) = n else {
                    return val.clone();
                };
                if n < *start {
                    CellValue::Text(format!("<{}", format_num(*start)))
                } else if n > *end {
                    CellValue::Text(format!(">{}", format_num(*end)))
                } else {
                    let k = if *interval > 0.0 { ((n - start) / interval).floor() as i64 } else { 0 };
                    let lo = start + (k as f64) * interval;
                    let hi = lo + interval;
                    let hi_incl = if interval.fract() == 0.0 && lo.fract() == 0.0 {
                        (hi - 1.0).min(*end)
                    } else {
                        hi.min(*end)
                    };
                    if hi_incl < lo || lo == hi_incl {
                        CellValue::Text(format_num(lo))
                    } else {
                        CellValue::Text(format!("{}-{}", format_num(lo), format_num(hi_incl)))
                    }
                }
            }
            PivotFieldGrouping::Date { by, start, end, .. } => {
                let serial = match val {
                    CellValue::Number(n) => Some(*n),
                    CellValue::Text(t) => ibis_calc::eval::convert::parse_date_time(t, false),
                    _ => None,
                };
                let Some(serial) = serial else {
                    return val.clone();
                };
                if let Some(s) = start {
                    if serial < *s {
                        return CellValue::Text(format!("<{}", format_date_grouped(*s, *by)));
                    }
                }
                if let Some(e) = end {
                    if serial > *e {
                        return CellValue::Text(format!(">{}", format_date_grouped(*e, *by)));
                    }
                }
                CellValue::Text(format_date_grouped(serial, *by))
            }
            PivotFieldGrouping::Manual { groups } => {
                let key = match val {
                    CellValue::Text(t) => t.as_str(),
                    _ => "",
                };
                if let Some(grp) = groups.get(key) {
                    CellValue::Text(grp.clone())
                } else {
                    val.clone()
                }
            }
        }
    }
}

fn sort_grouped_items(a: &CellValue, b: &CellValue, grouping: &PivotFieldGrouping) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match grouping {
        PivotFieldGrouping::Manual { .. } => compare_values(a, b),
        PivotFieldGrouping::Date { by: DateGroupBy::Months, .. } => {
            let month_num = |v: &CellValue| -> i32 {
                if let CellValue::Text(t) = v {
                    t.trim_end_matches('月').parse::<i32>().unwrap_or(0)
                } else {
                    0
                }
            };
            month_num(a).cmp(&month_num(b))
        }
        PivotFieldGrouping::Date { by: DateGroupBy::Quarters, .. } => {
            let qtr_num = |v: &CellValue| -> i32 {
                if let CellValue::Text(t) = v {
                    if let Some(strip) = t.strip_prefix('第') {
                        strip.trim_end_matches("四半期").parse::<i32>().unwrap_or(0)
                    } else {
                        0
                    }
                } else {
                    0
                }
            };
            qtr_num(a).cmp(&qtr_num(b))
        }
        PivotFieldGrouping::Date { by: DateGroupBy::Years, .. } => {
            let year_num = |v: &CellValue| -> i64 {
                if let CellValue::Text(t) = v {
                    t.trim().parse::<i64>().unwrap_or(0)
                } else {
                    0
                }
            };
            year_num(a).cmp(&year_num(b))
        }
        PivotFieldGrouping::Date { by: DateGroupBy::Days, .. } => compare_values(a, b),
        PivotFieldGrouping::Number { .. } => {
            let rank_and_val = |v: &CellValue| -> (u8, f64) {
                if let CellValue::Text(t) = v {
                    if let Some(rest) = t.strip_prefix('<') {
                        (0, rest.trim().parse::<f64>().unwrap_or(0.0))
                    } else if let Some(rest) = t.strip_prefix('>') {
                        (2, rest.trim().parse::<f64>().unwrap_or(0.0))
                    } else {
                        let lo = t.split('-').next().unwrap_or("0").trim().parse::<f64>().unwrap_or(0.0);
                        (1, lo)
                    }
                } else {
                    (3, 0.0)
                }
            };
            let (r1, v1) = rank_and_val(a);
            let (r2, v2) = rank_and_val(b);
            r1.cmp(&r2).then_with(|| v1.partial_cmp(&v2).unwrap_or(Ordering::Equal))
        }
    }
}

/// The value a blank cell shows for the "for empty cells show" caption: a
/// caption Excel reads as a number ("-", blank, "0", "-0" ...) shows as 0
/// (a negative zero shows as 0, not "-0"); any other text shows as it is.
fn missing_caption_value(c: &str) -> CellValue {
    let t = c.trim();
    if t.is_empty() || t == "-" {
        return CellValue::Number(0.0);
    }
    t.parse::<f64>().map_or_else(|_| CellValue::Text(c.to_string()), |n| CellValue::Number(n + 0.0))
}

fn compare_values(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let rank = |v: &CellValue| match v {
        CellValue::Number(_) => 0,
        CellValue::Text(_) => 1,
        CellValue::Bool(_) => 2,
        CellValue::Error(_) => 3,
        CellValue::Empty => 4,
    };
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (CellValue::Text(x), CellValue::Text(y)) => ibis_calc::text_order::compare_for_sort(x, y),
        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// A row field's place in the tree being laid out.
struct Level {
    field: usize,
    /// The items shown, in order.
    items: Vec<CellValue>,
}

/// A line of the body rows or columns: each with its path of items (indexes per level).
#[derive(Clone, Debug)]
struct Line {
    kind: PivotLineKind,
    path: Vec<usize>,
    /// Labels shown on the row: (level, item) — a tabular parent's on its first child's row.
    labels: Vec<(usize, usize)>,
}

type Agg<'a> = &'a dyn Fn(usize, &[&Vec<CellValue>]) -> CellValue;

#[allow(clippy::too_many_arguments)]
fn tree(
    t: &PivotTable,
    levels: &[Level],
    depth: usize,
    path: &mut Vec<usize>,
    recs: &[&Vec<CellValue>],
    key: &dyn Fn(&Vec<CellValue>, &Level) -> Option<usize>,
    agg: Agg,
    out: &mut Vec<Line>,
    carry: &mut Vec<(usize, usize)>,
) {
    let level = &levels[depth];
    let field = &t.fields[level.field];
    let last = depth + 1 == levels.len();
    // 値フィルター・トップテン: the items kept by what their records add up to, in this group.
    let mut allowed: Vec<bool> = vec![true; level.items.len()];
    for filter in t.filters.iter().filter(|f| f.field as usize == level.field && !f.is_label()) {
        let values: Vec<Option<f64>> = (0..level.items.len())
            .map(|i| {
                let mine: Vec<&Vec<CellValue>> = recs.iter().copied().filter(|r| key(r, level) == Some(i)).collect();
                if mine.is_empty() {
                    return None;
                }
                match agg(filter.measure as usize, &mine) {
                    CellValue::Number(n) => Some(n),
                    _ => None,
                }
            })
            .collect();
        for (a, k) in allowed.iter_mut().zip(filter.keeps_values(&values)) {
            *a = *a && k;
        }
    }
    for (i, _) in level.items.iter().enumerate() {
        let is_calc = t.calculated_items.iter().any(|ci| ci.field as usize == level.field && match &level.items[i] {
            CellValue::Text(name) => ci.name.eq_ignore_ascii_case(name),
            _ => false,
        });
        let mine: Vec<&Vec<CellValue>> = recs.iter().copied().filter(|r| key(r, level) == Some(i)).collect();
        if (mine.is_empty() && !is_calc) || !allowed[i] {
            continue;
        }
        path.push(i);
        if is_calc || last {
            let mut labels = std::mem::take(carry);
            labels.push((depth, i));
            out.push(Line { kind: PivotLineKind::Item { depth: depth as u32, last: true, subtotal: false }, path: path.clone(), labels });
        } else if field.outline {
            // Its own row, with the group's values when subtotals are at the top.
            let top = field.default_subtotal && field.subtotal_top;
            let mut labels = std::mem::take(carry);
            labels.push((depth, i));
            out.push(Line { kind: PivotLineKind::Item { depth: depth as u32, last: false, subtotal: top }, path: path.clone(), labels });
            let header_at = out.len();
            tree(t, levels, depth + 1, path, &mine, key, agg, out, carry);
            // A group none of whose items is kept goes.
            if out.len() == header_at {
                out.pop();
                path.pop();
                continue;
            }
            if field.default_subtotal && !field.subtotal_top {
                out.push(Line { kind: PivotLineKind::Subtotal { depth: depth as u32 }, path: path.clone(), labels: vec![(depth, i)] });
            }
            if field.insert_blank_row {
                out.push(Line { kind: PivotLineKind::Blank, path: path.clone(), labels: Vec::new() });
            }
        } else {
            // 表形式: on its first child's row; its subtotal below.
            let before = out.len();
            carry.push((depth, i));
            tree(t, levels, depth + 1, path, &mine, key, agg, out, carry);
            if out.len() == before {
                carry.pop();
                path.pop();
                continue;
            }
            if field.default_subtotal {
                out.push(Line { kind: PivotLineKind::Subtotal { depth: depth as u32 }, path: path.clone(), labels: vec![(depth, i)] });
            }
            if field.insert_blank_row {
                out.push(Line { kind: PivotLineKind::Blank, path: path.clone(), labels: Vec::new() });
            }
        }
        path.pop();
    }
}

/// The details records and headings for a drill-down (詳細データの展開) of a pivot cell.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotDetails {
    pub headings: Vec<String>,
    pub records: Vec<Vec<CellValue>>,
}

impl PivotTable {
    /// Each row field's label column (from the table's first) and indent:
    /// a compact field shares its column with the compact field before it,
    /// one level further in.
    pub fn row_label_places(&self) -> (Vec<u32>, Vec<u32>) {
        let row_fields: Vec<usize> = self.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let compact = |f: usize| self.fields[f].compact && self.fields[f].outline;
        let mut label_col: Vec<u32> = Vec::new();
        let mut indent: Vec<u32> = Vec::new();
        for (i, &f) in row_fields.iter().enumerate() {
            if i > 0 && compact(f) && compact(row_fields[i - 1]) {
                label_col.push(*label_col.last().unwrap());
                indent.push(indent.last().unwrap() + 1);
            } else {
                label_col.push(label_col.last().map_or(0, |c| c + 1));
                indent.push(0);
            }
        }
        (label_col, indent)
    }

    /// The column of the data holding field `f`: by its name among the
    /// headings, else its place. (Returns None for calculated fields).
    fn data_column(&self, f: usize, data: &PivotData) -> Option<usize> {
        let field = self.fields.get(f)?;
        if field.formula.is_some() {
            return None;
        }
        let name = &field.name;
        data.headings.iter().position(|h| h == name).or_else(|| (f < data.headings.len()).then_some(f))
    }

    /// The items of field `f` shown, in order: the table's own order of its
    /// items, then items new in the data (in the order they come), less the
    /// hidden ones; sorted when the field is.
    fn ordered_items(&self, f: usize, records: &[&Vec<CellValue>], column: Option<usize>, data: &PivotData) -> Vec<CellValue> {
        let field = &self.fields[f];
        if field.formula.is_some() {
            return Vec::new();
        }
        let mut items: Vec<CellValue> = Vec::new();
        let mut hidden: Vec<CellValue> = Vec::new();
        for it in &field.items {
            if !it.hidden {
                let Some(v) = it.x.and_then(|x| field.shared.get(x as usize)) else { continue };
                let mapped = field.map_value_to_group(v);
                if !items.iter().any(|i| same_item(i, &mapped)) {
                    items.push(mapped);
                }
            }
        }
        for it in &field.items {
            if it.hidden {
                let Some(v) = it.x.and_then(|x| field.shared.get(x as usize)) else { continue };
                let mapped = field.map_value_to_group(v);
                if !items.iter().any(|i| same_item(i, &mapped))
                    && !hidden.iter().any(|i| same_item(i, &mapped))
                {
                    hidden.push(mapped);
                }
            }
        }
        let listed = !items.is_empty() || !hidden.is_empty();
        let mut fresh: Vec<CellValue> = Vec::new();
        for r in records {
            let v = column.and_then(|c| r.get(c)).cloned().unwrap_or_default();
            let mapped = field.map_value_to_group(&v);
            if !items.iter().chain(&hidden).chain(&fresh).any(|i| same_item(i, &mapped)) {
                fresh.push(mapped);
            }
        }
        // A field the table lists no items of (a new one) sorts them.
        if !listed || field.grouping.is_some() {
            if let Some(grouping) = &field.grouping {
                fresh.sort_by(|a, b| sort_grouped_items(a, b, grouping));
            } else {
                fresh.sort_by(compare_values);
            }
        }
        items.extend(fresh);
        // Only the items the data has (`showAll` off, as Excel's default).
        items.retain(|i| records.iter().any(|r| {
            let v = column.and_then(|c| r.get(c)).cloned().unwrap_or_default();
            same_item(&field.map_value_to_group(&v), i)
        }));
        if let Some(grouping) = &field.grouping {
            items.sort_by(|a, b| sort_grouped_items(a, b, grouping));
        }
        if let Some(df_idx) = field.sort_data_field {
            if self.data_fields.get(df_idx).is_some() && field.sort != PivotSort::Manual {
                let mut item_values: Vec<(CellValue, f64)> = items
                    .into_iter()
                    .map(|item| {
                        let matching: Vec<&Vec<CellValue>> = records
                            .iter()
                            .copied()
                            .filter(|r| {
                                let v = column.and_then(|c| r.get(c)).cloned().unwrap_or_default();
                                same_item(&field.map_value_to_group(&v), &item)
                            })
                            .collect();
                        let agg = self.aggregate_of(df_idx, data, &matching);
                        let num = match agg {
                            CellValue::Number(n) => n,
                            _ => 0.0,
                        };
                        (item, num)
                    })
                    .collect();

                match field.sort {
                    PivotSort::Ascending => {
                        item_values.sort_by(|(a_it, a_val), (b_it, b_val)| {
                            a_val
                                .partial_cmp(b_val)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| compare_values(a_it, b_it))
                        });
                    }
                    PivotSort::Descending => {
                        item_values.sort_by(|(a_it, a_val), (b_it, b_val)| {
                            b_val
                                .partial_cmp(a_val)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| compare_values(a_it, b_it))
                        });
                    }
                    PivotSort::Manual => {}
                }
                items = item_values.into_iter().map(|(it, _)| it).collect();
            }
        } else {
            match field.sort {
                PivotSort::Ascending => {
                    if let Some(g) = &field.grouping {
                        items.sort_by(|a, b| sort_grouped_items(a, b, g));
                    } else {
                        items.sort_by(compare_values);
                    }
                }
                PivotSort::Descending => {
                    if let Some(g) = &field.grouping {
                        items.sort_by(|a, b| sort_grouped_items(a, b, g));
                        items.reverse();
                    } else {
                        items.sort_by(|a, b| compare_values(b, a));
                    }
                }
                PivotSort::Manual => {}
            }
        }
        // Calculated items of this field
        for ci in self.calculated_items.iter().filter(|ci| ci.field as usize == f) {
            let val = CellValue::Text(ci.name.clone());
            if !items.iter().any(|i| same_item(i, &val)) {
                items.push(val);
            }
        }
        items
    }

    /// Evaluate a calculated item formula over the matching records of other items in the same level.
    fn eval_calculated_item(
        &self,
        ci: &PivotCalculatedItem,
        level: &Level,
        col_recs: &[&Vec<CellValue>],
        d: usize,
        data: &PivotData,
        key: &dyn Fn(&Vec<CellValue>, &Level) -> Option<usize>,
    ) -> CellValue {
        let mut item_values = std::collections::HashMap::new();
        for (item_ix, item_val) in level.items.iter().enumerate() {
            let item_name = crate::sheet_command::display_text(item_val, &Default::default(), false);
            if ci.name.eq_ignore_ascii_case(&item_name) {
                continue;
            }
            let item_recs: Vec<&Vec<CellValue>> = col_recs.iter().copied().filter(|r| key(r, level) == Some(item_ix)).collect();
            let agg = self.aggregate_of(d, data, &item_recs);
            let num = match agg {
                CellValue::Number(n) => n,
                _ => 0.0,
            };
            item_values.insert(item_name.to_lowercase(), num);
        }

        let clean = ci.formula.trim().trim_start_matches('=');
        let Ok(expr) = ibis_calc::formula::parse(clean) else {
            return CellValue::Error(crate::sheet::CellError::Value);
        };
        match eval_calc_expr(&expr, &item_values) {
            Ok(n) => CellValue::Number(n),
            Err(e) => CellValue::Error(e),
        }
    }

    /// Evaluate a calculated field formula over the matching records.
    fn eval_calculated_field(&self, formula: &str, data: &PivotData, recs: &[&Vec<CellValue>]) -> CellValue {
        if recs.is_empty() {
            return CellValue::Empty;
        }
        let mut field_values = std::collections::HashMap::new();
        for (f_idx, f) in self.fields.iter().enumerate() {
            if f.formula.is_none() {
                if let Some(c) = self.data_column(f_idx, data) {
                    let sum: f64 = recs.iter().filter_map(|r| r.get(c)).filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).sum();
                    field_values.insert(f.name.to_lowercase(), sum);
                }
            }
        }
        let clean = formula.trim().trim_start_matches('=');
        let Ok(expr) = ibis_calc::formula::parse(clean) else {
            return CellValue::Error(crate::sheet::CellError::Value);
        };
        match eval_calc_expr(&expr, &field_values) {
            Ok(n) => CellValue::Number(n),
            Err(e) => CellValue::Error(e),
        }
    }

    /// The value `d` of the data fields over `recs`.
    fn aggregate_of(&self, d: usize, data: &PivotData, recs: &[&Vec<CellValue>]) -> CellValue {
        let Some(df) = self.data_fields.get(d) else { return CellValue::Empty };
        let field_idx = df.field as usize;
        let Some(field) = self.fields.get(field_idx) else { return CellValue::Empty };
        if let Some(formula) = &field.formula {
            return self.eval_calculated_field(formula, data, recs);
        }
        let Some(c) = self.data_column(field_idx, data) else { return CellValue::Empty };
        if recs.is_empty() {
            return CellValue::Empty;
        }
        let values: Vec<&CellValue> = recs.iter().filter_map(|r| r.get(c)).collect();
        let numbers: Vec<f64> = values.iter().filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).collect();
        let n = numbers.len() as f64;
        let mean = numbers.iter().sum::<f64>() / n;
        let var = |pop: bool| {
            let k = if pop { n } else { n - 1.0 };
            if k <= 0.0 { None } else { Some(numbers.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / k) }
        };
        let div0 = CellValue::Error(crate::sheet::CellError::Div0);
        match df.function.as_str() {
            "count" => CellValue::Number(values.iter().filter(|v| !matches!(v, CellValue::Empty)).count() as f64),
            "countNums" => CellValue::Number(n),
            "average" => if numbers.is_empty() { div0 } else { CellValue::Number(mean) },
            "max" => CellValue::Number(if numbers.is_empty() { 0.0 } else { numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max) }),
            "min" => CellValue::Number(if numbers.is_empty() { 0.0 } else { numbers.iter().cloned().fold(f64::INFINITY, f64::min) }),
            "product" => CellValue::Number(if numbers.is_empty() { 0.0 } else { numbers.iter().product() }),
            "stdDev" => var(false).map_or(div0.clone(), |v| CellValue::Number(v.sqrt())),
            "stdDevp" => var(true).map_or(div0.clone(), |v| CellValue::Number(v.sqrt())),
            "var" => var(false).map_or(div0.clone(), CellValue::Number),
            "varp" => var(true).map_or(div0, CellValue::Number),
            _ => CellValue::Number(numbers.iter().sum()),
        }
    }

    /// Build the table from `data` (the source's headings and records), at
    /// the place its definition says.
    pub fn build(&self, data: &PivotData) -> PivotBuilt {
        if self.is_empty() {
            let at = CellRange { last_row: self.location.first_row, last_column: self.location.first_column, ..self.location };
            return PivotBuilt { cells: Vec::new(), location: at, first_header_row: 1, first_data_row: 1, first_data_col: 1, page_range: None, rows: Vec::new(), columns: Vec::new(), label_columns: 1, row_items: Vec::new(), col_items: Vec::new() };
        }
        // The records the report filters and hidden items leave.
        let page_value = |p: &PivotPageField| p.item.and_then(|i| {
            let field = self.fields.get(p.field as usize)?;
            let x = field.items.get(i as usize).and_then(|it| it.x).unwrap_or(i);
            field.shared.get(x as usize).cloned()
        });
        let mut records: Vec<&Vec<CellValue>> = data.records.iter().collect();
        for p in &self.page_fields {
            if let (Some(v), Some(c)) = (page_value(p), self.data_column(p.field as usize, data)) {
                records.retain(|r| r.get(c).is_some_and(|x| same_item(x, &v)));
            }
        }
        for filter in self.filters.iter().filter(|f| f.is_label()) {
            if let Some(c) = self.data_column(filter.field as usize, data) {
                let f_idx = filter.field as usize;
                records.retain(|r| {
                    let v = r.get(c).cloned().unwrap_or_default();
                    let mapped = self.fields[f_idx].map_value_to_group(&v);
                    filter.keeps_label(&crate::sheet_command::display_text(&mapped, &Default::default(), false))
                });
            }
        }
        for (f, field) in self.fields.iter().enumerate() {
            let hidden: Vec<&CellValue> = field.items.iter().filter(|i| i.hidden).filter_map(|i| i.x.and_then(|x| field.shared.get(x as usize))).collect();
            if hidden.is_empty() {
                continue;
            }
            if let Some(c) = self.data_column(f, data) {
                records.retain(|r| {
                    let v = r.get(c).cloned().unwrap_or_default();
                    !hidden.iter().any(|h| same_item(&v, h))
                });
            }
        }

        let row_fields: Vec<usize> = self.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let col_fields: Vec<usize> = self.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let data_count = self.data_fields.len().max(1);
        let several = self.data_fields.len() > 1;
        let rows_levels: Vec<Level> = row_fields.iter().map(|&f| Level { field: f, items: self.ordered_items(f, &records, self.data_column(f, data), data) }).collect();
        let col_levels: Vec<Level> = col_fields.iter().map(|&f| Level { field: f, items: self.ordered_items(f, &records, self.data_column(f, data), data) }).collect();
        let key = |r: &Vec<CellValue>, level: &Level| -> Option<usize> {
            let c = self.data_column(level.field, data)?;
            let v = r.get(c).cloned().unwrap_or_default();
            let mapped = self.fields[level.field].map_value_to_group(&v);
            level.items.iter().position(|i| same_item(i, &mapped))
        };

        let (label_col, indent) = self.row_label_places();
        let label_columns = label_col.last().map_or(1, |c| c + 1);
        let mut row_lines: Vec<Line> = Vec::new();
        if rows_levels.is_empty() {
            row_lines.push(Line { kind: PivotLineKind::Item { depth: 0, last: true, subtotal: false }, path: Vec::new(), labels: Vec::new() });
        } else {
            tree(self, &rows_levels, 0, &mut Vec::new(), &records, &key, &|d, r| self.aggregate_of(d, data, r), &mut row_lines, &mut Vec::new());
            if self.row_grand_totals {
                row_lines.push(Line { kind: PivotLineKind::Grand, path: Vec::new(), labels: Vec::new() });
            }
        }
        // The data columns: the column fields' leaves (with subtotals), each
        // value's when there are several; then 総計.
        let mut col_lines: Vec<Line> = Vec::new();
        if !col_levels.is_empty() {
            let mut t2 = self.clone();
            // Column fields always lay their subtotals after their groups.
            for l in &col_levels {
                t2.fields[l.field].outline = true;
                t2.fields[l.field].subtotal_top = false;
            }
            let mut lines = Vec::new();
            tree(&t2, &col_levels, 0, &mut Vec::new(), &records, &key, &|d, r| self.aggregate_of(d, data, r), &mut lines, &mut Vec::new());
            // A group's own item line is not a column (its label goes over its leaves).
            col_lines = lines.into_iter().filter(|l| !matches!(l.kind, PivotLineKind::Item { last: false, .. })).collect();
            if self.col_grand_totals {
                col_lines.push(Line { kind: PivotLineKind::Grand, path: Vec::new(), labels: Vec::new() });
            }
        } else {
            col_lines.push(Line { kind: PivotLineKind::Item { depth: 0, last: true, subtotal: false }, path: Vec::new(), labels: Vec::new() });
        }

        // The header: a row per column level (the values' names one more
        // when there are several), and one row of captions above.
        let col_label_rows = col_levels.len() as u32 + u32::from(several);
        let header_rows = if col_label_rows == 0 { 1 } else { col_label_rows + 1 };
        let pages = self.page_fields.len() as u32;
        // The report filters and a row between them and the table: the
        // table moves down when they have no room above it, as Excel moves it.
        let top = if pages > 0 { self.location.first_row.max(pages + 1) } else { self.location.first_row };
        let left = self.location.first_column;
        let mut cells: Vec<PivotCell> = Vec::new();
        let mut put = |row: u32, column: u32, value: CellValue, role: PivotRole| cells.push(PivotCell { row, column, value, role });
        let item_value = |v: &CellValue| if matches!(v, CellValue::Empty) { CellValue::Text(BLANK_ITEM.into()) } else { v.clone() };

        // Report filters, above with a row between.
        let page_range = (pages > 0).then(|| {
            let first = top.saturating_sub(pages + 1);
            for (i, p) in self.page_fields.iter().enumerate() {
                let name = self.fields.get(p.field as usize).map(|f| f.name.clone()).unwrap_or_default();
                put(first + i as u32, left, CellValue::Text(name), PivotRole::PageLabel);
                let value = page_value(p).map_or(CellValue::Text(ALL_ITEMS.into()), |v| item_value(&v));
                put(first + i as u32, left + 1, value, PivotRole::PageValue);
            }
            CellRange { first_row: first, last_row: first + pages - 1, first_column: left, last_column: left + 1 }
        });

        let first_data_col = left + label_columns;
        let caption = |s: &Option<String>, default: &str| CellValue::Text(s.clone().unwrap_or_else(|| default.to_string()));
        let data_name = |d: usize| CellValue::Text(self.data_fields.get(d).map(|d| d.name.clone()).unwrap_or_default());
        let compact_rows = row_fields.first().is_none_or(|&f| self.fields[f].compact && self.fields[f].outline);
        let label_row = top + header_rows - 1;
        if col_label_rows == 0 {
            // One value and no column fields: 行ラベル | 合計 / 金額.
            if compact_rows {
                put(top, left, caption(&self.row_header_caption, ROW_LABELS), PivotRole::Caption);
            }
            put(top, first_data_col, data_name(0), PivotRole::ColumnLabel);
        } else {
            if !several {
                put(top, left, data_name(0), PivotRole::Caption);
            }
            if compact_rows {
                put(label_row, left, caption(&self.row_header_caption, ROW_LABELS), PivotRole::Caption);
            }
            let col_compact = col_fields.first().is_none_or(|&f| self.fields[f].compact && self.fields[f].outline);
            if col_levels.is_empty() {
                put(top, first_data_col, caption(&self.data_caption, VALUES), PivotRole::Caption);
            } else if col_compact {
                put(top, first_data_col, caption(&self.col_header_caption, COLUMN_LABELS), PivotRole::Caption);
            } else {
                for (i, l) in col_levels.iter().enumerate() {
                    put(top, first_data_col + i as u32, CellValue::Text(self.fields[l.field].name.clone()), PivotRole::Caption);
                }
            }
        }
        if !compact_rows {
            for (i, &f) in row_fields.iter().enumerate() {
                put(label_row, left + label_col[i], CellValue::Text(self.fields[f].name.clone()), PivotRole::Caption);
            }
        }

        // The column labels.
        let mut columns: Vec<PivotLineKind> = Vec::new();
        let mut col_keys: Vec<(Option<&Line>, usize)> = Vec::new();
        for line in &col_lines {
            for d in 0..data_count {
                col_keys.push((Some(line), d));
                columns.push(line.kind);
            }
        }
        let mut seen: Vec<Vec<usize>> = vec![Vec::new(); col_levels.len()];
        for (i, (line, d)) in col_keys.iter().enumerate() {
            let column = first_data_col + i as u32;
            let Some(line) = line else { continue };
            match line.kind {
                PivotLineKind::Grand if !col_levels.is_empty() => {
                    let label = if several { CellValue::Text(format!("{} の合計", self.data_fields[*d].name)) } else { caption(&self.grand_total_caption, GRAND_TOTAL) };
                    if *d == 0 || several {
                        put(top + 1, column, label, PivotRole::ColumnLabel);
                    }
                }
                PivotLineKind::Subtotal { depth } => {
                    if *d == 0 {
                        let item = &col_levels[depth as usize].items[line.path[depth as usize]];
                        put(top + 1 + depth, column, CellValue::Text(format!("{} 集計", crate::sheet_command::display_text(item, &Default::default(), false))), PivotRole::ColumnLabel);
                    }
                }
                _ => {
                    // Each level's item over the first column of its group.
                    for (lv, &ix) in line.path.iter().enumerate() {
                        let prefix: Vec<usize> = line.path[..=lv].to_vec();
                        let key_here = prefix.iter().fold(0usize, |a, &b| a.wrapping_mul(1009).wrapping_add(b + 1));
                        if *d == 0 && !seen[lv].contains(&key_here) {
                            seen[lv].push(key_here);
                            put(top + 1 + lv as u32, column, item_value(&col_levels[lv].items[ix]), PivotRole::ColumnLabel);
                        }
                    }
                }
            }
            if several && (col_levels.is_empty() || !matches!(line.kind, PivotLineKind::Grand)) {
                put(label_row, column, data_name(*d), PivotRole::ColumnLabel);
            }
        }

        // The body.
        let aggregate = |d: usize, recs: &[&&Vec<CellValue>]| -> CellValue {
            let plain: Vec<&Vec<CellValue>> = recs.iter().map(|r| **r).collect();
            self.aggregate_of(d, data, &plain)
        };
        // With a value filter, the totals add up the items kept only.
        let value_filtered = self.filters.iter().any(|f| !f.is_label());
        let leaf_paths = |lines: &[Line]| -> Vec<Vec<usize>> { lines.iter().filter(|l| matches!(l.kind, PivotLineKind::Item { last: true, .. })).map(|l| l.path.clone()).collect() };
        let (row_leafs, col_leafs) = (leaf_paths(&row_lines), leaf_paths(&col_lines));
        let shown = |r: &&&Vec<CellValue>| -> bool {
            !value_filtered
                || (rows_levels.is_empty() || row_leafs.iter().any(|p| p.iter().enumerate().all(|(lv, &ix)| key(r, &rows_levels[lv]) == Some(ix))))
                    && (col_levels.is_empty() || col_leafs.iter().any(|p| p.iter().enumerate().all(|(lv, &ix)| key(r, &col_levels[lv]) == Some(ix))))
        };
        let all_shown: Vec<&&Vec<CellValue>> = records.iter().filter(|r| shown(r)).collect();
        let grand_totals: Vec<Option<f64>> = (0..data_count)
            .map(|d| match aggregate(d, &all_shown) {
                CellValue::Number(n) => Some(n),
                _ => None,
            })
            .collect();
        let col_totals: Vec<Option<f64>> = col_keys
            .iter()
            .map(|(cline, d)| {
                let col_recs: Vec<&&Vec<CellValue>> = match cline {
                    Some(cl) => all_shown.iter().copied().filter(|r| cl.path.iter().enumerate().all(|(lv, &ix)| key(r, &col_levels[lv]) == Some(ix))).collect(),
                    None => all_shown.clone(),
                };
                match aggregate(*d, &col_recs) {
                    CellValue::Number(n) => Some(n),
                    _ => None,
                }
            })
            .collect();
        let first_body = top + header_rows;
        let mut rows: Vec<PivotLineKind> = Vec::new();
        for (ri, line) in row_lines.iter().enumerate() {
            let row = first_body + ri as u32;
            rows.push(line.kind);
            match line.kind {
                PivotLineKind::Grand => put(row, left, caption(&self.grand_total_caption, GRAND_TOTAL), PivotRole::TotalLabel),
                PivotLineKind::Subtotal { depth } => {
                    let item = &rows_levels[depth as usize].items[line.path[depth as usize]];
                    let text = format!("{} 集計", crate::sheet_command::display_text(item, &Default::default(), false));
                    put(row, left + label_col[depth as usize], CellValue::Text(text), PivotRole::TotalLabel);
                }
                PivotLineKind::Item { .. } => {
                    let mut labels = line.labels.clone();
                    // 項目ラベルを繰り返す: the outer items again on the rows of their group, where each field has its own column.
                    for lv in 0..line.path.len().min(rows_levels.len()) {
                        let field = &self.fields[rows_levels[lv].field];
                        let fill_down = field.fill_down_labels.unwrap_or(self.fill_down_labels);
                        if fill_down && !labels.iter().any(|&(l, _)| l == lv) && !(field.compact && field.outline) {
                            labels.push((lv, line.path[lv]));
                        }
                    }
                    for (lv, ix) in labels {
                        put(row, left + label_col[lv], item_value(&rows_levels[lv].items[ix]), PivotRole::RowLabel { indent: indent[lv] });
                    }
                }
                PivotLineKind::Blank => {}
            }
            let values_shown = !matches!(line.kind, PivotLineKind::Item { last: false, subtotal: false, .. } | PivotLineKind::Blank);
            if !values_shown {
                continue;
            }
            let row_recs: Vec<&&Vec<CellValue>> = records.iter().filter(|r| shown(r) && line.path.iter().enumerate().all(|(lv, &ix)| key(r, &rows_levels[lv]) == Some(ix))).collect();
            let row_totals: Vec<Option<f64>> = (0..data_count)
                .map(|d| match aggregate(d, &row_recs) {
                    CellValue::Number(n) => Some(n),
                    _ => None,
                })
                .collect();
            let calc_item: Option<(&PivotCalculatedItem, usize)> = rows_levels.iter().enumerate().find_map(|(lv, l)| {
                let ix = line.path.get(lv).copied()?;
                let item_val = l.items.get(ix)?;
                if let CellValue::Text(item_name) = item_val {
                    self.calculated_items.iter().find(|ci| ci.field as usize == l.field && ci.name.eq_ignore_ascii_case(item_name)).map(|ci| (ci, lv))
                } else {
                    None
                }
            });

            for (ci, (cline, d)) in col_keys.iter().enumerate() {
                let recs: Vec<&&Vec<CellValue>> = match cline {
                    Some(cl) => row_recs.iter().copied().filter(|r| cl.path.iter().enumerate().all(|(lv, &ix)| key(r, &col_levels[lv]) == Some(ix))).collect(),
                    None => row_recs.clone(),
                };
                let mut v = if let Some((ci_def, lv)) = calc_item {
                    let parent_col_recs: Vec<&Vec<CellValue>> = records.iter().copied().filter(|r| {
                        line.path[..lv].iter().enumerate().all(|(l, &ix)| key(r, &rows_levels[l]) == Some(ix))
                            && cline.map_or(true, |cl| cl.path.iter().enumerate().all(|(l, &ix)| key(r, &col_levels[l]) == Some(ix)))
                    }).collect();
                    self.eval_calculated_item(ci_def, &rows_levels[lv], &parent_col_recs, *d, data, &key)
                } else {
                    aggregate(*d, &recs)
                };
                if let Some(show_as) = self.data_fields.get(*d).and_then(|df| df.show_as.as_deref()) {
                    if let CellValue::Number(num) = v {
                        let denominator = match show_as {
                            "percentOfTotal" => grand_totals.get(*d).copied().flatten(),
                            "percentOfCol" => col_totals.get(ci).copied().flatten(),
                            "percentOfRow" => row_totals.get(*d).copied().flatten(),
                            _ => None,
                        };
                        v = match denominator {
                            Some(den) if den != 0.0 => CellValue::Number(num / den),
                            Some(_) => CellValue::Error(crate::sheet::CellError::Div0),
                            None => v,
                        };
                    }
                }
                // 空白セルに表示する値、エラー値に表示する値.
                match (&v, &self.missing_caption, &self.error_caption) {
                    // Excel reads a caption that is a number, blank or "-" as 0 (measured on its output: "-", "0", " ", "-0" show 0; "x", "--", "N/A" show 0).
                    (CellValue::Empty, Some(c), _) => {
                        v = missing_caption_value(c);
                    }
                    (CellValue::Empty, None, _) if self.missing_zero => v = CellValue::Number(0.0),
                    (CellValue::Error(_), _, Some(c)) => v = CellValue::Text(c.clone()),
                    _ => {}
                }
                put(row, first_data_col + ci as u32, v, PivotRole::Value);
            }
        }
        // The lines as the file writes them: what each repeats of the one before.
        let as_items = |lines: &[Line], several: bool| -> Vec<PivotLine> {
            let mut prev: Vec<usize> = Vec::new();
            let mut out = Vec::new();
            for l in lines {
                let repeat = prev.iter().zip(&l.path).take_while(|(a, b)| a == b).count();
                let (kind, repeat, items) = match l.kind {
                    PivotLineKind::Grand => (Some("grand".to_string()), 0, vec![0]),
                    PivotLineKind::Subtotal { .. } => (Some("default".to_string()), 0, l.path.iter().map(|&i| i as u32).collect()),
                    PivotLineKind::Item { .. } => (None, repeat as u32, l.path[repeat.min(l.path.len())..].iter().map(|&i| i as u32).collect()),
                    PivotLineKind::Blank => (Some("blank".to_string()), 0, vec![0]),
                };
                prev = l.path.clone();
                let n = if several { self.data_fields.len().max(1) } else { 1 };
                for d in 0..n {
                    out.push(PivotLine { kind: kind.clone(), repeat, items: items.clone(), data: d as u32 });
                }
            }
            out
        };
        let row_items = as_items(&row_lines, false);
        let col_items = as_items(&col_lines, several);
        let last_row = first_body + row_lines.len() as u32 - 1;
        let last_column = first_data_col + col_keys.len() as u32 - 1;
        PivotBuilt {
            cells,
            location: CellRange { first_row: top, last_row, first_column: left, last_column },
            first_header_row: 1,
            first_data_row: header_rows,
            first_data_col: label_columns,
            page_range,
            rows,
            columns,
            label_columns,
            row_items,
            col_items,
        }
    }

    /// The source records contributing to the body cell at (`row`, `column`).
    /// Returns `None` if the cell is outside the data area or has no contributing records.
    pub fn details_for_cell(&self, data: &PivotData, row: u32, column: u32) -> Option<PivotDetails> {
        if self.is_empty() {
            return None;
        }

        let page_value = |p: &PivotPageField| p.item.and_then(|i| {
            let field = self.fields.get(p.field as usize)?;
            let x = field.items.get(i as usize).and_then(|it| it.x).unwrap_or(i);
            field.shared.get(x as usize).cloned()
        });
        let mut records: Vec<&Vec<CellValue>> = data.records.iter().collect();
        for p in &self.page_fields {
            if let (Some(v), Some(c)) = (page_value(p), self.data_column(p.field as usize, data)) {
                records.retain(|r| r.get(c).is_some_and(|x| same_item(x, &v)));
            }
        }
        for filter in self.filters.iter().filter(|f| f.is_label()) {
            if let Some(c) = self.data_column(filter.field as usize, data) {
                let f_idx = filter.field as usize;
                records.retain(|r| {
                    let v = r.get(c).cloned().unwrap_or_default();
                    let mapped = self.fields[f_idx].map_value_to_group(&v);
                    filter.keeps_label(&crate::sheet_command::display_text(&mapped, &Default::default(), false))
                });
            }
        }
        for (f, field) in self.fields.iter().enumerate() {
            let hidden: Vec<&CellValue> = field.items.iter().filter(|i| i.hidden).filter_map(|i| i.x.and_then(|x| field.shared.get(x as usize))).collect();
            if hidden.is_empty() {
                continue;
            }
            if let Some(c) = self.data_column(f, data) {
                records.retain(|r| {
                    let v = r.get(c).cloned().unwrap_or_default();
                    !hidden.iter().any(|h| same_item(&v, h))
                });
            }
        }

        let row_fields: Vec<usize> = self.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let col_fields: Vec<usize> = self.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let data_count = self.data_fields.len().max(1);
        let several = self.data_fields.len() > 1;
        let rows_levels: Vec<Level> = row_fields.iter().map(|&f| Level { field: f, items: self.ordered_items(f, &records, self.data_column(f, data), data) }).collect();
        let col_levels: Vec<Level> = col_fields.iter().map(|&f| Level { field: f, items: self.ordered_items(f, &records, self.data_column(f, data), data) }).collect();
        let key = |r: &Vec<CellValue>, level: &Level| -> Option<usize> {
            let c = self.data_column(level.field, data)?;
            let v = r.get(c).cloned().unwrap_or_default();
            let mapped = self.fields[level.field].map_value_to_group(&v);
            level.items.iter().position(|i| same_item(i, &mapped))
        };

        let (label_col, _indent) = self.row_label_places();
        let label_columns = label_col.last().map_or(1, |c| c + 1);

        let mut row_lines: Vec<Line> = Vec::new();
        if rows_levels.is_empty() {
            row_lines.push(Line { kind: PivotLineKind::Item { depth: 0, last: true, subtotal: false }, path: Vec::new(), labels: Vec::new() });
        } else {
            tree(self, &rows_levels, 0, &mut Vec::new(), &records, &key, &|d, r| self.aggregate_of(d, data, r), &mut row_lines, &mut Vec::new());
            if self.row_grand_totals {
                row_lines.push(Line { kind: PivotLineKind::Grand, path: Vec::new(), labels: Vec::new() });
            }
        }

        let mut col_lines: Vec<Line> = Vec::new();
        if !col_levels.is_empty() {
            let mut t2 = self.clone();
            for l in &col_levels {
                t2.fields[l.field].outline = true;
                t2.fields[l.field].subtotal_top = false;
            }
            let mut lines = Vec::new();
            tree(&t2, &col_levels, 0, &mut Vec::new(), &records, &key, &|d, r| self.aggregate_of(d, data, r), &mut lines, &mut Vec::new());
            col_lines = lines.into_iter().filter(|l| !matches!(l.kind, PivotLineKind::Item { last: false, .. })).collect();
            if self.col_grand_totals {
                col_lines.push(Line { kind: PivotLineKind::Grand, path: Vec::new(), labels: Vec::new() });
            }
        } else {
            col_lines.push(Line { kind: PivotLineKind::Item { depth: 0, last: true, subtotal: false }, path: Vec::new(), labels: Vec::new() });
        }

        let col_label_rows = col_levels.len() as u32 + u32::from(several);
        let header_rows = if col_label_rows == 0 { 1 } else { col_label_rows + 1 };
        let pages = self.page_fields.len() as u32;
        let top = if pages > 0 { self.location.first_row.max(pages + 1) } else { self.location.first_row };
        let left = self.location.first_column;

        let first_data_col = left + label_columns;
        let first_body = top + header_rows;

        if row < first_body || column < first_data_col {
            return None;
        }
        let ri = (row - first_body) as usize;
        let row_line = row_lines.get(ri)?;
        let values_shown = !matches!(row_line.kind, PivotLineKind::Item { last: false, subtotal: false, .. } | PivotLineKind::Blank);
        if !values_shown {
            return None;
        }

        let mut col_keys: Vec<(Option<&Line>, usize)> = Vec::new();
        for line in &col_lines {
            for d in 0..data_count {
                col_keys.push((Some(line), d));
            }
        }
        let ci = (column - first_data_col) as usize;
        let (cline, _d) = col_keys.get(ci)?;

        let value_filtered = self.filters.iter().any(|f| !f.is_label());
        let leaf_paths = |lines: &[Line]| -> Vec<Vec<usize>> { lines.iter().filter(|l| matches!(l.kind, PivotLineKind::Item { last: true, .. })).map(|l| l.path.clone()).collect() };
        let (row_leafs, col_leafs) = (leaf_paths(&row_lines), leaf_paths(&col_lines));
        let shown = |r: &&&Vec<CellValue>| -> bool {
            !value_filtered
                || (rows_levels.is_empty() || row_leafs.iter().any(|p| p.iter().enumerate().all(|(lv, &ix)| key(r, &rows_levels[lv]) == Some(ix))))
                    && (col_levels.is_empty() || col_leafs.iter().any(|p| p.iter().enumerate().all(|(lv, &ix)| key(r, &col_levels[lv]) == Some(ix))))
        };

        let row_recs: Vec<&&Vec<CellValue>> = records.iter().filter(|r| shown(r) && row_line.path.iter().enumerate().all(|(lv, &ix)| key(r, &rows_levels[lv]) == Some(ix))).collect();
        let recs: Vec<Vec<CellValue>> = match cline {
            Some(cl) => row_recs.into_iter().filter(|r| cl.path.iter().enumerate().all(|(lv, &ix)| key(r, &col_levels[lv]) == Some(ix))).map(|r| (*r).clone()).collect(),
            None => row_recs.into_iter().map(|r| (*r).clone()).collect(),
        };

        Some(PivotDetails {
            headings: data.headings.clone(),
            records: recs,
        })
    }

    /// Whether cell `(row, column)` lies within this pivot table's bounds
    /// (including report filters above the table).
    pub fn contains_cell(&self, row: u32, column: u32) -> bool {
        let top = if !self.page_fields.is_empty() {
            self.location.first_row.saturating_sub(self.page_fields.len() as u32 + 1)
        } else {
            self.location.first_row
        };
        (top..=self.location.last_row).contains(&row)
            && (self.location.first_column..=self.location.last_column).contains(&column)
    }

    /// Retrieve a data cell value according to Excel's GETPIVOTDATA rules.
    pub fn get_pivot_data(
        &self,
        sheet: &crate::sheet::Sheet,
        data_field: &str,
        filter_pairs: &[(&str, &str)],
    ) -> Option<CellValue> {
        if self.is_empty() || self.data_fields.is_empty() {
            return None;
        }

        let norm = |s: &str| -> String {
            s.chars().filter(|c| !c.is_whitespace() && *c != '/' && *c != ':').collect::<String>().to_lowercase()
        };

        // 1. Identify the data field
        let d_idx = if data_field.is_empty() {
            if self.data_fields.len() == 1 {
                Some(0)
            } else {
                None
            }
        } else {
            self.data_fields.iter().position(|df| {
                df.name.eq_ignore_ascii_case(data_field)
                    || norm(&df.name) == norm(data_field)
                    || self.fields.get(df.field as usize).is_some_and(|f| {
                        f.name.eq_ignore_ascii_case(data_field) || norm(&f.name) == norm(data_field)
                    })
            })
        };
        let d_idx = d_idx?;

        // 2. Validate all query fields exist in the pivot table
        for &(fname, _) in filter_pairs {
            if !self.fields.iter().any(|f| f.name.eq_ignore_ascii_case(fname) || norm(&f.name) == norm(fname)) {
                return None;
            }
        }

        // 3. Check page fields (report filters)
        for p in &self.page_fields {
            let Some(field) = self.fields.get(p.field as usize) else { continue };
            if let Some((_, query_item)) = filter_pairs.iter().find(|(fname, _)| field.name.eq_ignore_ascii_case(fname) || norm(&field.name) == norm(fname)) {
                if let Some(item_idx) = p.item {
                    if !field_item_matches(field, item_idx, query_item) {
                        return None;
                    }
                } else {
                    let max_items = field.shared.len().max(field.items.len()) as u32;
                    let exists = (0..max_items).any(|ix| field_item_matches(field, ix, query_item));
                    if !exists {
                        return None;
                    }
                }
            }
        }

        let row_fields_clean: Vec<usize> = self.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();
        let col_fields_clean: Vec<usize> = self.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).filter(|&f| f < self.fields.len()).collect();

        // 4. Verify no query field is outside rows, columns, or pages
        for &(fname, _) in filter_pairs {
            let is_row = row_fields_clean.iter().any(|&f| self.fields[f].name.eq_ignore_ascii_case(fname) || norm(&self.fields[f].name) == norm(fname));
            let is_col = col_fields_clean.iter().any(|&f| self.fields[f].name.eq_ignore_ascii_case(fname) || norm(&self.fields[f].name) == norm(fname));
            let is_page = self.page_fields.iter().any(|p| self.fields.get(p.field as usize).is_some_and(|f| f.name.eq_ignore_ascii_case(fname) || norm(&f.name) == norm(fname)));
            if !is_row && !is_col && !is_page {
                return None;
            }
        }

        // 5. Build/use row and column lines
        let (row_items_to_use, col_items_to_use, first_data_row, first_data_col, built_cells) = if self.row_items.is_empty() && self.source.range.is_some() {
            let range = self.source.range.unwrap();
            let data = source_data(sheet, range);
            let built = self.build(&data);
            (built.row_items, built.col_items, built.first_data_row, built.first_data_col, Some(built.cells))
        } else {
            (self.row_items.clone(), self.col_items.clone(), self.first_data_row, self.first_data_col, None)
        };

        let data_count = self.data_fields.len().max(1);
        let mut row_paths: Vec<(PivotLineKind, Vec<u32>, usize)> = Vec::new();
        if row_items_to_use.is_empty() {
            if row_fields_clean.is_empty() {
                let n = if self.data_on_rows { data_count } else { 1 };
                for d in 0..n {
                    row_paths.push((PivotLineKind::Grand, Vec::new(), d));
                }
            }
        } else {
            let mut current: Vec<u32> = Vec::new();
            for line in &row_items_to_use {
                let kind = match line.kind.as_deref() {
                    Some("grand") => PivotLineKind::Grand,
                    Some("blank") => PivotLineKind::Blank,
                    Some("default") | Some("subtotal") => PivotLineKind::Subtotal { depth: line.repeat },
                    _ => PivotLineKind::Item { depth: line.repeat, last: true, subtotal: false },
                };
                current.truncate(line.repeat as usize);
                current.extend_from_slice(&line.items);
                row_paths.push((kind, current.clone(), line.data as usize));
            }
        }

        let mut col_paths: Vec<(PivotLineKind, Vec<u32>, usize)> = Vec::new();
        if col_items_to_use.is_empty() {
            if col_fields_clean.is_empty() {
                let n = if !self.data_on_rows { data_count } else { 1 };
                for d in 0..n {
                    col_paths.push((PivotLineKind::Grand, Vec::new(), d));
                }
            }
        } else {
            let mut current: Vec<u32> = Vec::new();
            for line in &col_items_to_use {
                let kind = match line.kind.as_deref() {
                    Some("grand") => PivotLineKind::Grand,
                    Some("blank") => PivotLineKind::Blank,
                    Some("default") | Some("subtotal") => PivotLineKind::Subtotal { depth: line.repeat },
                    _ => PivotLineKind::Item { depth: line.repeat, last: true, subtotal: false },
                };
                current.truncate(line.repeat as usize);
                current.extend_from_slice(&line.items);
                col_paths.push((kind, current.clone(), line.data as usize));
            }
        }

        // 6. Match row line
        let mut specified_row_levels: Vec<(usize, &str)> = Vec::new();
        for (lv, &f_idx) in row_fields_clean.iter().enumerate() {
            if let Some((_, it)) = filter_pairs.iter().find(|(name, _)| self.fields[f_idx].name.eq_ignore_ascii_case(name) || norm(&self.fields[f_idx].name) == norm(name)) {
                specified_row_levels.push((lv, *it));
            }
        }

        let matching_ri = if row_fields_clean.is_empty() {
            if self.data_on_rows {
                row_paths.iter().position(|(_, _, d)| *d == d_idx)
            } else {
                Some(0)
            }
        } else if specified_row_levels.is_empty() {
            row_paths.iter().position(|(kind, _, d)| matches!(kind, PivotLineKind::Grand) && (!self.data_on_rows || *d == d_idx))
        } else {
            let deepest_specified = specified_row_levels.iter().map(|&(lv, _)| lv).max().unwrap_or(0);
            let all_levels_specified = deepest_specified + 1 >= row_fields_clean.len();
            row_paths.iter().position(|(kind, path, d)| {
                if self.data_on_rows && *d != d_idx {
                    return false;
                }
                if matches!(kind, PivotLineKind::Grand | PivotLineKind::Blank) {
                    return false;
                }
                for &(lv, query_item) in &specified_row_levels {
                    let Some(&item_idx) = path.get(lv) else { return false; };
                    let f_idx = row_fields_clean[lv];
                    if !field_item_matches(&self.fields[f_idx], item_idx, query_item) {
                        return false;
                    }
                }
                if all_levels_specified {
                    !matches!(kind, PivotLineKind::Subtotal { .. }) && path.len() >= row_fields_clean.len()
                } else {
                    matches!(kind, PivotLineKind::Subtotal { .. }) || path.len() == deepest_specified + 1
                }
            })
        };
        let matching_ri = matching_ri?;

        // 7. Match col line
        let mut specified_col_levels: Vec<(usize, &str)> = Vec::new();
        for (lv, &f_idx) in col_fields_clean.iter().enumerate() {
            if let Some((_, it)) = filter_pairs.iter().find(|(name, _)| self.fields[f_idx].name.eq_ignore_ascii_case(name) || norm(&self.fields[f_idx].name) == norm(name)) {
                specified_col_levels.push((lv, *it));
            }
        }

        let matching_ci = if col_fields_clean.is_empty() {
            if !self.data_on_rows {
                col_paths.iter().position(|(_, _, d)| *d == d_idx)
            } else {
                Some(0)
            }
        } else if specified_col_levels.is_empty() {
            col_paths.iter().position(|(kind, _, d)| matches!(kind, PivotLineKind::Grand) && (self.data_on_rows || *d == d_idx))
        } else {
            let deepest_specified = specified_col_levels.iter().map(|&(lv, _)| lv).max().unwrap_or(0);
            let all_levels_specified = deepest_specified + 1 >= col_fields_clean.len();
            col_paths.iter().position(|(kind, path, d)| {
                if !self.data_on_rows && *d != d_idx {
                    return false;
                }
                if matches!(kind, PivotLineKind::Grand | PivotLineKind::Blank) {
                    return false;
                }
                for &(lv, query_item) in &specified_col_levels {
                    let Some(&item_idx) = path.get(lv) else { return false; };
                    let f_idx = col_fields_clean[lv];
                    if !field_item_matches(&self.fields[f_idx], item_idx, query_item) {
                        return false;
                    }
                }
                if all_levels_specified {
                    !matches!(kind, PivotLineKind::Subtotal { .. }) && path.len() >= col_fields_clean.len()
                } else {
                    matches!(kind, PivotLineKind::Subtotal { .. }) || path.len() == deepest_specified + 1
                }
            })
        };
        let matching_ci = matching_ci?;

        // 8. Find target cell coordinates
        let target_row = self.location.first_row + first_data_row + matching_ri as u32;
        let target_col = self.location.first_column + first_data_col + matching_ci as u32;

        if let Some(cell) = sheet.cell(target_row, target_col) {
            if !cell.value.is_empty() {
                return Some(cell.value.clone());
            }
        }

        if let Some(cells) = &built_cells {
            if let Some(c) = cells.iter().find(|c| c.row == target_row && c.column == target_col) {
                return Some(c.value.clone());
            }
        }

        if let Some(range) = self.source.range {
            let data = source_data(sheet, range);
            let built = self.build(&data);
            if let Some(c) = built.cells.iter().find(|c| c.row == target_row && c.column == target_col) {
                return Some(c.value.clone());
            }
        }

        None
    }
}

fn field_item_matches(field: &PivotField, item_idx: u32, query_item: &str) -> bool {
    let raw_val = if !field.items.is_empty() {
        field.items.get(item_idx as usize).and_then(|it| {
            if let Some(x) = it.x {
                field.shared.get(x as usize)
            } else {
                field.shared.get(item_idx as usize)
            }
        })
    } else {
        field.shared.get(item_idx as usize)
    };
    let Some(v) = raw_val else { return false; };
    let mapped = field.map_value_to_group(v);
    cell_value_matches(&mapped, query_item) || cell_value_matches(v, query_item)
}

fn cell_value_matches(val: &CellValue, query: &str) -> bool {
    let norm = |s: &str| -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
    };
    match val {
        CellValue::Text(t) => t.eq_ignore_ascii_case(query) || norm(t) == norm(query),
        CellValue::Number(n) => {
            if format_num(*n) == query {
                true
            } else if let Ok(q_num) = query.parse::<f64>() {
                (*n - q_num).abs() < 1e-9
            } else if let Some(q_date) = ibis_calc::eval::convert::parse_date_time(query, false) {
                (*n - q_date).abs() < 1e-5
            } else {
                false
            }
        }
        CellValue::Bool(b) => {
            if query.eq_ignore_ascii_case("true") {
                *b
            } else if query.eq_ignore_ascii_case("false") {
                !*b
            } else {
                false
            }
        }
        CellValue::Empty => query.is_empty() || query == BLANK_ITEM,
        CellValue::Error(_) => false,
    }
}

/// The data of `range` on `sheet`: its first row the headings, the rest the
/// records.
pub fn source_data(sheet: &crate::sheet::Sheet, range: CellRange) -> PivotData {
    let width = (range.last_column - range.first_column + 1) as usize;
    let mut grid: Vec<Vec<CellValue>> = vec![vec![CellValue::Empty; width]; (range.last_row - range.first_row + 1) as usize];
    for c in &sheet.cells {
        if (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column) {
            grid[(c.row - range.first_row) as usize][(c.column - range.first_column) as usize] = c.value.clone();
        }
    }
    let mut rows = grid.into_iter();
    let headings = rows.next().unwrap_or_default().into_iter().map(|v| crate::sheet_command::display_text(&v, &Default::default(), false)).collect();
    PivotData { headings, records: rows.collect() }
}

impl PivotTable {
    /// The kinds of the body's rows and columns as the file lays them out
    /// (`rowItems`, `colItems`).
    pub fn line_kinds(&self) -> (Vec<PivotLineKind>, Vec<PivotLineKind>) {
        let kinds = |lines: &[PivotLine], fields: &[i32]| -> Vec<PivotLineKind> {
            let fields: Vec<usize> = fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect();
            let deepest = fields.len().saturating_sub(1) as u32;
            lines
                .iter()
                .map(|l| match l.kind.as_deref() {
                    Some("grand") => PivotLineKind::Grand,
                    Some("blank") => PivotLineKind::Blank,
                    None => {
                        let depth = (l.repeat + l.items.len().max(1) as u32 - 1).min(deepest);
                        let subtotal = depth < deepest && fields.get(depth as usize).and_then(|&f| self.fields.get(f)).is_some_and(|f| f.outline && f.default_subtotal && f.subtotal_top);
                        PivotLineKind::Item { depth, last: depth == deepest, subtotal }
                    }
                    Some(_) => PivotLineKind::Subtotal { depth: l.repeat },
                })
                .collect()
        };
        (kinds(&self.row_items, &self.row_fields), kinds(&self.col_items, &self.col_fields))
    }

    /// How far Excel draws each row label in from its cell's left, in
    /// points, as (row, column, points): in a column that holds a field with
    /// fields under it, 12 points (the room of its +/− buttons) and 13 more
    /// a level in (measured on Excel's PDFs of XPVT-02, 03 and 07: the
    /// labels 12 and 25 points in),
    /// in place of the cell's own indent. Labels of 小計 and 総計 are not.
    pub fn label_pads(&self) -> Vec<(u32, u32, f32)> {
        let (rows, _) = self.line_kinds();
        let (label_col, indent) = self.row_label_places();
        let fields = label_col.len();
        if fields == 0 {
            return Vec::new();
        }
        // Columns holding a field that is not the last.
        let parent_cols: Vec<u32> = (0..fields.saturating_sub(1)).map(|i| label_col[i]).collect();
        let first_body = self.location.first_row + self.first_data_row;
        let mut out = Vec::new();
        for (i, kind) in rows.iter().enumerate() {
            let PivotLineKind::Item { depth, .. } = *kind else { continue };
            let line = &self.row_items[i.min(self.row_items.len().saturating_sub(1))];
            let from = if self.row_items.is_empty() { depth } else { line.repeat.min(depth) };
            for d in from..=depth {
                let d = d as usize;
                let col = label_col[d];
                if !parent_cols.contains(&col) {
                    continue;
                }
                out.push((first_body + i as u32, self.location.first_column + col, 12.0 + 13.0 * indent[d] as f32));
            }
        }
        out
    }

    /// The format a built cell takes, from the sheet's default `base`:
    /// labels to the left (an item indented by its depth), values in their
    /// value's number format.
    pub fn cell_style(&self, cell: &PivotCell, base: &crate::sheet::CellStyle) -> crate::sheet::CellStyle {
        let mut s = base.clone();
        match cell.role {
            PivotRole::RowLabel { indent } => {
                s.align = crate::sheet::CellAlign::Left;
                s.indent = indent;
            }
            PivotRole::TotalLabel => s.align = crate::sheet::CellAlign::Left,
            PivotRole::Value => {
                let column = cell.column.saturating_sub(self.location.first_column + self.first_data_col);
                let d = if self.data_fields.len() > 1 { column as usize % self.data_fields.len() } else { 0 };
                if let Some(df) = self.data_fields.get(d) {
                    if let Some(code) = &df.number_format {
                        s.number_format = code.clone();
                    } else if df.show_as.as_deref().is_some_and(|s| s.starts_with("percent")) {
                        s.number_format = "0.00%".to_string();
                    }
                }
            }
            _ => {}
        }
        s
    }
}

impl crate::sheet::Sheet {
    /// Take away the cells of `pivot`'s place and of its report filters.
    pub fn clear_pivot_area(&mut self, pivot: &PivotTable) {
        let loc = pivot.location;
        let pages = pivot.page_fields.len() as u32;
        let mut clear = vec![loc];
        if pages > 0 {
            clear.push(CellRange { first_row: loc.first_row.saturating_sub(pages + 1), last_row: loc.first_row.saturating_sub(1), first_column: loc.first_column, last_column: loc.first_column + 1 });
        }
        self.cells.retain(|c| !clear.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)));
    }

    /// Put the table `index` built as `built`: the cells of its old place
    /// (and its report filters') taken away, the new ones written in their
    /// formats from the workbook's default `base`, and its place kept.
    pub fn put_pivot(&mut self, index: usize, built: &PivotBuilt, base: &crate::sheet::CellStyle, date1904: bool) {
        let Some(old) = self.pivots.get(index) else { return };
        let mut clear = vec![old.location];
        let pages = old.page_fields.len() as u32;
        if pages > 0 {
            clear.push(CellRange { first_row: old.location.first_row.saturating_sub(pages + 1), last_row: old.location.first_row.saturating_sub(1), first_column: old.location.first_column, last_column: old.location.first_column + 1 });
        }
        clear.push(built.location);
        clear.extend(built.page_range);
        self.cells.retain(|c| !clear.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)));
        let mut pivot = self.pivots[index].clone();
        pivot.location = built.location;
        pivot.first_header_row = built.first_header_row;
        pivot.first_data_row = built.first_data_row;
        pivot.first_data_col = built.first_data_col;
        pivot.row_items = built.row_items.clone();
        pivot.col_items = built.col_items.clone();
        pivot.buttons = built.cells.iter().filter(|c| matches!(c.role, PivotRole::Caption | PivotRole::PageValue)).map(|c| (c.row, c.column)).collect();
        pivot.corner = built.cells.iter().find(|c| c.role == PivotRole::Caption && (c.row, c.column) == (built.location.first_row, built.location.first_column) && built.first_data_row > 1).map(|c| (c.row, c.column));
        for cell in &built.cells {
            if cell.row as usize >= self.rows.len() || cell.column as usize >= self.columns.len() {
                continue;
            }
            let style = pivot.cell_style(cell, base);
            let text = crate::sheet_command::display_text(&cell.value, &style, date1904);
            let kind = crate::sheet::CellKind::of(&cell.value);
            let style = self.style_index(style);
            self.put_cell(cell.row, cell.column, Some(crate::sheet::SheetCell { runs: None, phonetic: None, row: cell.row, column: cell.column, value: cell.value.clone(), text, kind, formula: None, style }));
        }
        self.pivots[index] = pivot;
    }
}

/// Excel's (Japanese) names of the ways values are summarized, as the
/// start of a value's name (合計 / 金額).
pub const FUNCTIONS: [(&str, &str); 11] = [
    ("sum", "合計"),
    ("count", "個数"),
    ("average", "平均"),
    ("max", "最大"),
    ("min", "最小"),
    ("product", "積"),
    ("countNums", "数値の個数"),
    ("stdDev", "標本標準偏差"),
    ("stdDevp", "標準偏差"),
    ("var", "標本分散"),
    ("varp", "分散"),
];

fn function_label(function: &str) -> &'static str {
    FUNCTIONS.iter().find(|(f, _)| *f == function).map_or("合計", |(_, l)| l)
}

impl PivotTable {
    /// A new, empty pivot table of `data` (from `source`), its corner at
    /// (`row`, `column`): Excel's defaults (コンパクト, PivotStyleLight16,
    /// 総計 of both), each field's cache items in the order the data has
    /// them, no field placed yet.
    pub fn new(name: &str, source: PivotSource, data: &PivotData, row: u32, column: u32) -> Self {
        let fields = data
            .headings
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let mut shared: Vec<CellValue> = Vec::new();
                for r in &data.records {
                    let v = r.get(i).cloned().unwrap_or_default();
                    if !shared.iter().any(|s| same_item(s, &v)) {
                        shared.push(v);
                    }
                }
                PivotField { name: h.clone(), formula: None, axis: None, shared, items: Vec::new(), compact: true, outline: true, subtotal_top: true, default_subtotal: true, sort: PivotSort::Manual, sort_data_field: None, grouping: None, insert_blank_row: false, fill_down_labels: None }
            })
            .collect();
        PivotTable {
            name: name.to_string(),
            part: None,
            cache_part: None,
            location: CellRange { first_row: row, last_row: row, first_column: column, last_column: column },
            first_header_row: 1,
            first_data_row: 1,
            first_data_col: 1,
            source,
            fields,
            row_fields: Vec::new(),
            col_fields: Vec::new(),
            page_fields: Vec::new(),
            data_fields: Vec::new(),
            row_items: Vec::new(),
            col_items: Vec::new(),
            row_grand_totals: true,
            col_grand_totals: true,
            compact: true,
            outline: true,
            row_header_caption: None,
            col_header_caption: None,
            grand_total_caption: None,
            data_caption: Some(VALUES.to_string()),
            missing_caption: None,
            missing_zero: false,
            error_caption: None,
            fill_down_labels: false,
            filters: Vec::new(),
            style: Some(PivotStyleInfo { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, row_stripes: false, col_stripes: false, last_column: true }),
            data_on_rows: false,
            refreshed: true,
            refresh_on_load: false,
            buttons: Vec::new(),
            corner: None,
            merge_item: false,
            auto_fit_columns: true,
            preserve_formatting: true,
            multiple_field_filters: false,
            calculated_items: Vec::new(),
            show_drill: true,
        }
    }

    /// Where field `f` is: its area (values when it is one of the values),
    /// and its place there.
    pub fn place_of(&self, f: usize) -> Option<(PivotAxis, usize)> {
        let at = |list: &[i32]| list.iter().position(|&x| x == f as i32);
        at(&self.row_fields)
            .map(|i| (PivotAxis::Row, i))
            .or_else(|| at(&self.col_fields).map(|i| (PivotAxis::Column, i)))
            .or_else(|| self.page_fields.iter().position(|p| p.field == f as u32).map(|i| (PivotAxis::Page, i)))
            .or_else(|| self.data_fields.iter().position(|d| d.field == f as u32).map(|i| (PivotAxis::Values, i)))
    }

    /// Put field `f` in `area` at place `at` (past the end: last), taken
    /// out of where it was; `None` takes it out of the table. A field put
    /// in the values is summed when all its values are numbers, else
    /// counted, as Excel does.
    pub fn place(&mut self, f: usize, area: Option<PivotAxis>, at: usize) {
        if f >= self.fields.len() {
            return;
        }
        let fi = f as i32;
        self.row_fields.retain(|&x| x != fi);
        self.col_fields.retain(|&x| x != fi);
        self.page_fields.retain(|p| p.field != f as u32);
        if area == Some(PivotAxis::Values) || area.is_none() {
            // Out of the rows, columns and filters; a value taken out only
            // when it is the one moved.
            if area.is_none() {
                self.data_fields.retain(|d| d.field != f as u32);
            }
        }
        self.fields[f].axis = None;
        match area {
            Some(PivotAxis::Row) => {
                let at = at.min(self.row_fields.len());
                self.row_fields.insert(at, fi);
            }
            Some(PivotAxis::Column) => {
                let at = at.min(self.col_fields.len());
                self.col_fields.insert(at, fi);
            }
            Some(PivotAxis::Page) => {
                let at = at.min(self.page_fields.len());
                self.page_fields.insert(at, PivotPageField { field: f as u32, item: None });
            }
            Some(PivotAxis::Values) => {
                let numbers = self.fields[f].shared.iter().filter(|v| !matches!(v, CellValue::Empty)).all(|v| matches!(v, CellValue::Number(_)));
                let function = if numbers { "sum" } else { "count" };
                let current = self.data_fields.iter().position(|d| d.field == f as u32);
                let entry = match current {
                    Some(i) => self.data_fields.remove(i),
                    None => PivotDataField { name: self.value_name(function, f), field: f as u32, function: function.into(), show_as: None, number_format: None },
                };
                let at = at.min(self.data_fields.len());
                self.data_fields.insert(at, entry);
            }
            None => {}
        }
        if matches!(area, Some(PivotAxis::Row | PivotAxis::Column | PivotAxis::Page)) {
            self.fields[f].axis = area;
            if self.fields[f].items.is_empty() {
                // A field placed the first time lists its items sorted, as Excel's do.
                let shared = &self.fields[f].shared;
                let mut order: Vec<usize> = (0..shared.len()).collect();
                order.sort_by(|&a, &b| compare_values(&shared[a], &shared[b]));
                let mut items: Vec<PivotItem> = order.into_iter().map(|x| PivotItem { x: Some(x as u32), hidden: false, kind: None }).collect();
                items.push(PivotItem { x: None, hidden: false, kind: Some("default".into()) });
                self.fields[f].items = items;
            }
        }
        self.fix_values_place();
    }

    /// Several values take a place of their own among the column fields
    /// (`-2`), as Excel lays them out; one or none, none.
    fn fix_values_place(&mut self) {
        let several = self.data_fields.len() > 1;
        let has = self.row_fields.contains(&-2) || self.col_fields.contains(&-2);
        if several && !has {
            self.col_fields.push(-2);
        } else if !several {
            self.row_fields.retain(|&x| x != -2);
            self.col_fields.retain(|&x| x != -2);
        }
    }

    /// A value's name as Excel gives it: 合計 / 金額, with a number when the
    /// name is taken.
    fn value_name(&self, function: &str, f: usize) -> String {
        let base = format!("{} / {}", function_label(function), self.fields[f].name);
        let mut name = base.clone();
        let mut n = 2;
        while self.data_fields.iter().any(|d| d.name == name) {
            name = format!("{base}{n}");
            n += 1;
        }
        name
    }

    /// How value `d` is summarized (`sum`, `count`, …; see [`FUNCTIONS`]),
    /// its name following when it was Excel's.
    pub fn set_function(&mut self, d: usize, function: &str) {
        let Some(field) = self.data_fields.get(d).map(|x| x.field as usize) else { return };
        let auto = FUNCTIONS.iter().any(|(_, l)| self.data_fields[d].name.starts_with(&format!("{l} / ")));
        self.data_fields[d].function = function.to_string();
        if auto {
            self.data_fields[d].name = String::new();
            self.data_fields[d].name = self.value_name(function, field);
        }
    }

    /// Set a custom name for value `d` (`name`). If empty, reverts to automatic name.
    pub fn set_data_field_name(&mut self, d: usize, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            let Some((field, func)) = self.data_fields.get(d).map(|df| (df.field as usize, df.function.clone())) else { return };
            let new_name = self.value_name(&func, field);
            if let Some(df) = self.data_fields.get_mut(d) {
                df.name = new_name;
            }
        } else if let Some(df) = self.data_fields.get_mut(d) {
            df.name = trimmed.to_string();
        }
    }

    /// Set the number format of value `d` (`numFmtId`).
    pub fn set_number_format(&mut self, d: usize, format: Option<String>) {
        if let Some(df) = self.data_fields.get_mut(d) {
            df.number_format = format;
        }
    }

    /// Set how value `d` is shown (`showDataAs`: normal, percentOfTotal, percentOfCol, percentOfRow).
    pub fn set_show_as(&mut self, d: usize, show_as: Option<String>) {
        if let Some(df) = self.data_fields.get_mut(d) {
            df.show_as = show_as.filter(|s| s != "normal");
        }
    }

    /// Group field `f` by `grouping` (日付・数値のグループ化).
    pub fn group_field(&mut self, f: usize, grouping: PivotFieldGrouping) {
        if let Some(field) = self.fields.get_mut(f) {
            field.grouping = Some(grouping);
        }
    }

    /// Ungroup field `f` (グループ解除).
    pub fn ungroup_field(&mut self, f: usize) {
        if let Some(field) = self.fields.get_mut(f) {
            field.grouping = None;
        }
    }

    /// Group items of field `f` into a manual group (`group_name` or "グループ1", "グループ2", etc.).
    pub fn group_manual(&mut self, f: usize, item_names: &[String], group_name: Option<String>) -> Option<String> {
        let field = self.fields.get_mut(f)?;
        let mut groups = match field.grouping.take() {
            Some(PivotFieldGrouping::Manual { groups }) => groups,
            _ => HashMap::new(),
        };

        let g_name = if let Some(gn) = group_name {
            gn.trim().to_string()
        } else {
            let mut n = 1;
            loop {
                let candidate = format!("グループ{n}");
                if !groups.values().any(|v| v == &candidate) {
                    break candidate;
                }
                n += 1;
            }
        };

        for item in item_names {
            groups.insert(item.trim().to_string(), g_name.clone());
        }

        field.grouping = Some(PivotFieldGrouping::Manual { groups });
        Some(g_name)
    }

    /// Ungroup specific manual items or groups in field `f`.
    pub fn ungroup_manual_items(&mut self, f: usize, item_or_group_names: &[String]) {
        let Some(field) = self.fields.get_mut(f) else { return };
        if let Some(PivotFieldGrouping::Manual { ref mut groups }) = field.grouping {
            for target in item_or_group_names {
                let t = target.trim();
                groups.remove(t);
                groups.retain(|_, grp| grp != t);
            }
            if groups.is_empty() {
                field.grouping = None;
            }
        }
    }

    /// Return manual groups map for field `f` if manually grouped.
    pub fn manual_groups_for_field(&self, f: usize) -> Option<&HashMap<String, String>> {
        let field = self.fields.get(f)?;
        match &field.grouping {
            Some(PivotFieldGrouping::Manual { groups }) => Some(groups),
            _ => None,
        }
    }

    /// The cells with a pivot button (▼) on screen and what each is for:
    /// 行ラベル (all the row fields, in コンパクト form) or a row field's
    /// name, 列ラベル or a column field's name, a report filter's value.
    pub fn button_places(&self) -> Vec<(u32, u32, PivotButton)> {
        let mut out = Vec::new();
        if self.is_empty() {
            return out;
        }
        let top = self.location.first_row;
        let left = self.location.first_column;
        let rows: Vec<usize> = self.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect();
        let cols: Vec<usize> = self.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect();
        let compact = |f: usize| self.fields.get(f).is_some_and(|f| f.compact && f.outline);
        let label_row = top + self.first_data_row.max(1) - 1;
        if !rows.is_empty() {
            if compact(rows[0]) {
                out.push((label_row, left, PivotButton::Rows));
            } else {
                let (label_col, _) = self.row_label_places();
                for (i, &f) in rows.iter().enumerate() {
                    out.push((label_row, left + label_col[i], PivotButton::Field(f)));
                }
            }
        }
        if !cols.is_empty() {
            let data_col = left + self.first_data_col;
            if compact(cols[0]) {
                out.push((top, data_col, PivotButton::Columns));
            } else {
                for (i, &f) in cols.iter().enumerate() {
                    out.push((top, data_col + i as u32, PivotButton::Field(f)));
                }
            }
        }
        let pages = self.page_fields.len() as u32;
        for i in 0..pages {
            out.push((top.saturating_sub(pages + 1) + i, left + 1, PivotButton::Page(i as usize)));
        }
        out
    }

    /// Each field's cache items with the values of `data` it lacks added
    /// after them (a field Excel used for nothing has none), the items the
    /// table points at keeping their places.
    pub fn fill_shared(&mut self, data: &PivotData) {
        for f in 0..self.fields.len() {
            let Some(c) = self.data_column(f, data) else { continue };
            for r in &data.records {
                let v = r.get(c).cloned().unwrap_or_default();
                if !self.fields[f].shared.iter().any(|s| same_item(s, &v)) {
                    self.fields[f].shared.push(v);
                }
            }
        }
    }

    /// Nothing placed yet: the table shows no cells.
    pub fn is_empty(&self) -> bool {
        self.row_fields.is_empty() && self.col_fields.is_empty() && self.data_fields.is_empty() && self.page_fields.is_empty()
    }

    /// Whether the field at `index` is a calculated field.
    pub fn is_calculated_field(&self, index: usize) -> bool {
        self.fields.get(index).and_then(|f| f.formula.as_ref()).is_some()
    }

    /// List all calculated fields as `(index, name, formula)`.
    pub fn calculated_fields(&self) -> Vec<(usize, &str, &str)> {
        self.fields
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.formula.as_deref().map(|formula| (i, f.name.as_str(), formula)))
            .collect()
    }

    /// Add a new calculated field (計算フィールド) to the table.
    /// Returns the index of the newly added field in `fields`.
    pub fn add_calculated_field(&mut self, name: &str, formula: &str) -> Result<usize, String> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err("Field name cannot be empty".to_string());
        }
        let clean_formula = formula.trim().trim_start_matches('=').trim();
        if clean_formula.is_empty() {
            return Err("Formula cannot be empty".to_string());
        }
        if self.fields.iter().any(|f| f.name.eq_ignore_ascii_case(trimmed_name)) {
            return Err(format!("Field '{trimmed_name}' already exists"));
        }
        // Verify formula syntax
        if ibis_calc::formula::parse(clean_formula).is_err() {
            return Err("Formula syntax error".to_string());
        }

        let new_field = PivotField {
            name: trimmed_name.to_string(),
            formula: Some(formula.trim().to_string()),
            axis: Some(PivotAxis::Values),
            compact: true,
            outline: true,
            subtotal_top: true,
            default_subtotal: true,
            sort: PivotSort::Manual,
            ..Default::default()
        };
        let field_idx = self.fields.len();
        self.fields.push(new_field);

        // Automatically add to data_fields
        let data_field = PivotDataField {
            name: format!("合計 / {trimmed_name}"),
            field: field_idx as u32,
            function: "sum".to_string(),
            show_as: None,
            number_format: None,
        };
        self.data_fields.push(data_field);
        if self.data_fields.len() > 1 && !self.col_fields.contains(&-2) && !self.row_fields.contains(&-2) {
            self.col_fields.push(-2);
        }

        Ok(field_idx)
    }

    /// Modify an existing calculated field's formula.
    pub fn modify_calculated_field(&mut self, name: &str, new_formula: &str) -> Result<usize, String> {
        let clean_formula = new_formula.trim().trim_start_matches('=').trim();
        if clean_formula.is_empty() {
            return Err("Formula cannot be empty".to_string());
        }
        if ibis_calc::formula::parse(clean_formula).is_err() {
            return Err("Formula syntax error".to_string());
        }
        let Some((idx, field)) = self.fields.iter_mut().enumerate().find(|(_, f)| f.name.eq_ignore_ascii_case(name) && f.formula.is_some()) else {
            return Err(format!("Calculated field '{name}' not found"));
        };
        field.formula = Some(new_formula.trim().to_string());
        Ok(idx)
    }

    /// Delete a calculated field from the table and adjust indices.
    pub fn delete_calculated_field(&mut self, name: &str) -> Result<usize, String> {
        let Some((idx, _)) = self.fields.iter().enumerate().find(|(_, f)| f.name.eq_ignore_ascii_case(name) && f.formula.is_some()) else {
            return Err(format!("Calculated field '{name}' not found"));
        };
        self.fields.remove(idx);
        self.data_fields.retain(|df| df.field != idx as u32);
        for df in &mut self.data_fields {
            if df.field > idx as u32 {
                df.field -= 1;
            }
        }
        self.row_fields.retain(|&f| f != idx as i32);
        for f in &mut self.row_fields {
            if *f > idx as i32 {
                *f -= 1;
            }
        }
        self.col_fields.retain(|&f| f != idx as i32);
        for f in &mut self.col_fields {
            if *f > idx as i32 {
                *f -= 1;
            }
        }
        self.page_fields.retain(|pf| pf.field != idx as u32);
        for pf in &mut self.page_fields {
            if pf.field > idx as u32 {
                pf.field -= 1;
            }
        }
        self.filters.retain(|flt| flt.field != idx as u32);
        for flt in &mut self.filters {
            if flt.field > idx as u32 {
                flt.field -= 1;
            }
        }
        self.calculated_items.retain(|ci| ci.field != idx as u32);
        for ci in &mut self.calculated_items {
            if ci.field > idx as u32 {
                ci.field -= 1;
            }
        }
        if self.data_fields.len() <= 1 {
            self.col_fields.retain(|&f| f != -2);
            self.row_fields.retain(|&f| f != -2);
        }
        Ok(idx)
    }

    /// Whether item `name` in field `field_idx` is a calculated item.
    pub fn is_calculated_item(&self, field_idx: usize, name: &str) -> bool {
        self.calculated_items.iter().any(|ci| ci.field as usize == field_idx && ci.name.eq_ignore_ascii_case(name))
    }

    /// List calculated items for `field_idx` as `(&str, &str)` (name, formula).
    pub fn calculated_items_for_field(&self, field_idx: usize) -> Vec<(&str, &str)> {
        self.calculated_items
            .iter()
            .filter(|ci| ci.field as usize == field_idx)
            .map(|ci| (ci.name.as_str(), ci.formula.as_str()))
            .collect()
    }

    /// Add a new calculated item (集計アイテム / 計算アイテム) to `field_idx`.
    pub fn add_calculated_item(&mut self, field_idx: usize, name: &str, formula: &str) -> Result<usize, String> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err("Item name cannot be empty".to_string());
        }
        let clean_formula = formula.trim().trim_start_matches('=').trim();
        if clean_formula.is_empty() {
            return Err("Formula cannot be empty".to_string());
        }
        if field_idx >= self.fields.len() {
            return Err("Invalid field index".to_string());
        }
        if self.calculated_items.iter().any(|ci| ci.field as usize == field_idx && ci.name.eq_ignore_ascii_case(trimmed_name)) {
            return Err(format!("Calculated item '{trimmed_name}' already exists"));
        }
        // Verify formula syntax
        if ibis_calc::formula::parse(clean_formula).is_err() {
            return Err("Formula syntax error".to_string());
        }

        let item = PivotCalculatedItem {
            field: field_idx as u32,
            name: trimmed_name.to_string(),
            formula: formula.trim().to_string(),
        };
        self.calculated_items.push(item);
        Ok(self.calculated_items.len() - 1)
    }

    /// Modify an existing calculated item.
    pub fn modify_calculated_item(&mut self, field_idx: usize, name: &str, formula: &str) -> Result<(), String> {
        let clean_formula = formula.trim().trim_start_matches('=').trim();
        if clean_formula.is_empty() {
            return Err("Formula cannot be empty".to_string());
        }
        if ibis_calc::formula::parse(clean_formula).is_err() {
            return Err("Formula syntax error".to_string());
        }
        let Some(ci) = self.calculated_items.iter_mut().find(|ci| ci.field as usize == field_idx && ci.name.eq_ignore_ascii_case(name.trim())) else {
            return Err(format!("Calculated item '{name}' not found"));
        };
        ci.formula = formula.trim().to_string();
        Ok(())
    }

    /// Delete a calculated item.
    pub fn delete_calculated_item(&mut self, field_idx: usize, name: &str) -> Result<(), String> {
        let pos = self.calculated_items.iter().position(|ci| ci.field as usize == field_idx && ci.name.eq_ignore_ascii_case(name.trim()));
        if let Some(idx) = pos {
            self.calculated_items.remove(idx);
            Ok(())
        } else {
            Err(format!("Calculated item '{name}' not found"))
        }
    }

    /// Sort field by the aggregated value of a data field (値による並べ替え).
    pub fn sort_field_by_value(&mut self, field: usize, data_field: usize, ascending: bool) {
        if let Some(f) = self.fields.get_mut(field) {
            f.sort = if ascending { PivotSort::Ascending } else { PivotSort::Descending };
            f.sort_data_field = Some(data_field);
        }
    }

    /// Sort field by label / item values (ラベルによる並べ替え).
    pub fn sort_field_by_label(&mut self, field: usize, ascending: bool) {
        if let Some(f) = self.fields.get_mut(field) {
            f.sort = if ascending { PivotSort::Ascending } else { PivotSort::Descending };
            f.sort_data_field = None;
        }
    }
}

fn eval_calc_expr(expr: &ibis_calc::formula::Expr, fields: &std::collections::HashMap<String, f64>) -> Result<f64, CellError> {
    use ibis_calc::formula::{BinaryOp, Expr};
    match expr {
        Expr::Number(n) => Ok(*n),
        Expr::Text(s) => s.trim().parse::<f64>().map_err(|_| CellError::Value),
        Expr::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Expr::Error(e) => Err(*e),
        Expr::Name { name, .. } => {
            let key = name.trim().trim_matches('\'').trim_matches('[').trim_matches(']').to_lowercase();
            Ok(fields.get(&key).copied().unwrap_or(0.0))
        }
        Expr::Neg(inner) => Ok(-eval_calc_expr(inner, fields)?),
        Expr::Plus(inner) => eval_calc_expr(inner, fields),
        Expr::Percent(inner) => Ok(eval_calc_expr(inner, fields)? / 100.0),
        Expr::Binary(op, left, right) => {
            let l = eval_calc_expr(left, fields)?;
            let r = eval_calc_expr(right, fields)?;
            match op {
                BinaryOp::Add => Ok(l + r),
                BinaryOp::Sub => Ok(l - r),
                BinaryOp::Mul => Ok(l * r),
                BinaryOp::Div => {
                    if r == 0.0 {
                        Err(CellError::Div0)
                    } else {
                        Ok(l / r)
                    }
                }
                BinaryOp::Pow => Ok(l.powf(r)),
                BinaryOp::Eq => Ok(if (l - r).abs() < f64::EPSILON { 1.0 } else { 0.0 }),
                BinaryOp::Ne => Ok(if (l - r).abs() >= f64::EPSILON { 1.0 } else { 0.0 }),
                BinaryOp::Lt => Ok(if l < r { 1.0 } else { 0.0 }),
                BinaryOp::Le => Ok(if l <= r { 1.0 } else { 0.0 }),
                BinaryOp::Gt => Ok(if l > r { 1.0 } else { 0.0 }),
                BinaryOp::Ge => Ok(if l >= r { 1.0 } else { 0.0 }),
                _ => Err(CellError::Value),
            }
        }
        Expr::Call { name, args } => match name.to_uppercase().as_str() {
            "IF" => {
                if args.len() < 2 {
                    return Err(CellError::Value);
                }
                let cond = eval_calc_expr(&args[0], fields)?;
                if cond != 0.0 {
                    eval_calc_expr(&args[1], fields)
                } else if let Some(alt) = args.get(2) {
                    eval_calc_expr(alt, fields)
                } else {
                    Ok(0.0)
                }
            }
            "SUM" => {
                let mut s = 0.0;
                for a in args {
                    s += eval_calc_expr(a, fields)?;
                }
                Ok(s)
            }
            "AVERAGE" => {
                if args.is_empty() {
                    return Err(CellError::Div0);
                }
                let mut s = 0.0;
                for a in args {
                    s += eval_calc_expr(a, fields)?;
                }
                Ok(s / args.len() as f64)
            }
            "MAX" => {
                if args.is_empty() {
                    return Ok(0.0);
                }
                let mut m = f64::NEG_INFINITY;
                for a in args {
                    m = m.max(eval_calc_expr(a, fields)?);
                }
                Ok(m)
            }
            "MIN" => {
                if args.is_empty() {
                    return Ok(0.0);
                }
                let mut m = f64::INFINITY;
                for a in args {
                    m = m.min(eval_calc_expr(a, fields)?);
                }
                Ok(m)
            }
            "ABS" => {
                let v = args.first().map(|a| eval_calc_expr(a, fields)).transpose()?.unwrap_or(0.0);
                Ok(v.abs())
            }
            "INT" => {
                let v = args.first().map(|a| eval_calc_expr(a, fields)).transpose()?.unwrap_or(0.0);
                Ok(v.floor())
            }
            "ROUND" => {
                if args.len() < 2 {
                    return Err(CellError::Value);
                }
                let v = eval_calc_expr(&args[0], fields)?;
                let digits = eval_calc_expr(&args[1], fields)? as i32;
                let factor = 10f64.powi(digits);
                Ok((v * factor).round() / factor)
            }
            _ => Err(CellError::Name),
        },
        _ => Err(CellError::Value),
    }
}

/// Create a new `Chart` (PivotChart) from a `PivotTable` and its built layout `built`.
pub fn chart_from_pivot(pivot: &PivotTable, built: &PivotBuilt) -> Chart {
    let mut chart = Chart {
        groups: Vec::new(),
        axes: Vec::new(),
        legend: None,
        title: Some(ChartTitle {
            text: pivot.data_fields.first().map(|d| d.name.clone()).unwrap_or_else(|| pivot.name.clone()),
            style: ChartText::default(),
            at: None,
        }),
        plot_area: None,
        background: Fill::None,
        border: None,
        plot_background: Fill::None,
        plot_border: None,
        disp_blanks_as: None,
        plot_vis_only: true,
        data_table: None,
        style: Some(2),
        pivot_source: Some(pivot.name.clone()),
        rounded_corners: false,
        title_overlay: false,
        ex: None,
        ex_format: None,
        excel_layout: false,
        word_layout: false,
        cell_refs: Vec::new(),
        ex_source: None,
    };
    update_chart_from_pivot(&mut chart, pivot, built);
    chart
}

/// Whether a PivotChart's `pivot_source` (`PivotTable1`, `Sheet1!PivotTable1`,
/// or `[Book.xlsx]Sheet1!PivotTable1`) refers to `pivot_name` on `sheet_name`.
pub fn pivot_source_matches(pivot_source: Option<&str>, sheet_name: Option<&str>, pivot_name: &str) -> bool {
    let Some(ps) = pivot_source.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    if let Some((prefix, name)) = ps.rsplit_once('!') {
        let name = name.trim().trim_matches('\'');
        if !name.eq_ignore_ascii_case(pivot_name) && name != pivot_name {
            return false;
        }
        if let Some(sn) = sheet_name.filter(|s| !s.is_empty()) {
            let unquoted = prefix.trim().trim_matches('\'').replace("''", "'");
            let sheet_part = unquoted.rsplit_once(']').map_or(unquoted.as_str(), |(_, s)| s).trim();
            if !sheet_part.is_empty() && !sheet_part.eq_ignore_ascii_case(sn) && sheet_part != sn {
                return false;
            }
        }
        true
    } else {
        ps.eq_ignore_ascii_case(pivot_name) || ps == pivot_name
    }
}

/// Update an existing `Chart`'s categories, series names and data values from `PivotTable` and `PivotBuilt`.
pub fn update_chart_from_pivot(chart: &mut Chart, pivot: &PivotTable, built: &PivotBuilt) {
    if chart.pivot_source.is_none() {
        chart.pivot_source = Some(pivot.name.clone());
    }

    // 1. Identify category rows (Item rows, not subtotal or grand total)
    // The data body starts at built.location.first_row + built.first_data_row
    let first_row = built.location.first_row + built.first_data_row;
    let mut cat_rows: Vec<(u32, String)> = Vec::new();
    for (r_offset, kind) in built.rows.iter().enumerate() {
        if let PivotLineKind::Item { subtotal: false, .. } = kind {
            let row_idx = first_row + r_offset as u32;
            // Find row label for this row
            let mut labels = Vec::new();
            for cell in built.cells.iter().filter(|c| c.row == row_idx && matches!(c.role, PivotRole::RowLabel { .. })) {
                let text = match &cell.value {
                    CellValue::Text(t) => t.clone(),
                    CellValue::Number(n) => n.to_string(),
                    _ => String::new(),
                };
                if !text.is_empty() {
                    labels.push(text);
                }
            }
            let label = if labels.is_empty() {
                format!("項目{}", cat_rows.len() + 1)
            } else {
                labels.join(" ")
            };
            cat_rows.push((row_idx, label));
        }
    }
    if cat_rows.is_empty() {
        cat_rows.push((first_row, "合計".to_string()));
    }

    let categories: Vec<String> = cat_rows.iter().map(|(_, l)| l.clone()).collect();

    // 2. Identify series columns (Item columns, not subtotal or grand total)
    let first_col = built.location.first_column + built.first_data_col;
    let mut col_series: Vec<(u32, String)> = Vec::new();
    for (c_offset, kind) in built.columns.iter().enumerate() {
        if let PivotLineKind::Item { subtotal: false, .. } = kind {
            let col_idx = first_col + c_offset as u32;
            // Find column label for this column
            let mut labels = Vec::new();
            for cell in built.cells.iter().filter(|c| c.column == col_idx && matches!(c.role, PivotRole::ColumnLabel | PivotRole::Caption)) {
                let text = match &cell.value {
                    CellValue::Text(t) => t.clone(),
                    CellValue::Number(n) => n.to_string(),
                    _ => String::new(),
                };
                if !text.is_empty() {
                    labels.push(text);
                }
            }
            let name = if labels.is_empty() {
                format!("系列{}", col_series.len() + 1)
            } else {
                labels.join(" ")
            };
            col_series.push((col_idx, name));
        }
    }

    // If there are no column item fields, create a series per data field (or single series)
    if col_series.is_empty() {
        if !pivot.data_fields.is_empty() {
            for (df_idx, df) in pivot.data_fields.iter().enumerate() {
                let col_idx = first_col + df_idx as u32;
                col_series.push((col_idx, df.name.clone()));
            }
        } else {
            col_series.push((first_col, "値".to_string()));
        }
    }

    // 3. Extract values for each series
    let default_palette = [
        Rgb(0x5B9BD5), // Blue
        Rgb(0xED7D31), // Orange
        Rgb(0xA5A5A5), // Grey
        Rgb(0xFFC000), // Yellow
        Rgb(0x4472C4), // Dark blue
        Rgb(0x70AD47), // Green
    ];

    let existing_series = chart.groups.first().map(|g| g.series.as_slice()).unwrap_or(&[]);
    let mut new_series: Vec<Series> = Vec::new();
    for (s_idx, (col_idx, name)) in col_series.iter().enumerate() {
        let mut values: Vec<Option<Number>> = Vec::new();
        for (row_idx, _) in &cat_rows {
            let val = built.cells.iter()
                .find(|c| c.row == *row_idx && c.column == *col_idx && matches!(c.role, PivotRole::Value))
                .and_then(|c| match &c.value {
                    CellValue::Number(n) => Some(Number(*n)),
                    _ => None,
                });
            values.push(val);
        }

        let old_ser = existing_series.get(s_idx);
        let color = default_palette[s_idx % default_palette.len()];
        let fill = old_ser.map(|s| s.fill.clone()).unwrap_or(Fill::Solid(Color::opaque(color)));
        let stroke = old_ser.and_then(|s| s.stroke.clone());
        let format = old_ser.map(|s| s.format.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| "General".to_string());

        new_series.push(Series {
            name: name.clone(),
            categories: categories.clone(),
            values,
            format,
            fill,
            stroke,
            marker: old_ser.and_then(|s| s.marker.clone()),
            labels: old_ser.and_then(|s| s.labels.clone()),
            trendline: old_ser.and_then(|s| s.trendline.clone()),
            error_bars: old_ser.and_then(|s| s.error_bars.clone()),
            point_fills: Vec::new(),
            point_strokes: Vec::new(),
            explosion: 0,
            point_explosions: Vec::new(),
            x_values: Vec::new(),
            bubble_sizes: Vec::new(),
            bubble_3d: false,
        });
    }

    // 4. Update or initialize group
    let existing_group = chart.groups.first();
    let chart_type = existing_group.map(|g| g.kind).unwrap_or(ChartType::Bar {
        horizontal: false,
        gap_width: 150,
        overlap: 0,
    });
    let grouping = existing_group.map(|g| g.grouping).unwrap_or(crate::chart::Grouping::Standard);
    let of_pie = existing_group.and_then(|g| g.of_pie.clone());
    let group_axes = existing_group
        .map(|g| g.axes.clone())
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| vec![100_000_001, 100_000_002]);
    let three_d = existing_group.and_then(|g| g.three_d);

    chart.groups = vec![ChartGroup {
        kind: chart_type,
        of_pie,
        grouping,
        series: new_series,
        axes: group_axes,
        three_d,
    }];

    // 5. Update axes
    if chart.axes.is_empty() {
        chart.axes = vec![
            Axis {
                id: 100_000_001,
                source_id: Some(100_000_001),
                source_kind: AxisSourceKind::Category,
                crosses: None,
                crosses_at: None,
                cross_axis_id: Some(100_000_002),
                kind: AxisKind::Category,
                side: Side::Bottom,
                deleted: false,
                reversed: false,
                log_base: None,
                min: None,
                max: None,
                major_unit: None,
                minor_unit: None,
                display_unit: None,
                format: "General".to_string(),
                show_labels: true,
                labels_high: false,
                labels_low: false,
                label_rotation: 0,
                fixed_label_rotation: false,
                label_skip: 0,
                date: false,
                time: None,
                ticks_outside: false,
                line: None,
                gridlines: None,
                minor_gridlines: None,
                text: ChartText::default(),
                title: None,
                stagger_labels: false,
                title_rotation: None,
                category_levels: Vec::new(),
            },
            Axis {
                id: 100_000_002,
                source_id: Some(100_000_002),
                source_kind: AxisSourceKind::Value,
                crosses: None,
                crosses_at: None,
                cross_axis_id: Some(100_000_001),
                kind: AxisKind::Value,
                side: Side::Left,
                deleted: false,
                reversed: false,
                log_base: None,
                min: None,
                max: None,
                major_unit: None,
                minor_unit: None,
                display_unit: None,
                format: "General".to_string(),
                show_labels: true,
                labels_high: false,
                labels_low: false,
                label_rotation: 0,
                fixed_label_rotation: false,
                label_skip: 0,
                date: false,
                time: None,
                ticks_outside: false,
                line: None,
                gridlines: Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), 9_525)),
                minor_gridlines: None,
                text: ChartText::default(),
                title: None,
                stagger_labels: false,
                title_rotation: None,
                category_levels: Vec::new(),
            },
        ];
    }

    // 6. Update legend
    if chart.groups[0].series.len() > 1 {
        if chart.legend.is_none() {
            chart.legend = Some(Legend {
                side: Side::Right,
                source_position: Some(LegendPosition::Right),
                overlay: Some(false),
                layout: None,
                text: ChartText::default(),
                hidden: Vec::new(),
            });
        }
    } else {
        chart.legend = None;
    }

    // 7. Update title if needed
    if chart.title.is_none() {
        let title_text = pivot.data_fields.first().map(|d| d.name.clone()).unwrap_or_else(|| pivot.name.clone());
        chart.title = Some(ChartTitle {
            text: title_text,
            style: ChartText::default(),
            at: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{PivotAxis, PivotData, PivotSource, PivotTable, missing_caption_value};
    use crate::sheet::CellValue;

    fn data() -> PivotData {
        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        PivotData {
            headings: vec!["地域".into(), "商品".into(), "金額".into()],
            records: vec![
                vec![t("関東"), t("みかん"), n(100.0)],
                vec![t("関西"), t("りんご"), n(200.0)],
                vec![t("関東"), t("りんご"), n(50.0)],
                vec![t("九州"), t("みかん"), n(10.0)],
            ],
        }
    }

    fn text(built: &super::PivotBuilt, row: u32, column: u32) -> Option<CellValue> {
        built.cells.iter().find(|c| (c.row, c.column) == (row, column)).map(|c| c.value.clone())
    }

    #[test]
    fn made_and_placed() {
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &data(), 2, 0);
        assert!(p.build(&data()).cells.is_empty(), "nothing placed: no cells");
        p.place(0, Some(PivotAxis::Row), 0);
        p.place(2, Some(PivotAxis::Values), 0);
        let b = p.build(&data());
        // Sorted as Excel sorts a new field: 九州, 関西, 関東 (by code point).
        assert_eq!(text(&b, 2, 0), Some(CellValue::Text("行ラベル".into())));
        assert_eq!(text(&b, 2, 1), Some(CellValue::Text("合計 / 金額".into())));
        let labels: Vec<_> = (3..6).map(|r| text(&b, r, 0)).collect();
        assert_eq!(labels.len(), 3);
        assert_eq!(text(&b, 6, 0), Some(CellValue::Text("総計".into())));
        assert_eq!(text(&b, 6, 1), Some(CellValue::Number(360.0)));
        // A text field in the values is counted.
        p.place(1, Some(PivotAxis::Values), 1);
        assert_eq!(p.data_fields[1].name, "個数 / 商品");
        assert_eq!(p.col_fields, vec![-2], "two values: their own place among the columns");
        let b = p.build(&data());
        assert_eq!(text(&b, 3, 1), Some(CellValue::Text("合計 / 金額".into())));
        assert_eq!(text(&b, 3, 2), Some(CellValue::Text("個数 / 商品".into())));
        // 平均 instead of 合計, its name following.
        p.set_function(0, "average");
        assert_eq!(p.data_fields[0].name, "平均 / 金額");
        let b = p.build(&data());
        assert_eq!(text(&b, 7, 1), Some(CellValue::Number(90.0)));
        // Taken out: one value again, no place of its own.
        p.place(1, None, 0);
        assert!(p.col_fields.is_empty());
        // Moved to the columns.
        p.place(0, Some(PivotAxis::Column), 0);
        assert!(p.row_fields.is_empty() && p.col_fields == vec![0]);
    }

    #[test]
    fn value_field_settings_and_ratios() {
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &data(), 2, 0);
        p.place(0, Some(PivotAxis::Row), 0);
        p.place(2, Some(PivotAxis::Values), 0);

        // Custom name
        p.set_data_field_name(0, "売上金額");
        assert_eq!(p.data_fields[0].name, "売上金額");
        let b = p.build(&data());
        assert_eq!(text(&b, 2, 1), Some(CellValue::Text("売上金額".into())));

        // Number format
        p.set_number_format(0, Some("#,##0".into()));
        assert_eq!(p.data_fields[0].number_format.as_deref(), Some("#,##0"));
        let val_cell = b.cells.iter().find(|c| c.row == 3 && c.column == 1).unwrap();
        let st = p.cell_style(val_cell, &Default::default());
        assert_eq!(st.number_format, "#,##0");

        // Show as: % of Grand Total
        p.set_number_format(0, None);
        p.set_show_as(0, Some("percentOfTotal".into()));
        let b = p.build(&data());
        // 関西: 200 / 360 = 5 / 9
        if let Some(CellValue::Number(n)) = text(&b, 3, 1) {
            assert!((n - 200.0 / 360.0).abs() < 1e-6);
        } else {
            panic!("expected number for 関西");
        }
        // 関東: 150 / 360 = 5 / 12
        if let Some(CellValue::Number(n)) = text(&b, 4, 1) {
            assert!((n - 150.0 / 360.0).abs() < 1e-6);
        } else {
            panic!("expected number for 関東");
        }
        // 九州: 10 / 360 = 1 / 36
        if let Some(CellValue::Number(n)) = text(&b, 5, 1) {
            assert!((n - 10.0 / 360.0).abs() < 1e-6);
        } else {
            panic!("expected number for 九州");
        }
        // 総計: 360 / 360 = 1.0
        assert_eq!(text(&b, 6, 1), Some(CellValue::Number(1.0)));
        let st = p.cell_style(val_cell, &Default::default());
        assert_eq!(st.number_format, "0.00%", "defaults to percentage format");

        // Show as: % of Row Total (each row total is itself, so 1.0)
        p.set_show_as(0, Some("percentOfRow".into()));
        let b = p.build(&data());
        assert_eq!(text(&b, 3, 1), Some(CellValue::Number(1.0)));
        assert_eq!(text(&b, 4, 1), Some(CellValue::Number(1.0)));
        assert_eq!(text(&b, 5, 1), Some(CellValue::Number(1.0)));

        // Reset to normal
        p.set_show_as(0, None);
        let b = p.build(&data());
        assert_eq!(text(&b, 6, 1), Some(CellValue::Number(360.0)));
    }

    #[test]
    fn pivot_grouping_numeric_and_date() {
        use super::{DateGroupBy, PivotFieldGrouping};

        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        let test_data = PivotData {
            headings: vec!["日付".into(), "金額".into()],
            records: vec![
                vec![n(45306.0), n(100.0)], // 2024-01-15
                vec![n(45311.0), n(50.0)],  // 2024-01-20
                vec![n(45332.0), n(200.0)], // 2024-02-10
                vec![n(45356.0), n(70.0)],  // 2024-03-05
                vec![n(45242.0), n(300.0)], // 2023-11-12
            ],
        };

        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 日付
        p.place(1, Some(PivotAxis::Values), 0); // 金額

        // 1. Date grouping: Months (1月, 2月, 3月, 11月)
        p.group_field(0, PivotFieldGrouping::Date {
            by: DateGroupBy::Months,
            start: None,
            end: None,
            auto_start: true,
            auto_end: true,
        });
        let b = p.build(&test_data);
        assert_eq!(text(&b, 2, 0), Some(t("行ラベル")));
        assert_eq!(text(&b, 2, 1), Some(t("合計 / 金額")));
        assert_eq!(text(&b, 3, 0), Some(t("1月")));
        assert_eq!(text(&b, 3, 1), Some(n(150.0))); // 100 + 50
        assert_eq!(text(&b, 4, 0), Some(t("2月")));
        assert_eq!(text(&b, 4, 1), Some(n(200.0)));
        assert_eq!(text(&b, 5, 0), Some(t("3月")));
        assert_eq!(text(&b, 5, 1), Some(n(70.0)));
        assert_eq!(text(&b, 6, 0), Some(t("11月")));
        assert_eq!(text(&b, 6, 1), Some(n(300.0)));
        assert_eq!(text(&b, 7, 0), Some(t("総計")));
        assert_eq!(text(&b, 7, 1), Some(n(720.0)));

        // 2. Date grouping: Years (2023, 2024)
        p.group_field(0, PivotFieldGrouping::Date {
            by: DateGroupBy::Years,
            start: None,
            end: None,
            auto_start: true,
            auto_end: true,
        });
        let b = p.build(&test_data);
        assert_eq!(text(&b, 3, 0), Some(t("2023")));
        assert_eq!(text(&b, 3, 1), Some(n(300.0)));
        assert_eq!(text(&b, 4, 0), Some(t("2024")));
        assert_eq!(text(&b, 4, 1), Some(n(420.0))); // 100 + 50 + 200 + 70
        assert_eq!(text(&b, 5, 0), Some(t("総計")));
        assert_eq!(text(&b, 5, 1), Some(n(720.0)));

        // 3. Date grouping: Quarters (第1四半期, 第4四半期)
        p.group_field(0, PivotFieldGrouping::Date {
            by: DateGroupBy::Quarters,
            start: None,
            end: None,
            auto_start: true,
            auto_end: true,
        });
        let b = p.build(&test_data);
        assert_eq!(text(&b, 3, 0), Some(t("第1四半期")));
        assert_eq!(text(&b, 3, 1), Some(n(420.0)));
        assert_eq!(text(&b, 4, 0), Some(t("第4四半期")));
        assert_eq!(text(&b, 4, 1), Some(n(300.0)));
        assert_eq!(text(&b, 5, 0), Some(t("総計")));
        assert_eq!(text(&b, 5, 1), Some(n(720.0)));

        // 4. Numeric grouping on 金額 field
        p.ungroup_field(0); // Restore dates
        p.place(0, None, 0); // Remove 日付 from row
        p.place(1, Some(PivotAxis::Row), 0); // 金額 to Row
        p.group_field(1, PivotFieldGrouping::Number {
            start: 50.0,
            end: 200.0,
            interval: 50.0,
            auto_start: false,
            auto_end: false,
        });
        let b = p.build(&test_data);
        // Ranges: 50-99 (50, 70 -> sum 120), 100-149 (100 -> sum 100), 200 (200 -> sum 200), >200 (300 -> sum 300)
        assert_eq!(text(&b, 3, 0), Some(t("50-99")));
        assert_eq!(text(&b, 3, 1), Some(n(120.0)));
        assert_eq!(text(&b, 4, 0), Some(t("100-149")));
        assert_eq!(text(&b, 4, 1), Some(n(100.0)));
        assert_eq!(text(&b, 5, 0), Some(t("200")));
        assert_eq!(text(&b, 5, 1), Some(n(200.0)));
        assert_eq!(text(&b, 6, 0), Some(t(">200")));
        assert_eq!(text(&b, 6, 1), Some(n(300.0)));
        assert_eq!(text(&b, 7, 0), Some(t("総計")));
        assert_eq!(text(&b, 7, 1), Some(n(720.0)));

        // 5. Ungroup
        p.ungroup_field(1);
        let b = p.build(&test_data);
        // Ungrouped raw numbers sorted ascending: 50, 70, 100, 200, 300
        assert_eq!(text(&b, 3, 0), Some(n(50.0)));
        assert_eq!(text(&b, 4, 0), Some(n(70.0)));
        assert_eq!(text(&b, 5, 0), Some(n(100.0)));
        assert_eq!(text(&b, 6, 0), Some(n(200.0)));
        assert_eq!(text(&b, 7, 0), Some(n(300.0)));
        assert_eq!(text(&b, 8, 0), Some(t("総計")));
    }

    #[test]
    fn pivot_options_properties() {
        let test_data = PivotData {
            headings: vec!["商品".into(), "売上".into()],
            records: vec![vec![CellValue::Text("A".into()), CellValue::Number(100.0)]],
        };
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 0, 0);
        // Verify default option settings
        assert_eq!(p.name, "ピボットテーブル1");
        assert!(!p.merge_item);
        assert!(p.auto_fit_columns);
        assert!(p.preserve_formatting);
        assert!(!p.multiple_field_filters);
        assert!(p.row_grand_totals);
        assert!(p.col_grand_totals);
        assert!(p.missing_caption.is_none());
        assert!(p.error_caption.is_none());

        // Modify options
        p.name = "売上ピボット".into();
        p.merge_item = true;
        p.auto_fit_columns = false;
        p.preserve_formatting = false;
        p.multiple_field_filters = true;
        p.row_grand_totals = false;
        p.col_grand_totals = true;
        p.missing_caption = Some("(空白)".into());
        p.error_caption = Some("ERR".into());

        assert_eq!(p.name, "売上ピボット");
        assert!(p.merge_item);
        assert!(!p.auto_fit_columns);
        assert!(!p.preserve_formatting);
        assert!(p.multiple_field_filters);
        assert!(!p.row_grand_totals);
        assert!(p.col_grand_totals);
        assert_eq!(p.missing_caption.as_deref(), Some("(空白)"));
        assert_eq!(p.error_caption.as_deref(), Some("ERR"));
    }

    #[test]
    fn missing_caption_reads_number_like_text_as_zero() {
        for c in ["-", "0", " ", "", "-0", "0.0", " -0 "] {
            match missing_caption_value(c) {
                CellValue::Number(n) => assert!(n == 0.0 && n.is_sign_positive(), "{c:?}"),
                other => panic!("{c:?} -> {other:?}"),
            }
        }
        assert!(matches!(missing_caption_value("2"), CellValue::Number(n) if n == 2.0));
        for c in ["x", "--", "N/A", "(空白)"] {
            assert!(matches!(missing_caption_value(c), CellValue::Text(ref t) if t == c), "{c:?}");
        }
    }

    #[test]
    fn calculated_fields_add_evaluate_delete() {
        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        let test_data = PivotData {
            headings: vec!["地域".into(), "商品".into(), "売上".into(), "原価".into()],
            records: vec![
                vec![t("関東"), t("みかん"), n(100.0), n(60.0)],
                vec![t("関西"), t("りんご"), n(200.0), n(140.0)],
                vec![t("関東"), t("りんご"), n(50.0), n(30.0)],
                vec![t("九州"), t("みかん"), n(10.0), n(8.0)],
            ],
        };
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 地域 to Row
        p.place(2, Some(PivotAxis::Values), 0); // 売上 to Values

        // 1. Add calculated field: 粗利 = 売上 - 原価
        let idx1 = p.add_calculated_field("粗利", "= 売上 - 原価").expect("add 粗利");
        assert!(p.is_calculated_field(idx1));

        // 2. Add calculated field: 税額 = 売上 * 0.1
        let idx2 = p.add_calculated_field("税額", "= 売上 * 0.1").expect("add 税額");
        assert!(p.is_calculated_field(idx2));

        let calcs = p.calculated_fields();
        assert_eq!(calcs.len(), 2);
        assert_eq!(calcs[0].1, "粗利");
        assert_eq!(calcs[1].1, "税額");

        let b = p.build(&test_data);
        // Header line at row 3 (values placed among columns since there are 3 data fields)
        // Values: 合計 / 売上, 合計 / 粗利, 合計 / 税額
        assert_eq!(text(&b, 3, 1), Some(t("合計 / 売上")));
        assert_eq!(text(&b, 3, 2), Some(t("合計 / 粗利")));
        assert_eq!(text(&b, 3, 3), Some(t("合計 / 税額")));

        // Rows sorted by reading: 関西(かんさい), 関東(かんとう), 九州(きゅうしゅう)
        // 関西 (row 4): 売上=200, 粗利=60, 税額=20
        assert_eq!(text(&b, 4, 0), Some(t("関西")));
        assert_eq!(text(&b, 4, 1), Some(n(200.0)));
        assert_eq!(text(&b, 4, 2), Some(n(60.0)));
        assert_eq!(text(&b, 4, 3), Some(n(20.0)));

        // 関東 (row 5): 売上=150, 粗利=60, 税額=15
        assert_eq!(text(&b, 5, 0), Some(t("関東")));
        assert_eq!(text(&b, 5, 1), Some(n(150.0)));
        assert_eq!(text(&b, 5, 2), Some(n(60.0)));
        assert_eq!(text(&b, 5, 3), Some(n(15.0)));

        // 九州 (row 6): 売上=10, 粗利=2, 税額=1
        assert_eq!(text(&b, 6, 0), Some(t("九州")));
        assert_eq!(text(&b, 6, 1), Some(n(10.0)));
        assert_eq!(text(&b, 6, 2), Some(n(2.0)));
        assert_eq!(text(&b, 6, 3), Some(n(1.0)));

        // 総計 (row 7): 売上=360, 粗利=122, 税額=36
        assert_eq!(text(&b, 7, 0), Some(t("総計")));
        assert_eq!(text(&b, 7, 1), Some(n(360.0)));
        assert_eq!(text(&b, 7, 2), Some(n(122.0)));
        assert_eq!(text(&b, 7, 3), Some(n(36.0)));

        // 3. Details for cell (Drill Down on 粗利 cell at row 6, column 2 -> 九州)
        let details = p.details_for_cell(&test_data, 6, 2).expect("details for 九州 粗利");
        assert_eq!(details.records.len(), 1);
        assert_eq!(details.records[0][0], t("九州"));

        // 4. Modify calculated field
        p.modify_calculated_field("税額", "= 売上 * 0.08").expect("modify 税額");
        let b2 = p.build(&test_data);
        assert_eq!(text(&b2, 6, 3), Some(n(0.8))); // 10 * 0.08
        assert_eq!(text(&b2, 7, 3), Some(n(28.8))); // 360 * 0.08

        // 5. Delete calculated field "税額"
        p.delete_calculated_field("税額").expect("delete 税額");
        assert_eq!(p.calculated_fields().len(), 1);
        let b3 = p.build(&test_data);
        // Only 売上 and 粗利 remaining
        assert_eq!(text(&b3, 3, 1), Some(t("合計 / 売上")));
        assert_eq!(text(&b3, 3, 2), Some(t("合計 / 粗利")));
        assert_eq!(text(&b3, 6, 2), Some(n(2.0)));
        assert_eq!(text(&b3, 7, 2), Some(n(122.0)));
    }

    #[test]
    fn calculated_items_add_evaluate_delete() {
        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        let test_data = PivotData {
            headings: vec!["地域".into(), "商品".into(), "売上".into()],
            records: vec![
                vec![t("関東"), t("みかん"), n(100.0)],
                vec![t("関西"), t("りんご"), n(200.0)],
                vec![t("九州"), t("みかん"), n(50.0)],
            ],
        };
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 地域 to Row
        p.place(2, Some(PivotAxis::Values), 0); // 売上 to Values

        // 1. Add calculated item to field 0 (地域): 西日本 = 関西 + 九州
        let idx = p.add_calculated_item(0, "西日本", "= 関西 + 九州").expect("add 西日本");
        assert_eq!(idx, 0);
        assert!(p.is_calculated_item(0, "西日本"));
        let items = p.calculated_items_for_field(0);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "西日本");
        assert_eq!(items[0].1, "= 関西 + 九州");

        let b = p.build(&test_data);

        // Header at row 2
        assert_eq!(text(&b, 2, 0), Some(t("行ラベル")));
        assert_eq!(text(&b, 2, 1), Some(t("合計 / 売上")));

        // Check rows:
        // 関東: 100
        // 関西: 200
        // 九州: 50
        // 西日本: 250 (関西 200 + 九州 50)
        let nishi_row = b.cells.iter().find(|cell| cell.column == 0 && cell.value == t("西日本")).map(|c| c.row).expect("西日本 row");
        assert_eq!(text(&b, nishi_row, 1), Some(n(250.0)));

        // 2. Modify calculated item: 西日本 = 関西 * 1.5
        p.modify_calculated_item(0, "西日本", "= 関西 * 1.5").expect("modify 西日本");
        let b2 = p.build(&test_data);
        assert_eq!(text(&b2, nishi_row, 1), Some(n(300.0)));

        // 3. Delete calculated item
        p.delete_calculated_item(0, "西日本").expect("delete 西日本");
        assert_eq!(p.calculated_items_for_field(0).len(), 0);
        let b3 = p.build(&test_data);
        assert!(b3.cells.iter().all(|c| c.value != t("西日本")));
    }

    #[test]
    fn test_manual_grouping() {
        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        let test_data = PivotData {
            headings: vec!["地域".into(), "商品".into(), "売上".into()],
            records: vec![
                vec![t("関東"), t("みかん"), n(100.0)],
                vec![t("関西"), t("りんご"), n(200.0)],
                vec![t("九州"), t("みかん"), n(50.0)],
            ],
        };
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 地域 to Row
        p.place(2, Some(PivotAxis::Values), 0); // 売上 to Values

        // Group 関西 and 九州 manually
        let g_name = p.group_manual(0, &["関西".into(), "九州".into()], None).expect("group manual");
        assert_eq!(g_name, "グループ1");

        let b = p.build(&test_data);

        // Find "グループ1" row
        let g1_row = b.cells.iter().find(|cell| cell.column == 0 && cell.value == t("グループ1")).map(|c| c.row).expect("グループ1 row exists");
        assert_eq!(text(&b, g1_row, 1), Some(n(250.0)), "グループ1 adds 関西 (200) + 九州 (50)");

        // Find "関東" row
        let kanto_row = b.cells.iter().find(|cell| cell.column == 0 && cell.value == t("関東")).map(|c| c.row).expect("関東 row exists");
        assert_eq!(text(&b, kanto_row, 1), Some(n(100.0)), "関東 remains unchanged");

        // Grand total
        let total_row = b.cells.iter().find(|cell| cell.column == 0 && cell.value == t("総計")).map(|c| c.row).expect("総計 row exists");
        assert_eq!(text(&b, total_row, 1), Some(n(350.0)), "総計 adds all groups (350)");

        // Ungroup
        p.ungroup_manual_items(0, &["グループ1".into()]);
        assert_eq!(p.manual_groups_for_field(0), None, "groups empty, grouping removed");

        let b2 = p.build(&test_data);
        assert!(b2.cells.iter().all(|c| c.value != t("グループ1")));
        let kansai_row = b2.cells.iter().find(|cell| cell.column == 0 && cell.value == t("関西")).map(|c| c.row).expect("関西 row restored");
        assert_eq!(text(&b2, kansai_row, 1), Some(n(200.0)));
        let kyushu_row = b2.cells.iter().find(|cell| cell.column == 0 && cell.value == t("九州")).map(|c| c.row).expect("九州 row restored");
        assert_eq!(text(&b2, kyushu_row, 1), Some(n(50.0)));
    }

    #[test]
    fn test_sort_by_data_field() {
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &data(), 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 地域 to Row
        p.place(2, Some(PivotAxis::Values), 0); // 金額 to Values

        // 1. Sort by 金額 descending: 関西 (200), 関東 (150), 九州 (10)
        p.sort_field_by_value(0, 0, false);
        let b_desc = p.build(&data());
        let rows_desc: Vec<CellValue> = (3..6).map(|r| text(&b_desc, r, 0).unwrap()).collect();
        assert_eq!(rows_desc, vec![CellValue::Text("関西".into()), CellValue::Text("関東".into()), CellValue::Text("九州".into())]);

        // 2. Sort by 金額 ascending: 九州 (10), 関東 (150), 関西 (200)
        p.sort_field_by_value(0, 0, true);
        let b_asc = p.build(&data());
        let rows_asc: Vec<CellValue> = (3..6).map(|r| text(&b_asc, r, 0).unwrap()).collect();
        assert_eq!(rows_asc, vec![CellValue::Text("九州".into()), CellValue::Text("関東".into()), CellValue::Text("関西".into())]);

        // 3. Revert to label ascending: 関西(かんさい), 関東(かんとう), 九州(きゅうしゅう)
        p.sort_field_by_label(0, true);
        let b_label = p.build(&data());
        let rows_label: Vec<CellValue> = (3..6).map(|r| text(&b_label, r, 0).unwrap()).collect();
        assert_eq!(rows_label, vec![CellValue::Text("関西".into()), CellValue::Text("関東".into()), CellValue::Text("九州".into())]);
    }

    #[test]
    fn test_pivot_getpivotdata() {
        let t = |s: &str| CellValue::Text(s.into());
        let n = CellValue::Number;
        let test_data = PivotData {
            headings: vec!["地域".into(), "商品".into(), "売上".into()],
            records: vec![
                vec![t("関東"), t("みかん"), n(100.0)],
                vec![t("関西"), t("りんご"), n(200.0)],
                vec![t("関東"), t("りんご"), n(50.0)],
                vec![t("九州"), t("みかん"), n(10.0)],
            ],
        };
        let mut p = PivotTable::new("ピボットテーブル1", PivotSource::default(), &test_data, 2, 0);
        p.place(0, Some(PivotAxis::Row), 0); // 地域 to Row
        p.place(1, Some(PivotAxis::Column), 0); // 商品 to Col
        p.place(2, Some(PivotAxis::Values), 0); // 売上 to Values

        let b = p.build(&test_data);
        p.row_items = b.row_items;
        p.col_items = b.col_items;
        p.location = b.location;
        p.first_data_row = b.first_data_row;
        p.first_data_col = b.first_data_col;

        let mut sheet = crate::sheet::Sheet::default();
        for c in &b.cells {
            sheet.put_cell(c.row, c.column, Some(crate::sheet::SheetCell {
                row: c.row,
                column: c.column,
                value: c.value.clone(),
                text: String::new(),
                kind: crate::sheet::CellKind::of(&c.value),
                formula: None,
                style: 0,
                phonetic: None,
                runs: None,
            }));
        }

        // Test 1: Grand total
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[]), Some(n(360.0)));
        assert_eq!(p.get_pivot_data(&sheet, "合計 / 売上", &[]), Some(n(360.0)));

        // Test 2: Row filter (地域 = 関東) -> 100 + 50 = 150
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("地域", "関東")]), Some(n(150.0)));

        // Test 3: Col filter (商品 = みかん) -> 100 + 10 = 110
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("商品", "みかん")]), Some(n(110.0)));

        // Test 4: Intersection (地域 = 関東, 商品 = みかん) -> 100
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("地域", "関東"), ("商品", "みかん")]), Some(n(100.0)));

        // Test 5: Intersection (地域 = 関東, 商品 = りんご) -> 50
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("地域", "関東"), ("商品", "りんご")]), Some(n(50.0)));

        // Test 6: Nonexistent item -> None
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("地域", "北海道")]), None);

        // Test 7: Nonexistent field -> None
        assert_eq!(p.get_pivot_data(&sheet, "売上", &[("担当", "田中")]), None);

        // Test 8: Nonexistent data field -> None
        assert_eq!(p.get_pivot_data(&sheet, "利益", &[]), None);
    }
}

