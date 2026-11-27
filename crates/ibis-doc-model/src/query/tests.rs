// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use std::collections::HashMap;

use ibis_calc::{CellError, CellValue};

use crate::Presentation;
use super::*;

fn t(s: &str) -> CellValue {
    CellValue::Text(s.into())
}
fn n(x: f64) -> CellValue {
    CellValue::Number(x)
}

fn sales() -> QTable {
    QTable::from_rows(
        vec!["店".into(), "商品".into(), "数量".into(), "単価".into()],
        vec![
            vec![t("北"), t("りんご"), n(10.0), n(120.0)],
            vec![t("北"), t("みかん"), n(5.0), n(80.0)],
            vec![t("南"), t("りんご"), n(7.0), n(120.0)],
            vec![t("南"), t("ぶどう"), n(2.0), n(450.0)],
            vec![t("東"), t("りんご"), CellValue::Empty, n(120.0)],
        ],
    )
}

fn run(steps: Vec<Step>, table: QTable) -> QTable {
    let mut q = Query::new("Q", Source::Table("T".into()));
    q.steps = steps;
    let (tables, error) = q.run(table, &[], &HashMap::<String, QTable>::new());
    assert!(error.is_none(), "{error:?}");
    tables.last().cloned().unwrap()
}

fn column(t: &QTable, name: &str) -> Vec<CellValue> {
    let i = t.column_index(name).unwrap_or_else(|| panic!("no column {name}: {:?}", t.names()));
    t.rows.iter().map(|r| r[i].clone()).collect()
}

#[test]
fn headers_are_promoted_from_the_first_row() {
    let raw = table_of_cells(vec![vec![t("a"), t("b"), t("a")], vec![n(1.0), n(2.0), n(3.0)]], 3);
    let out = run(vec![Step::PromoteHeaders], raw.clone());
    assert_eq!(out.names(), ["a", "b", "a.1"]);
    assert_eq!(out.rows.len(), 1);
    let back = run(vec![Step::PromoteHeaders, Step::DemoteHeaders], raw);
    assert_eq!(back.names(), ["Column1", "Column2", "Column3"]);
    assert_eq!(back.rows.len(), 2);
}

#[test]
fn columns_are_chosen_removed_renamed_and_reordered() {
    let out = run(vec![Step::SelectColumns(vec!["商品".into(), "数量".into()])], sales());
    assert_eq!(out.names(), ["商品", "数量"]);
    let out = run(vec![Step::RemoveColumns(vec!["店".into()]), Step::RenameColumns(vec![("単価".into(), "価格".into())])], sales());
    assert_eq!(out.names(), ["商品", "数量", "価格"]);
    // The listed columns take the places the listed columns have (Excel, measured: `{"c"}` on a,b,c,d changes nothing, `{"c","a"}` is c,b,a,d);
    // before, they went to the front. A list of all the columns, which is what Excel writes, is the new order.
    let out = run(vec![Step::ReorderColumns(vec!["単価".into()])], sales());
    assert_eq!(out.names(), ["店", "商品", "数量", "単価"]);
    let out = run(vec![Step::ReorderColumns(vec!["単価".into(), "店".into()])], sales());
    assert_eq!(out.names(), ["単価", "商品", "数量", "店"]);
    let out = run(vec![Step::ReorderColumns(vec!["単価".into(), "店".into(), "商品".into(), "数量".into()])], sales());
    assert_eq!(out.names(), ["単価", "店", "商品", "数量"]);
    let q = Query { name: "Q".into(), source: Source::Table("T".into()), steps: vec![Step::RemoveColumns(vec!["無い".into()])], output: None };
    let (_, error) = q.run(sales(), &[], &HashMap::<String, QTable>::new());
    assert_eq!(error.map(|e| e.0), Some(0));
}

#[test]
fn types_are_changed_by_the_culture() {
    let raw = QTable::from_rows(vec!["x".into()], vec![vec![t("1,234.5")], vec![t("abc")], vec![CellValue::Empty]]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture: Culture::EnglishUs, apart: false }], raw.clone());
    assert_eq!(column(&out, "x"), [n(1234.5), CellValue::Error(ibis_calc::CellError::Value), CellValue::Empty]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture: Culture::German, apart: false }], QTable::from_rows(vec!["x".into()], vec![vec![t("1.234,5")]]));
    assert_eq!(column(&out, "x"), [n(1234.5)]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Whole, culture: Culture::Invariant, apart: false }], QTable::from_rows(vec!["x".into()], vec![vec![t("2.5")], vec![t("-2.5")]]));
    // Halves go to the even number, as Excel gives (Int64.Type of 2.5 is 2).
    assert_eq!(column(&out, "x"), [n(2.0), n(-2.0)]);
    let dates = QTable::from_rows(vec!["d".into()], vec![vec![t("2024-03-05")], vec![t("3/5/2024")], vec![t("2024年3月5日")]]);
    let out = run(vec![Step::ChangeType { column: "d".into(), ty: QType::Date, culture: Culture::EnglishUs, apart: false }], dates);
    assert_eq!(column(&out, "d"), [n(45356.0), n(45356.0), n(45356.0)]);
    let dmy = QTable::from_rows(vec!["d".into()], vec![vec![t("5/3/2024")]]);
    let out = run(vec![Step::ChangeType { column: "d".into(), ty: QType::Date, culture: Culture::German, apart: false }], dmy);
    assert_eq!(column(&out, "d"), [n(45356.0)]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Logical, culture: Culture::Invariant, apart: false }], QTable::from_rows(vec!["x".into()], vec![vec![t("true")], vec![n(0.0)]]));
    assert_eq!(column(&out, "x"), [CellValue::Bool(true), CellValue::Bool(false)]);
}

#[test]
fn en_gb_and_thai_read_text_by_their_locale() {
    let one = |culture, text: &str| {
        let out = run(vec![Step::ChangeType { column: "d".into(), ty: QType::Date, culture, apart: false }], QTable::from_rows(vec!["d".into()], vec![vec![t(text)]]));
        column(&out, "d")[0].clone()
    };
    // Day first in en-GB; the year of th-TH is the Buddhist one (Gregorian + 543).
    assert_eq!(one(Culture::EnglishGb, "05/03/2024"), n(45356.0));
    assert_eq!(one(Culture::Thai, "5/3/2567"), n(45356.0));
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture: Culture::Thai, apart: false }], QTable::from_rows(vec!["x".into()], vec![vec![t("1,234.5")]]));
    assert_eq!(column(&out, "x"), [n(1234.5)]);
    // The locales of the dialog are written in M by their names and read back.
    for culture in Culture::LOCALES {
        let step = Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture, apart: false };
        let q = crate::query::Query { name: "q".into(), source: crate::query::Source::Blank, steps: vec![step.clone()], output: None };
        let m = crate::query::query_to_m(&q);
        assert!(m.contains(&format!("\"{}\"", culture.name())), "{m}");
    }
}

#[test]
fn rows_are_filtered_kept_removed_and_sorted() {
    let out = run(vec![Step::FilterRows { column: "商品".into(), op: Op::Equals, value: "りんご".into() }], sales());
    assert_eq!(out.rows.len(), 3);
    let out = run(vec![Step::FilterRows { column: "数量".into(), op: Op::Greater, value: "4".into() }], sales());
    assert_eq!(column(&out, "数量"), [n(10.0), n(5.0), n(7.0)]);
    let out = run(vec![Step::FilterRows { column: "数量".into(), op: Op::IsNull, value: String::new() }], sales());
    assert_eq!(out.rows.len(), 1);
    let out = run(vec![Step::FilterRows { column: "商品".into(), op: Op::Contains, value: "ご".into() }], sales());
    assert_eq!(out.rows.len(), 3);
    assert_eq!(run(vec![Step::KeepTopRows(2)], sales()).rows.len(), 2);
    assert_eq!(run(vec![Step::KeepBottomRows(2)], sales()).rows[0][1], t("ぶどう"));
    assert_eq!(run(vec![Step::RemoveTopRows(4)], sales()).rows.len(), 1);
    assert_eq!(run(vec![Step::RemoveBottomRows(4)], sales()).rows.len(), 1);
    let out = run(vec![Step::Sort(vec![("単価".into(), false), ("数量".into(), true)])], sales());
    assert_eq!(column(&out, "単価"), [n(450.0), n(120.0), n(120.0), n(120.0), n(80.0)]);
    // Ties in order of the second key, nothing first.
    assert_eq!(column(&out, "数量"), [n(2.0), CellValue::Empty, n(7.0), n(10.0), n(5.0)]);
    let out = run(vec![Step::RemoveDuplicates(vec!["商品".into()])], sales());
    assert_eq!(out.rows.len(), 3);
    let out = run(vec![Step::RemoveBlankRows], QTable::from_rows(vec!["a".into(), "b".into()], vec![vec![CellValue::Empty, t(" ")], vec![n(1.0), CellValue::Empty]]));
    assert_eq!(out.rows.len(), 1);
}

