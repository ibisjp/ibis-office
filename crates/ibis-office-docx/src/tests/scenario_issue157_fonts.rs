// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #157 (stage 1): the browser version reads the downloaded fonts behind the first drawing and gives each one to the text system as it
//! comes (`add_fonts_now`). A document laid out before it came is laid out again, not kept from the caches (the widths of its text changed).

use crate::ext::*;
use crate::shell::scenario::Scenario;
use std::sync::Arc;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::issue157_a_document_is_laid_out_again_when_fonts_come_late", laid_out_again)]
}

fn laid_out_again() {
    let mut s = Scenario::open("test/editing/autospace-letters.docx");
    let pages = |s: &mut Scenario| s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let first = pages(&mut s);
    assert!(Arc::ptr_eq(&first, &pages(&mut s)), "laid out once: the same pages while nothing changed");
    // A font arrives (Carlito, as セットアップ's download does): the generation moves and the pages are made again.
    let before = ibis_office_render::shape_view::fonts_generation();
    let font = std::fs::read(crate::shell::scenario::repo_root().join("crates/ibis-office-render/fonts/carlito/Carlito-Bold.ttf")).expect("a font file");
    assert!(s.update(move |_, _, cx| ibis_office_render::shape_view::add_fonts_now(cx, vec![font])));
    assert!(ibis_office_render::shape_view::fonts_generation() > before);
    let second = pages(&mut s);
    assert!(!Arc::ptr_eq(&first, &second), "the pages were laid out again for the new fonts");
    assert!(Arc::ptr_eq(&second, &pages(&mut s)), "and once");
}
