// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The browser version's vector PDF (K0037): the same export as the desktop's `--export pdf` (`export::render`), on the browser's text
//! system, in a headless window that only records what is painted (no renderer, no GPU), and the record written by `pdf_vector`
//! (krilla) as on the desktop.
//!
//! Fonts: text in a font the text system holds (IBM Plex, Carlito, what セットアップ downloaded) is embedded from the file it has.
//! Text the browser drew itself (its Canvas, for a family the text system does not hold: 游ゴシック, メイリオ, ヒラギノ…) has no file, so the
//! PDF gets one before it is written (`web_pdf_fonts`): the viewer's own font file, which Chrome's `queryLocalFonts` gives (the user
//! allows it once), else a font the text system holds that has the character.

use ibis_office_shell::format::PdfJob;
use ibis_office_shell::web_pdf::{PdfFuture, WebPdf};

use crate::export::{Format, finish_pdf, render};

/// The PDF of `job` (the file's bytes, the slides and the arguments the desktop's `--export` run takes) for the file called
/// `file_name`, made in the page. `extra_args` are the arguments of the file's format.
pub(crate) fn make(job: PdfJob, file_name: String, extra_args: Vec<&'static str>) -> PdfFuture {
    Box::pin(async move { make_now(job, file_name, extra_args).await })
}

async fn make_now(job: PdfJob, file_name: String, extra_args: Vec<&'static str>) -> Result<WebPdf, String> {
    let presentation = ibis_odf::read(&job.bytes).map_err(|e| e.to_string())?;
    let mut arguments: Vec<String> = vec!["--export".into(), "pdf".into()];
    if let Some(args) = &job.args {
        arguments.extend(args.iter().cloned());
    } else if let Some(slides) = &job.slides {
        arguments.push("--range".into());
        arguments.push(slides.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
    }
    arguments.extend(extra_args.iter().map(|a| a.to_string()));
    let args = crate::parse_args_from(arguments)?;
    let name = std::path::Path::new(&file_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    let options = crate::export_options(&args, Format::Pdf, ".".into(), name, Some(file_name), &job.bytes)?;
    let mut writer = render(presentation, &options)?.writer.ok_or("the PDF was not made")?;
    crate::web_pdf_fonts::give_fonts(&mut writer).await;
    let finished = finish_pdf(writer)?;
    for warning in &finished.warnings {
        log::warn!("PDF: {warning}");
    }
    Ok(WebPdf {
        bytes: finished.bytes,
        missing_glyphs: finished.missing_glyphs,
        // Pictures of the pages (colour, frame, handouts, notes, outline) need a renderer, which the browser has not: the pages as they are.
        pictures_skipped: job.format == "pdf-image",
    })
}
