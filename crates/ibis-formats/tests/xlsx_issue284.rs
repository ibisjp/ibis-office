// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #284: `list.xlsx` has one list validation over D2:E2 with `TAKE($A$2:$B$22,,1-D3*2)`; each cell reads its own D3/E3. The attachment is
//! `test/github-issue/284/list.xlsx` (not in the repository: the test passes at once without it).

use ibis_doc_model::sheet_calc::{sheet_of_slide, BookView};

#[test]
fn the_list_formula_of_a_two_cell_rule_follows_each_cell() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/284/list.xlsx");
    let Ok(data) = std::fs::read(&path) else { return };
    let opened = ibis_formats::open(data).unwrap();
    let p = &opened.presentation;
    let sheet = sheet_of_slide(p.slide(p.slides[0].id).unwrap()).unwrap();
    let rule = sheet.validations.first().expect("a validation");
    eprintln!("ranges {:?} formula {:?}", rule.ranges, rule.formula1);
    let view = BookView::new(p);
    let items = |row: u32, column: u32| {
        let (br, bc) = rule.base().unwrap();
        let f = rule.formula1.as_deref().unwrap().trim().trim_start_matches('=').to_string();
        let moved = ibis_calc::formula::shift(&f, i64::from(row) - i64::from(br), i64::from(column) - i64::from(bc));
        eprintln!("({row},{column}) {moved}");
        ibis_calc::recalc::evaluate_list(&view, (0, row, column), &moved)
    };
    let d = items(1, 3);
    let e = items(1, 4);
    eprintln!("D2 {d:?}\nE2 {e:?}");
    assert!(d.is_some() && e.is_some());
    assert_ne!(d, e);
}
