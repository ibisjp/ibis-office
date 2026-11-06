// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Defined names that go with pasted formulas, as in Excel (measured in Excel for Mac, issue #100):
//! pasting cells whose formulas use names (ranges, constants, LAMBDA functions) into another workbook
//! also defines those names there, so that the formulas keep working.
//!
//! - Only the names the pasted formulas use are brought, and the names those names use (a LAMBDA that
//!   calls another LAMBDA). A name that nothing pasted uses, or the cell a name stands for, is not.
//! - A name of the copied sheet (sheet scope) becomes a name of the sheet pasted into; a workbook name
//!   stays a workbook name. This also holds between two sheets of one workbook.
//! - What a name stands for is copied as written, except that references to the copied sheet follow the
//!   sheet pasted into (a name on `Sheet1!$A$1` becomes `Data!$A$1` when pasted into the sheet `Data`),
//!   and, into another workbook, references to the other sheets of the copied workbook become references
//!   to that workbook (`Sheet2!$A$1` becomes `[Source.xlsx]Sheet2!$A$1`).
//! - A name the destination can already use (a workbook name or a name of the sheet pasted into, of the
//!   same name) is a conflict, and Excel asks about it (`paste_names_dialog.rs`): はい uses the name the
//!   destination has (nothing is overwritten), いいえ asks for another name, which the pasted formulas
//!   and the names brought with them use instead, and すべて answers はい for the rest. Without an answer
//!   (none was needed) the destination's name stays.
//! - Pasting values or formats brings no names.

use ibis_doc_model::DefinedName;

/// The workbook the cells were copied from, as it was when they were copied.
#[derive(Clone, Debug)]
pub(crate) struct NameSource {
    /// Its defined names.
    pub(crate) names: Vec<DefinedName>,
    /// Its sheets' names, in order.
    pub(crate) sheet_names: Vec<String>,
    /// The sheet the cells were copied from.
    pub(crate) sheet: Option<usize>,
    /// The file the workbook was opened from: its folder (ending in `/`) and file name (None for a workbook never saved).
    pub(crate) book: Option<(String, String)>,
}

/// A name to define in the destination: name, sheet (index in the destination's sheets) when it is local,
/// and what it stands for.
pub(crate) type NewName = (String, Option<usize>, String);

/// What pasting brings with it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Brought {
    /// The names to define.
    pub(crate) names: Vec<NewName>,
    /// The names (as the source has them) the destination already has, in the order they are met.
    pub(crate) conflicts: Vec<String>,
    /// Names the answers renamed: (the source's name, the name to use here). Pasted formulas follow them
    /// (`rename_in_formula`).
    pub(crate) renames: Vec<(String, String)>,
}

/// An answer about a conflicting name: `None` uses the destination's name, `Some(new)` defines the name as `new`.
pub(crate) type Answer = (String, Option<String>);

/// `formula` with the names `renames` says renamed (`(old, new)`) written with their new names.
pub(crate) fn rename_in_formula(formula: &str, renames: &[(String, String)]) -> String {
    if renames.is_empty() {
        return formula.to_string();
    }
    let Ok(tokens) = ibis_calc::formula::lexer::tokenize(formula) else { return formula.to_string() };
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    for t in &tokens {
        let ibis_calc::formula::lexer::Token::Word(w) = &t.token else { continue };
        let bare = w.rsplit('.').next().filter(|_| w.starts_with("_xl")).unwrap_or(w);
        if let Some((_, new)) = renames.iter().find(|(old, _)| old.eq_ignore_ascii_case(bare)) {
            out.push_str(&formula[at..t.start]);
            out.push_str(&w[..w.len() - bare.len()]);
            out.push_str(new);
            at = t.end;
        }
    }
    out.push_str(&formula[at..]);
    out
}

fn is_internal(name: &str) -> bool {
    name.starts_with("_xlnm.") || name.starts_with("_xlfn.") || name.starts_with("_xlpm.")
}