#[test]
fn columns_are_split_and_merged() {
    let raw = QTable::from_rows(vec!["名前".into(), "x".into()], vec![vec![t("山田 太郎"), n(1.0)], vec![t("佐藤"), n(2.0)], vec![CellValue::Empty, n(3.0)]]);
    let out = run(vec![Step::SplitColumn { column: "名前".into(), by: SplitBy::Delimiter(" ".into()), columns: 0 }], raw.clone());
    assert_eq!(out.names(), ["名前.1", "名前.2", "x"]);
    assert_eq!(column(&out, "名前.2"), [t("太郎"), CellValue::Empty, CellValue::Empty]);
    let out = run(vec![Step::SplitColumn { column: "名前".into(), by: SplitBy::Length(1), columns: 0 }], raw);
    assert_eq!(out.names().len(), 6);
    let out = run(vec![Step::MergeColumns { columns: vec!["店".into(), "商品".into()], separator: "-".into(), name: "店商品".into() }], sales());
    assert_eq!(out.names(), ["店商品", "数量", "単価"]);
    assert_eq!(out.rows[0][0], t("北-りんご"));
}

#[test]
fn values_are_replaced_and_filled() {
    let out = run(vec![Step::ReplaceValue { column: "商品".into(), find: "ご".into(), with: "ゴ".into(), whole_cell: false }], sales());
    assert_eq!(out.rows[0][1], t("りんゴ"));
    let out = run(vec![Step::ReplaceValue { column: "商品".into(), find: "みかん".into(), with: "オレンジ".into(), whole_cell: true }], sales());
    assert_eq!(out.rows[1][1], t("オレンジ"));
    let out = run(vec![Step::ReplaceNull { columns: vec!["数量".into()], with: "0".into() }], sales());
    assert_eq!(out.rows[4][2], n(0.0));
    let raw = QTable::from_rows(vec!["k".into(), "v".into()], vec![vec![t("a"), n(1.0)], vec![CellValue::Empty, n(2.0)], vec![CellValue::Empty, n(3.0)], vec![t("b"), n(4.0)]]);
    let down = run(vec![Step::FillDown(vec!["k".into()])], raw.clone());
    assert_eq!(column(&down, "k"), [t("a"), t("a"), t("a"), t("b")]);
    let up = run(vec![Step::FillUp(vec!["k".into()])], raw);
    assert_eq!(column(&up, "k"), [t("a"), t("b"), t("b"), t("b")]);
}

#[test]
fn a_column_is_pivoted_and_unpivoted() {
    let long = QTable::from_rows(
        vec!["店".into(), "月".into(), "売上".into()],
        vec![vec![t("北"), t("4月"), n(1.0)], vec![t("北"), t("5月"), n(2.0)], vec![t("南"), t("4月"), n(3.0)], vec![t("北"), t("4月"), n(10.0)]],
    );
    let wide = run(vec![Step::Pivot { attribute: "月".into(), values: "売上".into(), agg: Agg::Sum }], long);
    assert_eq!(wide.names(), ["店", "4月", "5月"]);
    assert_eq!(wide.rows, vec![vec![t("北"), n(11.0), n(2.0)], vec![t("南"), n(3.0), CellValue::Empty]]);
    let back = run(vec![Step::Unpivot { columns: vec!["4月".into(), "5月".into()], attribute: "月".into(), value: "売上".into() }], wide.clone());
    assert_eq!(back.names(), ["店", "月", "売上"]);
    assert_eq!(back.rows.len(), 3, "the empty cell makes no row");
    let other = run(vec![Step::UnpivotOther { keep: vec!["店".into()], attribute: "Attribute".into(), value: "Value".into() }], wide);
    assert_eq!(other.names(), ["店", "Attribute", "Value"]);
}

#[test]
fn rows_are_grouped_and_aggregated() {
    let out = run(
        vec![Step::GroupBy {
            keys: vec!["店".into()],
            aggregations: vec![("合計".into(), Agg::Sum, Some("数量".into())), ("行数".into(), Agg::Count, None), ("商品数".into(), Agg::CountDistinct, Some("商品".into())), ("最大".into(), Agg::Max, Some("単価".into()))],
        }],
        sales(),
    );
    assert_eq!(out.names(), ["店", "合計", "行数", "商品数", "最大"]);
    // Sorted by the key: 北, 南, 東 (ordinal text order of the three).
    let by_store: HashMap<String, Vec<CellValue>> = out.rows.iter().map(|r| (value_text(&r[0]), r[1..].to_vec())).collect();
    assert_eq!(by_store["北"], vec![n(15.0), n(2.0), n(2.0), n(120.0)]);
    assert_eq!(by_store["南"], vec![n(9.0), n(2.0), n(2.0), n(450.0)]);
    assert_eq!(by_store["東"], vec![CellValue::Empty, n(1.0), n(1.0), n(120.0)]);
}

#[test]
fn tables_are_merged_and_appended() {
    let prices = QTable::from_rows(vec!["商品".into(), "産地".into()], vec![vec![t("りんご"), t("青森")], vec![t("みかん"), t("愛媛")], vec![t("もも"), t("山梨")]]);
    let mut book: HashMap<String, QTable> = HashMap::new();
    book.insert("産地".into(), prices.clone());
    let merge = |kind: JoinKind| {
        let step = Step::Merge { right: "産地".into(), left_keys: vec!["商品".into()], right_keys: vec!["商品".into()], kind, expand: vec!["産地".into()] };
        step.apply(&sales(), &[], &book).unwrap()
    };
    let left = merge(JoinKind::LeftOuter);
    assert_eq!(left.names(), ["店", "商品", "数量", "単価", "産地.産地"]);
    assert_eq!(left.rows.len(), 5);
    assert_eq!(left.rows[3][4], CellValue::Empty, "ぶどう has no origin");
    assert_eq!(merge(JoinKind::Inner).rows.len(), 4);
    assert_eq!(merge(JoinKind::LeftAnti).rows.len(), 1);
    let full = merge(JoinKind::FullOuter);
    assert_eq!(full.rows.len(), 6, "もも comes from the right");
    assert_eq!(full.rows[5][1], t("もも"));
    let right_anti = merge(JoinKind::RightAnti);
    assert_eq!(right_anti.rows, vec![vec![t("もも"), t("山梨")]]);
    let mut book2 = HashMap::new();
    book2.insert("追加".to_string(), QTable::from_rows(vec!["店".into(), "備考".into()], vec![vec![t("西"), t("新店")]]));
    let appended = Step::Append { other: "追加".into() }.apply(&sales(), &[], &book2).unwrap();
    assert_eq!(appended.names(), ["店", "商品", "数量", "単価", "備考"]);
    assert_eq!(appended.rows.len(), 6);
    assert_eq!(appended.rows[5], vec![t("西"), CellValue::Empty, CellValue::Empty, CellValue::Empty, t("新店")]);
}

#[test]
fn columns_are_added_by_expressions_conditions_and_index() {
    let out = run(vec![Step::AddColumn { name: "金額".into(), formula: "[数量] * [単価]".into(), ty: None }], sales());
    assert_eq!(column(&out, "金額"), [n(1200.0), n(400.0), n(840.0), n(900.0), CellValue::Empty]);
    let out = run(vec![Step::AddColumn { name: "ラベル".into(), formula: "[店] & \":\" & Text.Upper([商品])".into(), ty: None }], sales());
    assert_eq!(out.rows[0][4], t("北:りんご"));
    let out = run(vec![Step::AddColumn { name: "区分".into(), formula: "if [単価] >= 400 then \"高\" else if [単価] >= 100 then \"中\" else \"低\"".into(), ty: None }], sales());
    assert_eq!(column(&out, "区分"), [t("中"), t("低"), t("中"), t("高"), t("中")]);
    let out = run(
        vec![Step::AddConditionalColumn {
            name: "大口".into(),
            clauses: vec![Clause { column: "数量".into(), op: Op::GreaterOrEqual, value: "10".into(), output: "大".into() }, Clause { column: "数量".into(), op: Op::GreaterOrEqual, value: "5".into(), output: "中".into() }],
            otherwise: "小".into(),
        }],
        sales(),
    );
    assert_eq!(column(&out, "大口"), [t("大"), t("中"), t("中"), t("小"), t("小")]);
    let out = run(vec![Step::AddIndexColumn { name: "番号".into(), start: 1, step: 1 }], sales());
    assert_eq!(column(&out, "番号"), [n(1.0), n(2.0), n(3.0), n(4.0), n(5.0)]);
    assert!(Expr::parse("[a] +").is_err());
    assert!(Expr::parse("Evil.Call(1)").is_err());
    assert_eq!(Expr::parse("Text.Start([a], 2) & \"!\"").unwrap().columns(), ["a"]);
    let dated = QTable::from_rows(vec!["d".into()], vec![vec![n(45356.0)]]);
    let out = run(vec![Step::AddColumn { name: "y".into(), formula: "Date.Year([d]) * 100 + Date.Month([d])".into(), ty: None }], dated);
    assert_eq!(column(&out, "y"), [n(202403.0)]);
}

