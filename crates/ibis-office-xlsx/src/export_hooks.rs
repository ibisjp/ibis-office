// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the workbook asks of the exporter and cannot name (the exporter calls all three formats, so it is the app's): where a
//! worksheet breaks into pages, for the page break lines of the sheet and the page layout view. The app hands the function over
//! when it starts (`register`, from `ibis_office_app::register_hooks`), as for the drawing crate's `shape_view::format_hooks`
//! (tasks/doc/crate-split-plan.md, 18).

#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
use crate::render::shape_view::Resources;
#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
use ibis_doc_model::{EmuRect, Presentation, Slide};

/// The pages a worksheet prints on (as spans of the slide, EMU) and the scale it prints at (the app's `export.rs`, `preview_pages`).
#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
pub(crate) type PreviewPages = fn(&Presentation, &Slide, &Resources) -> (f64, Vec<EmuRect>);

#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
static PREVIEW_PAGES: std::sync::OnceLock<PreviewPages> = std::sync::OnceLock::new();

/// Hand over the exporter's page breaks. The first call counts; a later one is ignored.
#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
pub fn register(preview_pages: PreviewPages) {
    let _ = PREVIEW_PAGES.set(preview_pages);
}

#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
pub fn preview_pages(presentation: &Presentation, slide: &Slide, resources: &Resources) -> (f64, Vec<EmuRect>) {
    let f = PREVIEW_PAGES
        .get()
        .expect("ibis-office-xlsx: the exporter's page breaks are not registered (call ibis_office_app::register_hooks() first)");
    f(presentation, slide, resources)
}
