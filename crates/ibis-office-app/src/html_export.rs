// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `--export html` (File > Export > Web page of the three screens, GitHub issue #208): the document as one HTML page, with the charts and SmartArt (and the other
//! objects the page cannot draw) rasterised as PNG pictures, in the page as data URIs: the one place that does it. A presentation is its slides
//! (`ibis_formats::slides_html`), a workbook the sheet (`--range`: its number, the first by default), a document its body (`ibis_formats::docx_convert::export_html_with`).

use std::path::Path;

use ibis_doc_model::{EmuRect, EmuSize, Presentation, Shape, Slide, SlideId};

use crate::export::{self, Format, Options};

/// The picture of `shape` as PNG bytes: the shape alone on a slide of its own size, drawn as `--export png` draws a slide, `width` pixels wide at most. `members` are the shapes of a
/// SmartArt graphic (its frame draws nothing itself): drawn over it, moved so that the frame's corner is the slide's.
pub(crate) fn rasterize(presentation: &Presentation, shape: &Shape, members: &[Shape], width: u32, scratch: &Path) -> Option<Vec<u8>> {
    let (w, h) = (shape.frame.width.max(12_700), shape.frame.height.max(12_700));
    let mut alone = shape.clone();
    alone.frame = EmuRect::new(0, 0, w, h);
    alone.rotation = 0;
    alone.floating = None;
    let mut slide = Slide::new(SlideId(1));
    slide.shapes.push(alone);
    for member in members {
        let mut member = member.clone();
        member.floating = None;
        member.frame.x -= shape.frame.x;
        member.frame.y -= shape.frame.y;
        slide.shapes.push(member);
    }
    let mut mini = presentation.clone();
    mini.slide_size = EmuSize::new(w, h);
    mini.slides = vec![slide];
    mini.flow = None;
    mini.workbook = None;
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(scratch).ok()?;
    let options = Options {
        format: Format::Png,
        out_dir: scratch.to_path_buf(),
        name: "shape".into(),
        width,
        slides: Vec::new(),
        layout: false,
        file_name: None,
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: false,
        color: crate::export_print::PrintColor::Color,
        handout: None,
        frame: false,
        handout_landscape: false, paper: None, fit_paper: true,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    export::export(mini, &options).ok()?;
    std::fs::read(scratch.join("shape-01.png")).ok()
}

/// A page of the width of `shape` in points at twice the points (a picture of 144 dpi), at most 2400 pixels.
fn width_for(shape: &Shape) -> u32 {
    ((shape.frame.width as f64 / 12_700.0 * 2.0).round() as u32).clamp(64, 2400)
}

/// Write `<out_dir>/<name>.html` for the document in `presentation`; `sheet` is the 1-based sheet of a workbook (the first when `None`).
pub(crate) fn export_html(presentation: &Presentation, out_dir: &Path, name: &str, english: bool, sheet: Option<usize>) -> Result<std::path::PathBuf, String> {
    let scratch = std::env::temp_dir().join(format!("ibisoffice-html-{}", std::process::id()));
    let raster = |shape: &Shape, members: &[Shape]| rasterize(presentation, shape, members, width_for(shape), &scratch);
    let html = if presentation.flow.is_some() {
        ibis_formats::docx_convert::export_html_with(presentation.flow.as_ref().expect("a document"), &raster)
    } else if presentation.workbook.is_some() {
        let index = sheet.unwrap_or(1).saturating_sub(1);
        crate::xlsx::sheet_html::sheet_to_html_with(presentation, index, &raster).map_err(|e| format!("{e:?}"))?
    } else {
        let labels = ibis_formats::slides_html::Labels::default();
        ibis_formats::slides_html::slides_to_html_with(presentation, name, if english { "en" } else { "ja" }, &labels, &raster).map_err(|e| format!("{e:?}"))?
    };
    let _ = std::fs::remove_dir_all(&scratch);
    let path = out_dir.join(format!("{name}.html"));
    std::fs::write(&path, html).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