#[test]
fn parameters_fill_the_texts_of_steps() {
    let params = vec![Parameter { name: "最小".into(), value: "6".into() }];
    let step = Step::FilterRows { column: "数量".into(), op: Op::GreaterOrEqual, value: "{最小}".into() };
    let out = step.apply(&sales(), &params, &HashMap::<String, QTable>::new()).unwrap();
    assert_eq!(out.rows.len(), 2);
}

#[test]
fn errors_are_removed_and_replaced() {
    let raw = QTable::from_rows(vec!["x".into()], vec![vec![t("1")], vec![t("x")]]);
    let typed = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Whole, culture: Culture::Invariant, apart: false }], raw);
    assert_eq!(run(vec![Step::RemoveErrors(vec!["x".into()])], typed.clone()).rows.len(), 1);
    let out = run(vec![Step::ReplaceErrors { columns: vec!["x".into()], with: "0".into() }], typed);
    assert_eq!(column(&out, "x"), [n(1.0), n(0.0)]);
}

#[test]
fn the_m_code_names_the_steps_as_power_query_does() {
    let mut q = Query::new("売上", Source::Table("テーブル1".into()));
    q.steps = vec![
        Step::PromoteHeaders,
        Step::ChangeType { column: "数量".into(), ty: QType::Whole, culture: Culture::Invariant, apart: false },
        Step::FilterRows { column: "数量".into(), op: Op::Greater, value: "5".into() },
        Step::Sort(vec![("数量".into(), false)]),
    ];
    let m = query_to_m(&q);
    assert!(m.starts_with("let\n    Source = Excel.CurrentWorkbook(){[Name=\"テーブル1\"]}[Content],"), "{m}");
    assert!(m.contains("#\"Promoted Headers\" = Table.PromoteHeaders(Source, [PromoteAllScalars=true])"), "{m}");
    assert!(m.contains("#\"Changed Type\" = Table.TransformColumnTypes(#\"Promoted Headers\",{{\"数量\", Int64.Type}})"), "{m}");
    assert!(m.contains("#\"Filtered Rows\" = Table.SelectRows(#\"Changed Type\", each [数量] > 5)"), "{m}");
    assert!(m.ends_with("in\n    #\"Sorted Rows\""), "{m}");
    assert!(m.contains("Table.Sort(#\"Filtered Rows\",{{\"数量\", Order.Descending}})"), "{m}");
}

#[test]
fn a_query_gives_each_step_and_stops_at_the_one_that_fails() {
    let mut q = Query::new("Q", Source::Table("T".into()));
    q.steps = vec![Step::KeepTopRows(3), Step::RemoveColumns(vec!["無い".into()]), Step::ReverseRows];
    let (tables, error) = q.run(sales(), &[], &HashMap::<String, QTable>::new());
    assert_eq!(tables.len(), 2);
    assert_eq!(error.map(|e| e.0), Some(1));
}

#[test]
fn queries_are_written_and_read_back() {
    let mut q = Query::new("売上\t集計", Source::Range { sheet: "Sheet 1".into(), range: "A1:D5".into() });
    q.output = Some("売上 集計".into());
    q.steps = vec![
        Step::PromoteHeaders,
        Step::ChangeType { column: "数量".into(), ty: QType::Whole, culture: Culture::German, apart: false },
        Step::SelectColumns(vec!["店".into(), "数量".into()]),
        Step::FilterRows { column: "店".into(), op: Op::Contains, value: "北\n南".into() },
        Step::Sort(vec![("数量".into(), false), ("店".into(), true)]),
        Step::SplitColumn { column: "店".into(), by: SplitBy::Length(2), columns: 0 },
        Step::SplitColumn { column: "店".into(), by: SplitBy::Delimiter("\t".into()), columns: 0 },
        Step::GroupBy { keys: vec!["店".into()], aggregations: vec![("合計".into(), Agg::Sum, Some("数量".into())), ("行数".into(), Agg::Count, None)] },
        Step::Merge { right: "産地".into(), left_keys: vec!["a".into()], right_keys: vec!["b".into()], kind: JoinKind::FullOuter, expand: vec!["c".into(), "d".into()] },
        Step::AddConditionalColumn { name: "大口".into(), clauses: vec![Clause { column: "数量".into(), op: Op::GreaterOrEqual, value: "10".into(), output: "大".into() }], otherwise: "小".into() },
        Step::AddColumn { name: "金額".into(), formula: "[数量] * 2".into(), ty: None },
        Step::ReplaceValue { column: "店".into(), find: "a".into(), with: "b".into(), whole_cell: true },
        Step::Pivot { attribute: "月".into(), values: "売上".into(), agg: Agg::Average },
        Step::Unpivot { columns: vec!["4月".into()], attribute: "A".into(), value: "V".into() },
        Step::AddIndexColumn { name: "番号".into(), start: 0, step: 1 },
        Step::RemoveBlankRows,
        Step::Transpose,
        Step::ReplaceValue { column: "店".into(), find: "a\u{1f}b\u{1e}c".into(), with: "\u{1}".into(), whole_cell: false },
    ];
    let params = vec![Parameter { name: "最小".into(), value: "5".into() }];
    let text = encode_queries(&[q.clone()], &params);
    let (back, back_params) = decode_queries(&text);
    assert_eq!(back, vec![q]);
    assert_eq!(back_params, params);
}

#[test]
fn blank_query_roundtrip_and_m() {
    let mut q = Query::new("空のクエリ", Source::Blank);
    q.steps = vec![
        Step::AddIndexColumn { name: "Index".into(), start: 1, step: 1 },
        Step::AddColumn { name: "Value".into(), formula: "[Index] * 10".into(), ty: None },
    ];
    let m = query_to_m(&q);
    assert!(m.starts_with("let\n    Source = #table({}, {}),"), "{m}");
    assert!(m.contains("#\"Added Index\" = Table.AddIndexColumn(Source, \"Index\", 1, 1, Int64.Type)"), "{m}");
    assert!(m.contains("#\"Added Custom\" = Table.AddColumn(#\"Added Index\", \"Value\", each [Index] * 10)"), "{m}");
    assert!(m.ends_with("in\n    #\"Added Custom\""), "{m}");

    // Codec roundtrip
    let text = encode_queries(&[q.clone()], &[]);
    assert!(text.contains("source\tblank\n"), "{text}");
    let (back, _) = decode_queries(&text);
    assert_eq!(back, vec![q]);

    // read_source with Presentation dummy
    let p = Presentation::default();
    let src_table = read_source(&p, &Source::Blank).expect("read blank source");
    assert_eq!(src_table.names(), vec!["Column1"]);
    assert_eq!(src_table.rows.len(), 0);
}


fn s(x: &str) -> String {
    x.to_string()
}

