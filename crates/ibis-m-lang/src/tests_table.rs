// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `Table.*` functions of stage 2, by their results on small tables (what the Microsoft Learn reference says for each; the points
//! Excel has not been asked yet are in `tasks/doc/requests/2026-11-17-k0039-m-semantics.md`).
use crate::*;

/// A table as rows of its text form: header, then each row.
fn rows(src: &str) -> String {
    let it = Interp::new();
    let v = match it.run(src) {
        Ok(v) => v,
        Err(e) => return format!("ERR {}", e.message),
    };
    let Value::Table(t) = v else { return format!("not a table: {v:?}") };
    let cell = |v: &Value| match v {
        Value::Null => "null".to_string(),
        Value::Text(s) => format!("{s}"),
        Value::Number(n) => format!("{n}"),
        Value::Logical(b) => b.to_string(),
        Value::Failed(e) => format!("!{}", e.reason),
        Value::Table(t) => format!("<{} rows>", t.rows.len()),
        v => format!("{v:?}"),
    };
    let mut out = vec![t.columns.join(",")];
    for r in &t.rows {
        out.push(r.iter().map(cell).collect::<Vec<_>>().join(","));
    }
    out.join(" | ")
}

fn val(src: &str) -> String {
    let it = Interp::new();
    match it.run(src) {
        Ok(Value::Number(n)) => format!("{n}"),
        Ok(Value::Text(s)) => format!("\"{s}\""),
        Ok(Value::Logical(b)) => b.to_string(),
        Ok(Value::Null) => "null".into(),
        Ok(Value::List(l)) => format!("{{{}}}", l.iter().map(|x| match x.force(&it) { Ok(Value::Number(n)) => format!("{n}"), Ok(Value::Text(s)) => format!("\"{s}\""), Ok(v) => format!("{v:?}"), Err(e) => format!("!{}", e.message) }).collect::<Vec<_>>().join(", ")),
        Ok(v) => format!("{v:?}"),
        Err(e) => format!("ERR {}", e.message),
    }
}

const T: &str = "#table({\"a\",\"b\",\"c\"}, {{1,\"x\",10},{2,\"y\",20},{3,\"x\",30},{4,null,40}})";

fn on(f: &str) -> String {
    rows(&format!("let t = {T} in {f}"))
}

#[test]
fn headers() {
    assert_eq!(rows("Table.PromoteHeaders(#table({\"A\",\"B\"}, {{\"a\",\"b\"},{1,2}}))"), "a,b | 1,2");
    // A number is a name (as text); duplicates get _1 (Excel).
    assert_eq!(rows("Table.PromoteHeaders(#table({\"A\",\"B\"}, {{1,2},{3,4}}))"), "1,2 | 3,4");
    assert_eq!(rows("Table.PromoteHeaders(#table({\"A\",\"B\"}, {{1,2},{3,4}}), [PromoteAllScalars=true])"), "1,2 | 3,4");
    assert_eq!(rows("Table.PromoteHeaders(#table({\"A\",\"B\",\"C\"}, {{\"a\",\"a\",null},{1,2,3}}))"), "a,a_1,Column3 | 1,2,3");
    assert_eq!(rows("Table.DemoteHeaders(#table({\"a\",\"b\"}, {{1,2}}))"), "Column1,Column2 | a,b | 1,2");
}

