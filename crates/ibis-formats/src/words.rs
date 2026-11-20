// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The words of the documents that the medical readers (`dicom`, `fhir`, `uke`, `dpc`) make out of a file: their headings, table heads, sheet names and notes.
//!
//! The words are in the screen's tables (`crates/ibis-office-base/i18n/<ja|en>/medical.tsv`), which this crate reads from the same files (it is below the
//! tables' crate, so it cannot ask it). The language is the screen's, which the tables' crate tells with `set_english_names` (a file read without a screen,
//! a test, the command line, is in Japanese).
use std::collections::HashMap;
use std::sync::OnceLock;

const JA: &str = include_str!("../../ibis-office-base/i18n/ja/medical.tsv");
const EN: &str = include_str!("../../ibis-office-base/i18n/en/medical.tsv");

fn table(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines().filter_map(|l| l.split_once('\t')).collect()
}

fn tables() -> &'static (HashMap<&'static str, &'static str>, HashMap<&'static str, &'static str>) {
    static TABLES: OnceLock<(HashMap<&'static str, &'static str>, HashMap<&'static str, &'static str>)> = OnceLock::new();
    TABLES.get_or_init(|| (table(JA), table(EN)))
}

/// The words of `id` in the screen's language (the id itself if the tables do not have it).
pub fn text(id: &str) -> String {
    let (ja, en) = tables();
    let found = if crate::english_names() { en.get(id).or_else(|| ja.get(id)) } else { ja.get(id) };
    found.map_or_else(|| id.to_string(), |t| t.to_string())
}

/// The words of `id` with each `{name}` replaced by its argument.
pub fn format(id: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text(id);
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// Whether `heading` is the words of `id` in either language (a document made in one language is read back in the other).
pub fn is(heading: &str, id: &str) -> bool {
    let (ja, en) = tables();
    [ja.get(id), en.get(id)].into_iter().flatten().any(|t| heading.contains(*t))
}

/// Whether `cell` is exactly the words of `id` in either language.
pub fn same(cell: &str, id: &str) -> bool {
    let (ja, en) = tables();
    [ja.get(id), en.get(id)].into_iter().flatten().any(|t| cell.trim() == *t)
}
