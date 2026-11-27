// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A slide's comments (コメント): PowerPoint's threaded comments
//! (`p188:cmLst`, with answers and a resolved state) and the older ones
//! (`p:cmLst`), as one list.

use crate::{EmuPoint, ShapeId};

/// One comment on a slide, with its answers (返信).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideComment {
    /// The file's id for it (a GUID for a threaded comment, the author's
    /// index for an older one); empty for a new one until it is saved.
    pub id: String,
    pub author: Author,
    /// When it was written, as the file writes it (`2026-10-07T09:00:00.000`).
    pub created: String,
    pub text: String,
    /// Where its mark is on the slide, if it says.
    pub position: Option<EmuPoint>,
    /// The shape it is on (a threaded comment's anchor), if any.
    pub shape: Option<ShapeId>,
    /// 解決済み.
    pub resolved: bool,
    pub replies: Vec<CommentReply>,
}

/// An answer to a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentReply {
    pub id: String,
    pub author: Author,
    pub created: String,
    pub text: String,
}

/// Who wrote a comment or an answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub initials: String,
}

/// A GUID (`{xxxxxxxx-xxxx-4xxx-xxxx-xxxxxxxxxxxx}`) for a comment or an answer, mixed from `seed` (the callers make it from the clock and a count).
pub fn guid_from_seed(seed: u64) -> String {
    let mix = |mut x: u64| {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^ (x >> 33)
    };
    let (a, b) = (mix(seed), mix(seed ^ 0x5555_5555_5555_5555));
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xffff, a & 0xfff, 0x8000 | (b >> 48) & 0x3fff, b & 0xffff_ffff_ffff)
}
