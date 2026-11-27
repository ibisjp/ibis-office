// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #189, the spreadsheet (XLSX) side, on the issue's `n7-xlsx.xlsx` (Microsoft 365 for Windows, PDF):
//!
//! - D6 a cell with `c/@ph="1"` shows its furigana over the text (the cell's text with the 4 furigana: 8 characters); a cell without it, and
//!   a row alone with `row/@ph`, do not.
//! - D8 a text box's `anchor="dist"` and `"just"` sit where `"b"` sits (PowerPoint and Excel share the reading of `bodyPr`).
//!
//! The file is the issue's attachment (`test/github-issue/189/`, not in the repository): without it a scenario passes at once.
//! (D7, the turned characters' places, is not fixed: `tasks/doc/requests/2026-11-14-189-office.md`.)

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::export::{Format, Options, export};

const FILE: &str = "test/github-issue/189/n7-xlsx.xlsx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue189_furigana_of_a_cell_with_ph_is_drawn_and_only_that_cell", furigana_only_for_a_cell_with_ph),
        ("xlsx::issue189_just_and_dist_in_a_text_box_are_drawn_where_b_is", just_and_dist_in_a_box),
    ]
}

fn pdf() -> Option<PathBuf> {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join(FILE)).ok()?;
    let presentation = ibis_formats::read_xlsx(&data).unwrap();
    let out_dir = root.join("target/scenario/issue189");
    std::fs::create_dir_all(&out_dir).unwrap();
    let options = Options {
        format: Format::Pdf,
        out_dir: out_dir.clone(),
        name: "n7-xlsx".into(),
        width: 1920,
        slides: Vec::new(),
        layout: false,
        file_name: Some("n7-xlsx.xlsx".into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false, paper: None, fit_paper: true,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    let result = export(presentation, &options);
    gpui_macos::set_font_smoothing(true);
    result.unwrap();
    Some(out_dir.join("n7-xlsx.pdf"))
}

/// The lines of page `page` of the PDF (`pdftotext -bbox-layout`): (text, bottom y in points from the top).
fn lines(pdf: &Path, page: usize) -> Vec<(String, f32)> {
    let out = Command::new("pdftotext").args(["-bbox-layout", "-f", &page.to_string(), "-l", &page.to_string()]).arg(pdf).arg("-").output().expect("pdftotext（poppler）が要る");
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    let number = |tag: &str, name: &str| -> f32 {
        let start = tag.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        tag[start..].split('"').next().unwrap().parse().unwrap()
    };
    let mut result = Vec::new();
    let mut open: Option<(String, f32)> = None;
    for line in xml.lines().map(str::trim) {
        if line.starts_with("<line ") {
            open = Some((String::new(), number(line, "yMax")));
        } else if line.starts_with("<word ") {
            if let Some(l) = open.as_mut() {
                l.0.push_str(line.split_once('>').unwrap().1.trim_end_matches("</word>"));
            }
        } else if line == "</line>" {
            result.extend(open.take());
        }
    }
    result
}

/// D6: the furigana "さしすせ" is on the page once (B2: `ph="1"`), not for C2 (no `ph`) or B3 (`ph` on the row only).
fn furigana_only_for_a_cell_with_ph() {
    let Some(pdf) = pdf() else { return };
    let page = lines(&pdf, 1);
    assert_eq!(page.iter().filter(|(t, _)| t == "さしすせ").count(), 1, "{page:?}");
    assert_eq!(page.iter().map(|(t, _)| t.as_str()).collect::<String>().matches("かきくけ").count(), 3, "{page:?}");
}

/// D8: the five boxes (`t`, `ctr`, `b`, `dist`, `just`) are the same distance apart: the last line of `dist` and of `just` are where `b`'s
/// would be (Excel: 6.6 pt from the last line to the box's bottom for each).
fn just_and_dist_in_a_box() {
    let Some(pdf) = pdf() else { return };
    let page = lines(&pdf, 3);
    let last: Vec<f32> = page.iter().filter(|(t, _)| t == "へほまみむめも").map(|(_, y)| *y).collect();
    assert_eq!(last.len(), 5, "{page:?}");
    let pitch = last[4] - last[3];
    assert!((last[3] - last[2] - pitch).abs() <= 0.25, "dist as b: {last:?}");
}
