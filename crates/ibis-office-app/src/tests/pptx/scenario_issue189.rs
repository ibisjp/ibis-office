// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #189, the presentation (PPTX) side: where the text of a frame (`a:bodyPr`) and of a table cell (`a:tcPr`) sits, and the
//! ends and the order of a table's borders. The expected numbers are PowerPoint's (Microsoft 365 for Windows, PDF) on the issue's
//! `n7-pptx.pptx` (0.25 pt tolerance), each stated by the difference between two frames of one slide, which are the same but for the
//! attribute.
//!
//! The file is the issue's attachment (`test/github-issue/189/`, not in the repository): without it a scenario passes at once.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::export::{Format, Options, export};

const FILE: &str = "test/github-issue/189/n7-pptx.pptx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue189_space_first_last_para_moves_the_first_line_by_the_space_before", space_first_last),
        ("pptx::issue189_anchor_ctr_centres_the_widest_line_in_frames_and_cells", anchor_ctr),
        ("pptx::issue189_just_and_dist_are_drawn_where_b_is", just_and_dist),
        ("pptx::issue189_a_bottom_border_alone_ends_at_the_cell_edges", bottom_border_alone),
        ("pptx::issue189_thick_thin_borders_run_one_way_on_every_side", thick_thin),
        ("pptx::issue189_no_line_is_thinner_than_014_pt", line_width_floor),
    ]
}

fn pdf() -> Option<PathBuf> {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join(FILE)).ok()?;
    let presentation = ibis_formats::read(&data).unwrap();
    let out_dir = root.join("target/scenario/issue189");
    std::fs::create_dir_all(&out_dir).unwrap();
    let options = Options {
        format: Format::Pdf,
        out_dir: out_dir.clone(),
        name: "n7-pptx".into(),
        width: 1920,
        slides: Vec::new(),
        layout: false,
        file_name: Some("n7-pptx.pptx".into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    let result = export(presentation, &options);
    gpui_macos::set_font_smoothing(true);
    result.unwrap();
    Some(out_dir.join("n7-pptx.pdf"))
}

/// A line of a page (`pdftotext -bbox-layout`), points from the page's top left.
struct Line {
    x0: f32,
    x1: f32,
    y1: f32,
    text: String,
}

fn lines(pdf: &Path, page: usize) -> Vec<Line> {
    let out = Command::new("pdftotext").args(["-bbox-layout", "-f", &page.to_string(), "-l", &page.to_string()]).arg(pdf).arg("-").output().expect("pdftotext（poppler）が要る");
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    let number = |tag: &str, name: &str| -> f32 {
        let start = tag.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        tag[start..].split('"').next().unwrap().parse().unwrap()
    };
    let mut result = Vec::new();
    let mut open: Option<Line> = None;
    for line in xml.lines().map(str::trim) {
        if line.starts_with("<line ") {
            open = Some(Line { x0: number(line, "xMin"), x1: number(line, "xMax"), y1: number(line, "yMax"), text: String::new() });
        } else if line.starts_with("<word ") {
            if let Some(l) = open.as_mut() {
                l.text.push_str(line.split_once('>').unwrap().1.trim_end_matches("</word>"));
            }
        } else if line == "</line>" {
            result.extend(open.take());
        }
    }
    result
}

/// The lines of the text `text` of a page, in the order of the page.
fn with<'a>(lines: &'a [Line], text: &str) -> Vec<&'a Line> {
    let plain = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    lines.iter().filter(|l| plain(&l.text) == plain(text)).collect()
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() <= 0.25, "{what}: {a} against {b}");
}

