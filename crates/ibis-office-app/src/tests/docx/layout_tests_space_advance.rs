// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188, the advance of the spaces U+2000..U+200A, U+202F, U+205F and U+3000 in a line and in a column (10.5 pt), against Word for Windows' XPS
//! (`tasks/doc/requests/2026-11-25-188-space-advance-result.md` and `2026-11-25-188-mechanism-result.md`): U+2002..U+2006 take the advance of the typeface's EN DASH
//! (U+2013) and EM DASH (U+2014), not their own `hmtx`. They need the typefaces (Windows only). The documents are `test/editing/space-advance-<face>-<h|v>.docx`: a
//! paragraph の `あえお` and one with each space between え and お.

use crate::docx::flow_layout::Item;

/// The spaces of the documents, in their order, and Word's advance (pt) of each in メイリオ, 游明朝 and ＭＳ Ｐ明朝.
const SPACES: [(char, [f32; 3]); 16] = [
    ('\u{20}', [3.564, 2.724, 3.192]),
    ('\u{a0}', [3.564, 2.724, 3.192]),
    ('\u{2000}', [5.256, 5.256, 5.244]),
    ('\u{2001}', [10.503, 10.503, 10.491]),
    ('\u{2002}', [6.528, 5.256, 5.280]),
    ('\u{2003}', [10.503, 10.503, 7.008]),
    ('\u{2004}', [3.480, 3.480, 2.328]),
    ('\u{2005}', [2.628, 2.628, 1.752]),
    ('\u{2006}', [1.752, 1.752, 1.164]),
    ('\u{2007}', [5.736, 5.256, 6.552]),
    ('\u{2008}', [3.180, 2.628, 3.276]),
    ('\u{2009}', [2.100, 2.100, 1.308]),
    ('\u{200a}', [0.864, 0.864, 0.648]),
    ('\u{202f}', [2.100, 2.100, 2.100]),
    ('\u{205f}', [2.328, 2.328, 2.328]),
    ('\u{3000}', [10.503, 10.503, 6.960]),
];

/// The advance of each space (pt): the え-to-お distance with it, less the one without (the first paragraph).
fn advances(file: &str, vertical: bool) -> Vec<f32> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(file);
    let mut presentation = ibis_formats::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    let pages = crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources);
    let (mut e, mut o) = (Vec::new(), Vec::new());
    for item in &pages.pages[0].items {
        let mut put = |x: f32, y: f32, c: char| {
            let at = if vertical { y } else { x };
            if c == 'え' { e.push(at) } else if c == 'お' { o.push(at) }
        };
        match item {
            Item::Text { x, y, text, offsets, .. } => text.chars().zip(offsets.iter()).for_each(|(c, dx)| put(x + dx, *y, c)),
            Item::Turned { x, y, text, .. } | Item::Scaled { x, y, text, .. } => text.chars().for_each(|c| put(*x, *y, c)),
            _ => {}
        }
    }
    assert_eq!((e.len(), o.len()), (17, 17), "{file}: 17 paragraphs of え and お: {e:?} {o:?}");
    let base = o[0] - e[0];
    (1..17).map(|i| (o[i] - e[i]) - base).collect()
}

fn check(face: &str, column: usize) {
    let mut bad = Vec::new();
    for vertical in [false, true] {
        let file = format!("space-advance-{face}-{}.docx", if vertical { "v" } else { "h" });
        let got = advances(&file, vertical);
        for ((c, word), adv) in SPACES.iter().zip(got) {
            if (adv - word[column]).abs() > 0.25 {
                bad.push(format!("U+{:04X} vertical={vertical}: {adv:.3} against Word {}", *c as u32, word[column]));
            }
        }
    }
    assert!(bad.is_empty(), "{face}: {bad:#?}");
}

#[test]
fn the_spaces_of_meiryo_go_by_its_en_dash_and_em_dash_as_word() {
    check("meiryo", 0);
}

#[test]
fn the_spaces_of_yu_mincho_go_by_its_en_dash_and_em_dash_as_word() {
    check("yumincho", 1);
}

#[test]
fn the_spaces_of_ms_p_mincho_go_by_its_en_dash_and_em_dash_as_word() {
    check("mspmincho", 2);
}

/// Arial and Calibri (Word's XPS at 36 pt, em: U+2002 0.5561 / 0.4974, U+2003 1.0003 / 0.9063, U+2004 0.3333 / 0.3027): the EN DASH's and EM DASH's advance, a third of it, ...
#[test]
fn the_spaces_of_arial_and_calibri_go_by_their_dashes() {
    for (face, em) in [("arial", [0.5561f32, 1.0003, 0.3333, 0.2501, 0.1667]), ("calibri", [0.4974, 0.9063, 0.3027, 0.2268, 0.1512])] {
        for vertical in [false, true] {
            let got = advances(&format!("space-advance-{face}-{}.docx", if vertical { "v" } else { "h" }), vertical);
            for (i, want) in em.iter().enumerate() {
                let (adv, want) = (got[4 + i], want * 10.5);
                assert!((adv - want).abs() <= 0.25, "{face} U+{:04X} vertical={vertical}: {adv:.3} against Word {want:.3}", 0x2002 + i);
            }
        }
    }
}
