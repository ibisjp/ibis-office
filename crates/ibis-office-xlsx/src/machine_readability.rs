// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 機械可読性チェック (GitHub issue #194): 表計算のシートが、機械が直接読み取れるデータ（オープン データ）に
//! なっているかを調べる。判定は ibisOffice の中だけで行う（外部のプログラムは起動しない）。
//!
//! 項目の名前と番号は、デジタル庁の公開文書「機械判読可能なデータの表記方法の統一ルール」の
//! ルール ID（L1-xx：基本、L2-xx：数値・項目、L3-xx：メタデータ）による。判定の中身はこの文書の
//! ルール本文から ibisOffice で書いた（デジタル庁のチェック ツールのコードは見ていない・使っていない）。
//! 取り組むのは、シートの中身だけで判定できる項目に限る（コード表の添付・地域コード・年ごとの形式の違いなどは
//! ファイルの外の情報が要るので対象外）。
//!
//! 対応した版（ルールやチェック ツールが更新されたとき、差分を見て取り入れるため。2026-11-14 ユーザー指示）：
//! デジタル庁 https://github.com/digital-go-jp/machine_readability_rule の v0.2.3
//! （commit 88014ff9cfe685d4e0e5f24a2d4d2e021ca3fd9f、2026-10-27）の時点のルール文書。更新を取り入れるときは、
//! `git diff 88014ff9cfe685d4e0e5f24a2d4d2e021ca3fd9f <新しい版> -- <ルール文書>` でルールの本文の差分を見る
//! （ツールのコードは引き続き見ない）。
//!
//! 表の見つけ方：空白でない最初の行（その前にタイトルだけの行があれば、その次の行）を項目名の行とし、
//! そこから空白の行が現れるまでをデータ本体とする。

use ibis_doc_model::sheet::{CellKind, CellValue, Sheet, SheetCell};

/// The version of Harunobu (the Digital Agency's check tool of the rules, https://github.com/digital-go-jp/machine_readability_rule) the check corresponds to: the rules'
/// text of its v0.2.3 (see the top of this file). The panel says 「Harunobu ver.0.2.3 相当」.
pub const HARUNOBU_VERSION: &str = "0.2.3";

/// 検査の項目。ID はデジタル庁のルール ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// L1-02 １シートに複数の表が掲載されていないか
    MultipleTables,
    /// L1-03 データが分断されていないか（表の中の空白の行・列）
    SplitData,
    /// L1-04 データ本体と無関係な情報がないか
    StrayInfo,
    /// L1-05 すべての列に項目名があるか
    MissingHeader,
    /// L1-06 スペースや改行で体裁を整えていないか
    PaddedText,
    /// L1-10 オブジェクト（図形・画像・グラフ）を使用していないか
    Objects,
    /// L1-12 セルの結合をしていないか
    MergedCells,
    /// L1-13 不要な行や列が非表示のまま残されていないか
    HiddenLines,
    /// L2-01 数値データは数値属性か（単位・カンマ・脚注記号が混ざった文字列）
    NumberAsText,
    /// L2-03 各列が一意の項目名を持っているか
    DuplicateHeader,
    /// L2-06 数式を使っていないか
    Formula,
    /// L3-01 項目名の行から始まっているか（先頭のタイトル行）
    TitleRow,
}

