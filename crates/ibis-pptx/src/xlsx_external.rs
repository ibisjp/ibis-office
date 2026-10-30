// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading an external workbook link (`xl/externalLinks/externalLink*.xml`,
//! ECMA-376 Part 1 §18.14): the file's name for formulas to show
//! (`[売上.xlsx]Sheet1!A1`) and the values of its cells the link caches
//! (`sheetDataSet`), which the formulas read while the workbook is not open.

use ibis_doc_model::{CellError, CellValue, ExternalBook, ExternalSheet};
use roxmltree::Node;

use crate::xml::{attr, child, children};

/// The name of the file a link's target names, as Excel writes it in a
/// formula: its last path part, percent-decoded (`file:///C:/data/%E5%A3%B2.xlsx`
/// is `売上.xlsx`).
pub(crate) fn book_name(target: &str) -> String {
    let last = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let last = last.strip_prefix('[').and_then(|l| l.split_once(']').map(|(_, rest)| rest)).unwrap_or(last);
    percent_decode(last)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// A link part's root, with the target of its file (`None` for a link
/// that is not a workbook's).
pub(crate) fn read_link(root: Node, target: Option<&str>) -> ExternalBook {
    let Some(book) = child(root, "externalBook") else { return ExternalBook::default() };
    let mut sheets: Vec<ExternalSheet> = child(book, "sheetNames")
        .into_iter()
        .flat_map(|n| children(n, "sheetName"))
        .map(|n| ExternalSheet { name: attr(n, "val").unwrap_or("").to_string(), cells: Default::default() })
        .collect();
    if let Some(set) = child(book, "sheetDataSet") {
        for data in children(set, "sheetData") {
            let Some(sheet) = attr(data, "sheetId").and_then(|v| v.parse::<usize>().ok()).and_then(|i| sheets.get_mut(i)) else { continue };
            for row in children(data, "row") {
                for cell in children(row, "cell") {
                    let Some((column, r)) = attr(cell, "r").and_then(crate::xlsx::cell_ref) else { continue };
                    let Some(v) = child(cell, "v").and_then(|v| v.text()) else { continue };
                    let value = match attr(cell, "t") {
                        Some("str") => CellValue::Text(v.to_string()),
                        Some("b") => CellValue::Bool(v.trim() == "1" || v.trim().eq_ignore_ascii_case("true")),
                        Some("e") => CellValue::Error(CellError::parse(v.trim()).unwrap_or(CellError::Value)),
                        _ => match v.trim().parse::<f64>() {
                            Ok(n) => CellValue::Number(n),
                            Err(_) => continue,
                        },
                    };
                    sheet.cells.insert((r, column), value);
                }
            }
        }
    }
    ExternalBook { name: target.map(book_name).unwrap_or_default(), path: target.unwrap_or("").to_string(), sheets, broken: false }
}

/// The target of a link as a person reads it: percent-decoded, without `file:///`.
pub fn link_display(target: &str) -> String {
    percent_decode(target.strip_prefix("file:///").unwrap_or(target))
}

/// The file a link names, as a path to read: the target as written, with a
/// `file:///` prefix dropped and percent-decoded; a relative one is taken
/// from `dir` (the folder of the workbook that links to it).
pub fn link_path(target: &str, dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    if target.is_empty() {
        return None;
    }
    let decoded = percent_decode(target.strip_prefix("file:///").map(|t| if t.starts_with('/') || t.chars().nth(1) == Some(':') { t.to_string() } else { format!("/{t}") }).as_deref().unwrap_or(target));
    let path = std::path::PathBuf::from(&decoded);
    if path.is_absolute() { Some(path) } else { dir.map(|d| d.join(path)) }
}

/// The cached sheets of `book` made again from the workbook file `data`:
/// each cached cell takes the value of the same cell of the sheet of that
/// name there (empty when it is not there any more); a sheet the file has
/// no more keeps what it had.
pub fn refreshed_sheets(book: &ExternalBook, data: &[u8]) -> Result<Vec<ExternalSheet>, crate::Error> {
    let opened = crate::open(data.to_vec())?;
    Ok(book
        .sheets
        .iter()
        .map(|cached| {
            let found = opened.presentation.slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&cached.name)));
            let Some(sheet) = found.and_then(ibis_doc_model::sheet_calc::sheet_of_slide) else { return cached.clone() };
            ExternalSheet { name: cached.name.clone(), cells: cached.cells.keys().map(|&(r, c)| ((r, c), sheet.cell(r, c).map(|x| x.value.clone()).unwrap_or(CellValue::Empty))).collect() }
        })
        .collect())
}

