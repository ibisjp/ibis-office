// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #297: the advance widths of Malgun Gothic (Hangul, Latin letters, digits), made from the font file on Windows, for a PC that does not
//! have the font. Test design: `tasks/doc/test-design/297.md` (tests K7-K10).
//!
//! PENDING: this folder is not built by cargo (`tests/<dir>/` needs a `main.rs`). The implementer moves the file to
//! `crates/ibis-office-render/tests/issue297_malgun_advance_table.rs` when the table (`src/malgun_advance.tsv`) and
//! `font_metrics::malgun_gothic_advance_em` exist; they may choose other names and change them here with the reason.

use std::path::{Path, PathBuf};

fn tsv() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/malgun_advance.tsv")).expect("src/malgun_advance.tsv")
}

/// `(upem, runs)` of the section `@font <name>`: the shape of `meiryo_advance.tsv` (a line `<first hex>[-<last hex>]<TAB><advance>`, the advance left
/// out when it is the em).
fn section(text: &str, name: &str) -> (u32, Vec<(u32, u32, u32)>) {
    let mut upem = 0;
    let mut runs = Vec::new();
    let mut inside = false;
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut f = line.split('\t');
        let head = f.next().unwrap();
        if head == "@font" {
            inside = f.next() == Some(name);
            if inside {
                upem = f.next().unwrap().parse().unwrap();
            }
            continue;
        }
        if inside {
            let advance = f.next().map_or(upem, |a| a.parse().unwrap());
            let (a, b) = head.split_once('-').unwrap_or((head, head));
            runs.push((u32::from_str_radix(a, 16).unwrap(), u32::from_str_radix(b, 16).unwrap(), advance));
        }
    }
    (upem, runs)
}

fn advance(runs: &[(u32, u32, u32)], c: u32) -> Option<u32> {
    runs.iter().find(|r| (r.0..=r.1).contains(&c)).map(|r| r.2)
}

/// K7: the table has the regular and the bold face, units per em 2048 (the font file's), and says so in its header comments (made on Windows, not edited).
#[test]
fn k7_the_table_has_both_faces_in_the_shape_of_the_other_tables() {
    let text = tsv();
    assert!(text.lines().next().unwrap().starts_with('#'));
    assert!(text.contains("do not edit"), "a header comment says it is made by a script");
    for face in ["Malgun Gothic", "Malgun Gothic Bold"] {
        let (upem, runs) = section(&text, face);
        assert_eq!(upem, 2048, "{face}");
        assert!(!runs.is_empty(), "{face}");
    }
    // The values only: no outlines or other font data, so the file stays small.
    assert!(text.len() < 400_000, "{} bytes", text.len());
}

/// K8: what the font file (Windows 11, Malgun Gothic Version 6.69, malgun.ttf and malgunbd.ttf, read 2026-11-26) says: every Hangul syllable (U+AC00..D7A3),
/// the Hangul jamo and the compatibility jamo are one em; the half-width Hangul forms (U+FFA0..FFDC) half an em; and the Latin letters, digits and space.
#[test]
fn k8_the_hangul_and_latin_advances_are_the_font_files() {
    let text = tsv();
    let (_, regular) = section(&text, "Malgun Gothic");
    let (_, bold) = section(&text, "Malgun Gothic Bold");
    for (runs, name) in [(&regular, "regular"), (&bold, "bold")] {
        for c in (0xac00..=0xd7a3).step_by(97).chain([0xac00, 0xd7a3, 0x1100, 0x11ff, 0x3131, 0x318e, 0xa960, 0xd7b0]) {
            assert_eq!(advance(runs, c), Some(2048), "{name} U+{c:04X}");
        }
        assert_eq!(advance(runs, 0xffa0), Some(1024), "{name} half-width Hangul filler");
        assert_eq!(advance(runs, 0x20), Some(720), "{name} space");
    }
    // Regular / bold (units of 1/2048 em): 1 = 1128 / 1187, A = 1348 / 1441, B = 1195 / 1305, C = 1300 / 1308, comma = 448 / 536.
    for (c, r, b) in [('1', 1128, 1187), ('2', 1128, 1187), ('3', 1128, 1187), ('A', 1348, 1441), ('B', 1195, 1305), ('C', 1300, 1308), (',', 448, 536)] {
        assert_eq!(advance(&regular, c as u32), Some(r), "regular {c}");
        assert_eq!(advance(&bold, c as u32), Some(b), "bold {c}");
    }
}

/// K9: on a PC that has the font files (Windows: C:\Windows\Fonts), the whole table equals the font's hmtx for every character of the BMP that the
/// font has: no value is a guess. Skipped where the files are not at these paths.
#[test]
fn k9_the_table_equals_the_font_file_where_the_file_is_present() {
    let text = tsv();
    for (file, face) in [("malgun.ttf", "Malgun Gothic"), ("malgunbd.ttf", "Malgun Gothic Bold")] {
        let path = PathBuf::from(r"C:\Windows\Fonts").join(file);
        let Ok(data) = std::fs::read(&path) else { continue };
        let font = ttf_parser::Face::parse(&data, 0).unwrap();
        assert_eq!(u32::from(font.units_per_em()), 2048);
        let (_, runs) = section(&text, face);
        let mut checked = 0;
        for c in 0u32..=0xffff {
            let Some(ch) = char::from_u32(c) else { continue };
            let Some(gid) = font.glyph_index(ch) else { continue };
            let want = u32::from(font.glyph_hor_advance(gid).unwrap());
            assert_eq!(advance(&runs, c), Some(want), "{face} U+{c:04X}");
            checked += 1;
        }
        assert!(checked > 20_000, "{checked} characters checked in {face}");
    }
}

/// K10: the lookup the layout uses (`font_metrics::malgun_gothic_advance_em(bold, c)`, in ems) says the same, and `None` for what the font lacks.
#[test]
fn k10_the_lookup_gives_em_values() {
    use ibis_office_render::font_metrics::malgun_gothic_advance_em as em;
    assert_eq!(em(false, '종'), Some(1.0));
    assert_eq!(em(true, '종'), Some(1.0));
    assert_eq!(em(false, '1'), Some(1128.0 / 2048.0));
    assert_eq!(em(true, 'A'), Some(1441.0 / 2048.0));
    assert_eq!(em(false, ' '), Some(720.0 / 2048.0));
    assert_eq!(em(false, '\u{1f600}'), None);
}