/// GitHub issue #173: the code `query_to_m` writes reads back as the same steps (what the 詳細エディター lets be edited).
#[test]
fn m_code_reads_back_as_the_steps() {
    let steps = vec![
        Step::PromoteHeaders,
        Step::DemoteHeaders,
        Step::ChangeType { column: s("数量"), ty: QType::Whole, culture: Culture::Invariant, apart: false },
        Step::ChangeType { column: s("日付"), ty: QType::Date, culture: Culture::Japanese, apart: false },
        Step::SelectColumns(vec![s("店"), s("商\"品")]),
        Step::RemoveColumns(vec![s("単価")]),
        Step::RenameColumns(vec![(s("店"), s("店舗"))]),
        Step::RenameColumns(vec![(s("商品"), s("品名")), (s("数量"), s("個数")), (s("単価"), s("価格"))]),
        Step::ReorderColumns(vec![s("b"), s("a")]),
        Step::FilterRows { column: s("数量"), op: Op::GreaterOrEqual, value: s("5") },
        Step::FilterRows { column: s("店"), op: Op::Contains, value: s("北") },
        Step::FilterRows { column: s("店"), op: Op::DoesNotContain, value: s("北") },
        Step::FilterRows { column: s("店"), op: Op::BeginsWith, value: s("北") },
        Step::FilterRows { column: s("店"), op: Op::EndsWith, value: s("北") },
        Step::FilterRows { column: s("店"), op: Op::IsNull, value: s("") },
        Step::FilterRows { column: s("店"), op: Op::IsNotNull, value: s("") },
        Step::FilterRows { column: s("店"), op: Op::NotEquals, value: s("南") },
        Step::KeepTopRows(3),
        Step::KeepBottomRows(2),
        Step::RemoveTopRows(1),
        Step::RemoveBottomRows(1),
        Step::RemoveDuplicates(vec![]),
        Step::RemoveDuplicates(vec![s("店")]),
        Step::RemoveBlankRows,
        Step::RemoveErrors(vec![]),
        Step::RemoveErrors(vec![s("店")]),
        Step::ReplaceErrors { columns: vec![s("a"), s("b")], with: s("0") },
        Step::Sort(vec![(s("店"), true), (s("数量"), false)]),
        Step::SplitColumn { column: s("店"), by: SplitBy::Delimiter(s("-")), columns: 0 },
        Step::SplitColumn { column: s("店"), by: SplitBy::Length(3), columns: 0 },
        Step::MergeColumns { columns: vec![s("a"), s("b")], separator: s(", "), name: s("ab") },
        Step::ReplaceValue { column: s("店"), find: s("北"), with: s("N"), whole_cell: true },
        Step::ReplaceValue { column: s("店"), find: s("1"), with: s("x, y"), whole_cell: false },
        Step::ReplaceNull { columns: vec![s("a")], with: s("0") },
        Step::FillDown(vec![s("a")]),
        Step::FillUp(vec![s("a")]),
        Step::Transpose,
        Step::ReverseRows,
        Step::Trim(vec![s("a"), s("b")]),
        Step::Upper(vec![s("a")]),
        Step::Lower(vec![s("a")]),
        Step::Pivot { attribute: s("商品"), values: s("数量"), agg: Agg::Sum },
        Step::Pivot { attribute: s("商品"), values: s("数量"), agg: Agg::CountDistinct },
        Step::Unpivot { columns: vec![s("a"), s("b")], attribute: s("属性"), value: s("値") },
        Step::UnpivotOther { keep: vec![s("店")], attribute: s("属性"), value: s("値") },
        Step::GroupBy { keys: vec![s("店")], aggregations: vec![(s("件数"), Agg::Count, None), (s("合計"), Agg::Sum, Some(s("数量")))] },
        Step::Merge { right: s("Other Query"), left_keys: vec![s("店")], right_keys: vec![s("店")], kind: JoinKind::LeftOuter, expand: vec![s("住所")] },
        Step::Append { other: s("Q2") },
        Step::AddColumn { name: s("金額"), formula: s("[数量] * [単価]"), ty: None },
        Step::AddConditionalColumn { name: s("区分"), clauses: vec![Clause { column: s("数量"), op: Op::Greater, value: s("5"), output: s("多") }, Clause { column: s("店"), op: Op::Equals, value: s("北"), output: s("北") }], otherwise: s("少") },
        Step::AddIndexColumn { name: s("番号"), start: 1, step: 1 },
    ];
    for source in [Source::Blank, Source::Table(s("売上")), Source::Range { sheet: s("Sheet1"), range: s("A1:D9") }, Source::Parquet { names: vec![s("transactions.parquet")], columns: vec![s("amount")] }, Source::JsonLines { name: s("取引.ndjson") }, Source::ArrowIpc { name: s("取引.arrow") }, Source::Avro { name: s("取引.avro") }, Source::Sqlite { name: s("取引.db"), table: s("売上") }] {
        let query = Query { name: s("Q"), source, steps: steps.clone(), output: None };
        let code = query_to_m(&query);
        match m_to_query(&code, &query) {
            Ok(back) => {
                for (k, (a, b)) in query.steps.iter().zip(&back.steps).enumerate() {
                    assert_eq!(a, b, "step {k}\n{code}");
                }
                assert_eq!(back, query);
            }
            Err(e) => panic!("{e:?}\n{code}"),
        }
    }
}

#[test]
fn m_code_is_read_freely_and_refused_with_the_reason() {
    let base = Query::new("Q", Source::Table(s("T")));
    let free = "let\n  Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], // the table\n  /* a */ #\"Kept\" = Table.FirstN( Source , 2 ),\n  x=Table.Skip(#\"Kept\",1)\nin x";
    let q = m_to_query(free, &base).unwrap();
    assert_eq!(q.steps, vec![Step::KeepTopRows(2), Step::RemoveTopRows(1)]);
    let refused = |code: &str| m_to_query(code, &base).unwrap_err();
    assert_eq!(refused("Source = 1").key, "syntax");
    assert_eq!(refused("let Source = foo in Source").key, "source");
    // What the steps cannot hold is code from there on (K0039): it is read, and the interpreter says at run time what it cannot do.
    let code_of = |code: &str| m_to_query(code, &base).unwrap().steps;
    assert!(matches!(code_of("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.Foo(Source) in a").as_slice(), [Step::Code { .. }]));
    assert!(matches!(code_of("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Source, x) in a").as_slice(), [Step::Code { .. }]));
    assert_eq!(refused("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Source, 1) in Source").key, "result");
    assert!(matches!(code_of("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Other, 1) in a").as_slice(), [Step::Code { .. }]));
}


/// K0039: one `Table.RenameColumns` renames several columns (the user's report: "a rename could not take several columns at once"); the old names
/// are looked up before any of them is renamed, so two columns can exchange names; the code Excel writes (the one pair bare, `{a, b}`; the
/// optional third argument is not read) is read the same.
#[test]
fn several_columns_are_renamed_by_one_step() {
    let out = run(vec![Step::RenameColumns(vec![(s("店"), s("Shop")), (s("商品"), s("Item")), (s("単価"), s("Price"))])], sales());
    assert_eq!(out.names(), vec!["Shop", "Item", "数量", "Price"]);
    assert_eq!(column(&out, "Price")[0], n(120.0));
    // Exchanging two names in one step is allowed; the table keeps its columns.
    let out = run(vec![Step::RenameColumns(vec![(s("店"), s("商品")), (s("商品"), s("店"))])], sales());
    assert_eq!(out.names(), vec!["商品", "店", "数量", "単価"]);
    assert_eq!(column(&out, "商品")[0], t("北"));
    // Two columns with one name, or a missing column, fail.
    let mut q = Query::new("Q", Source::Table("T".into()));
    q.steps = vec![Step::RenameColumns(vec![(s("店"), s("数量"))])];
    assert!(q.run(sales(), &[], &HashMap::<String, QTable>::new()).1.is_some());
    q.steps = vec![Step::RenameColumns(vec![(s("無い"), s("x"))])];
    assert!(q.run(sales(), &[], &HashMap::<String, QTable>::new()).1.is_some());
    let base = Query::new("Q", Source::Table(s("T")));
    let src = "let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n  #\"Renamed Columns\" = Table.RenameColumns(Source,{{\"店\", \"Shop\"}, {\"商品\", \"Item\"}}),\n  #\"Renamed Columns1\" = Table.RenameColumns(#\"Renamed Columns\",{\"数量\", \"Qty\"}),\n  b = Table.RenameColumns(#\"Renamed Columns1\",{{\"単価\", \"P\"}}, MissingField.Ignore)\nin b";
    // The optional MissingField argument is read as a step that ignores a column that is not there.
    let q = m_to_query(src, &base).unwrap();
    assert_eq!(q.steps[2], Step::WithMissing { inner: Box::new(Step::RenameColumns(vec![(s("単価"), s("P"))])), missing: Missing::Ignore });
    let ok = src.replace(", MissingField.Ignore", "");
    let q = m_to_query(&ok, &base).unwrap();
    assert_eq!(q.steps, vec![
        Step::RenameColumns(vec![(s("店"), s("Shop")), (s("商品"), s("Item"))]),
        Step::RenameColumns(vec![(s("数量"), s("Qty"))]),
        Step::RenameColumns(vec![(s("単価"), s("P"))]),
    ]);
}