/// D1: with `spcFirstLastPara="1"` and 18 pt before, the first line is 18 pt below where it is in the frame without it (PowerPoint: 38.4 and
/// 20.4 pt from the frame's top); the last line, 18 pt after it, 18 pt above (PowerPoint 26.4 and 8.3 from the frame's bottom).
fn space_first_last() {
    let Some(pdf) = pdf() else { return };
    let page = lines(&pdf, 1);
    // The frames A (top, `spcFirstLastPara`), B (top, none), C (bottom, `spcFirstLastPara`) in a row, D (bottom, none) 220 pt below A.
    // (The full-width space splits the line: the lines starting at the frame's inset with 段落 are the paragraphs'.)
    let paragraphs = |x: f32| -> Vec<f32> {
        let mut found: Vec<&Line> = page.iter().filter(|l| l.text == "段落" && (l.x0 - x).abs() < 1.0).collect();
        found.sort_by(|a, b| a.y1.partial_cmp(&b.y1).unwrap());
        found.iter().map(|l| l.y1).collect()
    };
    let (column_a, column_b, column_c) = (paragraphs(47.0), paragraphs(347.0), paragraphs(647.0));
    assert!(column_a.len() >= 4 && !column_b.is_empty() && column_c.len() >= 2, "{column_a:?} {column_b:?} {column_c:?}");
    near(column_a[0] - column_b[0], 18.0, "the space before the first paragraph of A");
    // C (bottom, `spcFirstLastPara`) against D (bottom, none; the second paragraph of the lower frame at x = 47).
    near(column_a[3] - 220.0 - column_c[1], 18.0, "the space after the last paragraph of C");
}

/// D2: with `anchorCtr="1"` a left-aligned line of 180 pt in a room of 252 pt starts 36 pt further right (PowerPoint 43.2 and 7.2 pt from the
/// frame's left edge), a right-aligned one 36 pt left (223.3 and 259.5), and a centred one stays; in a cell the same
/// ((190 - 90) / 2 = 50 pt further: 55.0 and 7.2; right-aligned 145.1 and 193.0).
fn anchor_ctr() {
    let Some(pdf) = pdf() else { return };
    let page = lines(&pdf, 2);
    let first = with(&page, "０１あいうえおかきく");
    let by_x = |x: f32| first.iter().min_by(|a, b| (a.x0 - x).abs().partial_cmp(&(b.x0 - x).abs()).unwrap()).unwrap();
    // A (left, `anchorCtr`) against D (left, none): frames at x = 40 and the same y apart.
    near(by_x(83.0).x0 - by_x(47.0).x0, 36.0, "left-aligned, anchorCtr: 36 pt in");
    // B (right, `anchorCtr`) against E (right, none).
    near(by_x(383.5).x1 - by_x(419.5).x1, -36.0, "right-aligned, anchorCtr: 36 pt out");
    // C (centred) does not move.
    near(by_x(683.2).x0, 683.2, "centred");
    let cells = lines(&pdf, 3);
    let mut words = with(&cells, "かきくけこ");
    words.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());
    assert_eq!(words.len(), 4);
    // Cells A (left, `anchorCtr`) at x = 40, B (right, `anchorCtr`) at 240, C (left, none) at 440, D (right, `anchorCtr="0"`) at 640:
    // PowerPoint 55.0 and 145.1 from the cell's left edge against 7.2 and 193.0.
    near(words[0].x0 - 40.0, 55.0, "a cell, left-aligned, anchorCtr");
    near(words[1].x1 - 240.0, 145.0, "a cell, right-aligned, anchorCtr");
}

/// D3: `dist` and `just` are drawn where `b` is (the frames of the row below are 220 pt lower).
fn just_and_dist() {
    let Some(pdf) = pdf() else { return };
    let page = lines(&pdf, 4);
    // The second line of each frame (the full-width space splits "行　二"): t, ctr, b in the row above, dist, just in the row below.
    let mut second: Vec<&Line> = page.iter().filter(|l| l.text == "二").collect();
    second.sort_by(|a, b| a.y1.partial_cmp(&b.y1).unwrap().then(a.x0.partial_cmp(&b.x0).unwrap()));
    assert_eq!(second.len(), 5, "{}", page.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("|"));
    let (bottom, dist, just) = (second[2].y1, second[3].y1 - 220.0, second[4].y1 - 220.0);
    near(dist, bottom, "dist as b");
    near(just, bottom, "just as b");
}

