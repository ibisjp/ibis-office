// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-AUDIT-001〜003: the worksheet functions of Excel's alphabetical list
//! (the `XLSX-FN-*` entries of `tasks/doc/todo-xlsx.md`, 521 names) and the
//! functions this engine has, by name: every name of the list has a
//! calculation unless the entry says it is out of scope (the user's decision
//! of 2026-10-07: cube, add-in, web and service functions), and what the
//! engine has beyond the list includes internal and ibisOffice-specific functions.
use std::collections::BTreeSet;

fn official() -> (BTreeSet<String>, BTreeSet<String>) {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tasks/doc/todo-xlsx.md")).unwrap();
    let (mut all, mut out_of_scope) = (BTreeSet::new(), BTreeSet::new());
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("- [").and_then(|l| l.get(3..)) else { continue };
        let Some(id) = rest.strip_prefix("`XLSX-FN-").and_then(|r| r.split('`').next()) else { continue };
        let name = id.replace('_', ".");
        if line.contains("【対象外】") {
            out_of_scope.insert(name.clone());
        }
        all.insert(name);
    }
    (all, out_of_scope)
}

/// The names with no calculation yet (none: the exponential triple smoothing functions are in `functions/ets.rs`).
const NOT_YET: [&str; 0] = [];

#[test]
fn every_function_of_the_official_list_has_a_calculation_unless_it_is_out_of_scope() {
    let (all, out_of_scope) = official();
    assert_eq!(all.len(), 521, "the list of the todo");
    let have: BTreeSet<String> = ibis_calc::functions::implemented().into_iter().map(str::to_string).collect();
    let missing: Vec<&str> = all.iter().filter(|n| !have.contains(*n) && !out_of_scope.contains(*n)).map(String::as_str).collect();
    assert_eq!(missing, NOT_YET, "the names with no calculation");
    // What is out of scope is the user's decision, one by one.
    assert!(out_of_scope.len() >= 10, "{out_of_scope:?}");
}

#[test]
fn functions_beyond_the_official_list_are_explicitly_allowed() {
    let (all, _) = official();
    let beyond: BTreeSet<String> = ibis_calc::functions::implemented().into_iter().map(str::to_string).filter(|n| !all.contains(n)).collect();
    println!("beyond the list: {beyond:?}");
    let allowed: BTreeSet<&str> = ["ANCHORARRAY", "SINGLE", "ECMA.CEILING", "FIELDVALUE", "PY", "COPILOT", "DATESTRING", "NUMBERSTRING", "IBIS.FIELDMATCH", "IBIS.RECORDMATCH"].into_iter().collect();
    let unexpected: Vec<&String> = beyond.iter().filter(|n| !allowed.contains(n.as_str())).collect();
    assert!(unexpected.is_empty(), "{unexpected:?}");
}

#[test]
fn the_coverage_tests_function_table_differs_from_the_list_by_known_names() {
    // 526 names there, 521 in the list: 7 only there (what the engine uses
    // inside and the PY/COPILOT/FIELDVALUE forms of the services) and 2 only
    // in the list (JIS, whose tests are the width suite's, and EUROCONVERT, an add-in).
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/coverage/xlsx-functions.tsv")).unwrap();
    let table: BTreeSet<String> = text.lines().skip(1).filter_map(|l| l.split('\t').next()).map(str::to_string).collect();
    let (all, _) = official();
    assert_eq!(table.len(), 526);
    let only_table: Vec<&str> = table.iter().filter(|n| !all.contains(*n)).map(String::as_str).collect();
    let only_list: Vec<&str> = all.iter().filter(|n| !table.contains(*n)).map(String::as_str).collect();
    assert_eq!(only_table, ["ANCHORARRAY", "COPILOT", "DATESTRING", "FIELDVALUE", "NUMBERSTRING", "PY", "SINGLE"]);
    assert_eq!(only_list, ["EUROCONVERT", "JIS"]);
}
