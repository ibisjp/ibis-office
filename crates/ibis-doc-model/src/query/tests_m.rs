// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0039 stage 2: the M interpreter and the screen's steps give the same table. Each case is a list of steps; the M text they are
//! written as (`query_to_m`) is run by `eval_m` over the same source table and compared with what `Query::run` gives.
use super::*;

fn t(s: &str) -> CellValue {
    CellValue::Text(s.to_string())
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

fn s(x: &str) -> String {
    x.to_string()
}

/// The result of the steps by `Query::run`, and by the interpreter on the M text of the steps; None when the interpreter says a function is
/// not supported (listed by the test that calls this).
fn both(steps: Vec<Step>, source: QTable) -> (QTable, Result<QTable, MEvalError>, String) {
    let mut q = Query::new("Q", Source::Table("T".to_string()));
    q.steps = steps;
    let book: HashMap<String, QTable> = [("T".to_string(), source.clone())].into_iter().collect();
    let (tables, err) = q.run(source.clone(), &[], &book);
    assert!(err.is_none(), "{err:?}");
    let m = query_to_m(&q);
    let got = eval_m(&m, &[("T".to_string(), source)]);
    (tables.last().unwrap().clone(), got, m)
}

fn same_cells(a: &QTable, b: &QTable) -> bool {
    a.names() == b.names() && a.rows == b.rows
}

/// The same, where an empty cell and the empty text count as one (a piece of a split that is empty: M makes the empty text, the screen's
/// step an empty cell; the request to Windows, item 28, asks which Excel makes).
fn same_cells_loosely(a: &QTable, b: &QTable) -> bool {
    let blank = |c: &CellValue| matches!(c, CellValue::Empty) || matches!(c, CellValue::Text(t) if t.is_empty());
    // `Number.Sqrt(-4)` is NaN in M and #NUM! in the screen's step (not measured in Excel: listed with the open items).
    let nan = |c: &CellValue| matches!(c, CellValue::Number(n) if n.is_nan()) || matches!(c, CellValue::Error(CellError::Num));
    a.names() == b.names() && a.rows.len() == b.rows.len() && a.rows.iter().zip(&b.rows).all(|(r, q)| r.len() == q.len() && r.iter().zip(q).all(|(x, y)| x == y || (blank(x) && blank(y)) || (nan(x) && nan(y))))
}

/// None when the two agree, else what differs.
fn differs(steps: Vec<Step>) -> Option<String> {
    differs_in(steps, true)
}

fn differs_in(steps: Vec<Step>, types: bool) -> Option<String> {
    let (want, got, m) = both(steps, sales());
    let got = match got {
        Ok(g) => g,
        Err(e) => return Some(format!("{e:?}\n{m}")),
    };
    if !same_cells(&want, &got) {
        return Some(format!("{m}\nsteps give {:?} {:?}\nM gives   {:?} {:?}", want.names(), want.rows, got.names(), got.rows));
    }
    let (a, b): (Vec<_>, Vec<_>) = (want.columns.iter().map(|c| c.ty).collect(), got.columns.iter().map(|c| c.ty).collect());
    if types && a != b {
        return Some(format!("types differ: {a:?} / {b:?}\n{m}"));
    }
    None
}

fn all_agree(cases: Vec<Vec<Step>>) {
    let bad: Vec<String> = cases.into_iter().filter_map(differs).collect();
    assert!(bad.is_empty(), "{} cases differ:\n{}", bad.len(), bad.join("\n-----\n"));
}

#[test]
fn the_interpreter_runs_the_columns_and_rows_steps_as_the_steps_do() {
    all_agree(vec![
        vec![],
        vec![Step::SelectColumns(vec![s("数量"), s("店")])],
        vec![Step::RemoveColumns(vec![s("単価")])],
        vec![Step::RenameColumns(vec![(s("店"), s("Shop")), (s("数量"), s("Qty"))])],
        vec![Step::ReorderColumns(vec![s("単価"), s("店"), s("商品"), s("数量")])],
        vec![Step::KeepTopRows(2)],
        vec![Step::KeepBottomRows(2)],
        vec![Step::RemoveTopRows(1)],
        vec![Step::RemoveBottomRows(2)],
        vec![Step::KeepRange { offset: 1, count: Some(2) }],
        vec![Step::AlternateRows { offset: 1, skip: 1, take: 1 }],
        vec![Step::ReverseRows],
        vec![Step::RemoveDuplicates(vec![s("店")])],
        vec![Step::DuplicateColumn { column: s("店"), new_name: s("店2") }],
        vec![Step::AddIndexColumn { name: s("i"), start: 1, step: 1 }],
        vec![Step::FillDown(vec![s("数量")]), Step::KeepTopRows(4)],
        vec![Step::ChangeType { column: s("数量"), ty: QType::Whole, culture: Culture::Invariant, apart: false }],
        vec![Step::ChangeType { column: s("数量"), ty: QType::Text, culture: Culture::Invariant, apart: false }],
    ]);
}

#[test]
fn the_interpreter_runs_sorting_grouping_and_reshaping_as_the_steps_do() {
    all_agree(vec![
        vec![Step::Sort(vec![(s("数量"), false)])],
        vec![Step::Sort(vec![(s("店"), true), (s("単価"), false)])],
        vec![Step::UnpivotOther { keep: vec![s("店"), s("商品")], attribute: s("属性"), value: s("値") }],
        vec![Step::Unpivot { columns: vec![s("数量"), s("単価")], attribute: s("属性"), value: s("値") }],
        vec![Step::Transpose],
        vec![Step::Pivot { attribute: s("店"), values: s("数量"), agg: Agg::Sum }],
        vec![Step::ReplaceValue { column: s("商品"), find: s("りんご"), with: s("林檎"), whole_cell: false }],
        vec![Step::ReplaceValue { column: s("店"), find: s("北"), with: s("N"), whole_cell: true }],
        vec![Step::ReplaceNull { columns: vec![s("数量")], with: s("0") }],
    ]);
}

/// The types of the grouped columns and of a transformed column are what Excel's M declares (`type nullable number`, `Int64.Type`, `type
/// text`), in the screen's step too since it follows the measurement (K0039); the cells and the types are the same in both.
#[test]
fn grouped_transformed_merged_and_split_columns_are_the_same_in_the_steps_and_in_m() {
    let group = vec![Step::GroupBy { keys: vec![s("店")], aggregations: vec![(s("計"), Agg::Sum, Some(s("数量"))), (s("件数"), Agg::Count, None)] }];
    assert_eq!(differs(group.clone()), None);
    let (_, got, _) = both(group, sales());
    assert_eq!(got.unwrap().columns.iter().map(|c| c.ty).collect::<Vec<_>>(), [QType::Any, QType::Decimal, QType::Whole]);
    let transform = vec![Step::Upper(vec![s("商品")]), Step::Trim(vec![s("店")])];
    assert_eq!(differs(transform), None);
    // A merged column is `Text` in both.
    let merge = vec![Step::MergeColumns { columns: vec![s("店"), s("商品")], separator: s("-"), name: s("店商品") }];
    assert_eq!(differs(merge), None);
    // A piece after a trailing delimiter is the empty text in both (`Text.Split("みかん","ん")` is `{"みか",""}`).
    let split = vec![Step::SplitColumn { column: s("商品"), by: SplitBy::Delimiter(s("ん")), columns: 0 }];
    assert_eq!(differs(split.clone()), None);
    let (want, _, _) = both(split, sales());
    assert_eq!(want.rows[1][2], CellValue::Text(String::new()));
}

/// The order of the groups is the order their keys first come, not sorted (Excel, section 14), and text is sorted by its code units.
#[test]
fn groups_come_as_their_keys_do_and_text_sorts_by_code_units() {
    let t2 = QTable::from_rows(vec!["k".into(), "v".into()], vec![vec![t("b"), n(1.0)], vec![t("a"), n(2.0)], vec![t("B"), n(3.0)], vec![t("b"), n(4.0)]]);
    let group = vec![Step::GroupBy { keys: vec![s("k")], aggregations: vec![(s("s"), Agg::Sum, Some(s("v")))] }];
    let mut q = Query::new("Q", Source::Table("T".to_string()));
    q.steps = group;
    let book: HashMap<String, QTable> = HashMap::new();
    let (tables, _) = q.run(t2.clone(), &[], &book);
    assert_eq!(tables[1].rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(), [t("b"), t("a"), t("B")]);
    q.steps = vec![Step::Sort(vec![(s("k"), true)])];
    let (tables, _) = q.run(t2, &[], &book);
    assert_eq!(tables[1].rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(), [t("B"), t("a"), t("b"), t("b")]);
}

#[test]
fn what_the_interpreter_does_not_have_yet_is_named_with_its_line() {
    // Steps whose M the stage 2 interpreter does not run yet: the error names the function and the line, and is not a wrong table.
    let (_, got, m) = both(vec![Step::FilterRows { column: s("数量"), op: Op::Equals, value: s("10") }], sales());
    match got {
        Ok(_) => {}
        Err(MEvalError::Unsupported { name, line }) => {
            assert!(m.lines().nth(line as usize - 1).unwrap().contains(&name), "{name} at line {line}\n{m}");
        }
        Err(e) => panic!("{e:?}\n{m}"),
    }
}

#[test]
fn a_table_goes_through_the_interpreter_and_back() {
    let mut t0 = sales();
    t0.columns[2].ty = QType::Whole;
    let v = qtable_to_value(&t0);
    assert_eq!(value_to_qtable(&v).unwrap(), t0);
    // Dates, times and durations are values of their own types in M and serial numbers in the table.
    let dt = QTable {
        columns: vec![
            QColumn { name: s("d"), ty: QType::Date },
            QColumn { name: s("t"), ty: QType::Time },
            QColumn { name: s("dt"), ty: QType::DateTime },
            QColumn { name: s("du"), ty: QType::Duration },
        ],
        rows: vec![vec![n(46343.0), n(0.75), n(46343.5), n(1.5)]],
    };
    let back = value_to_qtable(&qtable_to_value(&dt)).unwrap();
    assert_eq!(back, dt);
    let r = eval_m("let T = Excel.CurrentWorkbook(){[Name=\"X\"]}[Content], a = Table.AddColumn(T, \"n\", each Date.Frobnicate([d])) in a", &[(s("X"), dt)]);
    assert!(matches!(r, Err(MEvalError::Unsupported { ref name, line: 1 }) if name == "Date.Frobnicate"), "{r:?}");
    assert!(matches!(eval_m("1 + 1", &[]), Err(MEvalError::NotATable(_))));
    assert!(matches!(eval_m("let x = in x", &[]), Err(MEvalError::Syntax { .. })));
}

/// The 26 queries Microsoft 365 Excel wrote (`tests_excel.rs`): the interpreter runs the M text itself; where the query reads from the
/// workbook's table `T` it gives the table the screen's steps give from the same text (`m_to_query`), or it says which function it does
/// not have yet. A table that differs, or any other error, fails the test. The names of the functions still missing are the list of what
/// stage 3 and 4 have to add (the same list is asserted, so it shrinks on purpose).
#[test]
fn the_excel_queries_run_the_same_in_the_interpreter() {
    let source = QTable::from_rows(
        ["店", "商品", "数量", "単価", "日付", "Sales Amount", "a-b"].iter().map(|x| s(x)).collect(),
        vec![
            vec![t("A"), t("Apple"), n(3.0), n(1.5), n(46343.0), n(10.0), n(-4.0)],
            vec![t("B"), t("Pear"), n(2.0), n(2.5), n(46344.5), n(20.0), n(9.0)],
            vec![t("A"), t("Fig"), CellValue::Empty, n(4.0), n(46345.0), n(30.0), n(16.0)],
            vec![t("C"), t("apple"), n(5.0), n(0.5), n(46346.0), n(40.0), n(-25.0)],
        ],
    );
    let base = Query::new("Q", Source::Blank);
    let (mut agree, mut skipped) = (0, Vec::new());
    let mut missing: Vec<String> = Vec::new();
    for (file, code) in super::tests_excel::EXCEL_M {
        let q = m_to_query(code, &base).unwrap_or_else(|e| panic!("{file}: {e:?}"));
        if !matches!(q.source, Source::Table(_)) {
            skipped.push(*file);
            continue;
        }
        let book: HashMap<String, QTable> = [("T".to_string(), source.clone())].into_iter().collect();
        let (tables, err) = q.run(source.clone(), &[], &book);
        match eval_m(code, &[("T".to_string(), source.clone())]) {
            Ok(got) => {
                if err.is_none() {
                    let want = tables.last().unwrap();
                    assert!(same_cells_loosely(want, &got), "{file}\n{code}\nsteps give {:?}\nM gives   {:?}", want.rows, got.rows);
                }
                agree += 1;
            }
            Err(MEvalError::Unsupported { name, .. }) => missing.push(name),
            Err(e) => panic!("{file}: {e:?}\n{code}"),
        }
    }
    missing.sort();
    missing.dedup();
    eprintln!("agree {agree}, from a file {skipped:?}, missing functions {missing:?}");
    assert!(agree >= 1);
}

/// What the screen's steps cannot hold (`List.Accumulate`, a name of its own, a function the steps do not know) is read from the code as a
/// code step from the first such binding on, kept as it was written, run by the interpreter, written back the same, saved and loaded.
const TAIL: &str = "let\n    Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    Total = List.Accumulate(Table.Column(Source, \"数量\"), 0, (s, x) => s + (x ?? 0)),\n    Result = Table.AddColumn(Source, \"total\", each Total, type number)\nin\n    Result";

fn code_query(code: &str) -> Query {
    m_to_query(code, &Query::new("Q", Source::Blank)).unwrap_or_else(|e| panic!("{e:?}"))
}

fn run_of(q: &Query) -> (Vec<QTable>, Option<(usize, String)>) {
    let src = sales();
    let book: HashMap<String, QTable> = [("T".to_string(), src.clone())].into_iter().collect();
    q.run(src, &[], &book)
}

#[test]
fn code_the_steps_cannot_hold_is_a_step_of_its_own_run_and_written_back_as_it_was() {
    let q = code_query(TAIL);
    assert_eq!(q.source, Source::Table("T".to_string()));
    assert_eq!(q.steps.len(), 1);
    let Step::Code { names, bindings, result } = &q.steps[0] else { panic!("{:?}", q.steps) };
    assert_eq!(names, &["Source".to_string()]);
    assert!(bindings.starts_with("Total = List.Accumulate") && bindings.contains("Result = Table.AddColumn"), "{bindings}");
    assert_eq!(result, "Result");
    // Run: the sum of the quantities is 24 (10 + 5 + 7 + 2 + nothing).
    let (tables, err) = run_of(&q);
    assert_eq!(err, None);
    let last = tables.last().unwrap();
    assert_eq!(last.names(), ["店", "商品", "数量", "単価", "total"]);
    assert!(last.rows.iter().all(|r| r[4] == n(24.0)));
    // Written back as it was written.
    assert_eq!(query_to_m(&q), TAIL);
    // Saved and loaded.
    let (loaded, _) = decode_queries(&encode_queries(std::slice::from_ref(&q), &[]));
    assert_eq!(loaded, vec![q]);
}

#[test]
fn the_steps_before_the_code_stay_steps_and_their_names_are_kept_for_the_code() {
    // Two steps the screen has, then code that names the second by the name Excel (in Japanese) gave it.
    let code = "let\n    ソース = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    保存された先頭行 = Table.FirstN(ソース, 3),\n    小計 = List.Sum(Table.Column(保存された先頭行, \"数量\")),\n    結果 = Table.AddColumn(保存された先頭行, \"小計\", each 小計)\nin\n    結果";
    let q = code_query(code);
    assert!(matches!(q.steps[0], Step::KeepTopRows(3)), "{:?}", q.steps);
    assert!(matches!(q.steps[1], Step::Code { .. }));
    let (tables, err) = run_of(&q);
    assert_eq!(err, None);
    assert_eq!(tables.len(), 3);
    assert_eq!(tables[1].rows.len(), 3);
    assert!(tables[2].rows.iter().all(|r| r[4] == n(22.0)), "{:?}", tables[2].rows);
    // The screen is in English now, but the code is written with the names it was written with.
    assert_eq!(query_to_m(&q), code);
}

#[test]
fn an_unsupported_function_is_told_with_its_line_and_does_not_stop_the_steps_before() {
    let code = "let\n    Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    Kept = Table.FirstN(Source,2),\n    Odd = List.Frobnicate(Kept),\n    Result = Table.AddColumn(Kept, \"x\", each Odd)\nin\n    Result";
    let q = code_query(code);
    let (tables, err) = run_of(&q);
    assert_eq!(tables.len(), 2, "the steps before the code ran");
    let (step, message) = err.expect("the code fails");
    assert_eq!(step, 1);
    let (key, args) = parse_run_error(&message).expect("a told error");
    assert_eq!(key, "unsupported");
    assert!(args.contains(&("name".to_string(), "List.Frobnicate".to_string())) && args.contains(&("line".to_string(), "4".to_string())), "{args:?}");
    // The line is where it stands in the code the user sees.
    assert!(query_to_m(&q).lines().nth(3).unwrap().contains("List.Frobnicate"));
    // An error raised by the code.
    let q = code_query("let\n    Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    Odd = error \"boom\",\n    Result = if Odd = 1 then Source else Source\nin\n    Result");
    let (_, err) = run_of(&q);
    let (key, args) = parse_run_error(&err.unwrap().1).unwrap();
    assert_eq!((key.as_str(), args[1].1.as_str()), ("raised", "boom"));
    // A function that is not unsupported but not there at all, in a branch that is not used, is no error.
    let q = code_query(&code.replace("Odd = List.Frobnicate(Kept)", "Odd = 1, Unused = List.Frobnicate(Kept)"));
    assert_eq!(run_of(&q).1, None);
}

#[test]
fn code_that_cannot_be_read_as_m_is_refused_with_its_line() {
    let code = "let\n    Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    Odd = List.Frobnicate(Source,,),\n    Result = Source\nin\n    Result";
    let e = m_to_query(code, &Query::new("Q", Source::Blank)).unwrap_err();
    assert_eq!(e.key, "syntax");
    assert!(e.line >= 3, "{e:?}");
}

/// A query whose source is a text file: the interpreter reads the file through the app's reading (`Csv.Document(File.Contents(…))`) and the
/// table is the one the screen's source gives (the steps after it, here the promoted headers, are the same too).
#[cfg(not(target_family = "wasm"))]
#[test]
fn the_interpreter_reads_a_csv_source_as_the_screen_does() {
    use crate::Presentation;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/k0039-csv-m");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.csv");
    std::fs::write(&file, "\u{feff}a,b\r\n1,\"x,\"\"y\"\"\"\r\n2,\r\n").unwrap();
    let source = Source::Csv { path: file.to_string_lossy().into_owned(), delimiter: ",".to_string(), columns: 2, encoding: 65001, quote: "Csv".to_string() };
    let mut q = Query::new("Q", source.clone());
    q.steps = vec![Step::PromoteHeaders];
    let table = super::book::read_source(&Presentation::default(), &source).unwrap();
    let book: HashMap<String, QTable> = HashMap::new();
    let (tables, err) = q.run(table, &[], &book);
    assert_eq!(err, None);
    let got = eval_m(&query_to_m(&q), &[]).unwrap();
    assert!(same_cells(tables.last().unwrap(), &got), "{:?}\n{:?}", tables.last().unwrap().rows, got.rows);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The window functions and the clock work in a code step (the app gives the interpreter the clock).
#[test]
fn a_code_step_can_use_the_clock() {
    let q = code_query("let\n    Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content],\n    Today = Date.From(DateTime.LocalNow()),\n    Result = Table.AddColumn(Source, \"today\", each Date.IsInCurrentDay(Today), type logical)\nin\n    Result");
    let (tables, err) = run_of(&q);
    assert_eq!(err, None);
    assert!(tables.last().unwrap().rows.iter().all(|r| r[4] == CellValue::Bool(true)));
}

/// The windows of dates the screen's filter writes (`Date.IsInPreviousNDays([日付], 2)`...) give the same rows in the interpreter, with the
/// same clock (both read it from `ibis-calc`).
#[test]
fn the_date_windows_of_a_filter_run_the_same_in_the_interpreter() {
    let today = ibis_calc::eval::convert::today_serial(false).expect("a clock") as f64;
    let dates: Vec<f64> = (-40..=40).map(|k| today + f64::from(k) * 3.0).chain([today, today - 1.0, today + 1.0, today - 7.0, today + 7.0]).collect();
    let table = QTable {
        columns: vec![QColumn { name: s("日付"), ty: QType::Date }, QColumn { name: s("i"), ty: QType::Whole }],
        rows: dates.iter().enumerate().map(|(i, d)| vec![n(*d), n(i as f64)]).collect(),
    };
    let mut windows = Vec::new();
    for unit in DateUnit::ALL {
        for (when, count) in [(DateWhen::Current, None), (DateWhen::Previous, None), (DateWhen::Next, None), (DateWhen::Previous, Some(3)), (DateWhen::Next, Some(2))] {
            windows.push(DateFilter { when, unit, count });
        }
    }
    windows.push(DateFilter { when: DateWhen::YearToDate, unit: DateUnit::Year, count: None });
    for filter in windows {
        let mut q = Query::new("Q", Source::Table("T".to_string()));
        q.steps = vec![Step::FilterCondition(Cond::Date { column: s("日付"), filter })];
        let book: HashMap<String, QTable> = [("T".to_string(), table.clone())].into_iter().collect();
        let (tables, err) = q.run(table.clone(), &[], &book);
        assert_eq!(err, None);
        let m = query_to_m(&q);
        let got = eval_m(&m, &[("T".to_string(), table.clone())]).unwrap_or_else(|e| panic!("{e:?}\n{m}"));
        assert!(same_cells(tables.last().unwrap(), &got), "{filter:?}\n{m}\nsteps {} rows, M {} rows", tables.last().unwrap().rows.len(), got.rows.len());
    }
}

/// What is not a number in M (NaN, infinity) is the error #NUM! in the sheet's cell, and nothing in a whole, currency or percentage column
/// (Excel, measured: the second request, section 2b).
#[test]
fn nan_and_infinity_are_num_errors_in_a_sheet_and_nothing_in_a_whole_column() {
    let book = [("T".to_string(), sales())];
    let m = |step: &str| format!("let\n    Source = Excel.CurrentWorkbook(){{[Name=\"T\"]}}[Content],\n    R = {step}\nin\n    R");
    let got = eval_m(&m("Table.AddColumn(Source, \"x\", each Number.Sqrt(-4), type number)"), &book).unwrap();
    assert!(got.rows.iter().all(|r| r[4] == CellValue::Error(CellError::Num)), "{:?}", got.rows);
    let got = eval_m(&m("Table.AddColumn(Source, \"x\", each 1 / 0)"), &book).unwrap();
    assert!(got.rows.iter().all(|r| r[4] == CellValue::Error(CellError::Num)));
    let got = eval_m(&m("Table.TransformColumnTypes(Table.AddColumn(Source, \"x\", each 0 / 0), {{\"x\", Int64.Type}})"), &book).unwrap();
    assert!(got.rows.iter().all(|r| r[4] == CellValue::Empty), "{:?}", got.rows);
    let got = eval_m(&m("Table.TransformColumnTypes(Table.AddColumn(Source, \"x\", each 0 / 0), {{\"x\", type text}})"), &book).unwrap();
    assert!(got.rows.iter().all(|r| r[4] == CellValue::Text("NaN".into())));
}
