// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > グラフ: a new chart, as PowerPoint makes one: column, bar, line,
//! area, pie or doughnut chart over its sample data in an embedded workbook,
//! its legend below (or to the right for pie/doughnut). The model holds the
//! chart as read from this template (`Source::new_chart` / `Source::new_chart_kind`);
//! edits to it are written over it (`pptx_chart.rs`).

use ibis_ooxml::{Package, XmlDocument};

use super::pptx::{next_part_name, relative};
use crate::Error;

pub(crate) const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const REL_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CT_CHART_EX: &str = "application/vnd.ms-office.chartex+xml";
const REL_CHART_EX: &str = "http://schemas.microsoft.com/office/2014/relationships/chartEx";
const REL_CHART_STYLE: &str = "http://schemas.microsoft.com/office/2011/relationships/chartStyle";
const REL_CHART_COLOR_STYLE: &str = "http://schemas.microsoft.com/office/2011/relationships/chartColorStyle";
const CT_CHART_STYLE: &str = "application/vnd.ms-office.chartstyle+xml";
const CT_CHART_COLOR_STYLE: &str = "application/vnd.ms-office.chartcolorstyle+xml";
const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Kinds of new chart available in 挿入 > グラフ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewChartKind {
    /// 集合縦棒 (既定)
    ClusteredColumn,
    /// 積み上げ縦棒
    StackedColumn,
    /// 100% 積み上げ縦棒
    PercentStackedColumn,
    /// 3D 集合縦棒
    ClusteredColumn3D,
    /// 3D 積み上げ縦棒
    StackedColumn3D,
    /// 100% 3D 積み上げ縦棒
    PercentStackedColumn3D,
    /// 3D 縦棒 (系列方向3D縦棒)
    Column3D,
    /// 集合横棒
    ClusteredBar,
    /// 積み上げ横棒
    StackedBar,
    /// 100% 積み上げ横棒
    PercentStackedBar,
    /// 3D 集合横棒
    ClusteredBar3D,
    /// 3D 積み上げ横棒
    StackedBar3D,
    /// 100% 3D 積み上げ横棒
    PercentStackedBar3D,
    /// 折れ線
    Line,
    /// 積み上げ折れ線
    StackedLine,
    /// 100% 積み上げ折れ線
    PercentStackedLine,
    /// 3D 折れ線
    Line3D,
    /// 面
    Area,
    /// 積み上げ面
    StackedArea,
    /// 100% 積み上げ面
    PercentStackedArea,
    /// 3D 面
    Area3D,
    /// 3D 積み上げ面
    StackedArea3D,
    /// 100% 3D 積み上げ面
    PercentStackedArea3D,
    /// 円
    Pie,
    /// 3D 円
    Pie3D,
    /// ドーナツ
    Doughnut,
    /// 散布図 (マーカー)
    Scatter,
    /// 散布図 (平滑線とマーカー)
    ScatterSmooth,
    /// 散布図 (直線とマーカー)
    ScatterLines,
    /// バブル
    Bubble,
    /// レーダー
    Radar,
    /// 塗りつぶしレーダー
    RadarFilled,
    /// 補助円グラフ
    PieOfPie,
    /// 補助棒グラフ
    BarOfPie,
    /// 株価 (高値-安値-終値)
    StockHLC,
    /// 株価 (始値-高値-安値-終値)
    StockOHLC,
    /// 株価 (出来高-高値-安値-終値)
    StockVHLC,
    /// 株価 (出来高-始値-高値-安値-終値)
    StockVOHLC,
    /// 3D 表面
    Surface3D,
    /// 3D ワイヤーフレーム表面
    Surface3DWireframe,
    /// 等高線
    Contour,
    /// ワイヤーフレーム等高線
    ContourWireframe,
    /// 集合縦棒 - 折れ線
    ComboColumnLine,
    /// 集合縦棒 - 折れ線 (第 2 軸)
    ComboColumnLineSecondaryAxis,
    /// 積み上げ面 - 集合縦棒
    ComboAreaColumn,
    /// ウォーターフォール (chartEx)
    Waterfall,
    /// ヒストグラム (chartEx)
    Histogram,
    /// パレート図 (chartEx)
    Pareto,
    /// じょうご (chartEx)
    Funnel,
    /// 箱ひげ図 (chartEx)
    BoxWhisker,
    /// ツリーマップ (chartEx)
    Treemap,
    /// サンバースト (chartEx)
    Sunburst,
}

impl NewChartKind {
    /// A new kind of chart (a `cx:chartSpace` part, "chartEx").
    pub fn is_chart_ex(self) -> bool {
        matches!(self, Self::Waterfall | Self::Histogram | Self::Pareto | Self::Funnel | Self::BoxWhisker | Self::Treemap | Self::Sunburst)
    }
}

impl Default for NewChartKind {
    fn default() -> Self {
        Self::ClusteredColumn
    }
}

/// The sample data PowerPoint puts in a new column/bar/line/area chart.
const SERIES: [&str; 3] = ["系列 1", "系列 2", "系列 3"];
const CATEGORIES: [&str; 4] = ["分類 1", "分類 2", "分類 3", "分類 4"];
const VALUES: [[f64; 4]; 3] = [[4.3, 2.5, 3.5, 4.5], [2.4, 4.4, 1.8, 2.8], [2.0, 2.0, 3.0, 5.0]];

/// The sample data PowerPoint puts in a new pie/doughnut chart.
const PIE_SERIES: [&str; 1] = ["売上"];
const PIE_CATEGORIES: [&str; 4] = ["第 1 四半期", "第 2 四半期", "第 3 四半期", "第 4 四半期"];
const PIE_VALUES: [f64; 4] = [8.2, 3.2, 1.4, 1.2];

/// The sample data PowerPoint puts in a new scatter/bubble chart.
const SCATTER_X_VALUES: [f64; 4] = [10.0, 20.0, 30.0, 40.0];
const SCATTER_Y_VALUES: [f64; 4] = [2.0, 4.0, 3.0, 5.0];
const BUBBLE_SIZES: [f64; 4] = [10.0, 25.0, 40.0, 15.0];

/// The data as the embedded workbook's CSV (A1 a space, as PowerPoint's:
/// the table's first column needs a name).
fn data_csv() -> String {
    let mut out = format!(" ,{}\n", SERIES.join(","));
    for (i, c) in CATEGORIES.iter().enumerate() {
        out.push_str(c);
        for v in VALUES.iter() {
            out.push_str(&format!(",{}", v[i]));
        }
        out.push('\n');
    }
    out
}

fn pie_data_csv() -> String {
    let mut out = format!(" ,{}\n", PIE_SERIES.join(","));
    for (i, c) in PIE_CATEGORIES.iter().enumerate() {
        out.push_str(&format!("{c},{}\n", PIE_VALUES[i]));
    }
    out
}

fn scatter_data_csv() -> String {
    let mut out = " ,系列 1\n".to_string();
    for i in 0..4 {
        out.push_str(&format!("{},{}\n", SCATTER_X_VALUES[i], SCATTER_Y_VALUES[i]));
    }
    out
}

fn bubble_data_csv() -> String {
    let mut out = " ,Y 値,サイズ\n".to_string();
    for i in 0..4 {
        out.push_str(&format!("{},{},{}\n", SCATTER_X_VALUES[i], SCATTER_Y_VALUES[i], BUBBLE_SIZES[i]));
    }
    out
}

const STOCK_CATEGORIES: [&str; 4] = ["日 1", "日 2", "日 3", "日 4"];
const STOCK_HLC_SERIES: [&str; 3] = ["高値", "安値", "終値"];
const STOCK_HLC_VALUES: [[f64; 4]; 3] = [
    [30.0, 35.0, 32.0, 28.0],
    [20.0, 22.0, 18.0, 19.0],
    [25.0, 31.0, 24.0, 22.0],
];
const STOCK_OHLC_SERIES: [&str; 4] = ["始値", "高値", "安値", "終値"];
const STOCK_OHLC_VALUES: [[f64; 4]; 4] = [
    [22.0, 25.0, 29.0, 21.0],
    [30.0, 35.0, 32.0, 28.0],
    [20.0, 22.0, 18.0, 19.0],
    [25.0, 31.0, 24.0, 22.0],
];

