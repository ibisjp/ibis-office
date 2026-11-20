// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Print the text boxes of a slide with the settings that decide where
//! their lines go (debugging aid for text layout).

use ibis_doc_model::{LineSpacing, ShapeKind};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let p = ibis_formats::read(&std::fs::read(&args[1]).unwrap()).unwrap();
    let px = 1280.0 / p.slide_size.width as f64;
    let slide = &p.slides[args[2].parse::<usize>().unwrap() - 1];
    for (layer, shapes) in [("backdrop", &slide.backdrop), ("slide", &slide.shapes)] {
        for s in shapes.iter() {
            let body = match &s.kind {
                ShapeKind::TextBox(b) => b,
                ShapeKind::Auto(a) => match &a.text { Some(b) => b, None => continue },
                _ => continue,
            };
            let first = &body.paragraphs[0];
            let size = first.runs.first().and_then(|r| r.style.font_size).unwrap_or(body.style.font_size) as f64 / 100.0;
            let spacing = match first.line_spacing { Some(LineSpacing::Percent(p)) => format!("{}%", p / 1000), Some(LineSpacing::Points(p)) => format!("{}pt", p / 100), None => "100%".into() };
            let text: String = body.text().chars().take(24).collect();
            println!(
                "{layer} x={:.0}-{:.0} y={:.1} h={:.1} inset_t={:.1} anchor={:?} size={size}pt ({:.2}px) spacing={spacing} before={} paras={} font={:?} {text:?}",
                s.frame.x as f64 * px, (s.frame.x + s.frame.width) as f64 * px, s.frame.y as f64 * px, s.frame.height as f64 * px, body.insets.top as f64 * px, body.anchor, size * 12700.0 * px,
                first.space_before, body.paragraphs.len(), first.runs.first().and_then(|r| r.style.font.clone()),
            );
        }
    }
}
