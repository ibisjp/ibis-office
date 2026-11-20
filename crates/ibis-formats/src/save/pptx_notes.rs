// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スピーカー ノート: a slide's notes written into its notes page (the
//! body placeholder's paragraphs), or a new notes page made for it from
//! the presentation's notes master.

use ibis_ooxml::{Package, XmlDocument};

use super::pptx::{next_part_name, relative};
use super::pptx_new::{Prefixes, escape};
use crate::Error;
use crate::save::unsupported;

const REL_NOTES_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
const REL_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const CT_NOTES: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";

/// The part a relationship of kind `kind` from `source` points at.
fn related(package: &Package, source: &str, kind: &str) -> Result<Option<String>, Error> {
    Ok(package.relationships(source)?.into_iter().find(|r| r.kind == kind).and_then(|r| r.target_part(source)))
}

/// The paragraphs of `notes` (`\n` between them), with prefix `a`.
fn paragraphs_xml(a: &str, notes: &str) -> String {
    notes
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                format!("<{a}p><{a}endParaRPr lang=\"ja-JP\" altLang=\"en-US\" dirty=\"0\"/></{a}p>")
            } else {
                format!("<{a}p><{a}r><{a}rPr lang=\"ja-JP\" altLang=\"en-US\" dirty=\"0\"/><{a}t>{}</{a}t></{a}r></{a}p>", escape(line))
            }
        })
        .collect()
}

/// Write `notes` as the speaker notes of slide `slide` (a part of the
/// presentation `presentation`).
pub(crate) fn write_notes(package: &mut Package, presentation: &str, slide: &str, notes: &str, marks: &ibis_doc_model::NotesMarks, number: usize) -> Result<(), Error> {
    match related(package, slide, REL_NOTES_SLIDE)? {
        Some(part) => rewrite_notes(package, &part, notes),
        None if notes.is_empty() => Ok(()),
        None => {
            new_notes(package, presentation, slide, notes)?;
            // A new notes page carries what the others do.
            match related(package, slide, REL_NOTES_SLIDE)? {
                Some(part) if !marks.is_empty() => sync_marks(package, &part, marks, number),
                _ => Ok(()),
            }
        }
    }
}

/// The header, date, footer and number of slide `slide`'s notes page (made if there is none and some are wanted).
pub(crate) fn write_marks(package: &mut Package, presentation: &str, slide: &str, marks: &ibis_doc_model::NotesMarks, number: usize) -> Result<(), Error> {
    let part = match related(package, slide, REL_NOTES_SLIDE)? {
        Some(part) => part,
        None if marks.is_empty() => return Ok(()),
        None => {
            new_notes(package, presentation, slide, "")?;
            related(package, slide, REL_NOTES_SLIDE)?.ok_or_else(|| Error::new("ノートを作れません"))?
        }
    };
    sync_marks(package, &part, marks, number)
}

/// A paragraph of `text`, a field of `field` (`slidenum`, `datetime1`) if there is one.
pub(crate) fn mark_paragraph(a: &str, text: &str, field: Option<&str>) -> String {
    const RPR: &str = "lang=\"ja-JP\" altLang=\"en-US\"";
    match field {
        Some("slidenum") => format!("<{a}p><{a}fld id=\"{{8E16B1EE-AF2B-9545-B564-41C11E7F89AC}}\" type=\"slidenum\"><{a}rPr {RPR}/><{a}t>{}</{a}t></{a}fld><{a}endParaRPr {RPR}/></{a}p>", escape(text)),
        Some(kind) => format!("<{a}p><{a}fld id=\"{{A38FE804-A0E7-5143-B38B-D998582213EC}}\" type=\"{kind}\"><{a}rPr {RPR}/><{a}t>{}</{a}t></{a}fld><{a}endParaRPr {RPR}/></{a}p>", escape(text)),
        None if text.is_empty() => format!("<{a}p><{a}endParaRPr {RPR}/></{a}p>"),
        None => format!("<{a}p><{a}r><{a}rPr {RPR}/><{a}t>{}</{a}t></{a}r></{a}p>", escape(text)),
    }
}

