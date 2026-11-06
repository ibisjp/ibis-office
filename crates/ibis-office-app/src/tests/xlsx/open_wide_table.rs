// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #82: a workbook with a table over whole rows (`ref="5:679"`, 16384 columns) opens, and its sheet shows.
//! The file is the reporter's (test/github-issue/82/, not kept in the repository); IBIS_WIDE_TABLE_FILE can name another.
use super::SheetSteps;
use crate::shell::scenario::Scenario;

pub(crate) fn whole_row_table_opens() {
    let path = std::env::var("IBIS_WIDE_TABLE_FILE").unwrap_or_else(|_| "test/github-issue/82/ibisOffice_開けないサンプル.xlsx".to_string()); // i18n-ok: test data
    if !crate::shell::scenario::repo_root().join(&path).is_file() && !std::path::Path::new(&path).is_file() {
        eprintln!("whole_row_table_opens: {path} is not here; skipped (the reader's own test covers the table)");
        return;
    }
    let started = crate::clock::Instant::now();
    let mut s = Scenario::open(&path);
    s.size(1200., 720.);
    let _ = s.shown("A1");
    assert!(started.elapsed().as_secs() < 60, "opening took {:?}", started.elapsed());
}
