// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use std::collections::HashMap;

use ibis_calc::CellValue;

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
    let out = run(vec![Step::RemoveColumns(vec!["店".into()]), Step::RenameColumn { from: "単価".into(), to: "価格".into() }], sales());
    assert_eq!(out.names(), ["商品", "数量", "価格"]);
    let out = run(vec![Step::ReorderColumns(vec!["単価".into()])], sales());
    assert_eq!(out.names(), ["単価", "店", "商品", "数量"]);
    let q = Query { name: "Q".into(), source: Source::Table("T".into()), steps: vec![Step::RemoveColumns(vec!["無い".into()])], output: None };
    let (_, error) = q.run(sales(), &[], &HashMap::<String, QTable>::new());
    assert_eq!(error.map(|e| e.0), Some(0));
}

#[test]
fn types_are_changed_by_the_culture() {
    let raw = QTable::from_rows(vec!["x".into()], vec![vec![t("1,234.5")], vec![t("abc")], vec![CellValue::Empty]]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture: Culture::EnglishUs }], raw.clone());
    assert_eq!(column(&out, "x"), [n(1234.5), CellValue::Error(ibis_calc::CellError::Value), CellValue::Empty]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Decimal, culture: Culture::German }], QTable::from_rows(vec!["x".into()], vec![vec![t("1.234,5")]]));
    assert_eq!(column(&out, "x"), [n(1234.5)]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Whole, culture: Culture::Invariant }], QTable::from_rows(vec!["x".into()], vec![vec![t("2.5")], vec![t("-2.5")]]));
    assert_eq!(column(&out, "x"), [n(3.0), n(-3.0)]);
    let dates = QTable::from_rows(vec!["d".into()], vec![vec![t("2024-03-05")], vec![t("3/5/2024")], vec![t("2024年3月5日")]]);
    let out = run(vec![Step::ChangeType { column: "d".into(), ty: QType::Date, culture: Culture::EnglishUs }], dates);
    assert_eq!(column(&out, "d"), [n(45356.0), n(45356.0), n(45356.0)]);
    let dmy = QTable::from_rows(vec!["d".into()], vec![vec![t("5/3/2024")]]);
    let out = run(vec![Step::ChangeType { column: "d".into(), ty: QType::Date, culture: Culture::German }], dmy);
    assert_eq!(column(&out, "d"), [n(45356.0)]);
    let out = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Logical, culture: Culture::Invariant }], QTable::from_rows(vec!["x".into()], vec![vec![t("true")], vec![n(0.0)]]));
    assert_eq!(column(&out, "x"), [CellValue::Bool(true), CellValue::Bool(false)]);
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
    let out = run(vec![Step::SplitColumn { column: "名前".into(), by: SplitBy::Delimiter(" ".into()) }], raw.clone());
    assert_eq!(out.names(), ["名前.1", "名前.2", "x"]);
    assert_eq!(column(&out, "名前.2"), [t("太郎"), CellValue::Empty, CellValue::Empty]);
    let out = run(vec![Step::SplitColumn { column: "名前".into(), by: SplitBy::Length(1) }], raw);
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
    let out = run(vec![Step::AddColumn { name: "金額".into(), formula: "[数量] * [単価]".into() }], sales());
    assert_eq!(column(&out, "金額"), [n(1200.0), n(400.0), n(840.0), n(900.0), CellValue::Empty]);
    let out = run(vec![Step::AddColumn { name: "ラベル".into(), formula: "[店] & \":\" & Text.Upper([商品])".into() }], sales());
    assert_eq!(out.rows[0][4], t("北:りんご"));
    let out = run(vec![Step::AddColumn { name: "区分".into(), formula: "if [単価] >= 400 then \"高\" else if [単価] >= 100 then \"中\" else \"低\"".into() }], sales());
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
    let out = run(vec![Step::AddColumn { name: "y".into(), formula: "Date.Year([d]) * 100 + Date.Month([d])".into() }], dated);
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
    let typed = run(vec![Step::ChangeType { column: "x".into(), ty: QType::Whole, culture: Culture::Invariant }], raw);
    assert_eq!(run(vec![Step::RemoveErrors(vec!["x".into()])], typed.clone()).rows.len(), 1);
    let out = run(vec![Step::ReplaceErrors { columns: vec!["x".into()], with: "0".into() }], typed);
    assert_eq!(column(&out, "x"), [n(1.0), n(0.0)]);
}

