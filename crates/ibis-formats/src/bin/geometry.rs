// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-office-geometry: where things are on ibisOffice's pages, in points,
//! for building the manifests of coverage tests (see
//! `scripts/office-check/README.md`).
//!
//!   ibis-office-geometry [--cells] <file.pptx|xlsx|docx> [<Sheet!A1:C3>...]
//!
//! Prints JSON:
//!   {"pages": [{"page", "name", "width_pt", "height_pt"}],
//!    "ranges": [{"range", "page", "rect_pt": [x, y, width, height],
//!                "cells": [{"address", "text", "formula", "value"[, "hidden"]}]}]}
//! `cells` (with `--cells`) are the range's cells as ibisOffice shows them
//! (`text`, number formats applied), for comparing with what Excel shows
//! (`scripts/office-check/excel-values.sh`, `compare-cells.py`), with the
//! `value` it holds (a number in full, `TRUE`/`FALSE`, the text or error).
//! A cell of a hidden row or column has `"hidden": true`: Excel shows no
//! text there, so only the value is compared.
//! `pages` are what `ibisOffice --export png` draws, in order (slides,
//! worksheets, document pages as read; a document is laid out into more
//! pages by the app). A range's rectangle is on its worksheet's page, from
//! the top-left corner of cell A1, as ibisOffice sizes the columns and rows.

use ibis_doc_model::{EMU_PER_POINT, ShapeKind};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ibis-office-geometry: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

fn pt(emu: i64) -> f64 {
    emu as f64 / EMU_PER_POINT as f64
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out + "\""
}

/// `B3` as (column, row), 0-based.
fn cell(reference: &str) -> Option<(usize, usize)> {
    let reference = reference.replace('$', "");
    let letters: String = reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &reference[letters.len()..];
    let column = letters.to_ascii_uppercase().bytes().try_fold(0usize, |n, b| Some(n * 26 + (b - b'A' + 1) as usize))?;
    let row: usize = digits.parse().ok()?;
    (column >= 1 && row >= 1).then(|| (column - 1, row - 1))
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let with_cells = args.first().is_some_and(|a| a == "--cells");
    if with_cells {
        args.remove(0);
    }
    let (file, ranges) = args.split_first().ok_or("使い方: ibis-office-geometry [--cells] <file> [<Sheet!A1:C3>...]")?;
    let data = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let presentation = ibis_formats::read(&data).map_err(|e| format!("{file}: {e}"))?;
    let pages: Vec<String> = presentation
        .slides
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let size = presentation.size_of(s);
            format!(
                "{{\"page\":{},\"name\":{},\"width_pt\":{},\"height_pt\":{}}}",
                i + 1,
                s.name.as_deref().map_or("null".to_string(), quote),
                pt(size.width),
                pt(size.height)
            )
        })
        .collect();
    let mut out_ranges = Vec::new();
    for spec in ranges {
        let (sheet_name, addr) = spec.rsplit_once('!').ok_or(format!("{spec}: 「シート名!A1:B2」の形で指定してください"))?;
        let sheet_name = sheet_name.trim_matches('\'');
        let (index, slide) = presentation
            .slides
            .iter()
            .enumerate()
            .find(|(_, s)| s.name.as_deref() == Some(sheet_name))
            .ok_or(format!("{spec}: シート {sheet_name} がありません"))?;
        let sheet = slide
            .shapes
            .iter()
            .find_map(|s| match &s.kind {
                ShapeKind::Sheet(sheet) => Some((s.frame, sheet.clone())),
                _ => None,
            })
            .ok_or(format!("{spec}: ワークシートではありません"))?;
        let (frame, sheet) = sheet;
        let (first, last) = addr.split_once(':').unwrap_or((addr, addr));
        let ((c0, r0), (c1, r1)) = (cell(first).ok_or(format!("{spec}: 参照が正しくありません"))?, cell(last).ok_or(format!("{spec}: 参照が正しくありません"))?);
        let sum = |v: &[i64], from: usize, to: usize| -> i64 { (from..to).map(|i| v.get(i).copied().unwrap_or(0)).sum() };
        let x = frame.x + sum(&sheet.columns, 0, c0.min(c1));
        let y = frame.y + sum(&sheet.rows, 0, r0.min(r1));
        let w = sum(&sheet.columns, c0.min(c1), c0.max(c1) + 1);
        let h = sum(&sheet.rows, r0.min(r1), r0.max(r1) + 1);
        let mut cells = String::new();
        if with_cells {
            let (cmin, cmax, rmin, rmax) = (c0.min(c1) as u32, c0.max(c1) as u32, r0.min(r1) as u32, r0.max(r1) as u32);
            let items: Vec<String> = (rmin..=rmax)
                .flat_map(|r| (cmin..=cmax).map(move |c| (r, c)))
                .map(|(r, c)| {
                    let found = sheet.cells.iter().find(|cell| cell.row == r && cell.column == c);
                    // Its value (numbers in full, as Rust writes them), and
                    // whether it is in a hidden row or column (no room:
                    // Excel shows no text there, so only the value counts).
                    let value = match found.map(|f| &f.value) {
                        Some(ibis_doc_model::CellValue::Number(n)) => quote(&format!("{n}")),
                        Some(ibis_doc_model::CellValue::Bool(b)) => quote(if *b { "TRUE" } else { "FALSE" }),
                        Some(ibis_doc_model::CellValue::Empty) | None => quote(""),
                        Some(ibis_doc_model::CellValue::Text(t)) => quote(t),
                        Some(ibis_doc_model::CellValue::Error(e)) => quote(e.as_str()),
                    };
                    let hidden = sheet.columns.get(c as usize) == Some(&0) || sheet.rows.get(r as usize) == Some(&0);
                    let extra = format!(",\"value\":{value}{}", if hidden { ",\"hidden\":true" } else { "" });
                    format!(
                        "{{\"address\":{},\"text\":{},\"formula\":{}{extra}}}",
                        quote(&ibis_doc_model::cell_name(c, r)),
                        quote(found.map_or("", |f| f.text.as_str())),
                        found.and_then(|f| f.formula.as_deref()).map_or("null".to_string(), |f| quote(&format!("={f}")))
                    )
                })
                .collect();
            cells = format!(",\"cells\":[{}]", items.join(","));
        }
        out_ranges.push(format!("{{\"range\":{},\"page\":{},\"rect_pt\":[{},{},{},{}]{cells}}}", quote(spec), index + 1, pt(x), pt(y), pt(w), pt(h)));
    }
    println!("{{\"pages\":[{}],\"ranges\":[{}]}}", pages.join(","), out_ranges.join(","));
    Ok(())
}