/// Notes page `part`'s header, date, footer and number placeholders made as `marks` say:
/// text put in those there, the others made (as the notes master has them) or taken away.
fn sync_marks(package: &mut Package, part: &str, marks: &ibis_doc_model::NotesMarks, number: usize) -> Result<(), Error> {
    use ibis_doc_model::NotesDate;
    // The notes master's placeholders: their `idx` and `sz`.
    let mut master_ph: std::collections::HashMap<String, (Option<String>, Option<String>)> = std::collections::HashMap::new();
    if let Some(master) = related(package, part, REL_NOTES_MASTER)?
        && let Ok(mdoc) = package.xml(&master)
        && let Some(root) = mdoc.root_element()
        && let Some(tree) = mdoc.path(root, &["cSld", "spTree"])
    {
        for sp in mdoc.children_named(tree, "sp") {
            if let Some(ph) = mdoc.path(sp, &["nvSpPr", "nvPr", "ph"])
                && let Some(kind) = mdoc.attr(ph, "type")
            {
                master_ph.insert(kind.to_string(), (mdoc.attr(ph, "idx").map(str::to_string), mdoc.attr(ph, "sz").map(str::to_string)));
            }
        }
    }
    let number_text = number.to_string();
    // (kind, name, wanted: text and field)
    let wanted: [(&str, &str, Option<(String, Option<&str>)>); 4] = [
        ("hdr", "ヘッダー プレースホルダー", marks.header.clone().map(|t| (t, None))),
        ("dt", "日付プレースホルダー", marks.date.clone().map(|d| match d {
            NotesDate::Auto(t) => (t, Some("datetime1")),
            NotesDate::Fixed(t) => (t, None),
        })),
        ("ftr", "フッター プレースホルダー", marks.footer.clone().map(|t| (t, None))),
        ("sldNum", "スライド番号プレースホルダー", marks.number.then(|| (number_text.clone(), Some("slidenum")))),
    ];
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let tree = doc.path(root, &["cSld", "spTree"]).ok_or_else(|| unsupported("この形のノート"))?;
    for (kind, name, want) in wanted {
        let existing = doc.children_named(tree, "sp").find(|&sp| doc.path(sp, &["nvSpPr", "nvPr", "ph"]).is_some_and(|ph| doc.attr(ph, "type") == Some(kind)));
        match (existing, want) {
            (Some(sp), None) => doc.detach(sp),
            (Some(sp), Some((text, field))) => {
                let Some(body) = doc.child(sp, "txBody") else { continue };
                let old: Vec<_> = doc.children_named(body, "p").collect();
                for p in old {
                    doc.detach(p);
                }
                let x = Prefixes::at(doc, body);
                let wrapper = format!("<{a}txBody>{}</{a}txBody>", mark_paragraph(&x.a, &text, field), a = x.a);
                let new_body = x.import(doc, &wrapper)?;
                for p in doc.children(new_body).to_vec() {
                    if doc.is_element(p) {
                        let end = doc.children(body).len();
                        doc.insert(body, end, p);
                    }
                }
            }
            (None, Some((text, field))) => {
                let id = doc.descendants(root).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id").and_then(|v| v.parse::<u64>().ok())).max().unwrap_or(1) + 1;
                let (idx, sz) = master_ph.get(kind).cloned().unwrap_or_default();
                let attrs = format!("type=\"{kind}\"{}{}", sz.map(|v| format!(" sz=\"{v}\"")).unwrap_or_default(), idx.map(|v| format!(" idx=\"{v}\"")).unwrap_or_default());
                let x = Prefixes::at(doc, tree);
                let xml = format!(
                    "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id}\" name=\"{name} {id}\"/><{p}cNvSpPr><{a}spLocks noGrp=\"1\"/></{p}cNvSpPr><{p}nvPr><{p}ph {attrs}/></{p}nvPr></{p}nvSpPr><{p}spPr/><{p}txBody><{a}bodyPr/><{a}lstStyle/>{}</{p}txBody></{p}sp>",
                    mark_paragraph(&x.a, &text, field),
                    p = x.p,
                    a = x.a
                );
                let sp = x.import(doc, &xml)?;
                let index = doc.children(tree).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tree).len());
                doc.insert(tree, index, sp);
            }
            (None, None) => {}
        }
    }
    Ok(())
}