#[test]
fn the_m_code_names_the_steps_as_power_query_does() {
    let mut q = Query::new("売上", Source::Table("テーブル1".into()));
    q.steps = vec![
        Step::PromoteHeaders,
        Step::ChangeType { column: "数量".into(), ty: QType::Whole, culture: Culture::Invariant },
        Step::FilterRows { column: "数量".into(), op: Op::Greater, value: "5".into() },
        Step::Sort(vec![("数量".into(), false)]),
    ];
    let m = query_to_m(&q);
    assert!(m.starts_with("let\n    Source = Excel.CurrentWorkbook(){[Name=\"テーブル1\"]}[Content],"), "{m}");
    assert!(m.contains("#\"Promoted Headers\" = Table.PromoteHeaders(Source, [PromoteAllScalars=true])"), "{m}");
    assert!(m.contains("#\"Changed Type\" = Table.TransformColumnTypes(#\"Promoted Headers\", {{\"数量\", Int64.Type}})"), "{m}");
    assert!(m.contains("#\"Filtered Rows\" = Table.SelectRows(#\"Changed Type\", each [数量] > 5)"), "{m}");
    assert!(m.ends_with("in\n    #\"Sorted Rows\""), "{m}");
    assert!(m.contains("Table.Sort(#\"Filtered Rows\", {{\"数量\", Order.Descending}})"), "{m}");
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
        Step::ChangeType { column: "数量".into(), ty: QType::Whole, culture: Culture::German },
        Step::SelectColumns(vec!["店".into(), "数量".into()]),
        Step::FilterRows { column: "店".into(), op: Op::Contains, value: "北\n南".into() },
        Step::Sort(vec![("数量".into(), false), ("店".into(), true)]),
        Step::SplitColumn { column: "店".into(), by: SplitBy::Length(2) },
        Step::SplitColumn { column: "店".into(), by: SplitBy::Delimiter("\t".into()) },
        Step::GroupBy { keys: vec!["店".into()], aggregations: vec![("合計".into(), Agg::Sum, Some("数量".into())), ("行数".into(), Agg::Count, None)] },
        Step::Merge { right: "産地".into(), left_keys: vec!["a".into()], right_keys: vec!["b".into()], kind: JoinKind::FullOuter, expand: vec!["c".into(), "d".into()] },
        Step::AddConditionalColumn { name: "大口".into(), clauses: vec![Clause { column: "数量".into(), op: Op::GreaterOrEqual, value: "10".into(), output: "大".into() }], otherwise: "小".into() },
        Step::AddColumn { name: "金額".into(), formula: "[数量] * 2".into() },
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
        Step::AddColumn { name: "Value".into(), formula: "[Index] * 10".into() },
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
        Step::ChangeType { column: s("数量"), ty: QType::Whole, culture: Culture::Invariant },
        Step::ChangeType { column: s("日付"), ty: QType::Date, culture: Culture::Japanese },
        Step::SelectColumns(vec![s("店"), s("商\"品")]),
        Step::RemoveColumns(vec![s("単価")]),
        Step::RenameColumn { from: s("店"), to: s("店舗") },
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
        Step::SplitColumn { column: s("店"), by: SplitBy::Delimiter(s("-")) },
        Step::SplitColumn { column: s("店"), by: SplitBy::Length(3) },
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
        Step::AddColumn { name: s("金額"), formula: s("[数量] * [単価]") },
        Step::AddConditionalColumn { name: s("区分"), clauses: vec![Clause { column: s("数量"), op: Op::Greater, value: s("5"), output: s("多") }, Clause { column: s("店"), op: Op::Equals, value: s("北"), output: s("北") }], otherwise: s("少") },
        Step::AddIndexColumn { name: s("番号"), start: 1, step: 1 },
    ];
    for source in [Source::Blank, Source::Table(s("売上")), Source::Range { sheet: s("Sheet1"), range: s("A1:D9") }] {
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
    assert_eq!(refused("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.Foo(Source) in a").key, "function");
    assert_eq!(refused("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Source, x) in a").key, "arguments");
    assert_eq!(refused("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Source, 1) in Source").key, "result");
    assert_eq!(refused("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content], a = Table.FirstN(Other, 1) in a").key, "arguments");
}