impl Rule {
    /// デジタル庁のルール ID。
    pub fn id(self) -> &'static str {
        match self {
            Rule::MultipleTables => "L1-02",
            Rule::SplitData => "L1-03",
            Rule::StrayInfo => "L1-04",
            Rule::MissingHeader => "L1-05",
            Rule::PaddedText => "L1-06",
            Rule::Objects => "L1-10",
            Rule::MergedCells => "L1-12",
            Rule::HiddenLines => "L1-13",
            Rule::NumberAsText => "L2-01",
            Rule::DuplicateHeader => "L2-03",
            Rule::Formula => "L2-06",
            Rule::TitleRow => "L3-01",
        }
    }

    /// i18n のキー（`xlsx.machine.<key>.title` と `.desc`）。
    pub fn key(self) -> &'static str {
        match self {
            Rule::MultipleTables => "multiple-tables",
            Rule::SplitData => "split-data",
            Rule::StrayInfo => "stray-info",
            Rule::MissingHeader => "missing-header",
            Rule::PaddedText => "padded-text",
            Rule::Objects => "objects",
            Rule::MergedCells => "merged-cells",
            Rule::HiddenLines => "hidden-lines",
            Rule::NumberAsText => "number-as-text",
            Rule::DuplicateHeader => "duplicate-header",
            Rule::Formula => "formula",
            Rule::TitleRow => "title-row",
        }
    }
}

/// 1 つの項目で見つかった問題（シートごと・項目ごとに 1 つにまとめる）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    /// 問題の数。
    pub count: usize,
    /// 最初の場所（行・列、0 始まり）。表示の「移動」はここへ行く。
    pub first: Option<(u32, u32)>,
    /// 場所の例（最大 [`MAX_EXAMPLES`]）。
    pub examples: Vec<(u32, u32)>,
}

pub const MAX_EXAMPLES: usize = 5;

#[derive(Default)]
struct Collector {
    findings: Vec<Finding>,
}

impl Collector {
    fn add(&mut self, rule: Rule, at: Option<(u32, u32)>) {
        let f = match self.findings.iter_mut().find(|f| f.rule == rule) {
            Some(f) => f,
            None => {
                self.findings.push(Finding { rule, count: 0, first: at, examples: Vec::new() });
                self.findings.last_mut().unwrap()
            }
        };
        f.count += 1;
        if let Some(at) = at {
            if f.first.is_none() {
                f.first = Some(at);
            }
            if f.examples.len() < MAX_EXAMPLES && !f.examples.contains(&at) {
                f.examples.push(at);
            }
        }
    }
}

/// セルに値があるか（空文字や空白だけの文字列は、値がないものとする）。
fn has_value(c: &SheetCell) -> bool {
    match &c.value {
        CellValue::Empty => c.formula.is_some() && !c.text.trim().is_empty(),
        CellValue::Text(s) => !s.trim().is_empty(),
        _ => true,
    }
}

/// 数値として読めるはずの文字列か。単位（円・%・人など）、桁区切り、通貨記号、脚注記号（※・*）を取り除いて数になるもの。
/// 0 で始まる桁数の多い数字（郵便番号・コード）は文字列でよいので除く。
pub fn looks_like_number(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let mut digits = String::new();
    for ch in t.chars() {
        match ch {
            '0'..='9' | '.' | '-' | '+' => digits.push(ch),
            '０'..='９' => digits.push(char::from_u32(ch as u32 - '０' as u32 + '0' as u32).unwrap()),
            '．' => digits.push('.'),
            '－' | '−' | '▲' | '△' => digits.push('-'),
            ',' | '，' | '、' | ' ' | '\u{3000}' | '¥' | '￥' | '$' | '円' | '%' | '％' | '人' | '件' | '台' | '個' | '年' | '万' | '千' | '億' | '※' | '*' | '＊' => {}
            _ => return false,
        }
    }
    if !digits.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // 「2024-01」「1-2」のように記号が数の途中にあるものは、日付や範囲であって数ではない。
    let body = digits.strip_prefix(['-', '+']).unwrap_or(&digits);
    if body.chars().any(|c| c == '-' || c == '+') || body.matches('.').count() > 1 {
        return false;
    }
    // コード（先頭が 0 の整数）は文字列のままでよい。
    let integer_part = body.split('.').next().unwrap_or("");
    if integer_part.len() > 1 && integer_part.starts_with('0') && !body.contains('.') {
        return false;
    }
    body.parse::<f64>().is_ok()
}