/// The text of paragraph `p` (a line break is `\n`).
fn paragraph_text(doc: &XmlDocument, p: ibis_ooxml::NodeId) -> String {
    doc.children(p)
        .iter()
        .filter(|&&n| doc.is_element(n))
        .map(|&n| match doc.local_name(n) {
            "r" | "fld" => doc.child(n, "t").map(|t| doc.text(t)).unwrap_or_default(),
            "br" => "\n".to_string(),
            _ => String::new(),
        })
        .collect()
}

/// For each of `lines`, the old paragraph it keeps: a longest common
/// subsequence of the unchanged lines (a paragraph holding a line break
/// is never kept).
fn align(old: &[String], lines: &[&str]) -> Vec<Option<usize>> {
    let (n, m) = (old.len(), lines.len());
    let same = |i: usize, j: usize| old[i] == lines[j] && !old[i].contains('\n');
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if same(i, j) { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let mut kept = vec![None; m];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same(i, j) {
            kept[j] = Some(i);
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    kept
}

/// Replace the paragraphs of notes page `part`'s body placeholder.
fn rewrite_notes(package: &mut Package, part: &str, notes: &str) -> Result<(), Error> {
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let tree = doc.path(root, &["cSld", "spTree"]).ok_or_else(|| unsupported("この形のノート"))?;
    let body = doc
        .children_named(tree, "sp")
        .filter(|&sp| doc.path(sp, &["nvSpPr", "nvPr", "ph"]).is_some_and(|ph| doc.attr(ph, "type") == Some("body")))
        .collect::<Vec<_>>()
        .into_iter()
        .find_map(|sp| doc.child(sp, "txBody").map(|b| (sp, b)));
    let Some((_, body)) = body else {
        return Err(unsupported("本文のプレースホルダーの無いノート"));
    };
    let old: Vec<_> = doc.children_named(body, "p").collect();
    let old_text: Vec<String> = old.iter().map(|&p| paragraph_text(doc, p)).collect();
    let lines: Vec<&str> = notes.split('\n').collect();
    // Paragraphs whose text stays keep their runs (bold, size, colour...).
    let kept = align(&old_text, &lines);
    for &p in &old {
        doc.detach(p);
    }
    let x = Prefixes::at(doc, body);
    for (line, keep) in lines.iter().zip(kept) {
        let node = match keep {
            Some(i) => old[i],
            None => {
                let wrapper = format!("<{a}txBody>{}</{a}txBody>", paragraphs_xml(&x.a, line), a = x.a);
                let new_body = x.import(doc, &wrapper)?;
                let Some(p) = doc.children(new_body).iter().copied().find(|&n| doc.is_element(n)) else { continue };
                p
            }
        };
        let end = doc.children(body).len();
        doc.insert(body, end, node);
    }
    Ok(())
}

/// Make a notes page for slide `slide` holding `notes`.
pub(crate) fn new_notes(package: &mut Package, presentation: &str, slide: &str, notes: &str) -> Result<(), Error> {
    let master = related(package, presentation, REL_NOTES_MASTER)?.ok_or_else(|| unsupported("ノートのマスターの無いファイルへのノートの追加"))?;
    let part = next_part_name(package, "ppt/notesSlides/notesSlide1.xml");
    let xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
            "<p:notes xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
            "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">",
            "<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>",
            "<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>",
            "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"スライド イメージ プレースホルダー 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr>",
            "<p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp>",
            "<p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"ノート プレースホルダー 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>",
            "<p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/>",
            "<p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>",
            "</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"
        ),
        paragraphs = paragraphs_xml("a:", notes)
    );
    let doc = XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("ノートを作れません: {e}")))?;
    package.set_xml(&part, doc);
    package.set_override(&part, CT_NOTES)?;
    package.add_relationship(&part, REL_NOTES_MASTER, &relative(&part, &master), false)?;
    package.add_relationship(&part, REL_SLIDE, &relative(&part, slide), false)?;
    package.add_relationship(slide, REL_NOTES_SLIDE, &relative(slide, &part), false)?;
    Ok(())
}
