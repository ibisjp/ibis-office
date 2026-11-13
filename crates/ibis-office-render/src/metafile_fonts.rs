// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The ascent and descent of the fonts that draw a metafile's text, for `ibis-emf` (the baseline of a string, the height
//! of a line): from the font's file as Office takes them (`font_metrics::font_file_metrics`), else those of the typeface
//! Office has (`font_metrics::office_metrics`) when it is not installed.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use gpui_kit::*;

use crate::font_metrics::{font_file_metrics, office_metrics};

struct Source {
    text_system: Arc<TextSystem>,
    installed: HashSet<String>,
}

static SOURCE: OnceLock<Source> = OnceLock::new();

/// Hand the text system to the metafile player. Call once the app has its text system (at start-up).
pub fn install(cx: &App) {
    // Once: listing the installed fonts takes tens of milliseconds, and every opened document (every page of the master
    // view) calls this.
    if SOURCE.get().is_some() {
        return;
    }
    let text_system = cx.text_system().clone();
    let installed = text_system.all_font_names().into_iter().collect();
    if SOURCE.set(Source { text_system, installed }).is_ok() {
        ibis_emf::set_font_metrics(metrics);
    }
}

/// The ascent and descent as fractions of the em size.
pub fn metrics(face: &str, bold: bool, italic: bool) -> Option<(f64, f64)> {
    let source = SOURCE.get()?;
    let found = if let Some((family, mut weight, face_italic)) =
        crate::shape_view::resolve_system_font(face, &source.installed)
    {
        let named_weight = crate::shape_view::strip_weight_suffix(face).is_some()
            || weight != FontWeight::NORMAL;
        if bold && (!named_weight || weight < FontWeight::SEMIBOLD) {
            weight = FontWeight::BOLD;
        }
        let mut font = gpui_kit::font(family);
        font.weight = weight;
        if italic || face_italic {
            font = font.italic();
        }
        let id = source.text_system.resolve_font(&font);
        source.text_system.font_data(id).and_then(|(data, index)| font_file_metrics(&data, index))
    } else {
        None
    };
    found
        .or_else(|| office_metrics(face))
        .or_else(|| crate::shape_view::strip_weight_suffix(face).and_then(|(base, _)| office_metrics(base)))
        .map(|(a, d)| (f64::from(a), f64::from(d)))
}
