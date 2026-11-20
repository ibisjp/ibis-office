// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving external workbook links (`xl/externalLinks/externalLink*.xml`):
//! the cached cells an update changed are written in their `cell`s, and a
//! broken link (リンクの解除) is taken out: its reference, relationship and
//! part go, and the numbers of the links after it (`[2]` in a formula)
//! close up in every formula of the file.

use ibis_doc_model::{CellValue, ExternalBook, Presentation};
use ibis_ooxml::{Package, XmlDocument};

use crate::Error;

/// The links' books of the workbook, in order.
fn books(p: &Presentation) -> &[ExternalBook] {
    p.workbook.as_deref().map_or(&[], |w| w.external_books.as_slice())
}

/// A cached value as a link's `cell` writes it: its `t` and its `v`.
fn cell_value(v: &CellValue) -> Option<(Option<&'static str>, String)> {
    Some(match v {
        CellValue::Number(n) => (None, ibis_calc::number_to_xml(*n)),
        CellValue::Text(t) => (Some("str"), t.clone()),
        CellValue::Bool(b) => (Some("b"), u8::from(*b).to_string()),
        CellValue::Error(e) => (Some("e"), e.as_str().to_string()),
        CellValue::Empty => return None,
    })
}

const CT_EXTERNAL_LINK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml";
const REL_EXTERNAL_LINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";
const REL_EXTERNAL_LINK_PATH: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";

fn escape(s: &str) -> String {
    s.chars().filter(|&c| c == '\t' || c == '\n' || c == '\r' || c >= ' ').map(|c| match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        c => c.to_string(),
    }).collect()
}

fn external_link_xml(path_rel: &str, book: &ExternalBook) -> String {
    let names: String = book.sheets.iter().map(|s| format!("<sheetName val=\"{}\"/>", escape(&s.name))).collect();
    let mut data = String::new();
    for (id, sheet) in book.sheets.iter().enumerate() {
        let mut rows = String::new();
        let mut by_row: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
        for (&(row, column), value) in &sheet.cells {
            let Some((t, text)) = cell_value(value) else { continue };
            let at = format!("{}{}", crate::csv::column_name(column), row + 1);
            by_row.entry(row).or_default().push_str(&format!("<cell r=\"{at}\"{}><v>{}</v></cell>", t.map(|t| format!(" t=\"{t}\"")).unwrap_or_default(), escape(&text)));
        }
        for (row, cells) in by_row {
            rows.push_str(&format!("<row r=\"{}\">{cells}</row>", row + 1));
        }
        data.push_str(&format!("<sheetData sheetId=\"{id}\">{rows}</sheetData>"));
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<externalLink xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><externalBook r:id=\"{path_rel}\"><sheetNames>{names}</sheetNames><sheetDataSet>{data}</sheetDataSet></externalBook></externalLink>")
}