/// The names to define in the destination for `formulas` pasted into the sheet `dest_sheet` (an index into
/// `dest_sheet_names`), given the destination's own names `dest_names`. `same_book` is true when the cells
/// stay in the workbook they came from.
pub(crate) fn names_to_bring(
    source: &NameSource,
    formulas: &[String],
    dest_names: &[DefinedName],
    dest_sheet_names: &[String],
    dest_sheet: usize,
    same_book: bool,
    answers: &[Answer],
) -> Brought {
    let Some(dest_sheet_name) = dest_sheet_names.get(dest_sheet) else { return Brought::default() };
    let source_sheet_name = source.sheet.and_then(|i| source.sheet_names.get(i));
    if same_book && source.sheet == Some(dest_sheet) {
        return Brought::default();
    }
    let exists = |name: &str| dest_names.iter().any(|n| n.name.eq_ignore_ascii_case(name) && (n.sheet.is_none() || n.sheet == Some(dest_sheet)));

    let mut out = Brought::default();
    // Formulas to look at, each with the sheet whose local names it may use.
    let mut work: Vec<(String, Option<usize>)> = formulas.iter().map(|f| (f.clone(), source.sheet)).collect();
    let mut seen: Vec<(String, Option<usize>)> = Vec::new();
    while let Some((formula, scope)) = work.pop() {
        for (written_sheet, word) in ibis_calc::formula::name_candidates(&formula) {
            if is_internal(&word) {
                continue;
            }
            // The name the word means in the source: a local name of the sheet written before it, else of the
            // sheet the formula is on, else a workbook name.
            let local_sheet = match &written_sheet {
                Some(s) => source.sheet_names.iter().position(|n| n.eq_ignore_ascii_case(s)),
                None => scope,
            };
            let found = local_sheet
                .and_then(|i| source.names.iter().find(|n| n.sheet == Some(i) && n.name.eq_ignore_ascii_case(&word)))
                .or_else(|| if written_sheet.is_none() { source.names.iter().find(|n| n.sheet.is_none() && n.name.eq_ignore_ascii_case(&word)) } else { None });
            let Some(found) = found else { continue };
            if found.name.starts_with("_xlnm.") || found.hidden {
                continue;
            }
            let key = (found.name.to_lowercase(), found.sheet);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            if found.sheet.is_some() && found.sheet != source.sheet {
                // A local name of another sheet (written as `Other!Name`) is not a name of this paste.
                continue;
            }
            let mut new_name = found.name.clone();
            if exists(&found.name) {
                out.conflicts.push(found.name.clone());
                match answers.iter().find(|(n, _)| n.eq_ignore_ascii_case(&found.name)) {
                    Some((_, Some(renamed))) => {
                        new_name = renamed.clone();
                        out.renames.push((found.name.clone(), renamed.clone()));
                    }
                    _ => continue,
                }
            }
            let target = found.sheet.map(|_| dest_sheet);
            let formula = retarget(&found.formula, source_sheet_name, dest_sheet_name, if same_book { None } else { source.book.as_ref() });
            work.push((found.formula.clone(), found.sheet.or(scope)));
            out.names.push((new_name, target, formula));
        }
    }
    out.names.reverse();
    for n in &mut out.names {
        n.2 = rename_in_formula(&n.2, &out.renames);
    }
    out
}