/// K0039: the shapes Power Query writes when its screens make a step (Microsoft Learn's Power Query M reference): a type change of several
/// columns (bare for one), a split column with the names it makes, Unpivot Columns (`Table.Unpivot`), Table.AddColumn with a type, a merge
/// as two steps, a replace of several columns, and text transforms without a type.
#[test]
fn the_steps_power_query_writes_are_read() {
    let base = Query::new("Q", Source::Table(s("T")));
    let read = |steps: &str| m_to_query(&format!("let\n  Source = Excel.CurrentWorkbook(){{[Name=\"T\"]}}[Content],\n{steps}\nin\n  z"), &base);
    let q = read("  z = Table.TransformColumnTypes(Source,{{\"a\", Int64.Type}, {\"b\", type text}})").unwrap();
    assert_eq!(q.steps, vec![
        Step::ChangeType { column: s("a"), ty: QType::Whole, culture: Culture::Invariant, apart: false },
        Step::ChangeType { column: s("b"), ty: QType::Text, culture: Culture::Invariant, apart: false },
    ]);
    let q = read("  z = Table.TransformColumnTypes(Source,{\"a\", type date}, \"ja-JP\")").unwrap();
    assert_eq!(q.steps, vec![Step::ChangeType { column: s("a"), ty: QType::Date, culture: Culture::Japanese, apart: false }]);
    let q = read("  z = Table.TransformColumnTypes(Source,{{\"a\", type date}}, [Culture=\"ja-JP\"])").unwrap();
    assert_eq!(q.steps, vec![Step::ChangeType { column: s("a"), ty: QType::Date, culture: Culture::Japanese, apart: false }]);
    // A type the steps do not have: the type is told.
    let q = read("\n  z = Table.TransformColumnTypes(Source,{{\"a\", type binary}})").unwrap();
    assert!(matches!(q.steps.as_slice(), [Step::Code { .. }]));
    let q = read("  z = Table.SplitColumn(Source, \"名前\", Splitter.SplitTextByDelimiter(\" \", QuoteStyle.Csv), {\"名前.1\", \"名前.2\"})").unwrap();
    assert_eq!(q.steps, vec![Step::SplitColumn { column: s("名前"), by: SplitBy::Delimiter(s(" ")), columns: 2 }]);
    assert!(matches!(read("  z = Table.SplitColumn(Source, \"名前\", Splitter.SplitTextByDelimiter(\" \"), {\"姓\", \"名\"})").unwrap().steps.as_slice(), [Step::Code { .. }]));
    let q = read("  z = Table.Unpivot(Source, {\"1月\", \"2月\"}, \"Attribute\", \"Value\")").unwrap();
    assert_eq!(q.steps, vec![Step::Unpivot { columns: vec![s("1月"), s("2月")], attribute: s("Attribute"), value: s("Value") }]);
    let q = read("  z = Table.AddColumn(Source, \"Custom\", each [a] * 2, type number)").unwrap();
    assert_eq!(q.steps, vec![Step::AddColumn { name: s("Custom"), formula: s("[a] * 2"), ty: Some(QType::Decimal) }]);
    let q = read("  #\"Merged Queries\" = Table.NestedJoin(Source, {\"k\"}, Other, {\"k\"}, \"Other\", JoinKind.LeftOuter),\n  z = Table.ExpandTableColumn(#\"Merged Queries\", \"Other\", {\"v\", \"w\"}, {\"Other.v\", \"Other.w\"})").unwrap();
    assert_eq!(q.steps, vec![Step::Merge { right: s("Other"), left_keys: vec![s("k")], right_keys: vec![s("k")], kind: JoinKind::LeftOuter, expand: vec![s("v"), s("w")] }]);
    // A merge that is not expanded cannot be a step: it is told.
    let e = read("  z = Table.NestedJoin(Source, {\"k\"}, Other, {\"k\"}, \"Other\", JoinKind.Inner)").unwrap_err();
    assert_eq!((e.key, e.line), ("merge", 3));
    let q = read("  z = Table.ReplaceValue(Source, \"a\", \"b\", Replacer.ReplaceText, {\"c1\", \"c2\"})").unwrap();
    assert_eq!(q.steps.len(), 2);
    let q = read("  z = Table.TransformColumns(Source, {{\"a\", Text.Trim}, {\"b\", Text.Trim}})").unwrap();
    assert_eq!(q.steps, vec![Step::Trim(vec![s("a"), s("b")])]);
    let q = read("  z = Table.Group(Source, {\"k\"}, {{\"n\", each List.Count(List.Distinct([a])), Int64.Type}})").unwrap();
    assert_eq!(q.steps, vec![Step::GroupBy { keys: vec![s("k")], aggregations: vec![(s("n"), Agg::CountDistinct, Some(s("a")))] }]);
    // An unknown function: the steps before are steps, the rest is code (the interpreter names the function and the line when it runs).
    let q = read("  a = Table.FirstN(Source, 2),\n  z = Table.Frobnicate(a)").unwrap();
    assert!(matches!(q.steps.as_slice(), [Step::KeepTopRows(2), Step::Code { names, bindings, .. }] if names.len() == 2 && bindings.starts_with("z = Table.Frobnicate")), "{:?}", q.steps);
}


fn base() -> Query {
    Query::new("Q", Source::Table(s("T")))
}

/// The steps a code of one step makes.
fn steps_of(step: &str) -> Vec<Step> {
    m_to_query(&format!("let\n  Source = Excel.CurrentWorkbook(){{[Name=\"T\"]}}[Content],\n  z = {step}\nin\n  z"), &base()).unwrap_or_else(|e| panic!("{step}: {e:?}")).steps
}

/// The code written for a step reads back as the same step, and the stored text keeps it.
fn roundtrip(step: Step) {
    let mut q = base();
    q.steps = vec![step.clone()];
    let code = query_to_m(&q);
    assert_eq!(m_to_query(&code, &base()).unwrap_or_else(|e| panic!("{code}: {e:?}")).steps, vec![step.clone()], "{code}");
    let (back, _) = decode_queries(&encode_queries(&[q], &[]));
    assert_eq!(back[0].steps, vec![step]);
}

fn leaf(column: &str, op: Op, value: &str) -> Cond {
    Cond::Leaf { column: s(column), op, value: s(value) }
}

/// K0039 (2): a row filter of several comparisons (and / or, brackets as in M), a list of values, "does not begin / end with", dates.
#[test]
fn row_conditions_of_several_comparisons_are_read_and_worked_out() {
    let q = |x: &str| steps_of(&format!("Table.SelectRows(Source, each {x})"));
    assert_eq!(q("[a] = 1 or [a] = 2"), vec![Step::FilterCondition(Cond::Or(vec![leaf("a", Op::Equals, "1"), leaf("a", Op::Equals, "2")]))]);
    // `and` binds tighter than `or`; brackets are kept as a group.
    assert_eq!(
        q("([a] >= 1 and [a] <= 5) or [b] = \"x\""),
        vec![Step::FilterCondition(Cond::Or(vec![Cond::Group(Box::new(Cond::And(vec![leaf("a", Op::GreaterOrEqual, "1"), leaf("a", Op::LessOrEqual, "5")]))), leaf("b", Op::Equals, "x")]))]
    );
    assert_eq!(q("[a] = 1 or [a] = 2 and [b] = 3"), vec![Step::FilterCondition(Cond::Or(vec![leaf("a", Op::Equals, "1"), Cond::And(vec![leaf("a", Op::Equals, "2"), leaf("b", Op::Equals, "3")])]))]);
    // A word inside a string is not an operator.
    assert_eq!(q("[a] = \"x or y\""), vec![Step::FilterRows { column: s("a"), op: Op::Equals, value: s("x or y") }]);
    assert_eq!(q("List.Contains({\"北\", \"南\"}, [店])"), vec![Step::FilterCondition(Cond::Or(vec![leaf("店", Op::Equals, "北"), leaf("店", Op::Equals, "南")]))]);
    assert_eq!(q("not Text.StartsWith([a], \"x\")"), vec![Step::FilterRows { column: s("a"), op: Op::DoesNotBeginWith, value: s("x") }]);
    assert_eq!(q("not Text.EndsWith([a], \"x\")"), vec![Step::FilterRows { column: s("a"), op: Op::DoesNotEndWith, value: s("x") }]);
    assert_eq!(q("not List.Contains({1, 2}, [a])"), vec![Step::FilterCondition(Cond::Not(Box::new(Cond::Or(vec![leaf("a", Op::Equals, "1"), leaf("a", Op::Equals, "2")]))))]);
    for step in [
        Step::FilterCondition(Cond::And(vec![leaf("a", Op::Greater, "1"), Cond::Group(Box::new(Cond::Or(vec![leaf("b", Op::BeginsWith, "x"), leaf("b", Op::IsNull, "")])))])),
        Step::FilterCondition(Cond::Not(Box::new(Cond::And(vec![leaf("a", Op::Equals, "1"), leaf("b", Op::Equals, "2")])))),
        Step::FilterRows { column: s("a"), op: Op::DoesNotBeginWith, value: s("x") },
        Step::FilterRows { column: s("日付"), op: Op::GreaterOrEqual, value: s("#date(2024, 1, 31)") },
    ] {
        roundtrip(step);
    }
    // Worked out on rows.
    let out = run(vec![Step::FilterCondition(Cond::Or(vec![leaf("店", Op::Equals, "東"), Cond::And(vec![leaf("店", Op::Equals, "南"), leaf("数量", Op::Greater, "5")])]))], sales());
    assert_eq!(column(&out, "商品"), vec![t("りんご"), t("りんご")]);
    let out = run(vec![Step::FilterRows { column: s("商品"), op: Op::DoesNotBeginWith, value: s("り") }], sales());
    assert_eq!(out.rows.len(), 2);
    let out = run(vec![Step::FilterRows { column: s("商品"), op: Op::DoesNotEndWith, value: s("ご") }], sales());
    assert_eq!(out.rows.len(), 2);
}