#[test]
fn rows_and_columns() {
    assert_eq!(on("Table.SelectRows(t, each [a] > 2)"), "a,b,c | 3,x,30 | 4,null,40");
    assert_eq!(on("Table.SelectRows(t, each [b] = \"x\")"), "a,b,c | 1,x,10 | 3,x,30");
    assert_eq!(on("Table.SelectColumns(t, {\"c\",\"a\"})"), "c,a | 10,1 | 20,2 | 30,3 | 40,4");
    assert_eq!(on("Table.SelectColumns(t, {\"c\",\"zz\"})"), "ERR The column 'zz' of the table wasn't found.");
    assert_eq!(on("Table.SelectColumns(t, {\"a\",\"zz\"}, MissingField.Ignore)"), "a | 1 | 2 | 3 | 4");
    assert_eq!(on("Table.SelectColumns(t, {\"a\",\"zz\"}, MissingField.UseNull)"), "a,zz | 1,null | 2,null | 3,null | 4,null");
    assert_eq!(on("Table.RemoveColumns(t, {\"b\"})"), "a,c | 1,10 | 2,20 | 3,30 | 4,40");
    assert_eq!(on("Table.RenameColumns(t, {{\"a\",\"A\"},{\"c\",\"C\"}})"), "A,b,C | 1,x,10 | 2,y,20 | 3,x,30 | 4,null,40");
    // Names are exchanged by one call.
    assert_eq!(on("Table.RenameColumns(Table.SelectColumns(t,{\"a\",\"c\"}), {{\"a\",\"c\"},{\"c\",\"a\"}})"), "c,a | 1,10 | 2,20 | 3,30 | 4,40");
    assert!(on("Table.RenameColumns(t, {{\"a\",\"b\"}})").starts_with("ERR"));
    assert_eq!(on("Table.ReorderColumns(t, {\"c\",\"a\",\"b\"})"), "c,a,b | 10,1,x | 20,2,y | 30,3,x | 40,4,null");
    assert_eq!(on("Table.FirstN(t, 2)"), "a,b,c | 1,x,10 | 2,y,20");
    assert_eq!(on("Table.FirstN(t, each [a] < 3)"), "a,b,c | 1,x,10 | 2,y,20");
    assert_eq!(on("Table.LastN(t, 1)"), "a,b,c | 4,null,40");
    assert_eq!(on("Table.Skip(t, 3)"), "a,b,c | 4,null,40");
    assert_eq!(on("Table.RemoveFirstN(t)"), "a,b,c | 2,y,20 | 3,x,30 | 4,null,40");
    assert_eq!(on("Table.RemoveLastN(t, 3)"), "a,b,c | 1,x,10");
    assert_eq!(on("Table.Range(t, 1, 2)"), "a,b,c | 2,y,20 | 3,x,30");
    assert!(on("Table.Range(t, 3, 5)").starts_with("ERR"));
    assert_eq!(on("Table.AlternateRows(t, 1, 1, 1)"), "a,b,c | 1,x,10 | 3,x,30");
    assert_eq!(on("Table.ReverseRows(Table.FirstN(t,2))"), "a,b,c | 2,y,20 | 1,x,10");
    assert_eq!(val(&format!("let t = {T} in {{Table.RowCount(t), Table.ColumnCount(t)}}")), "{4, 3}");
    assert_eq!(val(&format!("let t = {T} in Table.ColumnNames(t)")), "{\"a\", \"b\", \"c\"}");
    assert_eq!(val(&format!("let t = {T} in Table.Column(t, \"c\")")), "{10, 20, 30, 40}");
}

#[test]
fn new_columns() {
    assert_eq!(on("Table.AddColumn(Table.FirstN(t,2), \"d\", each [a] * 2)"), "a,b,c,d | 1,x,10,2 | 2,y,20,4");
    assert_eq!(on("Table.AddIndexColumn(Table.FirstN(t,2), \"i\", 1, 1)"), "a,b,c,i | 1,x,10,1 | 2,y,20,2");
    assert_eq!(on("Table.DuplicateColumn(Table.FirstN(t,1), \"a\", \"a2\")"), "a,b,c,a2 | 1,x,10,1");
    assert_eq!(on("Table.TransformColumns(Table.FirstN(t,2), {{\"b\", Text.Upper}, {\"c\", each _ + 1}})"), "a,b,c | 1,X,11 | 2,Y,21");
    // An error of one cell does not break the table.
    assert_eq!(on("Table.AddColumn(Table.FirstN(t,2), \"d\", each if [a] = 1 then error \"no\" else [a])"), "a,b,c,d | 1,x,10,!Expression.Error | 2,y,20,2");
    assert_eq!(val(&format!("let t = {T} in Table.RowCount(Table.AddColumn(t, \"d\", each error \"no\"))")), "4");
    assert!(val(&format!("let t = {T} in Table.AddColumn(t, \"d\", each error \"no\"){{0}}[d]")).starts_with("ERR no"));
}

