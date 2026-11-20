// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #157 (stage 1): fonts that come after the first drawing (the browser version reads the downloaded ones in the background) make the sheet
//! laid out again: what a text measures changed, and the layouts kept and the shaped texts of the sheet are by the fonts' generation.
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("xlsx::issue157_a_sheet_is_laid_out_again_when_fonts_come_late", laid_out_again)]
}

fn laid_out_again() {
    // The kept layouts are a fixed number (`CACHED_LAYOUTS`) shared by the scenarios of the run: with the cache full from earlier ones a new layout pushes the oldest out and the count stays.
    crate::sheet_view::release_layouts();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.settle();
    let before = crate::sheet_view::layouts_kept();
    assert!(before >= 1, "the sheet was laid out");
    let font = std::fs::read(crate::shell::scenario::repo_root().join("crates/ibis-office-render/fonts/carlito/Carlito-Bold.ttf")).expect("a font file");
    assert!(s.update(move |_, _, cx| ibis_office_render::shape_view::add_fonts_now(cx, vec![font])));
    s.update(|_, _, cx| cx.refresh_windows());
    s.settle();
    assert!(crate::sheet_view::layouts_kept() > before, "a layout was made again for the new fonts, beside the old one ({before} -> {})", crate::sheet_view::layouts_kept());
}