/// K0039 (2): `[#"a b"]`, a column whose name has marks, in conditions, groups and custom columns; the names are written the way M needs.
#[test]
fn columns_with_marks_in_their_names_are_read_and_written() {
    assert_eq!(steps_of("Table.SelectRows(Source, each [#\"a-b\"] > 1)"), vec![Step::FilterRows { column: s("a-b"), op: Op::Greater, value: s("1") }]);
    assert_eq!(steps_of("Table.SelectRows(Source, each [Sales Amount] > 1)"), vec![Step::FilterRows { column: s("Sales Amount"), op: Op::Greater, value: s("1") }]);
    assert_eq!(steps_of("Table.SelectRows(Source, each Text.Contains([#\"x\"\"y\"], \"z\"))"), vec![Step::FilterRows { column: s("x\"y"), op: Op::Contains, value: s("z") }]);
    let code = query_to_m(&{
        let mut q = base();
        q.steps = vec![Step::FilterRows { column: s("a-b"), op: Op::Equals, value: s("1") }, Step::FilterRows { column: s("a b"), op: Op::Equals, value: s("1") }];
        q
    });
    assert!(code.contains("each [#\"a-b\"] = 1"), "{code}");
    assert!(code.contains("each [a b] = 1"), "{code}");
    roundtrip(Step::FilterRows { column: s("a]b"), op: Op::Equals, value: s("1") });
    roundtrip(Step::GroupBy { keys: vec![s("k")], aggregations: vec![(s("n"), Agg::Sum, Some(s("a-b"))), (s("m"), Agg::CountDistinct, Some(s("c d")))] });
    roundtrip(Step::Pivot { attribute: s("a-b"), values: s("v"), agg: Agg::Sum });
    // A custom column's expression reads the same name.
    let e = Expr::parse("[#\"a-b\"] + 1").unwrap();
    let names = vec![s("a-b")];
    assert_eq!(e.eval(&names, &[n(2.0)]), n(3.0));
}

/// K0039 (3): the types Power Query has beyond the first eight.
#[test]
fn more_types_are_read_converted_and_written() {
    for (m, ty) in [
        ("Currency.Type", QType::Currency),
        ("Percentage.Type", QType::Percentage),
        ("Int32.Type", QType::Int32),
        ("type datetimezone", QType::DateTimeZone),
        ("type duration", QType::Duration),
        ("type nullable text", QType::Text),
        ("type nullable number", QType::Decimal),
        ("Number.Type", QType::Decimal),
        ("Text.Type", QType::Text),
    ] {
        assert_eq!(steps_of(&format!("Table.TransformColumnTypes(Source, {{{{\"a\", {m}}}}})")), vec![Step::ChangeType { column: s("a"), ty, culture: Culture::Invariant, apart: false }], "{m}");
    }
    for ty in QType::ALL {
        roundtrip(Step::ChangeType { column: s("a"), ty, culture: Culture::Invariant, apart: false });
    }
    let conv = |v: CellValue, ty| convert(&v, ty, Culture::Invariant);
    assert_eq!(conv(t("$1,234.56789"), QType::Currency), n(1234.5679));
    assert_eq!(conv(n(0.123456), QType::Currency), n(0.1235));
    assert_eq!(conv(t("12%"), QType::Percentage), n(0.12));
    assert_eq!(conv(t("2.5"), QType::Int32), n(2.0));
    assert_eq!(conv(t("3.5"), QType::Int32), n(4.0));
    assert_eq!(conv(n(3e9), QType::Int32), CellValue::Error(CellError::Value));
    // The zone is dropped; the clock time written stays.
    assert_eq!(conv(t("2024-03-05T10:00:00+09:00"), QType::DateTimeZone), conv(t("2024-03-05T10:00:00"), QType::DateTime));
    assert_eq!(conv(t("2024-03-05T10:00:00Z"), QType::DateTimeZone), conv(t("2024-03-05T10:00:00"), QType::DateTime));
    assert_eq!(conv(t("1.12:00:00"), QType::Duration), n(1.5));
    assert_eq!(conv(t("-06:00:00"), QType::Duration), n(-0.25));
    assert_eq!(conv(t("abc"), QType::Duration), CellValue::Error(CellError::Value));
}

/// K0039 (4): `MissingField.*`.
#[test]
fn a_missing_column_is_ignored_or_made_of_nulls_as_the_code_says() {
    let q = |x: &str| steps_of(x);
    let inner = Step::SelectColumns(vec![s("店"), s("無い")]);
    assert_eq!(q("Table.SelectColumns(Source, {\"店\", \"無い\"}, MissingField.UseNull)"), vec![Step::WithMissing { inner: Box::new(inner.clone()), missing: Missing::UseNull }]);
    assert_eq!(q("Table.SelectColumns(Source, {\"店\"}, MissingField.Error)"), vec![Step::SelectColumns(vec![s("店")])]);
    assert_eq!(
        q("Table.TransformColumnTypes(Source, {{\"a\", type text}}, [MissingField=MissingField.Ignore])"),
        vec![Step::WithMissing { inner: Box::new(Step::ChangeType { column: s("a"), ty: QType::Text, culture: Culture::Invariant, apart: false }), missing: Missing::Ignore }]
    );
    assert_eq!(
        q("Table.TransformColumnTypes(Source, {{\"a\", type text}}, [Culture=\"ja-JP\", MissingField=MissingField.UseNull])"),
        vec![Step::WithMissing { inner: Box::new(Step::ChangeType { column: s("a"), ty: QType::Text, culture: Culture::Japanese, apart: false }), missing: Missing::UseNull }]
    );
    for inner in [
        inner,
        Step::RemoveColumns(vec![s("店")]),
        Step::ReorderColumns(vec![s("数量")]),
        Step::RenameColumns(vec![(s("店"), s("Shop"))]),
        Step::ChangeType { column: s("a"), ty: QType::Whole, culture: Culture::Invariant, apart: false },
        Step::ChangeType { column: s("a"), ty: QType::Date, culture: Culture::Japanese, apart: false },
    ] {
        roundtrip(Step::WithMissing { inner: Box::new(inner.clone()), missing: Missing::UseNull });
        roundtrip(Step::WithMissing { inner: Box::new(inner), missing: Missing::Ignore });
    }
    let with = |inner, missing| Step::WithMissing { inner: Box::new(inner), missing };
    let out = run(vec![with(Step::SelectColumns(vec![s("店"), s("無い")]), Missing::UseNull)], sales());
    assert_eq!(out.names(), vec!["店", "無い"]);
    assert!(column(&out, "無い").iter().all(|v| *v == CellValue::Empty));
    let out = run(vec![with(Step::SelectColumns(vec![s("店"), s("無い")]), Missing::Ignore)], sales());
    assert_eq!(out.names(), vec!["店"]);
    let out = run(vec![with(Step::RemoveColumns(vec![s("店"), s("無い")]), Missing::Ignore)], sales());
    assert_eq!(out.names(), vec!["商品", "数量", "単価"]);
    let out = run(vec![with(Step::RenameColumns(vec![(s("店"), s("S")), (s("無い"), s("x"))]), Missing::Ignore)], sales());
    assert_eq!(out.names()[0], "S");
    let out = run(vec![with(Step::ChangeType { column: s("無い"), ty: QType::Text, culture: Culture::Invariant, apart: false }, Missing::Ignore)], sales());
    assert_eq!(out, sales());
    let mut failing = base();
    failing.steps = vec![with(Step::SelectColumns(vec![s("無い")]), Missing::Error)];
    assert!(failing.run(sales(), &[], &HashMap::<String, QTable>::new()).1.is_some());
}