#[test]
fn types() {
    assert_eq!(rows("Table.TransformColumnTypes(#table({\"v\"}, {{\"12\"},{\" 3.5 \"},{\"x\"},{null}}), {{\"v\", type number}})"), "v | 12 | 3.5 | !DataFormat.Error | null");
    assert_eq!(rows("Table.TransformColumnTypes(#table({\"v\"}, {{2.5},{-2.5}}), {{\"v\", Int64.Type}})"), "v | 2 | -2");
    assert_eq!(rows("Table.TransformColumnTypes(#table({\"v\"}, {{12},{true}}), {{\"v\", type text}})"), "v | 12 | true");
    assert_eq!(rows("Table.TransformColumnTypes(#table({\"v\"}, {{\"2026-11-17\"}}), {{\"v\", type date}})"), "v | date(739938)".replace("739938", &value::days_from_civil(2026, 11, 17).to_string()));
    assert_eq!(rows("Table.TransformColumnTypes(#table({\"v\"}, {{\"true\"},{\"no\"}}), {{\"v\", type logical}})"), "v | true | !Expression.Error");
    // The type is kept for the column.
    let it = Interp::new();
    let Ok(Value::Table(t)) = it.run("Table.TransformColumnTypes(#table({\"v\"}, {{\"1\"}}), {{\"v\", Int64.Type}})") else { panic!() };
    assert_eq!(t.types[0].ty, ast::Ty::Int64);
}

#[test]
fn order_and_duplicates() {
    assert_eq!(on("Table.Sort(t, {{\"b\", Order.Descending}, \"a\"})"), "a,b,c | 2,y,20 | 1,x,10 | 3,x,30 | 4,null,40");
    assert_eq!(on("Table.Sort(t, {\"b\", Order.Ascending})"), "a,b,c | 4,null,40 | 1,x,10 | 3,x,30 | 2,y,20");
    assert_eq!(on("Table.Sort(t, \"b\")"), "a,b,c | 4,null,40 | 1,x,10 | 3,x,30 | 2,y,20");
    assert_eq!(on("Table.Distinct(t, \"b\")"), "a,b,c | 1,x,10 | 2,y,20 | 4,null,40");
    assert_eq!(on("Table.Distinct(Table.SelectColumns(t,{\"b\"}))"), "b | x | y | null");
    assert_eq!(rows("Table.FillDown(#table({\"a\"}, {{1},{null},{2},{null}}), {\"a\"})"), "a | 1 | 1 | 2 | 2");
    assert_eq!(rows("Table.FillUp(#table({\"a\"}, {{null},{1},{null},{2}}), {\"a\"})"), "a | 1 | 1 | 2 | 2");
}