fn stock_hlc_data_csv() -> String {
    let mut out = format!(" ,{}\n", STOCK_HLC_SERIES.join(","));
    for (i, c) in STOCK_CATEGORIES.iter().enumerate() {
        out.push_str(c);
        for v in STOCK_HLC_VALUES.iter() {
            out.push_str(&format!(",{}", v[i]));
        }
        out.push('\n');
    }
    out
}

fn stock_ohlc_data_csv() -> String {
    let mut out = format!(" ,{}\n", STOCK_OHLC_SERIES.join(","));
    for (i, c) in STOCK_CATEGORIES.iter().enumerate() {
        out.push_str(c);
        for v in STOCK_OHLC_VALUES.iter() {
            out.push_str(&format!(",{}", v[i]));
        }
        out.push('\n');
    }
    out
}

const STOCK_VHLC_SERIES: [&str; 4] = ["出来高", "高値", "安値", "終値"];
const STOCK_VHLC_VALUES: [[f64; 4]; 4] = [
    [1000.0, 1500.0, 800.0, 1200.0],
    [30.0, 35.0, 32.0, 28.0],
    [20.0, 22.0, 18.0, 19.0],
    [25.0, 31.0, 24.0, 22.0],
];
const STOCK_VOHLC_SERIES: [&str; 5] = ["出来高", "始値", "高値", "安値", "終値"];
const STOCK_VOHLC_VALUES: [[f64; 4]; 5] = [
    [1000.0, 1500.0, 800.0, 1200.0],
    [22.0, 25.0, 29.0, 21.0],
    [30.0, 35.0, 32.0, 28.0],
    [20.0, 22.0, 18.0, 19.0],
    [25.0, 31.0, 24.0, 22.0],
];

fn stock_vhlc_data_csv() -> String {
    let mut out = format!(" ,{}\n", STOCK_VHLC_SERIES.join(","));
    for (i, c) in STOCK_CATEGORIES.iter().enumerate() {
        out.push_str(c);
        for v in STOCK_VHLC_VALUES.iter() {
            out.push_str(&format!(",{}", v[i]));
        }
        out.push('\n');
    }
    out
}

fn stock_vohlc_data_csv() -> String {
    let mut out = format!(" ,{}\n", STOCK_VOHLC_SERIES.join(","));
    for (i, c) in STOCK_CATEGORIES.iter().enumerate() {
        out.push_str(c);
        for v in STOCK_VOHLC_VALUES.iter() {
            out.push_str(&format!(",{}", v[i]));
        }
        out.push('\n');
    }
    out
}


