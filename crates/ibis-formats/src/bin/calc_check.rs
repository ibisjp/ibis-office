// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Checks the formula engine against Excel: calculates every formula of
//! workbooks Excel calculated and saved, and compares each result with
//! the value Excel stored (`c/v`).
//!
//! ```text
//! ibis-calc-check [--tsv out.tsv] [--show N] [--function NAME]... [--contains-function NAME]... book.xlsx...
//! ```
//!
//! A number matches when it is the same to 15 significant digits (what
//! Excel shows); text, logical values and errors must be the same. Random
//! and clock functions are skipped. Prints a summary by function (the
//! outermost function of each formula) and by file.

use std::collections::BTreeMap;
use std::io::Write;

use ibis_calc::eval::convert::round_significant;
use ibis_calc::formula::{Expr, parse};
use ibis_doc_model::sheet_calc::BookView;
use ibis_doc_model::{CellValue, cell_name};

#[derive(Default, Clone, Copy)]
struct Tally {
    total: usize,
    matched: usize,
    unimplemented: usize,
}

fn same(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x == y || round_significant(*x, 15) == round_significant(*y, 15) || (x.abs() < 1e-300 && y.abs() < 1e-300),
        (CellValue::Empty, CellValue::Number(y)) | (CellValue::Number(y), CellValue::Empty) => *y == 0.0,
        (CellValue::Empty, CellValue::Text(t)) | (CellValue::Text(t), CellValue::Empty) => t.is_empty(),
        _ => a == b,
    }
}

fn show(v: &CellValue) -> String {
    match v {
        CellValue::Empty => "(empty)".into(),
        CellValue::Number(n) => format!("{n}"),
        CellValue::Text(t) => format!("\"{t}\""),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        CellValue::Error(e) => e.as_str().into(),
    }
}

fn outer_function(formula: &str) -> String {
    match parse(formula) {
        Ok(Expr::Call { name, .. }) => name,
        Ok(_) => "(式)".into(),
        Err(_) => "(構文)".into(),
    }
}

