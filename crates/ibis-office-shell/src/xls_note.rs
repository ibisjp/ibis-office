// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 書き出し > Excel 97-2003 ブック（.xls）: the workbook as an old Excel file, and the words about what
//! the old format could not hold.
use gpui_kit::SharedString;

use crate::i18n::{format, text};

/// What was left out of a saved .xls, as a sentence (nothing if nothing was).
pub fn note(report: &ibis_pptx::XlsReport) -> Option<SharedString> {
    let mut items = Vec::new();
    for (n, id) in [(report.drawings, "xls.item.drawings"), (report.other, "xls.item.other"), (report.formulas_as_values, "xls.item.formulas"), (report.cells_out_of_range, "xls.item.cells")] {
        if n > 0 {
            items.push(format(id, &[("n", &n)]));
        }
    }
    (!items.is_empty()).then(|| format("xls.left-out", &[("items", &items.join(&*text("xls.separator")))]).into())
}
