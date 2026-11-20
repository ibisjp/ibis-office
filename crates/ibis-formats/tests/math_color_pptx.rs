// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #198: a colour given to the text run of a formula in a presentation (PPTX) is kept by a save: PowerPoint writes it in the run's properties of its own
//! (`a:rPr/a:solidFill`) on each `m:r` and in each `m:ctrlPr`, and reads it back from there.
use ibis_doc_model::math::Math;
use ibis_doc_model::{Presentation, Rgb};
use ibis_formats::SaveOptions;

fn formulas(p: &mut Presentation) -> Vec<&mut ibis_doc_model::Run> {
    p.slides.iter_mut().flat_map(|s| s.shapes.iter_mut()).filter_map(|sh| sh.text_body_mut()).flat_map(|b| b.paragraphs.iter_mut()).flat_map(|p| p.runs.iter_mut()).filter(|r| r.style.math.is_some()).collect()
}

/// The colours of the runs (`Some`) and structures (`None` for a run without) of a formula, and how many structures have one.
fn colors(m: &Math, runs: &mut Vec<Option<Rgb>>, structures: &mut Vec<Rgb>) {
    match m {
        Math::Text(_) => runs.push(None),
        Math::Colored { color, body } => match &**body {
            Math::Text(_) => runs.push(Some(*color)),
            other => {
                structures.push(*color);
                colors(other, runs, structures);
            }
        },
        Math::Row(items) | Math::EqArray(items) => items.iter().for_each(|i| colors(i, runs, structures)),
        Math::Frac { num, den, .. } => {
            colors(num, runs, structures);
            colors(den, runs, structures);
        }
        Math::Radical { degree, body } => {
            if let Some(d) = degree {
                colors(d, runs, structures);
            }
            colors(body, runs, structures);
        }
        Math::Script { base, sub, sup } => {
            colors(base, runs, structures);
            sub.iter().chain(sup.iter()).for_each(|m| colors(m, runs, structures));
        }
        Math::Delimited { items, .. } => items.iter().for_each(|i| colors(i, runs, structures)),
        Math::Nary { sub, sup, body, .. } => {
            sub.iter().chain(sup.iter()).for_each(|m| colors(m, runs, structures));
            colors(body, runs, structures);
        }
        Math::Mark { base, .. } => colors(base, runs, structures),
        Math::Function { name, arg } => {
            colors(name, runs, structures);
            colors(arg, runs, structures);
        }
        Math::Matrix(rows) => rows.iter().flatten().for_each(|m| colors(m, runs, structures)),
    }
}

#[test]
fn a_colour_of_the_text_run_of_a_formula_is_saved_and_read_again() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/pptx-math-omml-office.pptx")).unwrap();
    let opened = ibis_formats::open(bytes).unwrap();
    let mut p = opened.presentation.clone();
    let mut before = formulas(&mut p);
    assert!(!before.is_empty(), "a formula in the file");
    for run in &mut before {
        run.style.color = Some(Rgb(0xFF0000));
    }
    let saved = opened.source.save(&p, SaveOptions::default()).unwrap();
    let xml = ibis_formats::test_files::part_text(&saved, "ppt/slides/slide1.xml").unwrap();
    assert!(xml.contains("<m:ctrlPr><a:rPr><a:solidFill><a:srgbClr val=\"FF0000\"/>"), "the structures have it in their control properties");
    let mut again = ibis_formats::read(&saved).unwrap();
    let all = formulas(&mut again);
    assert!(!all.is_empty());
    let mut all_structures = Vec::new();
    for run in all {
        let (mut runs, mut structures) = (Vec::new(), Vec::new());
        colors(run.style.math.as_ref().unwrap(), &mut runs, &mut structures);
        assert!(!runs.is_empty() && runs.iter().all(|c| *c == Some(Rgb(0xFF0000))), "every run is red: {runs:?}");
        assert!(structures.iter().all(|c| *c == Rgb(0xFF0000)));
        all_structures.extend(structures);
    }
    assert!(all_structures.len() >= 3, "the fractions, radicals and sums are red: {all_structures:?}");
}

#[test]
fn a_formula_without_a_colour_of_its_run_is_written_as_it_was() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/pptx-math-omml-office.pptx")).unwrap();
    let opened = ibis_formats::open(bytes).unwrap();
    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    let xml = ibis_formats::test_files::part_text(&saved, "ppt/slides/slide1.xml").unwrap();
    assert!(!xml.contains("FF0000") && xml.contains("<m:ctrlPr><a:rPr lang=\"en-US\">"), "the file's own markup");
}
