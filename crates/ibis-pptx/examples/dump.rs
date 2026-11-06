// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Print the shapes of one slide as read, for debugging the reader.
//!
//!   cargo run -p ibis-pptx --example dump -- <file.pptx> <slide number>

use ibis_doc_model::ShapeKind;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(path), Some(number)) = (args.get(1), args.get(2).and_then(|n| n.parse::<usize>().ok())) else {
        eprintln!("usage: dump <file.pptx> <slide number>");
        std::process::exit(2);
    };
    let data = std::fs::read(path).expect("readable file");
    let p = if path.ends_with(".xlsx") { ibis_pptx::read_xlsx(&data) } else { ibis_pptx::read(&data) }.expect("a presentation");
    let slide = &p.slides[number - 1];
    println!("background {:?} size {:?} name {:?}", slide.background, slide.size, slide.name);
    for (layer, shapes) in [("backdrop", &slide.backdrop), ("slide", &slide.shapes)] {
        for s in shapes {
            let kind = match &s.kind {
                ShapeKind::Line(stroke) => format!("line {stroke:?}"),
                ShapeKind::Auto(a) => format!(
                    "{:?} fill={:?} sizes={:?} text={:?}",
                    a.geometry,
                    a.fill,
                    a.text.as_ref().map(|t| t.paragraphs.iter().flat_map(|p| p.runs.iter().map(|r| (r.style.font_size, r.style.font.clone(), r.style.outline))).take(3).collect::<Vec<_>>()),
                    a.text.as_ref().map(|t| t.text())
                ),
                ShapeKind::Picture(pic) => format!("picture {pic:?}"),
                ShapeKind::TextBox(t) => format!("text box {:?}", t.text()),
                ShapeKind::Placeholder(k) => format!("placeholder {k:?}"),
                ShapeKind::Table(t) => format!(
                    "table {}×{} {:?}",
                    t.rows.len(),
                    t.columns.len(),
                    t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.text()).collect::<Vec<_>>()).collect::<Vec<_>>()
                ),
                ShapeKind::Sheet(sheet) => format!(
                    "sheet {}×{} cells={} styles={} merges={} {:?}",
                    sheet.rows.len(),
                    sheet.columns.len(),
                    sheet.cells.len(),
                    sheet.styles.len(),
                    sheet.merges.len(),
                    sheet.cells.iter().filter(|c| !c.text.is_empty()).take(12).map(|c| (c.row, c.column, c.text.as_str())).collect::<Vec<_>>()
                ),
                ShapeKind::Slicer(s) => format!("slicer {} items={}", s.name, s.items.len()),
                ShapeKind::Timeline(t) => format!("timeline {} level={}", t.name, t.level),
                ShapeKind::Control(c) => format!("control {:?} {} link={:?}", c.kind, c.name, c.link),
                ShapeKind::Page { document, index } => {
                    let text = |b: &ibis_doc_model::Block| match b {
                        ibis_doc_model::Block::Paragraph(p) => p
                            .runs
                            .iter()
                            .map(|r| match &r.kind {
                                ibis_doc_model::FlowRunKind::Text(t) => t.clone(),
                                other => format!("[{other:?}]"),
                            })
                            .collect::<String>(),
                        ibis_doc_model::Block::Table(t) => format!("[table {}x{}]", t.rows.len(), t.columns.len()),
                    };
                    if std::env::var_os("DUMP_FULL").is_some() {
                        for b in document.header.iter().chain(&document.footer).chain(&document.body) {
                            match b {
                                ibis_doc_model::Block::Paragraph(p) => println!("P {:?} l={} r={} f={} line={:?} before={} after={} | {}", p.align, p.left, p.right, p.first_line, p.line, p.space_before, p.space_after, text(b)),
                                ibis_doc_model::Block::Table(_) => println!("{}", text(b)),
                            }
                        }
                    }
                    format!(
                        "page {index} of {:?}: {} blocks, header {:?}, footer {:?}",
                        document.page,
                        document.body.len(),
                        document.header.iter().map(text).collect::<Vec<_>>(),
                        document.footer.iter().map(text).collect::<Vec<_>>()
                    )
                }
                ShapeKind::Chart(c) if std::env::var_os("DUMP_FULL").is_some() => format!("chart {c:#?}"),
                ShapeKind::Chart(c) => format!(
                    "chart {:?}",
                    c.groups.iter().map(|g| (g.kind, g.series.iter().map(|s| (s.name.clone(), s.values.len())).collect::<Vec<_>>())).collect::<Vec<_>>()
                ),
            };
            let kind: String = if std::env::var_os("DUMP_FULL").is_some() { kind } else { kind.chars().take(160).collect() };
            println!("{layer} id={} hidden={} {:?} rot={} {kind}", s.id.0, slide.hidden_shapes.contains(&s.id), s.frame, s.rotation);
        }
    }
}
