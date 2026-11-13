// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation screen's scenarios that need more than the presentation and the shell: they call the export (the PDF, the
//! text rectangles, the corpus report) or another format (the XLSX and DOCX screens' methods: a sheet in a slide, the ink panel). The rest are in the presentation's own folder
//! (`FormatEditor::scenarios`). tasks/doc/crate-split-plan.md, 14.2.

use crate::pptx::scenarios::{cell_middle, png, press_panel_button};

#[path = "tests/pptx/binding_roundtrip.rs"]
mod binding_roundtrip;
#[path = "tests/pptx/m74_20261111.rs"]
mod m74_20261111;
#[path = "tests/pptx/panels_20261104.rs"]
mod panels_20261104;
#[path = "tests/pptx/panels_20261105_group1.rs"]
mod panels_20261105_group1;
#[path = "tests/pptx/cmyk_20261109.rs"]
mod cmyk_20261109;
#[path = "tests/pptx/chart_data_workbook_scenarios.rs"]
mod chart_data_workbook;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "tests/pptx/corpus_report.rs"]
mod corpus_report;

/// Their scenarios, as (name, function).
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    let mut all = Vec::new();
    #[cfg(target_os = "macos")]
    all.extend(corpus_report::all());
    all.extend(binding_roundtrip::all());
    all.extend(panels_20261104::all());
    all.extend(cmyk_20261109::all());
    all.extend(m74_20261111::all());
    all.extend(panels_20261105_group1::all());
    all.extend(chart_data_workbook::all());
    all.extend(crate::pptx_text_rect::all());
    all
}

/// Their unfixed-bug scenarios (the corpus report).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    #[cfg(target_os = "macos")]
    {
        corpus_report::ignored()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}
