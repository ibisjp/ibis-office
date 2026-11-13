// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Example scenarios (`scenario.rs`), for the formats' scenario tests to
//! follow. The formats' own go in `src/<format>/scenarios.rs`.

/// The examples, for `scenario::all`.
/// (The first example, a presentation's, moved to `pptx/scenarios.rs` as
/// `pptx::edit_text_make_bold_save_and_reopen`.)
pub fn all() -> Vec<(&'static str, fn())> {
    Vec::new()
}