/// The embedded workbook: the data, made a table (テーブル1, A1:D5) as
/// PowerPoint makes it, so that Excel grows the chart's ranges with it.
fn workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: CATEGORIES.len() as u32, last_column: SERIES.len() as u32 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: CATEGORIES.len() as u32, last_column: SERIES.len() as u32 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn pie_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&pie_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: PIE_CATEGORIES.len() as u32, last_column: PIE_SERIES.len() as u32 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(PIE_SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: PIE_CATEGORIES.len() as u32, last_column: PIE_SERIES.len() as u32 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn scatter_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&scatter_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 1 },
        header_rows: 1,
        totals_rows: 0,
        columns: vec![" ".to_string(), "系列 1".to_string()],
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 1 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn bubble_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&bubble_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 2 },
        header_rows: 1,
        totals_rows: 0,
        columns: vec![" ".to_string(), "Y 値".to_string(), "サイズ".to_string()],
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 2 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn stock_hlc_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&stock_hlc_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 3 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(STOCK_HLC_SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 3 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn stock_ohlc_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&stock_ohlc_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 4 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(STOCK_OHLC_SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 4 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn stock_vhlc_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&stock_vhlc_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 4 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(STOCK_VHLC_SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 4 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

fn stock_vohlc_workbook() -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let opened = crate::open(crate::csv::workbook_from(&stock_vohlc_data_csv(), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let table = SheetTable {
        name: "テーブル1".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 5 },
        header_rows: 1,
        totals_rows: 0,
        columns: std::iter::once(" ".to_string()).chain(STOCK_VOHLC_SERIES.iter().map(|s| s.to_string())).collect(),
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range: CellRange { first_row: 0, first_column: 0, last_row: 4, last_column: 5 }, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}


/// Text in grey (the theme's text colour lightened), `size` hundredths of a point.
/// The sample data of a new chartEx chart: the categories (none for a histogram) and the values.
fn ex_data(kind: NewChartKind) -> (Vec<&'static str>, Vec<f64>, &'static str) {
    match kind {
        NewChartKind::Waterfall => (vec!["開始", "増加 1", "減少 1", "増加 2", "合計"], vec![100.0, 40.0, -25.0, 10.0, 125.0], "売上"),
        NewChartKind::Pareto => (vec!["分類 A", "分類 B", "分類 C", "分類 D", "分類 E"], vec![42.0, 27.0, 16.0, 9.0, 6.0], "件数"),
        NewChartKind::Funnel => (vec!["問い合わせ", "見積", "商談", "受注"], vec![120.0, 80.0, 45.0, 20.0], "件数"),
        NewChartKind::BoxWhisker => (vec!["A", "A", "A", "A", "B", "B", "B", "B"], vec![10.0, 12.0, 14.0, 20.0, 8.0, 15.0, 16.0, 30.0], "値"),
        NewChartKind::Treemap | NewChartKind::Sunburst => (vec!["A", "A", "B", "B", "C"], vec![40.0, 25.0, 20.0, 10.0, 5.0], "売上"),
        // Histogram: the scores of a class.
        _ => (Vec::new(), vec![35.0, 42.0, 48.0, 51.0, 55.0, 58.0, 60.0, 62.0, 63.0, 65.0, 67.0, 68.0, 70.0, 72.0, 73.0, 75.0, 78.0, 81.0, 85.0, 92.0], "点数"),
    }
}

fn ex_csv(kind: NewChartKind) -> String {
    let (cats, values, name) = ex_data(kind);
    if cats.is_empty() {
        let mut out = format!("{name}\n");
        for v in values {
            out.push_str(&format!("{v}\n"));
        }
        return out;
    }
    let mut out = format!(" ,{name}\n");
    for (c, v) in cats.iter().zip(values) {
        out.push_str(&format!("{c},{v}\n"));
    }
    out
}

/// The workbook of a chartEx chart's sample data, as a table.
fn ex_workbook(kind: NewChartKind) -> Result<Vec<u8>, Error> {
    use ibis_doc_model::{CellRange, Command, SheetCommand, SheetTable};
    let (cats, values, name) = ex_data(kind);
    let opened = crate::open(crate::csv::workbook_from(&ex_csv(kind), "Sheet1")?)?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
    let last_column = if cats.is_empty() { 0 } else { 1 };
    let range = CellRange { first_row: 0, first_column: 0, last_row: values.len() as u32, last_column };
    let table = SheetTable {
        name: "テーブル1".into(),
        range,
        header_rows: 1,
        totals_rows: 0,
        columns: if cats.is_empty() { vec![name.to_string()] } else { vec![" ".to_string(), name.to_string()] },
        style: None,
        filter: Some(ibis_doc_model::AutoFilter { range, columns: Vec::new() }),
        totals: Vec::new(),
    };
    doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
    opened.source.save(doc.presentation(), crate::SaveOptions::default())
}

/// What a chartEx chart is made of: where its values are (a reference to cells) and what they are (cached).
pub(crate) struct ExCells {
    /// The series' name: its cell and the text.
    pub name: Option<(String, String)>,
    /// The categories (a histogram has none): their cells and the texts of each level, the finest level first (a
    /// treemap or a sunburst has a hierarchy: the cells are several columns, each column a level; the others one).
    pub categories: Option<(String, Vec<Vec<String>>)>,
    /// The values: their cells and numbers (a blank is `None`).
    pub values: (String, Vec<Option<f64>>),
    /// The title of the chart.
    pub title: String,
}

/// A chartEx part (`cx:chartSpace`) of `kind` with its sample data cached, the references into its workbook (Sheet1).
fn chart_ex_xml(book_rel: &str, kind: NewChartKind) -> String {
    let (cats, values, name) = ex_data(kind);
    let n = values.len();
    let with_cats = !cats.is_empty();
    let value_column = if with_cats { 'B' } else { 'A' };
    let cells = ExCells {
        name: Some((format!("Sheet1!${value_column}$1"), name.to_string())),
        categories: with_cats.then(|| (format!("Sheet1!$A$2:$A${}", n + 1), vec![cats.iter().map(|c| c.to_string()).collect()])),
        values: (format!("Sheet1!${value_column}$2:${value_column}${}", n + 1), values.iter().map(|v| Some(*v)).collect()),
        title: name.to_string(),
    };
    chart_ex_with(kind, &cells, true, Some(book_rel))
}

/// A chartEx part of `kind` over `cells`: with the total marks of a waterfall's first and last point when
/// `subtotals`, and its workbook `book_rel` when it has one (a chart in a presentation; one in a workbook reads
/// the sheet's cells).
pub(crate) fn chart_ex_with(kind: NewChartKind, cells: &ExCells, subtotals: bool, book_rel: Option<&str>) -> String {
    let name = super::pptx_new::escape(&cells.title);
    let n = cells.values.1.len();
    let points = |items: Vec<String>| items.iter().enumerate().map(|(i, v)| format!("<cx:pt idx=\"{i}\">{}</cx:pt>", super::pptx_new::escape(v))).collect::<String>();
    let cat_dim = match &cells.categories {
        Some((f, levels)) => {
            let count = levels.first().map_or(0, Vec::len);
            let lvls: String = levels.iter().map(|texts| format!("<cx:lvl ptCount=\"{}\">{}</cx:lvl>", texts.len().max(count), points(texts.clone()))).collect();
            format!("<cx:strDim type=\"cat\"><cx:f>{f}</cx:f>{lvls}</cx:strDim>")
        }
        None => String::new(),
    };
    // A blank value has no point.
    let value_points: String = cells.values.1.iter().enumerate().filter_map(|(i, v)| v.map(|v| format!("<cx:pt idx=\"{i}\">{v}</cx:pt>"))).collect();
    // A treemap's and a sunburst's values are sizes.
    let dim_type = if matches!(kind, NewChartKind::Treemap | NewChartKind::Sunburst) { "size" } else { "val" };
    let val_dim = format!("<cx:numDim type=\"{dim_type}\"><cx:f>{}</cx:f><cx:lvl ptCount=\"{n}\" formatCode=\"General\">{value_points}</cx:lvl></cx:numDim>", cells.values.0);
    let series_name = match &cells.name {
        Some((f, text)) => format!("<cx:tx><cx:txData><cx:f>{f}</cx:f><cx:v>{}</cx:v></cx:txData></cx:tx>", super::pptx_new::escape(text)),
        None => String::new(),
    };
    let total_marks = if subtotals { format!("<cx:subtotals><cx:idx val=\"0\"/><cx:idx val=\"{}\"/></cx:subtotals>", n.saturating_sub(1)) } else { String::new() };
    // A Pareto chart gets no bar labels: PowerPoint shows them in the cumulative line's percentage format even with numFmt General (checked), and an Excel-made one has none either.
    let labels = "<cx:dataLabels pos=\"outEnd\"><cx:numFmt formatCode=\"General\" sourceLinked=\"0\"/><cx:visibility seriesName=\"0\" categoryName=\"0\" value=\"1\"/></cx:dataLabels>";
    let (series, axes) = match kind {
        NewChartKind::Waterfall => (
            format!("<cx:series layoutId=\"waterfall\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}{labels}<cx:dataId val=\"0\"/><cx:layoutPr><cx:visibility connectorLines=\"1\"/>{total_marks}</cx:layoutPr></cx:series>"),
            "<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0.5\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/><cx:majorGridlines/><cx:tickLabels/></cx:axis>".to_string(),
        ),
        NewChartKind::Pareto => (
            format!("<cx:series layoutId=\"clusteredColumn\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}<cx:dataId val=\"0\"/><cx:axisId val=\"1\"/></cx:series><cx:series layoutId=\"paretoLine\" ownerIdx=\"0\" uniqueId=\"{{00000000-0000-4000-8000-000000000002}}\"><cx:axisId val=\"2\"/></cx:series>"),
            "<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0.5\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/><cx:majorGridlines/><cx:tickLabels/></cx:axis><cx:axis id=\"2\"><cx:valScaling max=\"1\" min=\"0\"/><cx:units unit=\"percentage\"/><cx:tickLabels/></cx:axis>".to_string(),
        ),
        NewChartKind::Funnel => (
            format!("<cx:series layoutId=\"funnel\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}{labels}<cx:dataId val=\"0\"/></cx:series>"),
            "<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0.06\"/><cx:tickLabels/></cx:axis>".to_string(),
        ),
        NewChartKind::BoxWhisker => (
            format!("<cx:series layoutId=\"boxWhisker\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}<cx:dataId val=\"0\"/><cx:layoutPr><cx:visibility meanLine=\"0\" meanMarker=\"1\" nonoutliers=\"0\" outliers=\"1\"/><cx:statistics quartileMethod=\"exclusive\"/></cx:layoutPr></cx:series>"),
            "<cx:axis id=\"0\"><cx:catScaling gapWidth=\"1\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/><cx:majorGridlines/><cx:tickLabels/></cx:axis>".to_string(),
        ),
        NewChartKind::Treemap => (
            format!("<cx:series layoutId=\"treemap\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}<cx:dataLabels pos=\"inEnd\"><cx:visibility seriesName=\"0\" categoryName=\"1\" value=\"0\"/></cx:dataLabels><cx:dataId val=\"0\"/><cx:layoutPr><cx:parentLabelLayout val=\"overlapping\"/></cx:layoutPr></cx:series>"),
            String::new(),
        ),
        NewChartKind::Sunburst => (
            format!("<cx:series layoutId=\"sunburst\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}<cx:dataLabels pos=\"inEnd\"><cx:visibility seriesName=\"0\" categoryName=\"1\" value=\"0\"/></cx:dataLabels><cx:dataId val=\"0\"/></cx:series>"),
            String::new(),
        ),
        _ => (
            format!("<cx:series layoutId=\"clusteredColumn\" uniqueId=\"{{00000000-0000-4000-8000-000000000001}}\">{series_name}<cx:dataId val=\"0\"/><cx:layoutPr><cx:binning intervalClosed=\"r\"/></cx:layoutPr></cx:series>"),
            "<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/><cx:majorGridlines/><cx:tickLabels/></cx:axis>".to_string(),
        ),
    };
    let external = book_rel.map(|rel| format!("<cx:externalData r:id=\"{rel}\" cx:autoUpdate=\"0\"/>")).unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cx:chartSpace xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chartData>{external}<cx:data id=\"0\">{cat_dim}{val_dim}</cx:data></cx:chartData><cx:chart><cx:title pos=\"t\" align=\"ctr\" overlay=\"0\"><cx:tx><cx:txData><cx:v>{name}</cx:v></cx:txData></cx:tx></cx:title><cx:plotArea><cx:plotAreaRegion>{series}</cx:plotAreaRegion>{axes}</cx:plotArea><cx:legend pos=\"t\" align=\"ctr\" overlay=\"0\"/></cx:chart></cx:chartSpace>"
    )
}

fn text_props(size: u32) -> String {
    format!(
        "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"{size}\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"65000\"/><a:lumOff val=\"35000\"/></a:schemeClr></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"ja-JP\"/></a:p></c:txPr>"
    )
}

/// Pie / doughnut / ofPie chart XML.
fn pie_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let cats: String = PIE_CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();
    let vals: String = PIE_VALUES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let ser = format!(
        "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>売上</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
    );
    let (chart_elem, is_3d) = match kind {
        NewChartKind::Doughnut => (format!("<c:doughnutChart><c:varyColors val=\"1\"/>{ser}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"75\"/></c:doughnutChart>"), false),
        NewChartKind::PieOfPie => (format!("<c:ofPieChart><c:ofPieType val=\"pie\"/><c:varyColors val=\"1\"/>{ser}<c:splitType val=\"pos\"/><c:splitPos val=\"2\"/><c:secondPieSize val=\"75\"/><c:serLines/></c:ofPieChart>"), false),
        NewChartKind::BarOfPie => (format!("<c:ofPieChart><c:ofPieType val=\"bar\"/><c:varyColors val=\"1\"/>{ser}<c:splitType val=\"pos\"/><c:splitPos val=\"2\"/><c:secondPieSize val=\"75\"/><c:serLines/></c:ofPieChart>"), false),
        NewChartKind::Pie3D => (format!("<c:pie3DChart><c:varyColors val=\"1\"/>{ser}</c:pie3DChart>"), true),
        _ => (format!("<c:pieChart><c:varyColors val=\"1\"/>{ser}<c:firstSliceAng val=\"0\"/></c:pieChart>"), false),
    };
    let view3d = if is_3d {
        "<c:view3D><c:rotX val=\"30\"/><c:rotY val=\"0\"/><c:rAngAx val=\"0\"/></c:view3D>"
    } else {
        ""
    };
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/>{view3d}<c:plotArea><c:layout/>\
{chart_elem}\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"r\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Scatter chart XML.
fn scatter_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let (style_str, smooth_val, line_elem) = match kind {
        NewChartKind::ScatterSmooth => ("smoothMarker", "1", "<c:spPr><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:round/></a:ln></c:spPr>"),
        NewChartKind::ScatterLines => ("lineMarker", "0", "<c:spPr><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:round/></a:ln></c:spPr>"),
        _ => ("marker", "0", "<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>"),
    };
    let x_pts: String = SCATTER_X_VALUES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let y_pts: String = SCATTER_Y_VALUES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let ser = format!(
        "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>系列 1</c:v></c:pt></c:strCache></c:strRef></c:tx>\
{line_elem}<c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/><c:spPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr></c:marker>\
<c:xVal><c:numRef><c:f>Sheet1!$A$2:$A$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{x_pts}</c:numCache></c:numRef></c:xVal>\
<c:yVal><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{y_pts}</c:numCache></c:numRef></c:yVal><c:smooth val=\"{smooth_val}\"/></c:ser>"
    );
    let chart_elem = format!("<c:scatterChart><c:scatterStyle val=\"{style_str}\"/><c:varyColors val=\"0\"/>{ser}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:scatterChart>");
    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>\
{chart_elem}\
<c:valAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Bubble chart XML.
fn bubble_chart_xml(book_rel: &str) -> String {
    let x_pts: String = SCATTER_X_VALUES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let y_pts: String = SCATTER_Y_VALUES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let b_pts: String = BUBBLE_SIZES.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let ser = format!(
        "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>系列 1</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr><c:invertIfNegative val=\"0\"/>\
<c:xVal><c:numRef><c:f>Sheet1!$A$2:$A$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{x_pts}</c:numCache></c:numRef></c:xVal>\
<c:yVal><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{y_pts}</c:numCache></c:numRef></c:yVal>\
<c:bubbleSize><c:numRef><c:f>Sheet1!$C$2:$C$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{b_pts}</c:numCache></c:numRef></c:bubbleSize><c:bubble3D val=\"0\"/></c:ser>"
    );
    let chart_elem = format!("<c:bubbleChart><c:varyColors val=\"0\"/>{ser}<c:bubbleScale val=\"100\"/><c:showNegBubbles val=\"0\"/><c:sizeRepresents val=\"area\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bubbleChart>");
    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>\
{chart_elem}\
<c:valAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Stock chart XML.
fn stock_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let is_ohlc = kind == NewChartKind::StockOHLC;
    let series_names = if is_ohlc { &STOCK_OHLC_SERIES[..] } else { &STOCK_HLC_SERIES[..] };
    let values_matrix = if is_ohlc { &STOCK_OHLC_VALUES[..] } else { &STOCK_HLC_VALUES[..] };
    let column = |i: usize| char::from(b'B' + i as u8);
    let mut series = String::new();
    for (s, name) in series_names.iter().enumerate() {
        let cats: String = STOCK_CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();
        let vals: String = values_matrix[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let col = column(s);
        let ser_body = format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
        );
        series.push_str(&ser_body);
    }
    let extra_elems = if is_ohlc {
        "<c:hiLowLines/><c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>"
    } else {
        "<c:hiLowLines/>"
    };
    let chart_elem = format!("<c:stockChart>{series}{extra_elems}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:stockChart>");
    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>\
{chart_elem}\
<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
{no_line}</c:plotArea><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Volume stock chart XML (combination of barChart for volume and stockChart for price).
fn stock_volume_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let is_vohlc = kind == NewChartKind::StockVOHLC;
    let series_names = if is_vohlc { &STOCK_VOHLC_SERIES[1..] } else { &STOCK_VHLC_SERIES[1..] };
    let values_matrix = if is_vohlc { &STOCK_VOHLC_VALUES[1..] } else { &STOCK_VHLC_VALUES[1..] };
    let vol_values = if is_vohlc { &STOCK_VOHLC_VALUES[0] } else { &STOCK_VHLC_VALUES[0] };

    // Group 0: barChart for Volume (column B, primary axes: catAx 100000001, valAx 100000002)
    let cats: String = STOCK_CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();
    let vol_vals: String = vol_values.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
    let vol_ser = format!(
        "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>出来高</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vol_vals}</c:numCache></c:numRef></c:val></c:ser>"
    );
    let bar_chart_elem = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{vol_ser}<c:gapWidth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>");

    // Group 1: stockChart for Price series (columns C.., axes: catAx 100000001, secondary valAx 100000003)
    let mut stock_series = String::new();
    for (i, name) in series_names.iter().enumerate() {
        let s = i + 1;
        let col = char::from(b'B' + s as u8);
        let vals: String = values_matrix[i].iter().enumerate().map(|(vi, v)| format!("<c:pt idx=\"{vi}\"><c:v>{v}</c:v></c:pt>")).collect();
        let ser_body = format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
        );
        stock_series.push_str(&ser_body);
    }
    let extra_elems = if is_vohlc {
        "<c:hiLowLines/><c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>"
    } else {
        "<c:hiLowLines/>"
    };
    let stock_chart_elem = format!("<c:stockChart>{stock_series}{extra_elems}<c:axId val=\"100000001\"/><c:axId val=\"100000003\"/></c:stockChart>");

    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>\
{bar_chart_elem}\
{stock_chart_elem}\
<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
<c:valAx><c:axId val=\"100000003\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"r\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"max\"/><c:crossBetween val=\"between\"/></c:valAx>\
{no_line}</c:plotArea><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Surface / Contour chart XML.
fn surface_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let (tag, is_3d) = match kind {
        NewChartKind::Surface3D => ("surface3DChart", true),
        NewChartKind::Surface3DWireframe => ("surface3DChart", true),
        NewChartKind::Contour => ("surfaceChart", false),
        NewChartKind::ContourWireframe => ("surfaceChart", false),
        _ => unreachable!(),
    };
    let wireframe_val = if matches!(kind, NewChartKind::Surface3DWireframe | NewChartKind::ContourWireframe) { "1" } else { "0" };
    let column = |i: usize| char::from(b'B' + i as u8);
    let mut series = String::new();
    for (s, name) in SERIES.iter().enumerate() {
        let cats: String = CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();
        let vals: String = VALUES[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let col = column(s);
        let ser_body = format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
        );
        series.push_str(&ser_body);
    }
    let chart_elem = format!("<c:{tag}><c:wireframe val=\"{wireframe_val}\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/><c:axId val=\"100000003\"/></c:{tag}>");
    let view3d = if is_3d {
        "<c:view3D><c:rotX val=\"30\"/><c:rotY val=\"30\"/><c:rAngAx val=\"1\"/></c:view3D>"
    } else {
        ""
    };
    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/>{view3d}<c:plotArea><c:layout/>\
{chart_elem}\
<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
<c:serAx><c:axId val=\"100000003\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/></c:serAx>\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"r\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// Combo chart XML (集合縦棒 - 折れ線、集合縦棒 - 折れ線 (第 2 軸)、積み上げ面 - 集合縦棒).
fn combo_chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    let column = |i: usize| char::from(b'B' + i as u8);
    let cats: String = CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();

    let make_bar_ser = |s: usize| {
        let col = column(s);
        let vals: String = VALUES[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let name = SERIES[s];
        let n = s + 1;
        format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr><c:invertIfNegative val=\"0\"/>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
        )
    };

    let make_line_ser = |s: usize| {
        let col = column(s);
        let vals: String = VALUES[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let name = SERIES[s];
        let n = s + 1;
        format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:round/></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val><c:smooth val=\"0\"/></c:ser>"
        )
    };

    let make_area_ser = |s: usize| {
        let col = column(s);
        let vals: String = VALUES[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let name = SERIES[s];
        let n = s + 1;
        format!(
            "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
        )
    };

    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);

    let (groups_elem, axes_elem) = match kind {
        NewChartKind::ComboColumnLineSecondaryAxis => {
            let bar_series = format!("{}{}", make_bar_ser(0), make_bar_ser(1));
            let line_series = make_line_ser(2);
            let bar_group = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{bar_series}<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>");
            let line_group = format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{line_series}<c:marker val=\"1\"/><c:axId val=\"100000001\"/><c:axId val=\"100000003\"/></c:lineChart>");
            let axes = format!(
                "<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
<c:valAx><c:axId val=\"100000003\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"r\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>"
            );
            (format!("{bar_group}{line_group}"), axes)
        }
        NewChartKind::ComboAreaColumn => {
            let area_series = format!("{}{}", make_area_ser(0), make_area_ser(1));
            let bar_series = make_bar_ser(2);
            let area_group = format!("<c:areaChart><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{area_series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:areaChart>");
            let bar_group = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{bar_series}<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>");
            let axes = format!(
                "<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>"
            );
            (format!("{area_group}{bar_group}"), axes)
        }
        _ => {
            let bar_series = format!("{}{}", make_bar_ser(0), make_bar_ser(1));
            let line_series = make_line_ser(2);
            let bar_group = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{bar_series}<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>");
            let line_group = format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{line_series}<c:marker val=\"1\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:lineChart>");
            let axes = format!(
                "<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>"
            );
            (format!("{bar_group}{line_group}"), axes)
        }
    };

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>\
{groups_elem}\
{axes_elem}\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// The chart part's XML, its data from the workbook related as `book_rel`.
fn chart_xml(book_rel: &str, kind: NewChartKind) -> String {
    if matches!(
        kind,
        NewChartKind::ComboColumnLine
            | NewChartKind::ComboColumnLineSecondaryAxis
            | NewChartKind::ComboAreaColumn
    ) {
        return combo_chart_xml(book_rel, kind);
    }
    if matches!(kind, NewChartKind::Pie | NewChartKind::Pie3D | NewChartKind::Doughnut | NewChartKind::PieOfPie | NewChartKind::BarOfPie) {
        return pie_chart_xml(book_rel, kind);
    }
    if matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines) {
        return scatter_chart_xml(book_rel, kind);
    }
    if kind == NewChartKind::Bubble {
        return bubble_chart_xml(book_rel);
    }
    if matches!(kind, NewChartKind::StockHLC | NewChartKind::StockOHLC) {
        return stock_chart_xml(book_rel, kind);
    }
    if matches!(kind, NewChartKind::StockVHLC | NewChartKind::StockVOHLC) {
        return stock_volume_chart_xml(book_rel, kind);
    }
    if matches!(
        kind,
        NewChartKind::Surface3D
            | NewChartKind::Surface3DWireframe
            | NewChartKind::Contour
            | NewChartKind::ContourWireframe
    ) {
        return surface_chart_xml(book_rel, kind);
    }
    let column = |i: usize| char::from(b'B' + i as u8);
    let mut series = String::new();
    for (s, name) in SERIES.iter().enumerate() {
        let cats: String = CATEGORIES.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{c}</c:v></c:pt>")).collect();
        let vals: String = VALUES[s].iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")).collect();
        let col = column(s);
        let n = s + 1;
        let ser_body = match kind {
            NewChartKind::Line | NewChartKind::StackedLine | NewChartKind::PercentStackedLine | NewChartKind::Line3D | NewChartKind::Radar => {
                format!(
                    "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:round/></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val><c:smooth val=\"0\"/></c:ser>"
                )
            }
            NewChartKind::Area | NewChartKind::StackedArea | NewChartKind::PercentStackedArea | NewChartKind::Area3D | NewChartKind::StackedArea3D | NewChartKind::PercentStackedArea3D | NewChartKind::RadarFilled => {
                format!(
                    "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
                )
            }
            _ => {
                // Column or Bar
                format!(
                    "<c:ser><c:idx val=\"{s}\"/><c:order val=\"{s}\"/><c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>\
<c:spPr><a:solidFill><a:schemeClr val=\"accent{n}\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr><c:invertIfNegative val=\"0\"/>\
<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val=\"4\"/>{cats}</c:strCache></c:strRef></c:cat>\
<c:val><c:numRef><c:f>Sheet1!${col}$2:${col}$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"
                )
            }
        };
        series.push_str(&ser_body);
    }
    let group_elem = match kind {
        NewChartKind::ClusteredColumn => format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::StackedColumn => format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:overlap val=\"100\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::PercentStackedColumn => format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:overlap val=\"100\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::ClusteredColumn3D => format!("<c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::StackedColumn3D => format!("<c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::PercentStackedColumn3D => format!("<c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::Column3D => format!("<c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::ClusteredBar => format!("<c:barChart><c:barDir val=\"bar\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::StackedBar => format!("<c:barChart><c:barDir val=\"bar\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:overlap val=\"100\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::PercentStackedBar => format!("<c:barChart><c:barDir val=\"bar\"/><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:overlap val=\"100\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:barChart>"),
        NewChartKind::ClusteredBar3D => format!("<c:bar3DChart><c:barDir val=\"bar\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::StackedBar3D => format!("<c:bar3DChart><c:barDir val=\"bar\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::PercentStackedBar3D => format!("<c:bar3DChart><c:barDir val=\"bar\"/><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:gapWidth val=\"150\"/><c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:bar3DChart>"),
        NewChartKind::Line => format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series}<c:marker val=\"1\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:lineChart>"),
        NewChartKind::StackedLine => format!("<c:lineChart><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:marker val=\"1\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:lineChart>"),
        NewChartKind::PercentStackedLine => format!("<c:lineChart><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:marker val=\"1\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:lineChart>"),
        NewChartKind::Line3D => format!("<c:line3DChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series}<c:gapDepth val=\"150\"/><c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:line3DChart>"),
        NewChartKind::Area => format!("<c:areaChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:areaChart>"),
        NewChartKind::StackedArea => format!("<c:areaChart><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:areaChart>"),
        NewChartKind::PercentStackedArea => format!("<c:areaChart><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:areaChart>"),
        NewChartKind::Area3D => format!("<c:area3DChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:area3DChart>"),
        NewChartKind::StackedArea3D => format!("<c:area3DChart><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:area3DChart>"),
        NewChartKind::PercentStackedArea3D => format!("<c:area3DChart><c:grouping val=\"percentStacked\"/><c:varyColors val=\"0\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:area3DChart>"),
        NewChartKind::Radar => format!("<c:radarChart><c:radarStyle val=\"standard\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:radarChart>"),
        NewChartKind::RadarFilled => format!("<c:radarChart><c:radarStyle val=\"filled\"/>{series}<c:axId val=\"100000001\"/><c:axId val=\"100000002\"/></c:radarChart>"),
        _ => unreachable!(),
    };

    let is_bar = matches!(
        kind,
        NewChartKind::ClusteredBar
            | NewChartKind::StackedBar
            | NewChartKind::PercentStackedBar
            | NewChartKind::ClusteredBar3D
            | NewChartKind::StackedBar3D
            | NewChartKind::PercentStackedBar3D
    );
    let cat_pos = if is_bar { "l" } else { "b" };
    let val_pos = if is_bar { "b" } else { "l" };
    let is_percent = matches!(
        kind,
        NewChartKind::PercentStackedColumn
            | NewChartKind::PercentStackedBar
            | NewChartKind::PercentStackedLine
            | NewChartKind::PercentStackedArea
            | NewChartKind::PercentStackedColumn3D
            | NewChartKind::PercentStackedBar3D
            | NewChartKind::PercentStackedArea3D
    );
    let val_num_fmt = if is_percent {
        "<c:numFmt formatCode=\"0%\" sourceLinked=\"0\"/>"
    } else {
        "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>"
    };
    let is_3d = matches!(
        kind,
        NewChartKind::ClusteredColumn3D
            | NewChartKind::StackedColumn3D
            | NewChartKind::PercentStackedColumn3D
            | NewChartKind::Column3D
            | NewChartKind::ClusteredBar3D
            | NewChartKind::StackedBar3D
            | NewChartKind::PercentStackedBar3D
            | NewChartKind::Line3D
            | NewChartKind::Area3D
            | NewChartKind::StackedArea3D
            | NewChartKind::PercentStackedArea3D
    );
    let view3d = if is_3d {
        "<c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:rAngAx val=\"1\"/><c:depthPercent val=\"100\"/></c:view3D>"
    } else {
        ""
    };

    let grid = "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr></c:majorGridlines>";
    let axis_line = "<c:spPr><a:noFill/><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"15000\"/><a:lumOff val=\"85000\"/></a:schemeClr></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
    let text = text_props(1197);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\