#[test]
fn build_and_convert() {
    assert_eq!(rows("Table.FromRows({{1,2},{3,4}}, {\"x\",\"y\"})"), "x,y | 1,2 | 3,4");
    assert_eq!(rows("Table.FromRecords({[a=1,b=2],[a=3,c=4]})"), "a,b,c | 1,2,null | 3,null,4");
    assert_eq!(rows("Table.FromColumns({{1,2},{\"x\",\"y\"}}, {\"n\",\"s\"})"), "n,s | 1,x | 2,y");
    assert_eq!(rows("Table.FromList({1,2,3})"), "Column1 | 1 | 2 | 3");
    assert_eq!(val(&format!("let t = {T} in Table.ToRows(Table.FirstN(t,1)){{0}}")), "{1, \"x\", 10}");
    assert_eq!(val(&format!("let t = {T} in Table.ToRecords(t){{1}}[b]")), "\"y\"");
    assert_eq!(val(&format!("let t = {T} in Table.ToColumns(Table.FirstN(t,2)){{0}}")), "{1, 2}");
    assert_eq!(on("Table.Transpose(Table.FirstN(Table.SelectColumns(t,{\"a\",\"c\"}),2))"), "Column1,Column2 | 1,2 | 10,20");
    assert_eq!(on("Table.Combine({Table.FirstN(t,1), Table.FromRecords({[a=9,z=1]})})"), "a,b,c,z | 1,x,10,null | 9,null,null,1");
    assert_eq!(on("Table.InsertRows(Table.FirstN(t,1), 1, {[a=7,b=\"q\",c=70]})"), "a,b,c | 1,x,10 | 7,q,70");
    assert_eq!(on("Table.ReplaceRows(Table.FirstN(t,2), 0, 1, {[a=7,b=\"q\",c=70]})"), "a,b,c | 7,q,70 | 2,y,20");
}

#[test]
fn reshaping() {
    assert_eq!(on("Table.Unpivot(Table.FirstN(t,2), {\"a\",\"c\"}, \"attr\", \"val\")"), "b,attr,val | x,a,1 | x,c,10 | y,a,2 | y,c,20");
    assert_eq!(on("Table.UnpivotOtherColumns(Table.FirstN(t,2), {\"b\"}, \"attr\", \"val\")"), "b,attr,val | x,a,1 | x,c,10 | y,a,2 | y,c,20");
    assert_eq!(rows("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"y\",20},{2,\"x\",30}}), {\"x\",\"y\"}, \"a\", \"v\")"), "k,x,y | 1,10,20 | 2,30,null");
    assert_eq!(rows("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"x\",5}}), {\"x\"}, \"a\", \"v\", List.Sum)"), "k,x | 1,15");
    // Two values and no aggregation: the cell is an error, the table is made.
    assert_eq!(rows("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"x\",5}}), {\"x\"}, \"a\", \"v\")"), "k,x | 1,!Expression.Error");
    assert_eq!(on("Table.Group(t, {\"b\"}, {{\"n\", each Table.RowCount(_), Int64.Type}, {\"s\", each List.Sum([a]), type number}})"), "b,n,s | x,2,4 | y,1,2 | null,1,4");
    assert_eq!(rows("Table.Group(#table({\"k\"}, {{1},{1},{2},{1}}), {\"k\"}, {{\"n\", each Table.RowCount(_)}}, GroupKind.Local)"), "k,n | 1,2 | 2,1 | 1,1");
    assert_eq!(on("Table.Group(t, {}, {{\"n\", each Table.RowCount(_)}})"), "n | 4");
}

const L: &str = "#table({\"k\",\"v\"}, {{1,\"a\"},{2,\"b\"},{2,\"c\"}})";
const RT: &str = "#table({\"k\",\"w\"}, {{2,\"x\"},{3,\"y\"}})";