/// K0039 (5): the other splitters (Microsoft Learn's examples), and the functions `Table.TransformColumns` takes.
#[test]
fn other_splitters_and_transform_functions_are_read_and_worked_out() {
    let split = |text: &str, by: SplitBy| cond::split_text(text, &by);
    let v = |x: &[&str]| x.iter().map(|y| y.to_string()).collect::<Vec<_>>();
    assert_eq!(split("a,b;c,d", SplitBy::EachDelimiter { delimiters: v(&[",", ";"]), from_end: false }), v(&["a", "b", "c,d"]));
    assert_eq!(split("a,\"b;c\",d", SplitBy::EachDelimiter { delimiters: v(&[",", ";"]), from_end: true }), v(&["a,\"b", "c\"", "d"]));
    assert_eq!(split("a,b;c", SplitBy::AnyDelimiter(v(&[",", ";"]))), v(&["a", "b", "c"]));
    assert_eq!(split("ABC|12345", SplitBy::Positions { positions: vec![0, 3, 4], from_end: false }), v(&["ABC", "|", "12345"]));
    assert_eq!(split("Redmond98052", SplitBy::Positions { positions: vec![0, 5], from_end: true }), v(&["Redmond", "98052"]));
    assert_eq!(split("AB123", SplitBy::Lengths { lengths: vec![2, 3], from_end: false }), v(&["AB", "123"]));
    assert_eq!(split("RedmondWA98052", SplitBy::Lengths { lengths: vec![5, 2], from_end: true }), v(&["WA", "98052"]));
    for (code, by) in [
        ("Splitter.SplitTextByAnyDelimiter({\",\", \";\"}, QuoteStyle.Csv)", SplitBy::AnyDelimiter(v(&[",", ";"]))),
        ("Splitter.SplitTextByEachDelimiter({\",\"}, QuoteStyle.Csv, true)", SplitBy::EachDelimiter { delimiters: v(&[","]), from_end: true }),
        ("Splitter.SplitTextByEachDelimiter({\",\", \";\"})", SplitBy::EachDelimiter { delimiters: v(&[",", ";"]), from_end: false }),
        ("Splitter.SplitTextByPositions({0, 3})", SplitBy::Positions { positions: vec![0, 3], from_end: false }),
        ("Splitter.SplitTextByLengths({2, 3}, true)", SplitBy::Lengths { lengths: vec![2, 3], from_end: true }),
    ] {
        assert_eq!(steps_of(&format!("Table.SplitColumn(Source, \"c\", {code}, {{\"c.1\", \"c.2\"}})")), vec![Step::SplitColumn { column: s("c"), by: by.clone(), columns: 2 }], "{code}");
        roundtrip(Step::SplitColumn { column: s("c"), by, columns: 0 });
    }
    // A transform of several functions is a step for each column.
    assert_eq!(
        steps_of("Table.TransformColumns(Source, {{\"a\", Text.Proper, type text}, {\"b\", each Number.Round(_, 2), type number}, {\"c\", each \"No.\" & _, type text}, {\"d\", Date.Year, Int64.Type}})"),
        vec![
            Step::Transform { column: s("a"), function: Transform::Proper },
            Step::Transform { column: s("b"), function: Transform::Round(2) },
            Step::Transform { column: s("c"), function: Transform::Prefix(s("No.")) },
            Step::Transform { column: s("d"), function: Transform::Year },
        ]
    );
    assert_eq!(steps_of("Table.TransformColumns(Source, {{\"a\", Text.Upper}, {\"b\", Text.Upper}})"), vec![Step::Upper(vec![s("a"), s("b")])]);
    // (Trim, Upper and Lower of one column are the steps of their own: `Step::Trim` and the others.)
    assert_eq!(steps_of("Table.TransformColumns(Source, {{\"a\", Text.Lower, type text}})"), vec![Step::Lower(vec![s("a")])]);
    for f in [
        Transform::Proper, Transform::Clean, Transform::Length, Transform::Round(-1), Transform::RoundUp, Transform::RoundDown,
        Transform::Abs, Transform::Sqrt, Transform::Year, Transform::Month, Transform::Day, Transform::Prefix(s("a\"b")), Transform::Suffix(s("-")),
    ] {
        roundtrip(Step::Transform { column: s("a"), function: f });
    }
    let apply = |f: Transform, v: CellValue| f.apply(&v);
    assert_eq!(apply(Transform::Proper, t("hello wORLD")), t("Hello World"));
    assert_eq!(apply(Transform::Clean, t("a\u{1}b\tc")), t("abc"));
    assert_eq!(apply(Transform::Length, t("りんご")), n(3.0));
    assert_eq!(apply(Transform::Round(1), n(2.25)), n(2.2));
    assert_eq!(apply(Transform::Sqrt, n(9.0)), n(3.0));
    assert_eq!(apply(Transform::Suffix(s("円")), n(5.0)), t("5円"));
    assert_eq!(apply(Transform::Year, n(45000.0)), n(2023.0));
    assert_eq!(apply(Transform::Upper, CellValue::Empty), CellValue::Empty);
    let out = run(vec![Step::Transform { column: s("商品"), function: Transform::Prefix(s("* ")) }, Step::Transform { column: s("単価"), function: Transform::Round(-2) }], sales());
    assert_eq!(column(&out, "商品")[0], t("* りんご"));
    // Number.Round rounds half to even: 450 is 400.
    assert_eq!(column(&out, "単価")[3], n(400.0));
}

/// K0039 (6): GroupKind.Local, Table.AlternateRows, Table.Range, Table.DuplicateColumn (Microsoft Learn's examples).
#[test]
fn more_steps_are_read_and_worked_out() {
    let agg = vec![(s("n"), Agg::Count, None)];
    assert_eq!(steps_of("Table.Group(Source, {\"k\"}, {{\"n\", each Table.RowCount(_), Int64.Type}}, GroupKind.Local)"), vec![Step::GroupByLocal { keys: vec![s("k")], aggregations: agg.clone() }]);
    assert_eq!(steps_of("Table.Group(Source, {\"k\"}, {{\"n\", each Table.RowCount(_), Int64.Type}}, GroupKind.Global)"), vec![Step::GroupBy { keys: vec![s("k")], aggregations: agg.clone() }]);
    // All the rows of a group: a column of tables, told with the reason.
    // (the interpreter holds it: it is a code step now)
    let q = m_to_query("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n z = Table.Group(Source, {\"k\"}, {{\"All\", each _, type table}}) in z", &base()).unwrap();
    assert!(matches!(q.steps.as_slice(), [Step::Code { .. }]));
    for step in [
        Step::GroupByLocal { keys: vec![s("k")], aggregations: agg },
        Step::AlternateRows { offset: 1, skip: 1, take: 1 },
        Step::KeepRange { offset: 1, count: Some(2) },
        Step::KeepRange { offset: 3, count: None },
        Step::DuplicateColumn { column: s("a b"), new_name: s("copied column") },
    ] {
        roundtrip(step);
    }
    let table = QTable::from_rows(vec![s("k"), s("v")], [("a", 1.0), ("a", 2.0), ("b", 3.0), ("a", 4.0), ("a", 5.0)].iter().map(|(k, v)| vec![t(k), n(*v)]).collect());
    let out = run(vec![Step::GroupByLocal { keys: vec![s("k")], aggregations: vec![(s("sum"), Agg::Sum, Some(s("v")))] }], table.clone());
    assert_eq!(column(&out, "k"), vec![t("a"), t("b"), t("a")]);
    assert_eq!(column(&out, "sum"), vec![n(3.0), n(3.0), n(9.0)]);
    let out = run(vec![Step::GroupBy { keys: vec![s("k")], aggregations: vec![(s("sum"), Agg::Sum, Some(s("v")))] }], table.clone());
    assert_eq!(column(&out, "sum"), vec![n(12.0), n(3.0)]);
    let three = QTable::from_rows(vec![s("v")], (1..=3).map(|i| vec![n(f64::from(i))]).collect());
    assert_eq!(column(&run(vec![Step::AlternateRows { offset: 1, skip: 1, take: 1 }], three), "v"), vec![n(1.0), n(3.0)]);
    let ten = QTable::from_rows(vec![s("v")], (1..=10).map(|i| vec![n(f64::from(i))]).collect());
    assert_eq!(column(&run(vec![Step::AlternateRows { offset: 0, skip: 2, take: 3 }], ten.clone()), "v"), (3..=5).chain(8..=10).map(|i| n(f64::from(i))).collect::<Vec<_>>());
    let four = QTable::from_rows(vec![s("v")], (1..=4).map(|i| vec![n(f64::from(i))]).collect());
    assert_eq!(column(&run(vec![Step::KeepRange { offset: 1, count: None }], four.clone()), "v"), vec![n(2.0), n(3.0), n(4.0)]);
    assert_eq!(column(&run(vec![Step::KeepRange { offset: 1, count: Some(1) }], four.clone()), "v"), vec![n(2.0)]);
    assert_eq!(run(vec![Step::KeepRange { offset: 9, count: Some(1) }], four).rows.len(), 0);
    let out = run(vec![Step::DuplicateColumn { column: s("店"), new_name: s("コピー") }], sales());
    assert_eq!(out.names(), vec!["店", "商品", "数量", "単価", "コピー"]);
    assert_eq!(column(&out, "コピー"), column(&out, "店"));
}

