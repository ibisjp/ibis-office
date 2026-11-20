// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A formula's colour is saved and read again (GitHub issue #171): Word keeps it in the `w:rPr/w:color` of the formula's `m:r`s.

use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::flow::FlowRunKind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::math_colour_is_saved_and_read_again", the_colour_of_a_formula_is_saved_and_read_again),
        ("docx::math_runs_have_each_their_colour", the_runs_of_a_formula_have_each_their_colour),
        ("docx::math_colour_of_the_structures_is_saved_and_read_again", the_colour_of_the_structures_of_a_formula_is_saved_and_read_again),
        ("docx::math_colour_of_the_whole_formula_replaces_those_of_its_runs", a_colour_for_the_whole_formula_replaces_those_of_its_runs),
    ]
}

/// The colour of the page text that has `letter` (a formula's letters are the Unicode math italic: 𝑎 for a).
fn colour_of(items: &[super::flow_layout::Item], letter: char) -> ibis_doc_model::Rgb {
    use super::flow_layout::Item;
    let item = items.iter().find_map(|i| if let Item::Text { text, color, .. } = i { text.contains(letter).then_some(*color) } else { None }).unwrap_or_else(|| panic!("no text with {letter}: {:?}", items.iter().filter_map(|i| if let Item::Text { text, .. } = i { Some(text.to_string()) } else { None }).collect::<Vec<_>>()));
    let to_byte = |v: f32| (v * 255.0).round() as u32;
    let c = gpui_kit::Rgba::from(item);
    ibis_doc_model::Rgb(to_byte(c.r) << 16 | to_byte(c.g) << 8 | to_byte(c.b))
}

fn the_runs_of_a_formula_have_each_their_colour() {
    // `test/editing/docx-math-run-colors.docx`: x= (no colour) a (red) +b (blue); y= and (c+d)/2 (all green).
    let mut s = Scenario::open("test/editing/docx-math-run-colors.docx");
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let items: Vec<super::flow_layout::Item> = pages.pages.iter().flat_map(|p| p.items.iter().cloned()).collect();
    let items = &items[..];
    let (a, b, x) = (colour_of(items, '\u{1d44e}'), colour_of(items, '\u{1d44f}'), colour_of(items, '\u{1d465}'));
    assert_eq!(a, ibis_doc_model::Rgb(0xff0000), "a is red");
    assert_eq!(b, ibis_doc_model::Rgb(0x0000ff), "b is blue");
    assert_eq!(x, ibis_doc_model::Rgb(0x000000), "x has no colour of its own: the text's");
    assert_eq!(colour_of(items, '\u{1d466}'), ibis_doc_model::Rgb(0x008000), "y is green");
    assert_eq!(colour_of(items, '\u{1d450}'), ibis_doc_model::Rgb(0x008000), "c in the fraction is green");
    // Saved without a change: each run keeps its colour.
    let saved = s.save_as("docx-math-run-colors-saved.docx");
    let xml = ibis_formats::test_files::part_text(&std::fs::read(&saved).unwrap(), "word/document.xml").unwrap();
    assert!(xml.contains("<w:color w:val=\"FF0000\"/>") && xml.contains("<w:color w:val=\"0000FF\"/>") && xml.contains("<w:color w:val=\"008000\"/>"), "the runs' colours are in the file");
}

fn a_colour_for_the_whole_formula_replaces_those_of_its_runs() {
    let mut s = Scenario::open("test/editing/docx-math-run-colors.docx");
    s.key("cmd-a");
    s.update(|e, _, cx| e.set_text_color(ibis_doc_model::Rgb(0x800080), cx));
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let items: Vec<super::flow_layout::Item> = pages.pages.iter().flat_map(|p| p.items.iter().cloned()).collect();
    let items = &items[..];
    for letter in ['\u{1d44e}', '\u{1d44f}', '\u{1d465}', '\u{1d466}', '\u{1d450}'] {
        assert_eq!(colour_of(items, letter), ibis_doc_model::Rgb(0x800080), "{letter}");
    }
    // Saved: every run of the formulas has it (the colours it had are gone).
    let saved = s.save_as("docx-math-run-colors-whole.docx");
    let xml = ibis_formats::test_files::part_text(&std::fs::read(&saved).unwrap(), "word/document.xml").unwrap();
    let start = xml.find("<m:oMath").expect("a formula");
    let formulas = &xml[start..];
    assert!(!formulas.contains("FF0000") && !formulas.contains("0000FF") && !formulas.contains("008000"), "the old colours are gone");
    assert!(formulas.matches("<m:r>").count() >= 6);
    let in_runs = formulas.matches("<w:color w:val=\"800080\"/>").count();
    assert!(in_runs >= 6, "{in_runs} runs have the colour");
}

