// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_doc_model::{FlowStyles, RunChange, RunFormat};

fn base() -> RunFormat {
    // Normal: ascii MS Mincho, hAnsi Courier New (GitHub issue #142; Word's inheritance per attribute, #M75).
    RunFormat { font: Some("MS Mincho".into()), h_ansi_font: Some("Courier New".into()), ..RunFormat::default() }
}

#[test]
fn style_keeps_h_ansi_that_differs() {
    let s = FlowStyles::default().run_format(&base(), None, &RunFormat::default()).style();
    assert_eq!(s.font.as_deref(), Some("MS Mincho"));
    assert_eq!(s.h_ansi_font.as_deref(), Some("Courier New"));
}

#[test]
fn ascii_only_run_keeps_the_h_ansi_of_the_style() {
    let direct = RunFormat { font: Some("Arial".into()), ..RunFormat::default() };
    let s = FlowStyles::default().run_format(&base(), None, &direct).style();
    assert_eq!(s.font.as_deref(), Some("Arial"));
    assert_eq!(s.h_ansi_font.as_deref(), Some("Courier New"));
}

#[test]
fn font_change_writes_both_typefaces() {
    let mut direct = RunFormat::default();
    RunChange::Font("Arial".into()).apply(&mut direct);
    assert_eq!(direct.font.as_deref(), Some("Arial"));
    assert_eq!(direct.h_ansi_font.as_deref(), Some("Arial"));
    let s = FlowStyles::default().run_format(&base(), None, &direct).style();
    assert_eq!(s.h_ansi_font.as_deref(), Some("Arial"));
    let mut all = RunFormat::default();
    RunChange::AllFonts("Arial".into()).apply(&mut all);
    assert_eq!(all.h_ansi_font.as_deref(), Some("Arial"));
}

#[test]
fn h_ansi_equal_to_font_stays_explicit() {
    // `ascii` = `hAnsi` = Arial over a style whose hAnsi is Courier New: Arial (dropping it would bring Courier New back).
    let direct = RunFormat { font: Some("Arial".into()), h_ansi_font: Some("Arial".into()), ..RunFormat::default() };
    let s = FlowStyles::default().run_format(&base(), None, &direct).style();
    assert_eq!(s.h_ansi_font.as_deref(), Some("Arial"));
}

#[test]
fn h_ansi_different_from_font_stays_direct() {
    let direct = RunFormat { font: Some("Arial".into()), h_ansi_font: Some("Verdana".into()), ..RunFormat::default() };
    let s = FlowStyles::default().run_format(&base(), None, &direct).style();
    assert_eq!(s.h_ansi_font.as_deref(), Some("Verdana"));
}