fn is_padded(s: &str) -> bool {
    s.starts_with([' ', '\u{3000}'])
        || s.ends_with([' ', '\u{3000}'])
        || s.contains('\n')
        || s.contains("  ")
        || s.contains('\u{3000}')
}

/// シートを調べる。`objects` はそのシートの図形・画像・グラフの数。
pub fn check_sheet(sheet: &Sheet, objects: usize) -> Vec<Finding> {
    let mut out = Collector::default();

    for m in &sheet.merges {
        if m.first_row != m.last_row || m.first_column != m.last_column {
            out.add(Rule::MergedCells, Some((m.first_row, m.first_column)));
        }
    }
    for (&r, _) in sheet.hidden_rows.iter() {
        out.add(Rule::HiddenLines, Some((r, 0)));
    }
    for (r, h) in sheet.rows.iter().enumerate() {
        if *h == 0 && !sheet.hidden_rows.contains_key(&(r as u32)) {
            out.add(Rule::HiddenLines, Some((r as u32, 0)));
        }
    }
    for (&c, _) in sheet.hidden_columns.iter() {
        out.add(Rule::HiddenLines, Some((0, c)));
    }
    for (c, w) in sheet.columns.iter().enumerate() {
        if *w == 0 && !sheet.hidden_columns.contains_key(&(c as u32)) {
            out.add(Rule::HiddenLines, Some((0, c as u32)));
        }
    }
    for _ in 0..objects {
        out.add(Rule::Objects, None);
    }
    for c in sheet.cells.iter().filter(|c| c.formula.is_some()) {
        out.add(Rule::Formula, Some((c.row, c.column)));
    }

    let cells: Vec<&SheetCell> = sheet.cells.iter().filter(|c| has_value(c)).collect();
    if cells.is_empty() {
        return sorted(out);
    }
    let mut rows: Vec<u32> = cells.iter().map(|c| c.row).collect();
    rows.dedup(); // `cells` is in row-major order.

    // 行を、空白の行で区切られた塊に分ける。
    let mut blocks: Vec<(u32, u32)> = Vec::new();
    for &r in &rows {
        match blocks.last_mut() {
            Some((_, last)) if *last + 1 == r => *last = r,
            _ => blocks.push((r, r)),
        }
    }
    let in_rows = |a: u32, b: u32| cells.iter().filter(move |c| c.row >= a && c.row <= b).copied();

    // 先頭がタイトルだけの行か。
    let (first_start, first_end) = blocks[0];
    let mut header_row = first_start;
    if first_end > first_start && in_rows(first_start, first_start).count() == 1 && in_rows(first_start + 1, first_start + 1).count() >= 2 {
        let c = in_rows(first_start, first_start).next().unwrap();
        out.add(Rule::TitleRow, Some((c.row, c.column)));
        header_row = first_start + 1;
    }

    // 表（最初の塊の項目名の行から下）。
    let header: Vec<&SheetCell> = in_rows(header_row, header_row).collect();
    let (col_min, col_max) = match (header.first(), header.last()) {
        (Some(a), Some(b)) => (a.column, b.column),
        _ => (0, 0),
    };
    let table: Vec<&SheetCell> = in_rows(header_row, first_end).collect();

    // 項目名: 重複、データがあるのに項目名のない列、データのない列（分断）。
    let mut seen: Vec<String> = Vec::new();
    for c in &header {
        let name = text_of(c);
        if seen.contains(&name) {
            out.add(Rule::DuplicateHeader, Some((c.row, c.column)));
        }
        seen.push(name);
    }
    for col in col_min..=col_max {
        let has_header = header.iter().any(|c| c.column == col);
        let has_data = table.iter().any(|c| c.column == col && c.row > header_row);
        if !has_header && has_data {
            out.add(Rule::MissingHeader, Some((header_row, col)));
        } else if !has_header && !has_data {
            out.add(Rule::SplitData, Some((header_row, col)));
        }
    }
    // 項目名の行の外（左右）にあるもの。
    for c in table.iter().filter(|c| c.column < col_min || c.column > col_max) {
        out.add(Rule::StrayInfo, Some((c.row, c.column)));
    }

    // 後ろの塊: 空白の行で分断。1 列だけならメモなど、複数列なら別の表。
    for &(start, end) in &blocks[1..] {
        out.add(Rule::SplitData, Some((start - 1, col_min)));
        let block: Vec<&SheetCell> = in_rows(start, end).collect();
        let first = block[0];
        let columns: std::collections::BTreeSet<u32> = block.iter().map(|c| c.column).collect();
        if columns.len() == 1 {
            out.add(Rule::StrayInfo, Some((first.row, first.column)));
        } else {
            out.add(Rule::MultipleTables, Some((first.row, first.column)));
        }
    }

    // セルの中身（データ本体）。
    for c in table.iter().filter(|c| c.row > header_row && c.column >= col_min && c.column <= col_max) {
        if let CellValue::Text(s) = &c.value {
            if c.formula.is_none() && looks_like_number(s) {
                out.add(Rule::NumberAsText, Some((c.row, c.column)));
            }
        }
    }
    for c in &table {
        if c.kind == CellKind::Text || matches!(c.value, CellValue::Text(_)) {
            if let CellValue::Text(s) = &c.value {
                if is_padded(s) {
                    out.add(Rule::PaddedText, Some((c.row, c.column)));
                }
            }
        }
    }
    sorted(out)
}