fn the_colour_of_a_formula_is_saved_and_read_again() {
    let root = crate::shell::scenario::repo_root();
    let input = root.join("target/scenario-input/docx-math-color.docx");
    std::fs::create_dir_all(input.parent().unwrap()).unwrap();
    std::fs::write(&input, ibis_formats::test_files::docx_with_math(&std::fs::read(root.join("test/editing/docx-report.docx")).unwrap())).unwrap();
    let mut s = Scenario::open("target/scenario-input/docx-math-color.docx");
    s.key("cmd-a");
    s.update(|e, _, cx| e.set_text_color(ibis_doc_model::Rgb(0xff0000), cx));
    let saved = s.save_as("docx-math-color-saved.docx");
    let xml = ibis_formats::test_files::part_text(&std::fs::read(&saved).unwrap(), "word/document.xml").unwrap();
    assert!(xml.contains("<w:color w:val=\"FF0000\"/>") && xml.matches("<m:r>").count() > 0, "the colour is in the file");
    // Each formula's runs carry it.
    let m_start = xml.find("<m:oMath").expect("a formula");
    assert!(xml[m_start..].contains("FF0000"), "inside the formula: {}", &xml[m_start..(m_start + 400).min(xml.len())]);
    // Opened again, each of the formula's runs has the colour (it is on the runs now).
    let reopened = s.reopen();
    let colours = reopened_run_colours(&reopened);
    assert!(!colours.is_empty() && colours.iter().all(|c| *c == Some(ibis_doc_model::Rgb(0xff0000))), "{colours:?}");
}

/// The colour of each text run of each formula of the document (none: it has none of its own).
fn reopened_run_colours(p: &ibis_doc_model::Presentation) -> Vec<Option<ibis_doc_model::Rgb>> {
    use ibis_doc_model::math::Math;
    fn runs(m: &Math, out: &mut Vec<Option<ibis_doc_model::Rgb>>) {
        match m {
            Math::Text(_) => out.push(None),
            Math::Colored { color, body } if matches!(**body, Math::Text(_)) => out.push(Some(*color)),
            Math::Colored { body, .. } => runs(body, out),
            Math::Delimited { items, .. } => items.iter().for_each(|i| runs(i, out)),
            Math::Row(items) => items.iter().for_each(|i| runs(i, out)),
            Math::Frac { num, den, .. } => {
                runs(num, out);
                runs(den, out);
            }
            Math::Radical { body, .. } => runs(body, out),
            other => panic!("not in the test file: {other:?}"),
        }
    }
    let doc = p.flow.as_deref().expect("a document");
    let mut out = Vec::new();
    for b in &doc.body {
        if let ibis_doc_model::flow::Block::Paragraph(p) = b {
            for r in &p.runs {
                if let FlowRunKind::Math { math, .. } = &r.kind {
                    runs(math, &mut out);
                }
            }
        }
    }
    out
}

/// A formula's structures (its brackets, fraction bar and radical) have a colour of their own in the control properties (`m:ctrlPr/w:rPr/w:color`) of their `m:dPr`, `m:fPr`
/// and `m:radPr` (GitHub issue #197: after the whole formula was coloured, saved and opened again, the brackets, the bar and the radical were black).
fn the_colour_of_the_structures_of_a_formula_is_saved_and_read_again() {
    use super::flow_layout::Item;
    let mut s = Scenario::open("test/editing/docx-math-structures.docx");
    s.key("cmd-a");
    s.update(|e, _, cx| e.set_text_color(ibis_doc_model::Rgb(0xff0000), cx));
    let saved = s.save_as("docx-math-structures-saved.docx");
    let xml = ibis_formats::test_files::part_text(&std::fs::read(&saved).unwrap(), "word/document.xml").unwrap();
    for tag in ["dPr", "fPr", "radPr"] {
        let at = xml.find(&format!("<m:{tag}>")).unwrap_or_else(|| panic!("<m:{tag}> in the file"));
        let end = at + xml[at..].find(&format!("</m:{tag}>")).unwrap();
        assert!(xml[at..end].contains("<m:ctrlPr>") && xml[at..end].contains("<w:color w:val=\"FF0000\"/>"), "{tag} has the colour: {}", &xml[at..end]);
    }
    // The properties it had (the control properties' font and italic) are kept beside the colour... or at least the file is read again with the colours.
    let again = s.reopen();
    let pages = s.with(|e, _| super::flow_layout::pages(again.flow.as_ref().expect("a document"), &e.resources));
    let items: Vec<Item> = pages.pages.iter().flat_map(|p| p.items.iter().cloned()).collect();
    let red = |c: gpui_kit::Hsla| {
        let c = gpui_kit::Rgba::from(c);
        (c.r * 255.0).round() as u32 == 0xff && (c.g * 255.0).round() as u32 == 0 && (c.b * 255.0).round() as u32 == 0
    };
    let (mut bars, mut strokes, mut texts) = (0, 0, 0);
    for it in &items {
        match it {
            Item::Rect { color, h, .. } if *h < 3.0 && *h > 0.0 => {
                // The fraction bar and the radical's bar (the page's other rectangles are not this thin and in the formulas' place).
                if red(*color) {
                    bars += 1;
                }
            }
            Item::Stroke { color, .. } => {
                strokes += 1;
                assert!(red(*color), "the radical's strokes are red");
            }
            Item::Text { text, color, .. } if text.chars().any(|c| matches!(c, '(' | ')' | '\u{1d453}' | '\u{1d465}' | '=')) => {
                texts += 1;
                assert!(red(*color), "{text} is red");
            }
            _ => {}
        }
    }
    assert!(bars >= 2 && strokes >= 3 && texts >= 4, "bars {bars}, strokes {strokes}, texts {texts}");
}