/// K0039 (2): windows of dates, parts of a date, date literals.
#[test]
fn date_filters_are_read_and_worked_out() {
    for (code, when, unit, count) in [
        ("Date.IsInPreviousNDays([d], 7)", DateWhen::Previous, DateUnit::Day, Some(7)),
        ("Date.IsInNextNMonths([d], 2)", DateWhen::Next, DateUnit::Month, Some(2)),
        ("Date.IsInCurrentWeek([d])", DateWhen::Current, DateUnit::Week, None),
        ("Date.IsInPreviousQuarter([d])", DateWhen::Previous, DateUnit::Quarter, None),
        ("Date.IsInNextYear([d])", DateWhen::Next, DateUnit::Year, None),
        ("Date.IsInYearToDate([d])", DateWhen::YearToDate, DateUnit::Year, None),
    ] {
        let step = Step::FilterCondition(Cond::Date { column: s("d"), filter: DateFilter { when, unit, count } });
        assert_eq!(steps_of(&format!("Table.SelectRows(Source, each {code})")), vec![step.clone()], "{code}");
        roundtrip(step);
    }
    assert_eq!(
        steps_of("Table.SelectRows(Source, each Date.Year([d]) = 2024 and Date.Month([d]) >= 4)"),
        vec![Step::FilterCondition(Cond::And(vec![
            Cond::Part { column: s("d"), part: DatePart::Year, op: Op::Equals, value: s("2024") },
            Cond::Part { column: s("d"), part: DatePart::Month, op: Op::GreaterOrEqual, value: s("4") },
        ]))]
    );
    roundtrip(Step::FilterCondition(Cond::Part { column: s("d"), part: DatePart::Quarter, op: Op::Less, value: s("3") }));
    assert_eq!(steps_of("Table.SelectRows(Source, each [d] >= #date(2024, 4, 1))"), vec![Step::FilterRows { column: s("d"), op: Op::GreaterOrEqual, value: s("#date(2024, 4, 1)") }]);
    // The days of 2024-03-05 (a Tuesday) and the windows around it.
    let serial = |y, m, d| (ibis_calc::eval::convert::days_from_civil(y, m, d) + 25569) as i64;
    let today = serial(2024, 3, 5);
    let f = |when, unit, count| DateFilter { when, unit, count };
    let at = |filter: DateFilter, y, m, d| filter.contains(serial(y, m, d) as f64 + 0.75, today);
    assert!(at(f(DateWhen::Previous, DateUnit::Day, Some(7)), 2024, 2, 27));
    assert!(!at(f(DateWhen::Previous, DateUnit::Day, Some(7)), 2024, 2, 26));
    // Today is not in "the previous days".
    assert!(!at(f(DateWhen::Previous, DateUnit::Day, Some(7)), 2024, 3, 5));
    assert!(at(f(DateWhen::Next, DateUnit::Day, Some(2)), 2024, 3, 7));
    assert!(!at(f(DateWhen::Next, DateUnit::Day, Some(2)), 2024, 3, 8));
    // Weeks run Sunday to Saturday: 3/3 to 3/9.
    assert!(at(f(DateWhen::Current, DateUnit::Week, None), 2024, 3, 3));
    assert!(at(f(DateWhen::Current, DateUnit::Week, None), 2024, 3, 9));
    assert!(!at(f(DateWhen::Current, DateUnit::Week, None), 2024, 3, 10));
    assert!(!at(f(DateWhen::Current, DateUnit::Week, None), 2024, 3, 2));
    assert!(at(f(DateWhen::Previous, DateUnit::Week, None), 2024, 2, 25));
    assert!(at(f(DateWhen::Previous, DateUnit::Month, None), 2024, 2, 29));
    assert!(!at(f(DateWhen::Previous, DateUnit::Month, None), 2024, 3, 1));
    assert!(at(f(DateWhen::Previous, DateUnit::Quarter, None), 2023, 12, 31));
    assert!(at(f(DateWhen::Current, DateUnit::Quarter, None), 2024, 1, 1));
    assert!(at(f(DateWhen::Next, DateUnit::Year, None), 2025, 12, 31));
    assert!(at(f(DateWhen::YearToDate, DateUnit::Year, None), 2024, 3, 5));
    assert!(!at(f(DateWhen::YearToDate, DateUnit::Year, None), 2024, 3, 6));
    assert!(!at(f(DateWhen::YearToDate, DateUnit::Year, None), 2023, 12, 31));
    // A comparison with a date literal, and a part of a date, on rows.
    let dates = QTable::from_rows(vec![s("d")], vec![vec![n(serial(2024, 3, 1) as f64)], vec![n(serial(2024, 4, 1) as f64)], vec![n(serial(2025, 5, 9) as f64)], vec![t("text")]]);
    let out = run(vec![Step::FilterRows { column: s("d"), op: Op::GreaterOrEqual, value: s("#date(2024, 4, 1)") }], dates.clone());
    assert_eq!(out.rows.len(), 2);
    let out = run(vec![Step::FilterCondition(Cond::Part { column: s("d"), part: DatePart::Year, op: Op::Equals, value: s("2024") })], dates);
    assert_eq!(out.rows.len(), 2);
}

/// K0039 (7): the sources that can be read; the others are told with the line.
#[test]
fn the_sources_that_are_read_and_those_that_are_not() {
    let read = |source: &str| m_to_query(&format!("let\n  Source = {source}\nin\n  Source"), &base());
    assert_eq!(read("Excel.CurrentWorkbook(){[Name=\"T\"]}[Content]").unwrap().source, Source::Table(s("T")));
    assert_eq!(read("Excel.CurrentWorkbook(){[ Name = \"T 1\" ]}[Content]").unwrap().source, Source::Table(s("T 1")));
    assert_eq!(read("#table({}, {})").unwrap().source, Source::Blank);
    assert_eq!(
        read("Csv.Document(File.Contents(\"C:\\a.csv\"), [Delimiter=\",\", Columns=3, Encoding=65001, QuoteStyle=QuoteStyle.None])").unwrap().source,
        Source::Csv { path: s("C:\\a.csv"), delimiter: s(","), columns: 3, encoding: 65001, quote: s("None") }
    );
    for code in [
        "Excel.Workbook(File.Contents(\"C:\\a.xlsx\"), null, true)",
        "Web.Contents(\"https://example.com\")",
        "Sql.Database(\"srv\", \"db\")",
    ] {
        let e = read(code).unwrap_err();
        assert_eq!((e.key, e.line), ("source", 2), "{code}");
    }
}

#[test]
fn parquet_query_saves_its_source_without_granting_file_access() {
    let query = Query::new("Source", Source::Parquet { names: vec!["明細.parquet".into(), "別\t表.parquet".into()], columns: vec!["code".into(), " 金額\t".into()] });
    let (saved, _) = decode_queries(&encode_queries(&[query.clone()], &[]));
    assert_eq!(saved, [query]);
    assert!(read_source(&Presentation::default(), &saved[0].source).is_err());
    let (legacy, _) = decode_queries("ibis-queries 1\nquery\tSource\t\nsource\tparquet\tlegacy.parquet\n");
    assert_eq!(legacy[0].source, Source::Parquet { names: vec!["legacy.parquet".into()], columns: Vec::new() });
}

#[test]
fn single_file_query_saves_its_source_without_granting_file_access() {
    for source in [
        Source::JsonLines { name: "取引\t記録.ndjson".into() },
        Source::Sqlite { name: "取引\t記録.db".into(), table: "売上\t集計".into() },
        Source::JsonArray { name: "cargo.json".into(), key: Some("packages".into()) },
        Source::JsonArray { name: "records.json".into(), key: None },
        Source::Delimited { name: "log.tsv".into(), encoding: "UTF-8".into(), delimiter: Some('\t'), start_row: 2, headers: false },
        Source::ParquetFolder { name: "dataset".into(), columns: vec!["amount".into()] },
        Source::Avro { name: "取引\t記録.avro".into() },
        Source::ArrowIpc { name: "取引\t記録.arrow".into() },
    ] {
        let query = Query::new("Records", source);
        let (saved, _) = decode_queries(&encode_queries(std::slice::from_ref(&query), &[]));
        assert_eq!(saved, [query]);
        assert!(read_source(&Presentation::default(), &saved[0].source).is_err());
    }
}