/// A link to a workbook a formula names that the file does not link yet: `name` is the file's name as the
/// formula writes it (`Budget.xlsx`, kept as the link's path: a name beside the workbook). With the file's
/// bytes `data` (when it was found) the link knows its sheets and caches the cells `formula` reads of it
/// (the areas named in `[name]…`, at most 10,000 cells each); without them it has no sheet (its cells are `#REF!`).
pub fn new_external_book(name: &str, data: Option<&[u8]>, formula: &str) -> ExternalBook {
    let (name_only, path) = if let Some(p) = name.find('[') && let Some(e) = name[p + 1..].find(']') {
        let dir = &name[..p];
        let file = &name[p + 1..p + 1 + e];
        let path = if dir.is_empty() { file.to_string() } else { format!("{dir}{file}") };
        (file.to_string(), path)
    } else {
        (name.to_string(), name.to_string())
    };
    let mut book = ExternalBook { name: name_only.clone(), path, sheets: Vec::new(), broken: false };
    let Some(opened) = data.and_then(|d| crate::open(d.to_vec()).ok()) else { return book };
    let sheets: Vec<(String, std::sync::Arc<ibis_doc_model::Sheet>)> = opened
        .presentation
        .slides
        .iter()
        .filter_map(|s| Some((s.name.clone()?, ibis_doc_model::sheet_calc::sheet_of_slide(s)?.clone())))
        .collect();
    book.sheets = sheets.iter().map(|(n, _)| ExternalSheet { name: n.clone(), cells: Default::default() }).collect();
    ibis_calc::formula::rewrite_references(formula, |area| {
        let Some(sheet_ref) = area.sheet.as_ref().filter(|s| {
            s.book.as_deref().is_some_and(|b| {
                b.eq_ignore_ascii_case(name) || b.eq_ignore_ascii_case(&name_only)
            })
        }) else { return Some(area.clone()) };
        let Some(i) = sheets.iter().position(|(n, _)| n.eq_ignore_ascii_case(&sheet_ref.first)) else { return Some(area.clone()) };
        let end_sheet = match &sheet_ref.last {
            Some(l) => sheets.iter().position(|(n, _)| n.eq_ignore_ascii_case(l)).unwrap_or(i),
            None => i,
        };
        let (first_s, last_s) = (i.min(end_sheet), i.max(end_sheet));
        for s_idx in first_s..=last_s {
            let (rows, columns) = (area.start.row.min(area.end.row)..=area.start.row.max(area.end.row).min(area.start.row.min(area.end.row) + 9_999), area.start.column.min(area.end.column)..=area.start.column.max(area.end.column).min(area.start.column.min(area.end.column) + 99));
            for r in rows {
                for c in columns.clone() {
                    if let Some(cell) = sheets[s_idx].1.cell(r, c)
                        && !cell.value.is_empty()
                    {
                        book.sheets[s_idx].cells.insert((r, c), cell.value.clone());
                    }
                }
            }
        }
        Some(area.clone())
    });
    book
}

/// Reads an external workbook from `data`: its sheets and non-empty cells up to 10,000 cells per sheet.
pub fn read_external_book(name: &str, path: &str, data: &[u8]) -> Result<ExternalBook, crate::Error> {
    let opened = crate::open(data.to_vec())?;
    let mut book = ExternalBook {
        name: name.to_string(),
        path: path.to_string(),
        sheets: Vec::new(),
        broken: false,
    };
    for (i, slide) in opened.presentation.slides.iter().enumerate() {
        let Some(sheet) = ibis_doc_model::sheet_calc::sheet_of_slide(slide) else { continue };
        let sheet_name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", i + 1));
        let mut cells = std::collections::BTreeMap::new();
        for cell in &sheet.cells {
            if !cell.value.is_empty() && cells.len() < 10_000 {
                cells.insert((cell.row, cell.column), cell.value.clone());
            }
        }
        book.sheets.push(ExternalSheet { name: sheet_name, cells });
    }
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_gives_its_name_and_cached_cells() {
        let xml = r#"<externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><externalBook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"><sheetNames><sheetName val="Data"/><sheetName val="Plan"/></sheetNames><sheetDataSet><sheetData sheetId="0"><row r="1"><cell r="A1"><v>1.5</v></cell><cell r="B1" t="str"><v>名前</v></cell></row><row r="3"><cell r="A3" t="b"><v>1</v></cell><cell r="C3" t="e"><v>#N/A</v></cell></row></sheetData><sheetData sheetId="1"><row r="2"><cell r="B2"><v>7</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let book = read_link(doc.root_element(), Some("file:///C:/data/%E5%A3%B2%E4%B8%8A.xlsx"));
        assert_eq!(book.name, "売上.xlsx");
        assert_eq!(book.sheets.len(), 2);
        assert_eq!(book.sheets[0].cells[&(0, 0)], CellValue::Number(1.5));
        assert_eq!(book.sheets[0].cells[&(0, 1)], CellValue::Text("名前".into()));
        assert_eq!(book.sheets[0].cells[&(2, 0)], CellValue::Bool(true));
        assert_eq!(book.sheets[0].cells[&(2, 2)], CellValue::Error(CellError::NA));
        assert_eq!(book.sheets[1].cells[&(1, 1)], CellValue::Number(7.0));
        // An absolute path starts with `/` on Unix and with a drive on Windows.
        let (root, docs) = if cfg!(windows) { ("C:/Users/x", "C:/docs") } else { ("/Users/x", "/docs") };
        assert_eq!(link_path(&format!("file:///{}/%E5%A3%B2.xlsx", root.trim_start_matches('/')), None), Some(std::path::PathBuf::from(format!("{root}/売.xlsx"))));
        assert_eq!(link_path("sub/b.xlsx", Some(std::path::Path::new(docs))), Some(std::path::Path::new(docs).join("sub/b.xlsx")));
        assert_eq!(link_path("b.xlsx", None), None);
        assert_eq!(book_name("Book2.xlsx"), "Book2.xlsx");
        assert_eq!(book_name(r"C:\dir\Book 3.xlsx"), "Book 3.xlsx");
        assert_eq!(book_name("[1]Book4.xlsx"), "Book4.xlsx");
    }
}