/// A raster of page `page` at 576 dpi, grey levels.
fn raster(pdf: &Path, page: usize) -> (usize, usize, Vec<u8>) {
    let base = pdf.with_file_name(format!("n7-pptx-p{page}"));
    let status = Command::new("pdftoppm").args(["-r", "576", "-f", &page.to_string(), "-l", &page.to_string(), "-singlefile", "-gray"]).arg(pdf).arg(&base).status().expect("pdftoppm（poppler）が要る");
    assert!(status.success());
    let data = std::fs::read(format!("{}.pgm", base.display())).unwrap();
    let mut at = 0;
    let mut token = || {
        while data[at].is_ascii_whitespace() {
            at += 1;
        }
        let start = at;
        while !data[at].is_ascii_whitespace() {
            at += 1;
        }
        String::from_utf8_lossy(&data[start..at]).into_owned()
    };
    assert_eq!(token(), "P5");
    let (w, h): (usize, usize) = (token().parse().unwrap(), token().parse().unwrap());
    let _ = token();
    (w, h, data[at + 1..].to_vec())
}

const PX: f32 = 8.0;

/// The runs (start, end in points) of dark pixels along the row at `y` pt.
fn row_runs(r: &(usize, usize, Vec<u8>), y: f32) -> Vec<(f32, f32)> {
    let row = &r.2[(y * PX) as usize * r.0..][..r.0];
    runs(row)
}

/// The runs (start, end in points) of dark pixels down the column at `x` pt.
fn column_runs(r: &(usize, usize, Vec<u8>), x: f32) -> Vec<(f32, f32)> {
    let column: Vec<u8> = (0..r.1).map(|y| r.2[y * r.0 + (x * PX) as usize]).collect();
    runs(&column)
}

fn runs(line: &[u8]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, v) in line.iter().enumerate() {
        match (*v < 128, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s as f32 / PX, i as f32 / PX));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// D4: a cell with a bottom border alone: the line is as wide as the cell (PowerPoint 40.0 to 190.0 pt for 12 pt, 240.0 to 390.0 for a
/// 6 pt double line, 640.0 to 790.0 for 1 pt); it does not run past the cell's edges by half its width.
fn bottom_border_alone() {
    let Some(pdf) = pdf() else { return };
    let r = raster(&pdf, 6);
    for (x, y, what) in [(40.0, 98.8, "12 pt"), (240.0, 96.8, "double 6 pt"), (640.0, 98.8, "1 pt")] {
        let line = row_runs(&r, y).into_iter().filter(|(a, b)| b - a > 100.0 && (a - x).abs() < 5.0).collect::<Vec<_>>();
        assert_eq!(line.len(), 1, "{what}: {line:?}");
        near(line[0].0, x, what);
        near(line[0].1, x + 150.0, what);
    }
}

/// D5: `thickThin` 6 pt on all four sides of the table at x = 637..783, y = 67..113: on the left edge the thin line is outside and the thick
/// one inside (PowerPoint thin 637.0..638.3, thick 639.4..643.0), on the bottom edge the thick line is above the thin one (thick 107.0..110.6,
/// thin 111.8..113.0).
fn thick_thin() {
    let Some(pdf) = pdf() else { return };
    let r = raster(&pdf, 7);
    let left = row_runs(&r, 90.0).into_iter().filter(|(a, _)| (636.0..645.0).contains(a)).collect::<Vec<_>>();
    assert_eq!(left.len(), 2, "{left:?}");
    near(left[0].0, 637.0, "left, thin from");
    near(left[0].1, 638.3, "left, thin to");
    near(left[1].0, 639.4, "left, thick from");
    near(left[1].1, 643.0, "left, thick to");
    let bottom = column_runs(&r, 700.0).into_iter().filter(|(a, _)| (100.0..120.0).contains(a)).collect::<Vec<_>>();
    assert_eq!(bottom.len(), 2, "{bottom:?}");
    near(bottom[0].0, 107.0, "bottom, thick from");
    near(bottom[0].1, 110.6, "bottom, thick to");
    near(bottom[1].0, 111.8, "bottom, thin from");
    near(bottom[1].1, 113.0, "bottom, thin to");
}

