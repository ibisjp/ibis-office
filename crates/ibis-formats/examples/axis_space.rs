// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! For each chart with a manual plot layout, print the space the layout
//! leaves beside the plot and the value-axis labels there (debugging aid).

use ibis_doc_model::{AxisKind, ShapeKind, Side, format_number};

fn main() {
    let path = std::env::args().nth(1).expect("pptx");
    let p = ibis_formats::read(&std::fs::read(path).unwrap()).unwrap();
    let px = 1280.0 / p.slide_size.width as f64;
    for (n, slide) in p.slides.iter().enumerate() {
        for shape in &slide.shapes {
            let ShapeKind::Chart(chart) = &shape.kind else { continue };
            let Some(plot) = chart.plot_area else { continue };
            let w = shape.frame.width as f64 * px;
            let left = plot.x as f64 / 100_000.0 * w;
            let right = (1.0 - (plot.x + plot.width) as f64 / 100_000.0) * w;
            let mut out = Vec::new();
            for a in chart.axes.iter().filter(|a| a.kind == AxisKind::Value && !a.deleted) {
                let high = a.max.map(|m| m.0).unwrap_or(0.0);
                let label = format_number(high, &a.format);
                let side = if (a.side == Side::Right) != a.labels_high { "R" } else { "L" };
                out.push(format!("{side} {label:?} {}pt", a.text.size as f64 / 100.0));
            }
            println!("slide {:2} left {left:5.1}px right {right:5.1}px  {}", n + 1, out.join(" | "));
        }
    }
}