<c:chart><c:autoTitleDeleted val=\"1\"/>{view3d}<c:plotArea><c:layout/>\
{group_elem}\
<c:catAx><c:axId val=\"100000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{axis_line}{text}<c:crossAx val=\"100000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"100000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/>{grid}{val_num_fmt}<c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{no_line}{text}<c:crossAx val=\"100000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>\
{no_line}</c:plotArea><c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/>{no_line}{text}</c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>\
{no_line}{text}<c:externalData r:id=\"{book_rel}\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
}

/// A new chart part with its workbook, related from slide `slide`;
/// returns the relationship's id.
pub(crate) fn add_chart_part(package: &mut Package, slide: &str, kind: NewChartKind) -> Result<(String, String), Error> {
    if kind.is_chart_ex() {
        return add_chart_ex_part(package, slide, kind);
    }
    add_chart_part_at(package, slide, kind, "ppt/charts/chart1.xml", "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx")
}

/// A new chart part (named like `chart_like`) with its workbook (named like `book_like`), related
/// from `owner` (a slide, or a document's body): the relationship's id and the part. Not a chartEx kind.
pub(crate) fn add_chart_part_at(package: &mut Package, slide: &str, kind: NewChartKind, chart_like: &str, book_like: &str) -> Result<(String, String), Error> {
    if kind.is_chart_ex() {
        return Err(Error::new("この種類のグラフは挿入できません"));
    }
    let chart = next_part_name(package, chart_like);
    let book = next_part_name(package, book_like);
    let bytes = match kind {
        NewChartKind::Pie | NewChartKind::Pie3D | NewChartKind::Doughnut | NewChartKind::PieOfPie | NewChartKind::BarOfPie => pie_workbook()?,
        NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines => scatter_workbook()?,
        NewChartKind::Bubble => bubble_workbook()?,
        NewChartKind::StockHLC => stock_hlc_workbook()?,
        NewChartKind::StockOHLC => stock_ohlc_workbook()?,
        NewChartKind::StockVHLC => stock_vhlc_workbook()?,
        NewChartKind::StockVOHLC => stock_vohlc_workbook()?,
        _ => workbook()?,
    };
    package.set_bytes(&book, bytes);
    package.set_override(&book, CT_XLSX)?;
    // The chart part first (empty), for its relationship to the workbook.
    package.set_xml(&chart, XmlDocument::parse(b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/>").map_err(|e| Error::new(format!("グラフを作れません: {e}")))?);
    let book_rel = package.add_relationship(&chart, REL_PACKAGE, &relative(&chart, &book), false)?;
    let doc = XmlDocument::parse(chart_xml(&book_rel, kind).as_bytes()).map_err(|e| Error::new(format!("グラフを作れません: {e}")))?;
    package.set_xml(&chart, doc);
    package.set_override(&chart, CT_CHART)?;
    let rel = package.add_relationship(slide, REL_CHART, &relative(slide, &chart), false)?;
    Ok((rel, chart))
}

/// The chart style (`cs:chartStyle`) and colour style (`cs:colorStyle`) parts of a chartEx part: Excel and
/// PowerPoint do not open the chart without them. Written from the schema ([MS-ODRAWXML]) with its required
/// elements only: every part of the chart takes its line, fill, effect and font from the theme (index 0:
/// none; the minor font) or from its own `spPr`, the colours cycle through the theme's accents.
fn add_chart_ex_style_parts(package: &mut Package, chart: &str) -> Result<(), Error> {
    let dir = chart.rsplit_once('/').map_or("", |(d, _)| d);
    let style = next_part_name(package, &format!("{dir}/style1.xml"));
    let colors = next_part_name(package, &format!("{dir}/colors1.xml"));
    const ENTRIES: [&str; 31] = [
        "axisTitle", "categoryAxis", "chartArea", "dataLabel", "dataLabelCallout", "dataPoint", "dataPoint3D", "dataPointLine", "dataPointMarker", "dataPointWireframe", "dataTable", "downBar", "dropLine", "errorBar", "floor",
        "gridlineMajor", "gridlineMinor", "hiLoLine", "leaderLine", "legend", "plotArea", "plotArea3D", "seriesAxis", "seriesLine", "title", "trendline", "trendlineLabel", "upBar", "valueAxis", "wall", "dataPointMarkerLayout",
    ];
    // The structure is [MS-ODRAWXML]'s; the values are chosen here: the data points take their series' colour (`phClr`, which the
    // colour style hands out; `styleClr auto` says so), the lines of the axes and of the grid are the text colour at a quarter of
    // its opacity, and the text is the theme's text colour.
    let line = "<cs:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"tx1\"><a:alpha val=\"25000\"/></a:schemeClr></a:solidFill></a:ln></cs:spPr>";
    let mut body = String::new();
    for name in ENTRIES.iter().filter(|n| **n != "dataPointMarkerLayout") {
        let (fill, extra) = match *name {
            "dataPoint" | "dataPoint3D" => ("<cs:fillRef idx=\"0\"><cs:styleClr val=\"auto\"/></cs:fillRef>".to_string(), "<cs:spPr><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></cs:spPr>".to_string()),
            "categoryAxis" => ("<cs:fillRef idx=\"0\"/>".to_string(), format!("{line}<cs:defRPr sz=\"1000\"/>")),
            "valueAxis" | "seriesAxis" | "dataLabel" | "legend" | "axisTitle" => ("<cs:fillRef idx=\"0\"/>".to_string(), "<cs:defRPr sz=\"1000\"/>".to_string()),
            "title" => ("<cs:fillRef idx=\"0\"/>".to_string(), "<cs:defRPr sz=\"1600\"/>".to_string()),
            "gridlineMajor" | "gridlineMinor" => ("<cs:fillRef idx=\"0\"/>".to_string(), line.to_string()),
            _ => ("<cs:fillRef idx=\"0\"/>".to_string(), String::new()),
        };
        body += &format!("<cs:{name}><cs:lnRef idx=\"0\"/>{fill}<cs:effectRef idx=\"0\"/><cs:fontRef idx=\"minor\"><a:schemeClr val=\"tx1\"/></cs:fontRef>{extra}</cs:{name}>");
    }
    let style_xml = format!("<cs:chartStyle xmlns:cs=\"http://schemas.microsoft.com/office/drawing/2012/chartStyle\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" id=\"201\">{body}</cs:chartStyle>");
    let colors_xml = "<cs:colorStyle xmlns:cs=\"http://schemas.microsoft.com/office/drawing/2012/chartStyle\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" meth=\"cycle\" id=\"10\"><a:schemeClr val=\"accent1\"/><a:schemeClr val=\"accent2\"/><a:schemeClr val=\"accent3\"/><a:schemeClr val=\"accent4\"/><a:schemeClr val=\"accent5\"/><a:schemeClr val=\"accent6\"/><cs:variation/></cs:colorStyle>";
    package.set_xml(&style, XmlDocument::parse(style_xml.as_bytes()).map_err(|e| Error::new(format!("グラフのスタイルを作れません: {e}")))?);
    package.set_override(&style, CT_CHART_STYLE)?;
    package.set_xml(&colors, XmlDocument::parse(colors_xml.as_bytes()).map_err(|e| Error::new(format!("グラフの色を作れません: {e}")))?);
    package.set_override(&colors, CT_CHART_COLOR_STYLE)?;
    package.add_relationship(chart, REL_CHART_STYLE, &relative(chart, &style), false)?;
    package.add_relationship(chart, REL_CHART_COLOR_STYLE, &relative(chart, &colors), false)?;
    Ok(())
}

/// A new chartEx part with its workbook, related from slide `slide`; returns the relationship's id and the part.
fn add_chart_ex_part(package: &mut Package, slide: &str, kind: NewChartKind) -> Result<(String, String), Error> {
    let chart = next_part_name(package, "ppt/charts/chartEx1.xml");
    let book = next_part_name(package, "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx");
    package.set_bytes(&book, ex_workbook(kind)?);
    package.set_override(&book, CT_XLSX)?;
    package.set_xml(&chart, XmlDocument::parse(b"<cx:chartSpace xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"/>").map_err(|e| Error::new(format!("グラフを作れません: {e}")))?);
    let book_rel = package.add_relationship(&chart, REL_PACKAGE, &relative(&chart, &book), false)?;
    let doc = XmlDocument::parse(chart_ex_xml(&book_rel, kind).as_bytes()).map_err(|e| Error::new(format!("グラフを作れません: {e}")))?;
    package.set_xml(&chart, doc);
    package.set_override(&chart, CT_CHART_EX)?;
    add_chart_ex_style_parts(package, &chart)?;
    let rel = package.add_relationship(slide, REL_CHART_EX, &relative(slide, &chart), false)?;
    Ok((rel, chart))
}

/// A new chart part (`xl/charts/chartN.xml`) of `kind` for a sheet's drawing
/// `drawing`, with no workbook of its own (it takes its data from the
/// sheet's cells, its series' references written after); returns the
/// relationship's id and the part.
pub(crate) fn add_sheet_chart_part(package: &mut Package, drawing: &str, kind: NewChartKind) -> Result<(String, String), Error> {
    let chart = next_part_name(package, "xl/charts/chart1.xml");
    let mut text = chart_xml("rId1", kind);
    if let (Some(a), Some(b)) = (text.find("<c:externalData"), text.find("</c:externalData>")) {
        text.replace_range(a..b + "</c:externalData>".len(), "");
    }
    let mut doc = XmlDocument::parse(text.as_bytes()).map_err(|e| Error::new(format!("グラフを作れません: {e}")))?;
    // ibisOffice's sheet-chart insertion defaults: reuse the existing colorful style,
    // and draw an opaque white card with the app's neutral gray, 1-point frame.
    // These are application UI choices, not inferred Office rendering coefficients.
    let root=doc.root_element().ok_or_else(||Error::new("グラフのXMLが空です"))?;
    super::pptx_chart::write_series_fill(&mut doc,root,&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff))),"c:","a")?;
    super::pptx_chart::write_series_stroke(&mut doc,root,Some(&ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xdadce0)),ibis_doc_model::EMU_PER_POINT)),"c:","a")?;
    let style=doc.append_element(root,"c:style");doc.set_attr(style,"val","2");
    if let Some(before)=doc.child(root,"chart"){doc.detach(style);doc.insert_before(before,style);}
    package.set_xml(&chart, doc);
    package.set_override(&chart, CT_CHART)?;
    let rel = package.add_relationship(drawing, REL_CHART, &relative(drawing, &chart), false)?;
    Ok((rel, chart))
}