/// Match calls through wrappers such as ARRAYTOTEXT; quoted text is not a call.
fn contains_function(expr: &Expr, names: &[String]) -> bool {
    match expr {
        Expr::Call {name,args} => names.contains(name) || args.iter().any(|x|contains_function(x,names)),
        Expr::Invoke {callee,args} => contains_function(callee,names) || args.iter().any(|x|contains_function(x,names)),
        Expr::Array(rows) => rows.iter().flatten().any(|x|contains_function(x,names)),
        Expr::Union(args) => args.iter().any(|x|contains_function(x,names)),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Spill(x) | Expr::Implicit(x) => contains_function(x,names),
        Expr::Binary(_,a,b) | Expr::Range(a,b) | Expr::Intersect(a,b) => contains_function(a,names) || contains_function(b,names),
        _ => false,
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut tsv: Option<std::fs::File> = None;
    let mut show_n = 0usize;
    let mut show_all = false;
    let mut files = Vec::new();
    let mut functions = Vec::new();
    let mut contained_functions = Vec::new();
    while !args.is_empty() {
        let a = args.remove(0);
        match a.as_str() {
            "--tsv" => tsv = Some(std::fs::File::create(args.remove(0)).expect("tsv")),
            "--show" => show_n = args.remove(0).parse().unwrap_or(20),
            "--all" => show_all = true,
            "--function" => functions.push(args.remove(0).to_uppercase()),
            "--contains-function" => contained_functions.push(args.remove(0).to_uppercase()),
            _ => files.push(a),
        }
    }
    if let Some(f) = tsv.as_mut() {
        writeln!(f, "file\tsheet\tcell\tformula\texcel\tibis\tstatus").unwrap();
    }
    let mut by_function: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_file: Vec<(String, Tally)> = Vec::new();
    let mut shown = 0;
    for path in &files {
        let p = std::path::Path::new(path);
        let book_name = if p.file_name().is_some_and(|n| n == "office.xlsx") {
            p.parent().and_then(|parent| parent.file_name()).map(|suite| format!("cov-{}.xlsx", suite.to_string_lossy()))
        } else {
            p.file_name().map(|n| n.to_string_lossy().to_string())
        };
        ibis_calc::eval::convert::set_workbook_file(book_name);
        let data = std::fs::read(path).expect("read");
        let presentation = ibis_formats::read_xlsx(&data).expect("xlsx");
        let book = BookView::new(&presentation);
        let mut tally = Tally::default();
        let formula_cells = book.formula_cells();
        let recalculated = book.recalculate(&formula_cells);
        let mut recalc_results: std::collections::HashMap<(usize, u32, u32), (CellValue, bool)> = std::collections::HashMap::new();
        for r in recalculated {
            use ibis_calc::eval::Book;
            if let Some((rows, columns, _)) = &r.spill {
                let mut blocked = false;
                for dr in 0..*rows as u32 {
                    for dc in 0..*columns as u32 {
                        if (dr, dc) == (0, 0) {
                            continue;
                        }
                        let tr = r.cell.1 + dr;
                        let tc = r.cell.2 + dc;
                        let in_old = book.spill_area(r.cell.0, r.cell.1, r.cell.2).is_some_and(|(r0, c0, r1, c1)| tr >= r0 && tr <= r1 && tc >= c0 && tc <= c1);
                        if !in_old && (!book.value(r.cell.0, tr, tc).is_empty() || book.formula(r.cell.0, tr, tc).is_some()) {
                            blocked = true;
                            break;
                        }
                    }
                    if blocked {
                        break;
                    }
                }
                if blocked {
                    recalc_results.insert(r.cell, (CellValue::Error(ibis_calc::value::CellError::Spill), r.unsupported));
                } else {
                    recalc_results.insert(r.cell, (r.value, r.unsupported));
                }
            } else {
                recalc_results.insert(r.cell, (r.value, r.unsupported));
            }
        }
        for (sheet, row, column) in formula_cells {
            use ibis_calc::eval::Book;
            let formula = book.formula(sheet, row, column).unwrap().to_string();
            if !functions.is_empty() && !functions.contains(&outer_function(&formula)) {
                continue;
            }
            if !contained_functions.is_empty() && !parse(&formula).is_ok_and(|expr|contains_function(&expr,&contained_functions)) {
                continue;
            }
            let upper = formula.to_uppercase();
            if ["RAND(", "RANDBETWEEN(", "RANDARRAY(", "NOW(", "TODAY("].iter().any(|f| upper.contains(f)) {
                continue;
            }
            if std::env::var_os("IBIS_CALC_TRACE").is_some() {
                eprintln!("{}!{} ={formula}", book.sheet_name(sheet), cell_name(column, row));
            }
            let excel = book.value(sheet, row, column);
            let (ibis, unsupported) = recalc_results.remove(&(sheet, row, column)).unwrap_or_else(|| {
                ibis_calc::recalc::evaluate(&book, (sheet, row, column), &formula)
            });
            let status = if unsupported {
                "unimplemented"
            } else if same(&excel, &ibis) {
                "match"
            } else {
                "differ"
            };
            let t = by_function.entry(outer_function(&formula)).or_default();
            for t in [t, &mut tally] {
                t.total += 1;
                match status {
                    "match" => t.matched += 1,
                    "unimplemented" => t.unimplemented += 1,
                    _ => {}
                }
            }
            let cell = format!("{}!{}", book.sheet_name(sheet), cell_name(column, row));
            if (show_all || status != "match") && shown < show_n {
                shown += 1;
                println!("{status}\t{cell}\t={formula}\texcel={}\tibis={}", show(&excel), show(&ibis));
            }
            if let Some(f) = tsv.as_mut() {
                writeln!(f, "{path}\t{}\t{}\t={formula}\t{}\t{}\t{status}", book.sheet_name(sheet), cell_name(column, row), show(&excel), show(&ibis)).unwrap();
            }
        }
        by_file.push((path.clone(), tally));
    }
    println!("function\ttotal\tmatch\tdiffer\tunimplemented");
    for (name, t) in &by_function {
        println!("{name}\t{}\t{}\t{}\t{}", t.total, t.matched, t.total - t.matched - t.unimplemented, t.unimplemented);
    }
    let mut all = Tally::default();
    println!("\nfile\ttotal\tmatch\tdiffer\tunimplemented");
    for (name, t) in &by_file {
        println!("{name}\t{}\t{}\t{}\t{}", t.total, t.matched, t.total - t.matched - t.unimplemented, t.unimplemented);
        all.total += t.total;
        all.matched += t.matched;
        all.unimplemented += t.unimplemented;
    }
    println!(
        "\nall\t{}\t{}\t{}\t{}\t({:.1}%)",
        all.total,
        all.matched,
        all.total - all.matched - all.unimplemented,
        all.unimplemented,
        if all.total > 0 { all.matched as f64 * 100.0 / all.total as f64 } else { 0.0 }
    );
}
