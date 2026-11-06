// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 94: the `w:w` Word for Mac stores when 縦中横 is applied with 行の幅に合わせる
//! (`tasks/doc/reports/2026-11-03-mac-office-front-measure-2.md`, digits `2027456...`, 12 pt, one run of n digits).
//! One law, `floor(100 * 1.29 / (n * digit advance in em))`, fits six typefaces and n = 2..8.

use ibis_doc_model::tate_chu_yoko_fit_scale;

/// (typeface, a digit's advance in em, the `w:w` Word wrote for n = 2..=8; 0 = none written).
/// The advances are the typefaces' own (MS Mincho / Gothic and Times New Roman 0.5, Arial 0.556); for Yu Mincho and Meiryo the advance
/// Word's `w:w` implies (Mac's own digits, 0.4875 and 0.41; Windows' PDFs show 0.54 for Yu Mincho, which these do not follow).
const MEASURED: &[(&str, f32, [u32; 7])] = &[
    ("MS Mincho", 0.5, [0, 86, 64, 51, 42, 36, 32]),
    ("MS Gothic", 0.5, [0, 86, 64, 51, 42, 36, 32]),
    ("Yu Gothic", 0.5, [0, 86, 64, 51, 42, 36, 32]),
    ("Times New Roman", 0.5, [0, 86, 64, 51, 43, 37, 32]),
    ("Yu Mincho", 0.4875, [0, 88, 66, 52, 44, 37, 33]),
    ("Arial", 0.556, [0, 77, 58, 46, 38, 33, 29]),
    ("Meiryo", 0.41, [0, 0, 78, 62, 52, 44, 39]),
];

#[test]
fn fit_scale_follows_one_law_across_typefaces_and_lengths() {
    let mut exact = 0;
    let mut off_by_one = Vec::new();
    for (name, d, expected) in MEASURED {
        for (k, want) in expected.iter().enumerate() {
            let n = k + 2;
            let got = tate_chu_yoko_fit_scale(&vec![*d; n]).unwrap_or(0);
            if got == *want {
                exact += 1;
            } else {
                assert_eq!(got.abs_diff(*want), 1, "{name} n={n}: law {got}, Word {want}");
                off_by_one.push((name, n));
            }
        }
    }
    // 49 measured points: 45 are exact, the other 4 are one off at the border of the floor (n = 6 of the three 0.5 em faces: the law
    // gives exactly 43.0, Word wrote 42; Times New Roman n = 7: 36.86, Word wrote 37), so the unit Word floors in is not known to the last digit.
    assert!(exact >= 45, "exact {exact}, off by one {off_by_one:?}");
}

#[test]
fn a_run_not_wider_than_the_line_is_not_shrunk() {
    assert_eq!(tate_chu_yoko_fit_scale(&[0.5]), None);
    assert_eq!(tate_chu_yoko_fit_scale(&[0.5, 0.5]), None);
    assert_eq!(tate_chu_yoko_fit_scale(&[]), None);
}

#[test]
fn the_shrunk_width_is_the_same_whatever_the_digits() {
    // Proportional digits of different advances give the same shrunk width (1.29 em), which is the law.
    let advances = [0.556, 0.278, 0.556, 0.556, 0.5];
    let w = tate_chu_yoko_fit_scale(&advances).unwrap() as f32;
    let width = advances.iter().sum::<f32>() * w / 100.0;
    assert!((1.27..=1.29).contains(&width), "{width}");
}

#[test]
fn the_dialog_sets_tate_chu_yoko_and_the_scale_together_on_the_selected_characters() {
    use ibis_doc_model::{CharacterFormat, TateChuYoko, TextBody, TextStyle};
    let mut body = TextBody::plain("あ2027あ", TextStyle::default());
    let scale = tate_chu_yoko_fit_scale(&[0.5; 4]);
    assert_eq!(scale, Some(64));
    let format = CharacterFormat { tate_chu_yoko: Some(Some(TateChuYoko::Fit)), char_scale: Some(scale), ..Default::default() };
    body.format_runs(1, 5, &format);
    let runs = &body.paragraphs[0].runs;
    let digits = runs.iter().find(|r| r.text == "2027").expect("the digits are a run of their own");
    assert_eq!(digits.style.tate_chu_yoko, Some(TateChuYoko::Fit));
    assert_eq!(digits.style.char_scale, Some(64));
    assert!(runs.iter().filter(|r| r.text != "2027").all(|r| r.style.char_scale.is_none()));
    // Turned off: no `w:w`.
    body.format_runs(1, 5, &CharacterFormat { tate_chu_yoko: Some(Some(TateChuYoko::Grow)), char_scale: Some(None), ..Default::default() });
    assert!(body.paragraphs[0].runs.iter().all(|r| r.style.char_scale.is_none()));
}