fn text_of(c: &SheetCell) -> String {
    match &c.value {
        CellValue::Text(s) => s.trim().to_string(),
        _ => c.text.trim().to_string(),
    }
}

fn sorted(mut out: Collector) -> Vec<Finding> {
    out.findings.sort_by_key(|f| f.rule);
    out.findings
}

/// 場所を「A1」の形にする。
pub fn cell_ref(row: u32, column: u32) -> String {
    format!("{}{}", super::sheet_ui::column_letters(column), row + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::sheet::{CellRange, SheetCell};

    fn text(r: u32, c: u32, s: &str) -> SheetCell {
        SheetCell { row: r, column: c, value: CellValue::Text(s.into()), text: s.into(), kind: CellKind::Text, formula: None, style: 0, phonetic: None, runs: None }
    }
    fn num(r: u32, c: u32, n: f64) -> SheetCell {
        SheetCell { row: r, column: c, value: CellValue::Number(n), text: n.to_string(), kind: CellKind::Number, formula: None, style: 0, phonetic: None, runs: None }
    }
    fn sheet(mut cells: Vec<SheetCell>) -> Sheet {
        cells.sort_by_key(|c| (c.row, c.column));
        let mut s = Sheet::default();
        s.cells.extend(cells);
        s
    }
    fn rules(s: &Sheet) -> Vec<&'static str> {
        check_sheet(s, 0).iter().map(|f| f.rule.id()).collect()
    }
    fn clean() -> Vec<SheetCell> {
        vec![text(0, 0, "地域"), text(0, 1, "人口"), text(1, 0, "東京"), num(1, 1, 100.0), text(2, 0, "大阪"), num(2, 1, 50.0)]
    }

    #[test]
    fn a_clean_table_has_no_finding() {
        assert!(rules(&sheet(clean())).is_empty());
    }

    #[test]
    fn merged_cells_are_found_at_their_first_cell() {
        let mut s = sheet(clean());
        s.merges.push(CellRange { first_row: 1, first_column: 0, last_row: 2, last_column: 0 });
        s.merges.push(CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 });
        let f = check_sheet(&s, 0);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].rule, f[0].count, f[0].first), (Rule::MergedCells, 1, Some((1, 0))));
    }

    #[test]
    fn hidden_rows_and_columns_are_found() {
        let mut s = sheet(clean());
        s.hidden_rows.insert(2, 100);
        s.hidden_columns.insert(1, 100);
        let f = check_sheet(&s, 0);
        assert_eq!((f[0].rule, f[0].count), (Rule::HiddenLines, 2));
    }

    #[test]
    fn a_formula_is_found() {
        let mut cells = clean();
        cells[3].formula = Some("1+99".into());
        assert_eq!(rules(&sheet(cells)), ["L2-06"]);
    }

    #[test]
    fn objects_are_counted() {
        let f = check_sheet(&sheet(clean()), 2);
        assert_eq!((f[0].rule, f[0].count, f[0].first), (Rule::Objects, 2, None));
    }

    #[test]
    fn a_number_with_a_unit_is_a_text_that_should_be_a_number() {
        for t in ["1,234", "100円", "12.5%", "１２３", "45※1", "▲300"] {
            assert!(looks_like_number(t), "{t}");
        }
        for t in ["東京", "2024-01", "0123", "A1", "", "1-2", "3.4.5"] {
            assert!(!looks_like_number(t), "{t}");
        }
        let mut cells = clean();
        cells[3] = text(1, 1, "1,000人");
        let f = check_sheet(&sheet(cells), 0);
        assert_eq!((f[0].rule, f[0].first), (Rule::NumberAsText, Some((1, 1))));
    }

    #[test]
    fn a_title_row_before_the_header_is_found_and_the_header_is_the_next_row() {
        let mut cells = vec![text(0, 0, "人口の表")];
        cells.extend(clean().into_iter().map(|mut c| {
            c.row += 1;
            c
        }));
        let s = sheet(cells);
        assert_eq!(rules(&s), ["L3-01"]);
    }

    #[test]
    fn a_blank_row_then_a_note_is_a_split_and_stray_info() {
        let mut cells = clean();
        cells.push(text(4, 0, "注：単位は万人"));
        assert_eq!(rules(&sheet(cells)), ["L1-03", "L1-04"]);
    }

    #[test]
    fn a_blank_row_then_another_table_is_a_split_and_multiple_tables() {
        let mut cells = clean();
        cells.extend([text(4, 0, "年"), text(4, 1, "件数"), num(5, 0, 2020.0), num(5, 1, 3.0)]);
        assert_eq!(rules(&sheet(cells)), ["L1-02", "L1-03"]);
    }

    #[test]
    fn a_blank_column_inside_the_table_is_a_split() {
        let cells = vec![text(0, 0, "a"), text(0, 2, "b"), num(1, 0, 1.0), num(1, 2, 2.0)];
        let f = check_sheet(&sheet(cells), 0);
        assert_eq!((f[0].rule, f[0].first), (Rule::SplitData, Some((0, 1))));
    }

    #[test]
    fn a_column_with_data_and_no_header_and_a_repeated_header_are_found() {
        let cells = vec![text(0, 0, "a"), text(0, 2, "a"), num(1, 0, 1.0), num(1, 1, 5.0), num(1, 2, 2.0)];
        assert_eq!(rules(&sheet(cells)), ["L1-05", "L2-03"]);
    }

    #[test]
    fn cells_beside_the_table_are_stray_info() {
        let mut cells = clean();
        cells.push(text(1, 3, "メモ"));
        assert_eq!(rules(&sheet(cells)), ["L1-04"]);
    }

    #[test]
    fn spaces_and_line_breaks_used_for_layout_are_found() {
        let mut cells = clean();
        cells[2] = text(1, 0, "　東京");
        cells[4] = text(2, 0, "大\n阪");
        let f = check_sheet(&sheet(cells), 0);
        assert_eq!((f[0].rule, f[0].count), (Rule::PaddedText, 2));
    }

    #[test]
    fn an_empty_sheet_has_no_finding() {
        assert!(check_sheet(&Sheet::default(), 0).is_empty());
    }
}