/// A new chartEx part in a workbook (`xl/charts/chartEx1.xml`), related from sheet drawing `drawing`: it
/// reads the sheet's cells `cells` name (the values cached as they are now), no workbook of its own.
/// Returns the relationship's id and the part.
pub(crate) fn add_sheet_chart_ex_part(package: &mut Package, drawing: &str, kind: NewChartKind, cells: &ExCells) -> Result<(String, String), Error> {
    let chart = next_part_name(package, "xl/charts/chartEx1.xml");
    let mut doc = XmlDocument::parse(chart_ex_with(kind, cells, false, None).as_bytes()).map_err(|e| Error::new(format!("グラフを作れません: {e}")))?;
    // MS-ODRAWXML 2.24.3.11: cx:chartSpace/cx:spPr styles the whole chart.
    let root=doc.root_element().ok_or_else(||Error::new("グラフのXMLが空です"))?;
    super::pptx_chart::write_series_fill(&mut doc,root,&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff))),"cx:","a")?;
    super::pptx_chart::write_series_stroke(&mut doc,root,Some(&ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xdadce0)),ibis_doc_model::EMU_PER_POINT)),"cx:","a")?;
    package.set_xml(&chart, doc);
    package.set_override(&chart, CT_CHART_EX)?;
    add_chart_ex_style_parts(package, &chart)?;
    let rel = package.add_relationship(drawing, REL_CHART_EX, &relative(drawing, &chart), false)?;
    Ok((rel, chart))
}

