// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #208: the sheet shown is written as an HTML page (ファイル > 書き出し > Web ページ): the formats and merged cells of the sheet, and the
//! menu item.

use crate::shell::scenario::Scenario;
use ibis_doc_model::sheet_command::StyleChange;
use ibis_doc_model::{Command, SheetCommand, CellRange, Rgb};

pub(super) fn run() {
    let mut s = Scenario::open("test/editing/issue100-names-src.xlsx");
    assert!(s.menu_state("xlsx.export-html").is_some(), "File > Export has the Web page item");
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let ranges = vec![CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }];
        let commands = [StyleChange::Bold(true), StyleChange::Fill(Some(Rgb(0x336699)))]
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { sheet: slide, ranges: ranges.clone(), change }))
            .collect();
        e.document.execute(commands).unwrap();
    });
    let html = s.with(|e, _| {
        let index = e.presentation().slide_index(e.current_slide).unwrap_or(0);
        crate::sheet_html::sheet_to_html(e.presentation(), index).unwrap()
    });
    assert!(html.contains("font-weight:700"), "{html}");
    assert!(html.contains("background:#336699"), "{html}");
    assert!(!html.contains("mso-"), "no Excel-only declarations");
}
