// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #297: a DOCX that names a Korean Office family (Malgun Gothic, Gulim, Dotum, Batang, Gungsuh, HCR Dotum, HCR Batang, by their English
//! or Korean names) in all four font slots and has no `word/fontTable.xml` is drawn with a Korean stand-in when the PC does not have the family.
//! Test design: `tasks/doc/test-design/297.md` (tests K1-K6).
//!
//! PENDING: not compiled until registered. The implementer adds to `shape_view.rs`, after `mod font_resolution_tests`:
//!   `#[cfg(test)] #[path = "tests/korean_stand_in.rs"] mod korean_stand_in_tests;`
//! (a child module of `shape_view`, which is why it can build a `Fonts`).

use super::*;

fn fonts(installed: &[&str], font_table: bool) -> Fonts {
    Fonts {
        installed: installed.iter().map(|s| s.to_string()).collect(),
        embedded_index: OpenTypeIndex::default(),
        east_asian_fallback: Some("Yu Gothic".into()),
        substitutions: HashMap::new(),
        calibri_for_missing: false,
        docx_font_table: font_table.then(Default::default),
        traits: Default::default(),
    }
}

const BASE: [&str; 6] = ["Calibri", "Arial", "Times New Roman", "MS Gothic", "MS Mincho", "Yu Gothic"];

fn with(extra: &[&str]) -> Vec<&'static str> {
    BASE.iter().copied().chain(extra.iter().map(|s| &*Box::leak(s.to_string().into_boxed_str()))).collect()
}

const SANS: &[&str] = &["Malgun Gothic", "맑은 고딕", "맑은고딕", "MALGUN GOTHIC", " Malgun Gothic ", "Gulim", "굴림", "Dotum", "돋움", "HCR Dotum", "함초롬돋움"];
const SERIF: &[&str] = &["Batang", "바탕", "HCR Batang", "함초롬바탕", "Gungsuh", "궁서", "batang"];
const KOREAN_FACES: &[&str] = &["Apple SD Gothic Neo", "AppleMyungjo", "Noto Sans CJK KR", "Noto Sans KR", "Noto Serif CJK KR", "NanumGothic", "NanumMyeongjo"];

/// K1: a sans Korean family the PC lacks is drawn with an installed Korean sans face, for each of its English and Korean names (case and surrounding
/// spaces do not matter), with and without a fontTable (the repro has none; a Word file has an empty one for these families).
#[test]
fn k1_sans_korean_families_take_the_installed_korean_sans_face() {
    for (face, installed) in [("Apple SD Gothic Neo", with(&["Apple SD Gothic Neo"])), ("Noto Sans CJK KR", with(&["Noto Sans CJK KR"])), ("NanumGothic", with(&["NanumGothic"]))] {
        for table in [false, true] {
            let f = fonts(&installed, table);
            for name in SANS {
                assert_eq!(f.family(name).0.as_ref(), face, "{name} table={table}");
            }
        }
    }
}

/// K2: a serif Korean family (Batang, Gungsuh, HCR Batang) takes an installed Korean serif face; with none installed, the Korean sans face.
#[test]
fn k2_serif_korean_families_take_the_serif_face_or_else_the_sans_one() {
    let f = fonts(&with(&["Apple SD Gothic Neo", "AppleMyungjo"]), true);
    for name in SERIF {
        assert_eq!(f.family(name).0.as_ref(), "AppleMyungjo", "{name}");
    }
    let f = fonts(&with(&["Apple SD Gothic Neo"]), true);
    for name in SERIF {
        assert_eq!(f.family(name).0.as_ref(), "Apple SD Gothic Neo", "{name}");
    }
}

/// K3: what comes first stays first. The family itself installed wins; so does a face the document embeds; and the alternate name the document's own
/// fontTable gives (`w:altName`) wins over the stand-in.
#[test]
fn k3_the_installed_family_and_the_documents_own_alternate_come_first() {
    let mut f = fonts(&with(&["Apple SD Gothic Neo", "AppleMyungjo", "Malgun Gothic", "Batang", "Gungsuh"]), true);
    assert_eq!(f.family("Malgun Gothic").0.as_ref(), "Malgun Gothic");
    assert_eq!(f.family("Batang").0.as_ref(), "Batang");
    assert_eq!(f.family("Gungsuh").0.as_ref(), "Gungsuh");
    // HCR Batang is not installed: the serif Korean face installed stands in (Batang itself is a Korean serif face).
    assert_eq!(f.family("HCR Batang").0.as_ref(), "Batang");
    f.installed.remove("Batang");
    f.docx_font_table.as_mut().unwrap().insert("Batang".into(), ibis_doc_model::FontTableEntry { alt_names: vec!["Times New Roman".into()], ..Default::default() });
    assert_eq!(f.family("Batang").0.as_ref(), "Times New Roman");
}

/// K4: no Korean face on the PC: what a Korean name got before (the East Asian fallback, never a Latin-only face such as Calibri or Arial).
#[test]
fn k4_without_any_korean_face_the_east_asian_fallback_stays() {
    let f = fonts(&BASE, true);
    for name in SANS.iter().chain(SERIF) {
        let got = f.family(name).0;
        assert!(got.as_ref() == "Yu Gothic" || got.as_ref() == "MS Gothic", "{name} -> {got}");
    }
}

/// K5: other names are not touched: a Japanese or Latin family keeps its own result whatever Korean faces are installed; a Korean-looking word in a
/// made-up family name is not read (Word does not read the words of a missing typeface's name, `a_missing_typeface_is_not_read_for_the_words_of_its_name`).
#[test]
fn k5_other_families_are_unchanged() {
    let without = fonts(&BASE, true);
    let with_korean = fonts(&with(KOREAN_FACES), true);
    for name in ["Arial", "Calibri", "MS Gothic", "Yu Gothic", "Zzqx Foo", "架空ゴシック", "Fake Malgun", "Malgun Gothic Pro Max", "Batangas", "Dotum Extra"] {
        assert_eq!(with_korean.family(name).0, without.family(name).0, "{name}");
    }
}

/// K6: the weight suffix is kept (a bold run names "Malgun Gothic Bold" or uses `w:b`): the stand-in is chosen from the family and the weight follows.
#[test]
fn k6_a_weight_in_the_name_keeps_the_stand_in_and_the_weight() {
    let f = fonts(&with(&["Apple SD Gothic Neo"]), true);
    let (face, weight) = f.family("Malgun Gothic Bold");
    assert_eq!(face.as_ref(), "Apple SD Gothic Neo");
    assert_eq!(weight, FontWeight::BOLD);
}
