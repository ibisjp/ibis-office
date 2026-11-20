// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Decodes the fonts embedded in a user's private presentation (not
//! distributed with the source), named by `IBIS_PRIVATE_PPTX`, when it is set.

#[test]
fn decodes_every_embedded_font() {
    let Some(data) = std::env::var_os("IBIS_PRIVATE_PPTX").and_then(|p| std::fs::read(p).ok()) else {
        eprintln!("skipped: IBIS_PRIVATE_PPTX is not set or not found");
        return;
    };
    let p = ibis_formats::read(&data).expect("the test data reads");
    assert_eq!(p.fonts.len(), 18);
    let mut families = std::collections::BTreeSet::new();
    for font in &p.fonts {
        let ttf = ibis_eot::decode(&font.data).unwrap_or_else(|e| panic!("{}: {e}", font.typeface));
        // TrueType outlines, or CFF ones ("OTTO", Hiragino).
        assert!(ttf[..4] == [0, 1, 0, 0] || &ttf[..4] == b"OTTO", "{}: an OpenType file", font.typeface);
        families.insert(ibis_eot::family_name(&ttf).expect("a family name"));
    }
    let expected = ["Hiragino Kaku Gothic Pro", "M PLUS 1p", "Poppins", "Yu Gothic"];
    assert_eq!(families.into_iter().collect::<Vec<_>>(), expected);
}