/// The links the file does not have yet (the model's books past the `existing` ones it was read with): each a
/// new part with its cached cells, its file as an external relationship, and an `externalReference` in the
/// workbook, numbered after the others.
fn add_new_links(package: &mut Package, workbook_part: &str, existing: usize, new: &[ExternalBook]) -> Result<(), Error> {
    for book in new.iter().skip(existing).filter(|b| !b.broken && !b.name.is_empty() && !b.sheets.is_empty()) {
        let part = crate::save::pptx::next_part_name(package, "xl/externalLinks/externalLink1.xml");
        package.set_xml(&part, XmlDocument::parse(b"<externalLink xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"/>").map_err(|e| Error::new(format!("{e}")))?);
        let path_rel = package.add_relationship(&part, REL_EXTERNAL_LINK_PATH, &book.path, true)?;
        let xml = external_link_xml(&path_rel, book);
        package.set_xml(&part, XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("{e}")))?);
        package.set_override(&part, CT_EXTERNAL_LINK)?;
        let rel = package.add_relationship(workbook_part, REL_EXTERNAL_LINK, &crate::save::pptx::relative(workbook_part, &part), false)?;
        let doc = package.xml_mut(workbook_part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
        let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
        let list = match doc.child(root, "externalReferences") {
            Some(l) => l,
            None => {
                // After sheets and functionGroups, before the defined names and the rest.
                let l = doc.insert_element(XmlDocument::ROOT, 0, &format!("{prefix}externalReferences"));
                doc.detach(l);
                let before = ["definedNames", "calcPr", "oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
                let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
                match anchor {
                    Some(a) => doc.insert_before(a, l),
                    None => {
                        let n = doc.children(root).len();
                        doc.insert(root, n, l);
                    }
                }
                l
            }
        };
        let count = doc.children(list).len();
        let reference = doc.insert_element(list, count, &format!("{prefix}externalReference"));
        doc.set_attr(reference, "r:id", &rel);
    }
    Ok(())
}

/// The cells of links whose cache changed are written again; broken links go.
pub(super) fn write_external_links(package: &mut Package, workbook_part: &str, parts: &[Option<String>], baseline: &Presentation, current: &Presentation) -> Result<(), Error> {
    let (old, new) = (books(baseline), books(current));
    add_new_links(package, workbook_part, old.len(), new)?;
    for (i, (a, b)) in old.iter().zip(new).enumerate() {
        if (a.sheets == b.sheets && a.path == b.path && a.name == b.name) || b.broken {
            continue;
        }
        let Some(part) = parts.get(i).cloned().flatten() else { continue };
        let mut path_rel_id = String::new();
        if let Ok(rels) = package.relationships(&part) {
            if let Some(rel) = rels.iter().find(|r| r.kind == REL_EXTERNAL_LINK_PATH) {
                path_rel_id = rel.id.clone();
            }
        }
        if a.path != b.path || a.name != b.name {
            if !path_rel_id.is_empty() {
                package.remove_relationship(&part, &path_rel_id).ok();
            }
            if let Ok(rel) = package.add_relationship(&part, REL_EXTERNAL_LINK_PATH, &b.path, true) {
                path_rel_id = rel;
            }
        }
        if !path_rel_id.is_empty() {
            let xml = external_link_xml(&path_rel_id, b);
            if let Ok(doc) = XmlDocument::parse(xml.as_bytes()) {
                package.set_xml(&part, doc);
                continue;
            }
        }
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for data in doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "sheetData").collect::<Vec<_>>() {
            let Some(sheet) = doc.attr(data, "sheetId").and_then(|v| v.parse::<usize>().ok()).and_then(|k| b.sheets.get(k)) else { continue };
            for cell in doc.descendants(data).into_iter().filter(|&n| doc.local_name(n) == "cell").collect::<Vec<_>>() {
                let Some((column, row)) = doc.attr(cell, "r").and_then(crate::xlsx::cell_ref) else { continue };
                let Some((t, text)) = sheet.cells.get(&(row, column)).and_then(cell_value) else { continue };
                match t {
                    Some(t) => doc.set_attr(cell, "t", t),
                    None => doc.remove_attr(cell, "t"),
                }
                if let Some(v) = doc.child(cell, "v") {
                    doc.set_text(v, &text);
                }
            }
        }
    }
    let broken: Vec<usize> = new.iter().enumerate().filter(|(i, b)| b.broken && !old.get(*i).is_some_and(|o| o.broken)).map(|(i, _)| i).collect();
    if broken.is_empty() {
        return Ok(());
    }
    remove_links(package, workbook_part, parts, &broken, new.len())
}

fn remove_links(package: &mut Package, workbook_part: &str, parts: &[Option<String>], broken: &[usize], count: usize) -> Result<(), Error> {
    // The new numbers (1-based) of the links that stay; the broken ones have none.
    let mut next = 0;
    let map: Vec<(usize, Option<usize>)> = (0..count)
        .map(|i| {
            if broken.contains(&i) {
                (i + 1, None)
            } else {
                next += 1;
                (i + 1, Some(next))
            }
        })
        .collect();
    // Their references, relationships and parts.
    let rels = package.relationships(workbook_part)?;
    let doc = package.xml_mut(workbook_part)?;
    let Some(root) = doc.root_element() else { return Ok(()) };
    let mut removed_ids = Vec::new();
    if let Some(list) = doc.child(root, "externalReferences") {
        let refs: Vec<_> = doc.children_named(list, "externalReference").collect();
        for &i in broken {
            if let Some(&r) = refs.get(i) {
                if let Some(id) = doc.attr_local(r, "id") {
                    removed_ids.push(id.to_string());
                }
                doc.detach(r);
            }
        }
        if doc.children_named(list, "externalReference").next().is_none() {
            doc.detach(list);
        }
    }
    for id in &removed_ids {
        package.remove_relationship(workbook_part, id)?;
    }
    for rel in rels.iter().filter(|r| removed_ids.contains(&r.id)) {
        if let Some(part) = rel.target_part(workbook_part) {
            package.remove_override(&part)?;
            package.remove(&part);
            package.remove(&ibis_ooxml::package::relationships_part(&part));
        }
    }
    let _ = parts;
    // The numbers in every formula of the file.
    let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/worksheets/") || n.starts_with("xl/charts/chart") || *n == workbook_part).map(str::to_string).collect();
    for name in names.into_iter().filter(|n| n.ends_with(".xml")) {
        let doc = package.xml_mut(&name)?;
        let Some(root) = doc.root_element() else { continue };
        for n in doc.descendants(root) {
            if !matches!(doc.local_name(n), "f" | "formula" | "formula1" | "formula2" | "definedName") {
                continue;
            }
            let text = doc.text(n);
            if text.contains('[') && ibis_calc::formula::has_external_book(&text) {
                let new = ibis_calc::formula::renumber_books(&text, &map);
                if new != text {
                    doc.set_text(n, &new);
                }
            }
        }
    }
    Ok(())
}
