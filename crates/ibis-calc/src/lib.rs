// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice's spreadsheet engine: cell values, formulas (parsing,
//! references, calculation) and number formats (what a value looks like
//! in a cell).
//!
//! Written for ibisOffice, from Excel's documented behaviour and from what
//! Excel itself shows and calculates (the answers of the coverage tests,
//! `test/coverage` of the test worktree). No UI or file format here: the
//! document model builds on it.

pub mod clock;
mod collation;
pub mod eval;
pub mod formula;
pub mod functions;
pub mod input;
pub mod numfmt;
pub mod random;
pub mod recalc;
pub mod special;
pub mod text_order;
pub mod value;

pub use value::{CellError, CellValue, number_to_xml};
