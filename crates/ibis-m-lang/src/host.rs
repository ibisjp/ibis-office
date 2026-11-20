// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the application gives the interpreter so that M can read files. This crate reads no file itself (it runs in the browser, where
//! there is none); `File.Contents(path)` only names one, and `Csv.Document` and `Excel.Workbook` ask the host to read it.
use crate::value::Value;

/// The options of `Csv.Document`.
#[derive(Clone, Debug, PartialEq)]
pub struct CsvOptions {
    pub delimiter: String,
    /// The columns at least (0: as many as the widest line has).
    pub columns: usize,
    /// The character code (65001 is UTF-8).
    pub encoding: u32,
    /// `QuoteStyle.Csv`: a field may be in quotes (the default); `QuoteStyle.None`: quotes are characters.
    pub quoted: bool,
}

pub trait Host {
    /// The table of a text file: `Column1`, `Column2`… of text, the first line not yet the names.
    fn read_csv(&self, path: &str, options: &CsvOptions) -> Result<Value, String>;
    /// The sheets and tables of a workbook file as the table `Excel.Workbook` gives: the columns `Name`, `Data` (a table each), `Item`, `Kind`
    /// (`"Sheet"` or `"Table"`) and `Hidden`.
    fn read_workbook(&self, path: &str) -> Result<Value, String>;
}