/// `formula` (what a name stands for) for the sheet it is pasted into: references to the copied sheet
/// `from` go to the sheet `to`, and, with `book` (another workbook), those to its other sheets name that
/// workbook.
pub(crate) fn retarget(formula: &str, from: Option<&String>, to: &str, book: Option<&(String, String)>) -> String {
    ibis_calc::formula::rewrite_references(formula, |area| {
        let mut area = area.clone();
        if let Some(sheet) = area.sheet.as_mut()
            && sheet.book.is_none()
        {
            if from.is_some_and(|f| f.eq_ignore_ascii_case(&sheet.first)) {
                sheet.first = to.to_string();
                if sheet.last.as_ref().is_some_and(|l| from.is_some_and(|f| f.eq_ignore_ascii_case(l))) {
                    sheet.last = Some(to.to_string());
                }
            } else if let Some((dir, file)) = book {
                sheet.book = Some(file.clone());
                sheet.path = Some(dir.clone());
            }
        }
        Some(area)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(name: &str, sheet: Option<usize>, formula: &str) -> DefinedName {
        DefinedName { name: name.into(), sheet, formula: formula.into(), hidden: false }
    }

    fn source() -> NameSource {
        NameSource {
            names: vec![
                name("MyRng", None, "Sheet1!$A$1"),
                name("Cst", None, "42"),
                name("Dbl", None, "LAMBDA(x,x*2)"),
                name("DblPlus", None, "LAMBDA(x,Dbl(x)+MyRng)"),
                name("Unused", None, "Sheet1!$A$2"),
                name("Loc", Some(0), "Sheet1!$A$2"),
            ],
            sheet_names: vec!["Sheet1".into(), "Sheet2".into()],
            sheet: Some(0),
            book: Some(("/work/".into(), "Source.xlsx".into())),
        }
    }

    fn sheets(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn bring(formulas: &[&str], dest: &[DefinedName], dest_sheets: &[&str], dest_sheet: usize, same_book: bool) -> Vec<NewName> {
        let formulas: Vec<String> = formulas.iter().map(|s| s.to_string()).collect();
        names_to_bring(&source(), &formulas, dest, &sheets(dest_sheets), dest_sheet, same_book, &[]).names
    }

    #[test]
    fn brings_the_names_a_formula_uses() {
        let got = bring(&["MyRng*2"], &[], &["Sheet1"], 0, false);
        assert_eq!(got, vec![("MyRng".to_string(), None, "Sheet1!$A$1".to_string())]);
        assert!(bring(&["1+1"], &[], &["Sheet1"], 0, false).is_empty(), "no name is used");
        assert_eq!(bring(&["Cst+1"], &[], &["Sheet1"], 0, false), vec![("Cst".to_string(), None, "42".to_string())]);
    }

    #[test]
    fn brings_the_names_a_lambda_uses() {
        let got = bring(&["DblPlus(1)"], &[], &["Sheet1"], 0, false);
        let mut names: Vec<&str> = got.iter().map(|n| n.0.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["Dbl", "DblPlus", "MyRng"]);
    }

    #[test]
    fn a_sheet_name_becomes_a_name_of_the_sheet_pasted_into() {
        let got = bring(&["Loc+1"], &[], &["Sheet1", "Data"], 1, false);
        assert_eq!(got, vec![("Loc".to_string(), Some(1), "Data!$A$2".to_string())]);
        // The same workbook: onto another sheet only.
        let got = bring(&["Loc+1"], &[], &["Sheet1", "Sheet2"], 1, true);
        assert_eq!(got, vec![("Loc".to_string(), Some(1), "Sheet2!$A$2".to_string())]);
        assert!(bring(&["Loc+1", "MyRng"], &[], &["Sheet1", "Sheet2"], 0, true).is_empty());
    }

    #[test]
    fn other_sheets_of_the_source_become_external() {
        let mut src = source();
        src.names.push(name("Mixed", None, "Sheet1!$A$1+Sheet2!$A$1"));
        let got = names_to_bring(&src, &["Mixed".to_string()], &[], &sheets(&["Data"]), 0, false, &[]).names;
        assert_eq!(got[0].2, "Data!$A$1+'/work/[Source.xlsx]Sheet2'!$A$1");
        // In the same workbook they stay.
        let got = names_to_bring(&src, &["Mixed+Loc".to_string()], &[name("Mixed", None, "x")], &sheets(&["Sheet1", "Sheet2"]), 1, true, &[]).names;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "Sheet2!$A$2");
    }

    #[test]
    fn references_follow_the_sheet_pasted_into() {
        let got = bring(&["MyRng"], &[], &["Data"], 0, false);
        assert_eq!(got[0].2, "Data!$A$1");
    }

    #[test]
    fn existing_names_stay() {
        let dest = vec![name("MyRng", None, "Sheet1!$B$2")];
        assert!(bring(&["MyRng*2"], &dest, &["Sheet1"], 0, false).is_empty());
        let local = vec![name("MyRng", Some(0), "Sheet1!$B$2")];
        assert!(bring(&["MyRng*2"], &local, &["Sheet1"], 0, false).is_empty());
        let wb = vec![name("Loc", None, "Sheet1!$B$2")];
        assert!(bring(&["Loc+1"], &wb, &["Sheet1"], 0, false).is_empty());
        // A local name of another sheet does not block.
        let other = vec![name("MyRng", Some(1), "Sheet2!$B$2")];
        assert_eq!(bring(&["MyRng*2"], &other, &["Sheet1", "Sheet2"], 0, false).len(), 1);
        // Names that the kept name stood for are not needed either.
        let dbl = vec![name("DblPlus", None, "LAMBDA(x,x)")];
        assert!(bring(&["DblPlus(1)"], &dbl, &["Sheet1"], 0, false).is_empty());
    }

    #[test]
    fn conflicts_are_reported_and_renamed() {
        let dest = vec![name("Dbl", None, "LAMBDA(x,x*3)")];
        let formulas = vec!["DblPlus(1)+Dbl(2)".to_string()];
        let none = names_to_bring(&source(), &formulas, &dest, &sheets(&["Sheet1"]), 0, false, &[]);
        assert_eq!(none.conflicts, vec!["Dbl".to_string()]);
        assert!(none.names.iter().all(|n| n.0 != "Dbl"));
        assert!(none.renames.is_empty());
        let answers = vec![("Dbl".to_string(), Some("Dbl_2".to_string()))];
        let renamed = names_to_bring(&source(), &formulas, &dest, &sheets(&["Sheet1"]), 0, false, &answers);
        assert_eq!(renamed.renames, vec![("Dbl".to_string(), "Dbl_2".to_string())]);
        let plus = renamed.names.iter().find(|n| n.0 == "DblPlus").unwrap();
        assert_eq!(plus.2, "LAMBDA(x,Dbl_2(x)+MyRng)");
        assert!(renamed.names.iter().any(|n| n.0 == "Dbl_2" && n.2 == "LAMBDA(x,x*2)"));
        assert_eq!(rename_in_formula("DblPlus(1)+Dbl(2)+Dbl_x(3)", &renamed.renames), "DblPlus(1)+Dbl_2(2)+Dbl_x(3)");
        // A kept name's own names are not looked at.
        let use_it = vec![("Dbl".to_string(), None)];
        let kept = names_to_bring(&source(), &formulas, &dest, &sheets(&["Sheet1"]), 0, false, &use_it);
        assert!(kept.names.iter().all(|n| n.0 != "Dbl" && n.0 != "Dbl_2"));
    }
}
