// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A small interpreter of the Power Query M language (design `tasks/doc/design/2026-11-16-k0039-query-m.md` section 9).
//!
//! No dependencies and nothing of the UI or the platform: it runs natively and in wasm. The application's query editor reads the
//! M a user wrote with this crate and turns the result into its own table (the mapping lives above this crate, in `ibis-doc-model`).
pub mod ast;
pub mod culture;
mod culture_data;
mod builtin;
mod format;
mod lib_date;
mod lib_io;
mod lib_list;
mod lib_text;
mod lib_zone;
mod table;
mod table_more;
pub mod error;
pub mod eval;
pub mod host;
pub mod lex;
pub mod parse;
pub mod value;

pub use error::{ErrKind, MError};

/// Whether the text is M that can be read (no evaluation): the error has the line of the first thing that cannot be read.
pub fn check_syntax(src: &str) -> Result<(), MError> {
    parse::parse(src).map(|_| ())
}

pub use eval::Interp;
pub use culture::Culture;
pub use table::convert_value;
pub use host::{CsvOptions, Host};
pub use value::{Lazy, Table, Value};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_excel_values;
#[cfg(test)]
mod tests_excel_values2;
#[cfg(test)]
mod tests_host;
#[cfg(test)]
mod tests_lib;
#[cfg(test)]
mod tests_locales;
#[cfg(test)]
mod tests_table;
