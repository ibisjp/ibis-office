// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188 (A11): the foot of a vertical (`tbRl`) section with `w:mirrorMargins` (left margin 72 pt, right 108 pt, gutter 0 or 36 pt): the box of the foot is
//! [72, 487.32 - gutter] on an odd page and [108 + gutter, 523.32] on an even one (the margins swap, the gutter goes with the right one). The documents are
//! `test/editing/mirror-vertical-foot-<gutter>-<jc>.docx` (4 pages, the foot `ABC` aligned `jc`); left aligned feet start at the box's left + 0.02 (Word: 72.02 / 108.02
//! and 72.02 / 144.02), the others are the box's middle and right edge less the line's width 15.78 pt of ABC (derived).

use crate::docx::flow_layout::Item;

#[test]
fn the_foot_box_of_a_vertical_section_swaps_its_margins_on_even_pages_with_mirror_margins() {
    let mut bad = Vec::new();
    for (gutter, g) in [(0, 0.0f32), (720, 36.0)] {
        let (odd, even) = ((72.0f32, 595.32 - 108.0 - g), (108.0 + g, 595.32 - 72.0));
        for (jc, at) in [("left", (odd.0, even.0)), ("center", ((odd.0 + odd.1 - 15.78) / 2.0, (even.0 + even.1 - 15.78) / 2.0)), ("right", (odd.1 - 15.78, even.1 - 15.78))] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../test/editing/mirror-vertical-foot-{gutter}-{jc}.docx"));
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
            for (page, want) in [(0usize, at.0), (1, at.1), (2, at.0), (3, at.1)] {
                let x = pages.pages[page].items.iter().find_map(|i| match i {
                    Item::Text { x, y, text, offsets, .. } if (*y - 802.9).abs() < 1.5 && text.starts_with('A') => Some(x + offsets[0]),
                    _ => None,
                });
                match x {
                    Some(x) if (x - want).abs() <= 0.35 => {}
                    other => bad.push(format!("gutter {g} {jc} page {}: {other:?}, wanted {want:.2}", page + 1)),
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
