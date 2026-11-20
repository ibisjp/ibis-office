// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ヘッダーとフッター's marks for the whole presentation, as PowerPoint's
//! すべてに適用 writes them: `showSpecialPlsOnTitleSld="0"` on
//! `p:presentation` (タイトル スライドに表示しない), and each master's `p:hf`
//! naming the kinds not shown (`hdr="0" dt="0"`; a kind shown is left
//! unstated, and with every kind shown there is no `p:hf`).

use ibis_doc_model::model::HeaderFooterMarks;
use ibis_ooxml::Package;

use super::pptx_new::Prefixes;
use crate::Error;

const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";

/// What follows `p:hf` in a master (`p:txStyles`, `p:extLst`).
const AFTER_HF: [&str; 2] = ["txStyles", "extLst"];

pub(crate) fn write(package: &mut Package, presentation: &str, marks: HeaderFooterMarks) -> Result<(), Error> {
    let masters: Vec<String> = package.relationships(presentation)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER && !r.external).filter_map(|r| r.target_part(presentation)).collect();
    let doc = package.xml_mut(presentation)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    if marks.on_title_slides {
        doc.remove_attr(root, "showSpecialPlsOnTitleSld");
    } else {
        doc.set_attr(root, "showSpecialPlsOnTitleSld", "0");
    }
    let kinds = [("hdr", marks.header), ("dt", marks.date), ("ftr", marks.footer), ("sldNum", marks.number)];
    for master in masters {
        if !package.contains(&master) {
            continue;
        }
        let doc = package.xml_mut(&master)?;
        let Some(root) = doc.root_element() else { continue };
        let existing = doc.child(root, "hf");
        if kinds.iter().all(|(_, shown)| *shown) {
            if let Some(hf) = existing {
                doc.detach(hf);
            }
            continue;
        }
        let hf = match existing {
            Some(hf) => hf,
            None => {
                let x = Prefixes::at(doc, root);
                let hf = x.import(doc, &format!("<{}hf/>", x.p))?;
                let next = doc.elements(root).find(|&n| AFTER_HF.contains(&doc.local_name(n)));
                if let Some(next) = next {
                    doc.insert_before(next, hf);
                } else {
                    let end = doc.children(root).len();
                    doc.insert(root, end, hf);
                }
                hf
            }
        };
        for (name, shown) in kinds {
            if shown {
                doc.remove_attr(hf, name);
            } else {
                doc.set_attr(hf, name, "0");
            }
        }
    }
    Ok(())
}
