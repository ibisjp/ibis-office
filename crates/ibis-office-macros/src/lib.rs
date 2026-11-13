// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `#[editor_ext(Name)]`: the methods a format (the document, the workbook, the presentation) writes for the editor of the
//! shell become extension traits, so that the block can live in a crate other than the one of the type
//! (tasks/doc/crate-split-plan.md, section 10).
//!
//! ```
//! # struct SlideEditor { count: i32 }
//! #[ibis_office_macros::editor_ext(Keys)]
//! impl SlideEditor {
//!     /// Seen in the whole crate: goes to the trait `KeysCrate`.
//!     pub(crate) fn key_down(&mut self, key: &str) -> bool {
//!         self.count += 1;
//!         self.is_letter(key)
//!     }
//!     /// Seen in this module only: goes to the trait `KeysPrivate`.
//!     fn is_letter(&self, key: &str) -> bool {
//!         key.len() == 1
//!     }
//! }
//! # let mut editor = SlideEditor { count: 0 };
//! # assert!(editor.key_down("a"));
//! ```
//!
//! becomes, for each visibility written on the methods, a trait with that visibility and its implementation for the type:
//!
//! | written on the method | trait |
//! |---|---|
//! | `pub` | `Name` |
//! | `pub(crate)` | `NameCrate` |
//! | `pub(super)` | `NameSuper` |
//! | `pub(in path)` | `NameIn` |
//! | nothing | `NamePrivate` |
//!
//! A method is called as before (`self.key_down(..)`, `Self::f(..)`) wherever its trait is in scope: in the module of the
//! block, and where the list of the format is imported (`use crate::docx::ext::*;`, written by scripts/gen-editor-ext.py).
//!
//! What the macro reads is the signature of each method; the body is passed on as it is. Every token the macro writes
//! has the place of the name in the attribute, so the compiler treats the traits as code of that file (a method nobody
//! calls is warned about at its own line, like a method of an `impl` without the attribute).
//!
//! Only methods can be written in the block, and only what the macro knows how to carry to a trait: anything else is an
//! error at its place, not something passed on quietly. A constant or a type goes outside the block:
//!
//! ```compile_fail
//! # struct SlideEditor;
//! #[ibis_office_macros::editor_ext(Keys)]
//! impl SlideEditor {
//!     const LIMIT: usize = 3;
//! }
//! ```
//!
//! ```compile_fail
//! # struct SlideEditor;
//! #[ibis_office_macros::editor_ext(Keys)]
//! impl SlideEditor {
//!     pub(crate) const fn limit() -> usize { 3 }
//! }
//! ```
//!
//! A method of one of the traits cannot be reached from a module that does not see the trait:
//!
//! ```compile_fail,E0599
//! # struct SlideEditor;
//! mod keys {
//!     # use super::SlideEditor;
//!     #[ibis_office_macros::editor_ext(Keys)]
//!     impl SlideEditor {
//!         fn only_here(&self) {}
//!     }
//! }
//! fn main() {
//!     SlideEditor.only_here();
//! }
//! ```
//!
//! (with `pub(crate) fn only_here` and `use keys::KeysCrate as _;` in `main`, the same compiles: tests/editor_ext.rs.)
use proc_macro::TokenStream;

mod expand;
#[cfg(test)]
mod tests;

/// See the crate's documentation.
#[proc_macro_attribute]
pub fn editor_ext(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::editor_ext(attr.into(), item.into()).into()
}
