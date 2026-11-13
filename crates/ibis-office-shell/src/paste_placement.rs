// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a paste of shapes is told about the clipboard's data, so that a screen can tell a repeated paste of the
//! same data from a new one. Where the pasted shapes go is the screen's to say
//! (`FormatEditor::place_pasted_shapes`; a sheet: `xlsx/paste_placement.rs`).

/// A number that stands for the clipboard's data: the same for the same bytes.
pub fn stamp(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}
