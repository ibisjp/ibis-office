// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the app hands to the crates below the formats, and to the formats, when it starts: the drawing crate's functions for what
//! the formats draw as shapes (the worksheet, the page of a document, the slicer, the timeline, the form control, a picture's size,
//! the images the page borders made, the underline's shapes), the shell's drivers and setup, and the workbook's page breaks. Each is
//! a call of the format's own function. `register_hooks` is the one place; every entry point calls it before anything is drawn or
//! opened (`init`, which the window and the scenarios run; `run_native`; the browser's `run`; the export's `headless_app`).
//! Drawing or opening a window before it panics and says so.

use std::sync::Arc;

use gpui_kit::{Div, Image};
use ibis_doc_model::slicer::Slicer;
use ibis_doc_model::timeline::Timeline;
use ibis_doc_model::{EmuRect, FlowDocument, FormControl, Sheet, UnderlineStyle};
use ibis_office_render::shape_view::Resources;

fn sheet_element(sheet: &Arc<Sheet>, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    crate::xlsx::sheet_view::sheet_element(sheet, frame, scale, res)
}

fn page_element(document: &Arc<FlowDocument>, index: usize, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    crate::docx::page_view::page_element(document, index, frame, scale, res)
}

fn slicer_element(slicer: &Slicer, frame: EmuRect, scale: f32) -> Div {
    crate::xlsx::slicer_view::slicer_element(slicer, frame, scale)
}

fn timeline_element(timeline: &Timeline, frame: EmuRect, scale: f32) -> Div {
    crate::xlsx::timeline_view::timeline_element(timeline, frame, scale)
}

fn control_element(control: &FormControl, frame: EmuRect, scale: f32) -> Div {
    crate::xlsx::control_view::control_element(control, frame, scale)
}

fn picture_size(data: &[u8]) -> Option<(u32, u32)> {
    crate::shell::shape_items::picture_size(data)
}

fn extra_images() -> Vec<Arc<Image>> {
    crate::docx::art_border::all_images()
}

fn underline_shapes(style: UnderlineStyle, size_pt: f32, x_pt: f32, width_pt: f32, baseline_pt: f32) -> (Vec<[f32; 4]>, Vec<[f32; 5]>) {
    use crate::docx::flow_layout::{Item, underline_items};
    let (mut boxes, mut strokes) = (Vec::new(), Vec::new());
    for item in underline_items(style, size_pt, x_pt, width_pt, baseline_pt, gpui_kit::black()) {
        match item {
            Item::Rect { x, y, w, h, .. } => boxes.push([x, y, w, h]),
            Item::Stroke { x, y, x2, y2, w, .. } => strokes.push([x, y, x2, y2, w]),
            _ => {}
        }
    }
    (boxes, strokes)
}

/// The page breaks of a worksheet, for the workbook's page break lines (`xlsx/export_hooks.rs`).
#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
fn preview_pages(presentation: &ibis_doc_model::Presentation, slide: &ibis_doc_model::Slide, resources: &Resources) -> (f64, Vec<EmuRect>) {
    crate::export::preview_pages(presentation, slide, resources)
}

/// Hand everything over (once; later calls do nothing).
pub fn register_hooks() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        ibis_office_render::shape_view::format_hooks::register(ibis_office_render::shape_view::format_hooks::Hooks {
            sheet_element,
            page_element,
            slicer_element,
            timeline_element,
            control_element,
            picture_size,
            extra_images,
            underline_shapes,
        });
        ibis_office_shell::app_hooks::register(ibis_office_shell::app_hooks::AppHooks { app_init: crate::init, drivers: crate::format_drivers::DRIVERS });
        #[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
        ibis_office_xlsx::export_hooks::register(preview_pages);
    });
}

#[cfg(test)]
mod tests {
    /// Every entry point calls `register_hooks`; after it the shell has its three drivers, and a second call changes nothing.
    #[test]
    fn registering_gives_the_shell_its_drivers() {
        super::register_hooks();
        super::register_hooks();
        assert_eq!(crate::shell::format::drivers().len(), 3);
    }
}