/// A sheet drawing's frame showing chartEx relationship `rel`: in `mc:AlternateContent`, as Excel writes it
/// (a version that cannot draw the chart shows the fallback). `xdr` is the drawing's prefix.
pub(crate) fn frame_ex_sheet_xml(xdr: &str, a: &str, r: &str, id: u32, name: &str, rel: &str) -> String {
    let name = super::pptx_new::escape(name);
    let id2 = id + 1;
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:cx1=\"http://schemas.microsoft.com/office/drawing/2015/9/8/chartex\" Requires=\"cx1\">\
<{xdr}graphicFrame macro=\"\"><{xdr}nvGraphicFramePr><{xdr}cNvPr id=\"{id}\" name=\"{name}\"/><{xdr}cNvGraphicFramePr/></{xdr}nvGraphicFramePr><{xdr}xfrm><{a}off x=\"0\" y=\"0\"/><{a}ext cx=\"0\" cy=\"0\"/></{xdr}xfrm>\
<{a}graphic><{a}graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\" {r}id=\"{rel}\"/></{a}graphicData></{a}graphic></{xdr}graphicFrame></mc:Choice>\
<mc:Fallback><{xdr}sp macro=\"\" textlink=\"\"><{xdr}nvSpPr><{xdr}cNvPr id=\"{id2}\" name=\"{name}\"/><{xdr}cNvSpPr/></{xdr}nvSpPr><{xdr}spPr><{a}xfrm><{a}off x=\"0\" y=\"0\"/><{a}ext cx=\"4572000\" cy=\"2743200\"/></{a}xfrm><{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom><{a}solidFill><{a}schemeClr val=\"bg1\"/></{a}solidFill><{a}ln><{a}solidFill><{a}schemeClr val=\"tx1\"/></{a}solidFill></{a}ln></{xdr}spPr>\
<{xdr}txBody><{a}bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" rtlCol=\"0\" anchor=\"ctr\"/><{a}lstStyle/><{a}p><{a}r><{a}rPr lang=\"ja-JP\" sz=\"1200\"/><{a}t>このグラフは、お使いのバージョンの Excel では表示できません。</{a}t></{a}r></{a}p></{xdr}txBody></{xdr}sp></mc:Fallback></mc:AlternateContent>"
    )
}