#[test]
fn joins() {
    let j = |kind: &str| rows(&format!("Table.Join({L}, {{\"k\"}}, {RT}, {{\"k\"}}, {kind})"));
    // A right key with the name of its left key is one column in an inner join; an outer or anti join with such a key is an error (Excel).
    assert_eq!(j("JoinKind.Inner"), "k,v,w | 2,b,x | 2,c,x");
    for kind in ["LeftOuter", "RightOuter", "FullOuter", "LeftAnti", "RightAnti"] {
        assert!(j(&format!("JoinKind.{kind}")).starts_with("ERR"), "{kind}");
    }
    let j2 = |kind: &str| rows(&format!("Table.Join({L}, {{\"k\"}}, #table({{\"k2\",\"w\"}}, {{{{2,\"x\"}},{{3,\"y\"}}}}), {{\"k2\"}}, {kind})"));
    assert_eq!(j2("JoinKind.LeftOuter"), "k,v,k2,w | 2,b,2,x | 2,c,2,x | 1,a,null,null");
    assert_eq!(j2("JoinKind.RightOuter"), "k,v,k2,w | 2,b,2,x | 2,c,2,x | null,null,3,y");
    assert_eq!(j2("JoinKind.FullOuter"), "k,v,k2,w | 2,b,2,x | 2,c,2,x | null,null,3,y | 1,a,null,null");
    assert_eq!(rows(&format!("Table.NestedJoin({L}, {{\"k\"}}, {RT}, {{\"k\"}}, \"r\", JoinKind.LeftOuter)")), "k,v,r | 1,a,<0 rows> | 2,b,<1 rows> | 2,c,<1 rows>");
    assert_eq!(rows(&format!("Table.ExpandTableColumn(Table.NestedJoin({L}, {{\"k\"}}, {RT}, {{\"k\"}}, \"r\", JoinKind.LeftOuter), \"r\", {{\"w\"}})")), "k,v,w | 1,a,null | 2,b,x | 2,c,x");
    assert_eq!(rows(&format!("Table.NestedJoin({L}, {{\"k\"}}, {RT}, {{\"k\"}}, \"r\", JoinKind.LeftAnti)")), "k,v,r | 1,a,<1 rows>");
}

#[test]
fn split_and_combine_columns() {
    let s = "#table({\"n\",\"x\"}, {{1,\"a,b\"},{2,\"c\"},{3,null}})";
    assert_eq!(rows(&format!("Table.SplitColumn({s}, \"x\", Splitter.SplitTextByDelimiter(\",\"))")), "n,x.1,x.2 | 1,a,b | 2,c,null | 3,null,null");
    assert_eq!(rows(&format!("Table.SplitColumn({s}, \"x\", Splitter.SplitTextByDelimiter(\",\"), {{\"p\",\"q\"}})")), "n,p,q | 1,a,b | 2,c,null | 3,null,null");
    assert_eq!(rows("Table.CombineColumns(#table({\"a\",\"b\",\"c\"}, {{\"x\",\"y\",1}}), {\"a\",\"b\"}, Combiner.CombineTextByDelimiter(\"-\"), \"ab\")"), "ab,c | x-y,1");
    assert_eq!(rows("Table.ReplaceValue(#table({\"a\"}, {{\"apple\"},{\"pear\"}}), \"p\", \"P\", Replacer.ReplaceText, {\"a\"})"), "a | aPPle | Pear");
    assert_eq!(rows("Table.ReplaceValue(#table({\"a\"}, {{1},{2}}), 1, 9, Replacer.ReplaceValue, {\"a\"})"), "a | 9 | 2");
}

#[test]
fn workbook_source() {
    let it = Interp::new();
    let inner = Value::Table(std::rc::Rc::new(Table { columns: vec!["a".into()], types: vec![ast::TypeSpec { ty: ast::Ty::Any, nullable: true }], rows: vec![vec![Value::Number(1.0)], vec![Value::Number(2.0)]] }));
    let wb = Value::Table(std::rc::Rc::new(Table {
        columns: vec!["Name".into(), "Content".into()],
        types: vec![ast::TypeSpec { ty: ast::Ty::Text, nullable: false }, ast::TypeSpec { ty: ast::Ty::Table, nullable: false }],
        rows: vec![vec![Value::text("T"), inner]],
    }));
    it.set_global("#workbook", wb);
    let v = it.run("let Source = Excel.CurrentWorkbook(){[Name=\"T\"]}[Content] in Table.RowCount(Source)").unwrap();
    assert!(matches!(v, Value::Number(n) if n == 2.0));
}