/// D5-E: PowerPoint draws no line thinner than 0.14 pt (1778 EMU): `w="0"`, `w="1"` and 0.1 pt are drawn 0.14 pt (a table's borders, a shape's outline and a
/// connector alike); from 0.14 pt up a line is `w` wide, and one without `w` is 0.75 pt. Slide 7's table E has `sng w=0` on its four sides. The model keeps
/// the file's `w` (0): only the drawing takes the floor (`Stroke::drawn_width`), and a file saved after an edit still says `w="0"`.
fn line_width_floor() {
    use ibis_doc_model::{ShapeKind, Stroke};
    let Some(data) = std::fs::read(crate::shell::scenario::repo_root().join(FILE)).ok() else { return };
    let widths_of = |p: &ibis_doc_model::Presentation| -> Vec<i64> {
        let mut widths = Vec::new();
        for shape in &p.slides[6].shapes {
            if let ShapeKind::Table(table) = &shape.kind {
                for cell in table.rows.iter().flat_map(|r| r.cells.iter()) {
                    let b = &cell.borders;
                    widths.extend([&b.left, &b.right, &b.top, &b.bottom].into_iter().flatten().map(|s| s.width));
                }
            }
        }
        widths
    };
    let mut opened = ibis_formats::open(data).unwrap();
    let before = widths_of(&opened.presentation);
    assert!(before.contains(&0), "table E's lines are w=0 in the model: {before:?}");
    let zero = Stroke { min_drawn_width: Stroke::POWERPOINT_MIN_WIDTH, ..Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0)), 0) };
    assert_eq!(zero.drawn_width(), Stroke::POWERPOINT_MIN_WIDTH);
    assert_eq!(Stroke { width: 1_270, ..zero.clone() }.drawn_width(), 1_778);
    assert_eq!(Stroke { width: 1_905, ..zero.clone() }.drawn_width(), 1_905);
    // A line of no floor (a chart's, one drawn on the screen) is drawn as wide as it says.
    assert_eq!(Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0)), 0).drawn_width(), 0);
    // The presentation's table E: every border has the floor.
    let floors = |p: &ibis_doc_model::Presentation| -> Vec<i64> {
        let mut out = Vec::new();
        for slide in &p.slides {
            for shape in &slide.shapes {
                match &shape.kind {
                    ShapeKind::Table(t) => out.extend(t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| [&c.borders.left, &c.borders.top].into_iter().flatten().map(|s| s.min_drawn_width))),
                    ShapeKind::Auto(a) => out.extend(a.stroke.iter().map(|s| s.min_drawn_width)),
                    _ => {}
                }
            }
        }
        out
    };
    let pptx_floors = floors(&opened.presentation);
    assert!(!pptx_floors.is_empty() && pptx_floors.iter().all(|f| *f == Stroke::POWERPOINT_MIN_WIDTH), "{pptx_floors:?}");
    // A workbook's drawings and a document's shapes have the floor too (Excel's PDF and Word's XPS draw max(w, 0.14 pt), `tasks/doc/requests/2026-11-14-189-office-result.md` 7),
    // and the file's `w` stays as it was read.
    let root = crate::shell::scenario::repo_root();
    if let Ok(xlsx) = std::fs::read(root.join("test/github-issue/189/n7-xlsx.xlsx")) {
        let sheets = floors(&ibis_formats::read_xlsx(&xlsx).unwrap());
        assert!(!sheets.is_empty() && sheets.iter().all(|f| *f == Stroke::POWERPOINT_MIN_WIDTH), "xlsx: {sheets:?}");
    }
    if let Ok(docx) = std::fs::read(root.join("test/github-issue/188/n5-vertical.docx")) {
        let docs = floors(&ibis_formats::read_docx(&docx).unwrap());
        assert!(docs.iter().all(|f| *f == Stroke::POWERPOINT_MIN_WIDTH), "docx: {docs:?}");
    }
    assert_eq!(Stroke::DEFAULT_WIDTH, 9_525);
    // An edit elsewhere (slide 1's first text) and a save: the borders still say 0.
    let edited = opened.presentation.slides[0].shapes.iter_mut().find_map(|s| s.text_body_mut()).map(|b| {
        b.paragraphs[0].runs[0].text.push('x');
    });
    assert!(edited.is_some());
    let saved = opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::read(&saved).unwrap();
    assert_eq!(widths_of(&again), before, "w is kept as it was");
}