/// The slide's `p:graphicFrame` showing chart relationship `rel`.
pub(crate) fn frame_xml(p: &str, a: &str, r: &str, id: u32, name: &str, frame: ibis_doc_model::EmuRect, rel: &str) -> String {
    format!(
        "<{p}graphicFrame><{p}nvGraphicFramePr><{p}cNvPr id=\"{id}\" name=\"{}\"/><{p}cNvGraphicFramePr/><{p}nvPr/></{p}nvGraphicFramePr>\
<{p}xfrm><{a}off x=\"{}\" y=\"{}\"/><{a}ext cx=\"{}\" cy=\"{}\"/></{p}xfrm>\
<{a}graphic><{a}graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" {r}id=\"{rel}\"/></{a}graphicData></{a}graphic></{p}graphicFrame>",
        super::pptx_new::escape(name),
        frame.x,
        frame.y,
        frame.width,
        frame.height
    )
}

/// The slide's frame showing chartEx relationship `rel`: in `mc:AlternateContent`, as PowerPoint
/// writes it (a version that cannot draw the chart shows the fallback).
pub(crate) fn frame_ex_xml(p: &str, a: &str, r: &str, id: u32, name: &str, frame: ibis_doc_model::EmuRect, rel: &str) -> String {
    let xfrm = format!("<{a}off x=\"{}\" y=\"{}\"/><{a}ext cx=\"{}\" cy=\"{}\"/>", frame.x, frame.y, frame.width, frame.height);
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:cx1=\"http://schemas.microsoft.com/office/drawing/2015/9/8/chartex\" Requires=\"cx1\">\
<{p}graphicFrame><{p}nvGraphicFramePr><{p}cNvPr id=\"{id}\" name=\"{n}\"/><{p}cNvGraphicFramePr/><{p}nvPr/></{p}nvGraphicFramePr><{p}xfrm>{xfrm}</{p}xfrm>\
<{a}graphic><{a}graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\" {r}id=\"{rel}\"/></{a}graphicData></{a}graphic></{p}graphicFrame></mc:Choice>\
<mc:Fallback><{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id2}\" name=\"{n}\"/><{p}cNvSpPr/><{p}nvPr/></{p}nvSpPr><{p}spPr><{a}xfrm>{xfrm}</{a}xfrm><{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom><{a}solidFill><{a}schemeClr val=\"bg1\"/></{a}solidFill><{a}ln><{a}solidFill><{a}schemeClr val=\"tx1\"/></{a}solidFill></{a}ln></{p}spPr>\
<{p}txBody><{a}bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" rtlCol=\"0\" anchor=\"ctr\"/><{a}lstStyle/><{a}p><{a}r><{a}rPr lang=\"ja-JP\" sz=\"1200\"/><{a}t>このグラフは、お使いのバージョンの PowerPoint では表示できません。</{a}t></{a}r></{a}p></{p}txBody></{p}sp></mc:Fallback></mc:AlternateContent>",
        n = super::pptx_new::escape(name),
        id2 = id + 1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_templates_are_well_formed() {
        let kinds = [
            NewChartKind::ClusteredColumn,
            NewChartKind::StackedColumn,
            NewChartKind::PercentStackedColumn,
            NewChartKind::ClusteredColumn3D,
            NewChartKind::StackedColumn3D,
            NewChartKind::PercentStackedColumn3D,
            NewChartKind::Column3D,
            NewChartKind::ClusteredBar,
            NewChartKind::StackedBar,
            NewChartKind::PercentStackedBar,
            NewChartKind::ClusteredBar3D,
            NewChartKind::StackedBar3D,
            NewChartKind::PercentStackedBar3D,
            NewChartKind::Line,
            NewChartKind::StackedLine,
            NewChartKind::PercentStackedLine,
            NewChartKind::Line3D,
            NewChartKind::Area,
            NewChartKind::StackedArea,
            NewChartKind::PercentStackedArea,
            NewChartKind::Area3D,
            NewChartKind::StackedArea3D,
            NewChartKind::PercentStackedArea3D,
            NewChartKind::Pie,
            NewChartKind::Pie3D,
            NewChartKind::Doughnut,
            NewChartKind::PieOfPie,
            NewChartKind::BarOfPie,
            NewChartKind::Radar,
            NewChartKind::RadarFilled,
            NewChartKind::StockHLC,
            NewChartKind::StockOHLC,
            NewChartKind::StockVHLC,
            NewChartKind::StockVOHLC,
            NewChartKind::Surface3D,
            NewChartKind::Surface3DWireframe,
            NewChartKind::Contour,
            NewChartKind::ContourWireframe,
            NewChartKind::ComboColumnLine,
            NewChartKind::ComboColumnLineSecondaryAxis,
            NewChartKind::ComboAreaColumn,
        ];
        for kind in kinds {
            assert!(XmlDocument::parse(chart_xml("rId1", kind).as_bytes()).is_ok(), "{kind:?} is well-formed");
        }
        assert!(data_csv().starts_with(" ,系列 1,系列 2,系列 3\n分類 1,4.3,2.4,2\n"));
        assert!(pie_data_csv().starts_with(" ,売上\n第 1 四半期,8.2\n"));

        // The column workbook has its table.
        let book = workbook().unwrap();
        let p = crate::read(&book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(sheet) = &p.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 3)]);

        // The pie workbook has its table.
        let pie_book = pie_workbook().unwrap();
        let p_pie = crate::read(&pie_book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(pie_sheet) = &p_pie.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(pie_sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 1)]);

        // The stock HLC workbook has its table.
        let hlc_book = stock_hlc_workbook().unwrap();
        let p_hlc = crate::read(&hlc_book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(hlc_sheet) = &p_hlc.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(hlc_sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 3)]);

        // The stock OHLC workbook has its table.
        let ohlc_book = stock_ohlc_workbook().unwrap();
        let p_ohlc = crate::read(&ohlc_book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(ohlc_sheet) = &p_ohlc.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(ohlc_sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 4)]);

        // The stock VHLC workbook has its table.
        let vhlc_book = stock_vhlc_workbook().unwrap();
        let p_vhlc = crate::read(&vhlc_book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(vhlc_sheet) = &p_vhlc.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(vhlc_sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 4)]);

        // The stock VOHLC workbook has its table.
        let vohlc_book = stock_vohlc_workbook().unwrap();
        let p_vohlc = crate::read(&vohlc_book).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(vohlc_sheet) = &p_vohlc.slides[0].shapes[0].kind else { panic!("a sheet") };
        assert_eq!(vohlc_sheet.tables.iter().map(|t| (t.name.as_str(), t.range.last_row, t.range.last_column)).collect::<Vec<_>>(), vec![("テーブル1", 4, 5)]);
    }
}
