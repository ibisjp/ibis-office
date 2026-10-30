// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing a shape's text whose paragraphs or run formats changed (see
//! `pptx_shapes.rs`; text typed inside runs is written in place there).
//! The presentation担当's file.
//!
//! The paragraphs (`a:p`) of the text body are written again from the
//! model, each from a template in the original: a paragraph takes a copy
//! of the `a:pPr` of the original paragraph with the same format, a run a
//! copy of the `a:r` (or `a:fld`) of an original run with the same format,
//! so what the model does not know (languages, spelling marks, extensions)
//! stays. Where no original run has the format, the nearest one is copied
//! and the attributes that differ are written.

use ibis_doc_model::{LineSpacing, Paragraph, Rgb, RunStyle, TextAlign, TextBody, TextStyle};
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx::ShapeSource;
use super::pptx_new::{self as new, Prefixes};
use super::unsupported;
use crate::Error;

/// A run's format as it shows: the body's style with the run's overrides.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    style: TextStyle,
    underline: bool,
    underline_kind: ibis_doc_model::Underline,
    highlight: Option<Rgb>,
    outline: Option<(Rgb, i64)>,
    baseline: i32,
    spacing: i32,
    field: Option<ibis_doc_model::TextField>,
    strike: ibis_doc_model::Strike,
    caps: ibis_doc_model::Caps,
    link: Option<ibis_doc_model::SlideLink>,
    link_tooltip: Option<String>,
    shadow: Option<ibis_doc_model::Shadow>,
    lang: Option<String>,
}

fn look(body: &TextStyle, run: &RunStyle) -> Look {
    let style = TextStyle { align: TextAlign::Left, ..body.with_run(run) };
    Look { style, underline: run.underline, underline_kind: run.underline_kind, highlight: run.highlight, outline: run.outline, shadow: run.shadow, baseline: run.baseline, spacing: run.spacing, field: run.field, strike: run.strike, caps: run.caps, link: run.link.clone(), link_tooltip: run.link_tooltip.clone(), lang: run.lang.clone() }
}

/// A paragraph's format as it shows.
fn para_key(body: &TextStyle, p: &Paragraph) -> Paragraph {
    Paragraph { runs: Vec::new(), align: Some(p.align.unwrap_or(body.align)), end_style: RunStyle::default(), ..p.clone() }
}

/// A paragraph's format without what a writer sets (the alignment) and
/// what follows from where a line is in its `a:p` (space above the first
/// line, below the last).
fn relax(key: &Paragraph) -> Paragraph {
    Paragraph { align: None, space_before: 0, space_after: 0, line_spacing: None, line_break: false, ..key.clone() }
}

struct OldParagraph {
    key: Paragraph,
    p: NodeId,
    runs: Vec<(Look, NodeId)>,
    run_lengths: Vec<usize>,
    /// The formulas (`a14:m` or the wrapper around it) of the paragraph's runs.
    maths: Vec<(ibis_doc_model::math::Math, NodeId)>,
    end: Look,
}

pub(crate) fn rewrite_text(doc: &mut XmlDocument, node: NodeId, source: &ShapeSource, old: &TextBody, new: &TextBody) -> Result<(), Error> {
    let body = doc.child(node, "txBody").ok_or_else(|| unsupported("この図形の文字の変更"))?;
    if source.paragraphs.len() != old.paragraphs.len() {
        return Err(unsupported("この図形の段落の変更"));
    }
    if old.paragraphs == new.paragraphs && old.style == new.style {
        return write_body_properties(doc, body, old, new);
    }
    // The original paragraphs and runs, found while the paths hold.
    let mut olds: Vec<OldParagraph> = Vec::new();
    for (para, src) in old.paragraphs.iter().zip(&source.paragraphs) {
        let p = doc.by_path(&src.p).filter(|&n| doc.local_name(n) == "p").ok_or_else(|| Error::new("文字の段落の位置が見つかりません"))?;
        let mut runs = Vec::new();
        let mut run_lengths = Vec::new();
        let mut maths = Vec::new();
        for (run, path) in para.runs.iter().zip(&src.runs) {
            if let Some(math) = &run.style.math {
                if let Some(n) = doc.by_path(path).filter(|&n| matches!(doc.local_name(n), "AlternateContent" | "m")) {
                    maths.push(((**math).clone(), n));
                    runs.push((look(&old.style, &run.style), n));
                    run_lengths.push(run.text.chars().count());
                }
            } else if let Some(r) = doc.by_path(path).filter(|&n| matches!(doc.local_name(n), "r" | "fld")) {
                runs.push((look(&old.style, &run.style), r));
                run_lengths.push(run.text.chars().count());
            }
        }
        olds.push(OldParagraph { key: para_key(&old.style, para), p, runs, run_lengths, maths, end: look(&old.style, &para.end_style) });
    }
    if olds.is_empty() {
        return Err(unsupported("この図形の段落の変更"));
    }
    // Line spacing shrunk to fit (`normAutofit/@lnSpcReduction`): the model
    // has it taken off, the file does not.
    let reduction: u32 = doc
        .child(body, "bodyPr")
        .and_then(|b| doc.elements(b).find(|&n| doc.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main") && doc.local_name(n)=="normAutofit"))
        .and_then(|f| doc.attr(f, "lnSpcReduction"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let x = Prefixes::at(doc, body);
    let mut written: Vec<NodeId> = Vec::new();
    for (i, para) in new.paragraphs.iter().enumerate() {
        let same_text = old.paragraphs.get(i).is_some_and(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>() == para.runs.iter().map(|r| r.text.as_str()).collect::<String>());
        let key = para_key(&new.style, para);
        // The template: the original paragraph at this place if it has the
        // format, else the first that has it, else the one at this place.
        let at = i.min(olds.len() - 1);
        let near = |f: &dyn Fn(&OldParagraph) -> bool| -> Option<usize> {
            // The nearest to this place.
            (0..olds.len()).filter(|&j| f(&olds[j])).min_by_key(|&j| j.abs_diff(at))
        };
        let relaxed = relax(&key);
        // Formatting a paragraph must keep its own opaque pPr, not a different
        // paragraph whose modeled formatting happens to match the new values.
        let t = if same_text { at } else { near(&|o| o.key == key).or_else(|| near(&|o| relax(&o.key) == relaxed)).unwrap_or(at) };
        let template = &olds[t];
        // A line after a line break of the same paragraph.
        let continues = para.line_break && !written.is_empty();
        let p = if continues {
            let p = *written.last().expect("a paragraph");
            if let Some(end) = doc.child(p, "endParaRPr") {
                doc.detach(end);
            }
            let br = doc.append_element(p, &format!("{}br", x.a));
            if i > 0 && new.paragraphs[i - 1].runs.is_empty() {
                let prev_end = look(&new.style, &new.paragraphs[i - 1].end_style);
                let rpr = x.import(doc, &run_props(&x, &prev_end, ""))?;
                doc.insert(br, 0, rpr);
            } else if let Some(rpr) = template.runs.first().and_then(|(_, r)| doc.child(*r, "rPr")) {
                let copy = doc.deep_clone(rpr);
                doc.insert(br, 0, copy);
            }
            p
        } else {
            let p = doc.append_element(body, doc.name(template.p).to_string().as_str());
            doc.detach(p);
            if let Some(ppr) = doc.child(template.p, "pPr") {
                let copy = doc.deep_clone(ppr);
                doc.insert(p, 0, copy);
            }
            // The space below a paragraph is its last line's.
            let group_after = |keys: &mut dyn Iterator<Item = (bool, u32)>| keys.take_while(|(line_break, _)| *line_break).last().map(|(_, a)| a);
            // What the copied `a:pPr` gives: its first line's format.
            let mut have = olds.iter().find(|o| o.p == template.p).map_or_else(|| template.key.clone(), |o| o.key.clone());
            if let Some(a) = group_after(&mut olds[t + 1..].iter().map(|o| (o.p == template.p, o.key.space_after))) {
                have.space_after = a;
            }
            let mut want = key.clone();
            if let Some(a) = group_after(&mut new.paragraphs[i + 1..].iter().map(|q| (q.line_break, q.space_after))) {
                want.space_after = a;
            }
            write_paragraph_format(doc, p, &x, &have, &want, reduction)?;
            written.push(p);
            p
        };
        // Keep source boundaries in the serialized XML even if equal modeled
        // styles were coalesced. One XML rPr cannot represent two opaque origins.
        let mut boundaries = Vec::new();
        if same_text {
            if let Some(p) = olds.get(i) {
                let mut at = 0;
                for length in &p.run_lengths { at += length; boundaries.push(at); }
            }
        }
        let mut runs = Vec::new();
        let mut at = 0;
        for run in &para.runs {
            if run.style.field.is_some() || run.style.math.is_some() || run.text.is_empty() {
                at += run.text.chars().count(); runs.push(run.clone()); continue;
            }
            let mut piece = run.clone(); piece.text.clear();
            for ch in run.text.chars() {
                piece.text.push(ch); at += 1;
                if boundaries.binary_search(&at).is_ok() { runs.push(piece.clone()); piece.text.clear(); }
            }
            if !piece.text.is_empty() { runs.push(piece); }
        }
        let mut offset = 0;
        for run in &runs {
            let start = offset;
            offset += run.text.chars().count();
            if run.text.is_empty() && run.style.field.is_none() {
                continue;
            }
            // A formula that is as it was is the original markup.
            if let Some(math) = &run.style.math
                && let Some(&(_, node)) = olds.iter().flat_map(|o| o.maths.iter()).find(|(m, _)| m == &**math)
            {
                let copy = doc.deep_clone(node);
                let index = doc.children(p).len();
                doc.insert(p, index, copy);
                continue;
            }
            // A formula that was typed again is written as a formula.
            if let Some(math) = &run.style.math {
                let copy = x.import(doc, &crate::math_xml::write_math_content(math, &x.a, run.style.math_inline))?;
                let index = doc.children(p).len();
                doc.insert(p, index, copy);
                continue;
            }
            let want = look(&new.style, &run.style);
            // A format-only split has an unambiguous source character range.
            // Prefer that source even when another run has the same modeled look:
            // their languages, complex-script properties or extensions may differ.
            let origin = if same_text { olds.get(i).and_then(|p| {
                let mut at = 0;
                p.runs.iter().zip(&p.run_lengths).find(|(_, length)| {
                    // The reader can split one XML run by script, repeating its
                    // source path; offsets follow model segments, not XML text.
                    let end = at + **length;
                    let contains = start >= at && start < end && offset <= end;
                    at = end;
                    contains
                }).map(|(run, _)| run).filter(|(_, r)| matches!(doc.local_name(*r), "r" | "fld"))
            }) } else { None };
            let exact = match origin {
                Some((l, r)) => (*l == want).then_some(*r),
                None => template.runs.iter().chain(olds.iter().flat_map(|o| o.runs.iter())).find(|(l, r)| *l == want && matches!(doc.local_name(*r), "r" | "fld")).map(|(_, r)| *r),
            };
            let r = match exact {
                Some(r) => doc.deep_clone(r),
                None => {
                    // A field from a field of the same kind, text from text.
                    let base = origin.or_else(|| template.runs.iter().chain(olds.iter().flat_map(|o| o.runs.iter())).find(|(l, r)| {
                        l.field == want.field && (want.field.is_some() || doc.local_name(*r) == "r")
                    }));
                    match base {
                        Some((base_look, base_node)) => {
                            let (base_look, base_node) = (base_look.clone(), *base_node);
                            let r = doc.deep_clone(base_node);
                            write_run_format(doc, r, &x, &base_look, &want, &run.text)?;
                            r
                        }
                        None if want.field.is_some() => return Err(unsupported("自動で入る文字（スライド番号など）の追加")),
                        None => {
                            let xml = format!("<{a}r>{}<{a}t/></{a}r>", run_props(&x, &want, &run.text), a = x.a);
                            x.import(doc, &xml)?
                        }
                    }
                }
            };
            set_run_text(doc, r, &x, &run.text);
            let index = doc.children(p).len();
            doc.insert(p, index, r);
        }
        // The paragraph mark.
        let end_look = look(&new.style, &para.end_style);
        let end = match doc.child(template.p, "endParaRPr") {
            Some(e) if template.end == end_look => Some(doc.deep_clone(e)),
            Some(e) => {
                let copy = doc.deep_clone(e);
                write_run_format(doc, copy, &x, &template.end, &end_look, "")?;
                Some(copy)
            }
            None if template.end == end_look => None,
            None => Some(x.import(doc, &run_props(&x, &end_look, "").replacen("rPr", "endParaRPr", 2))?),
        };
        if let Some(end) = end {
            let index = doc.children(p).len();
            doc.insert(p, index, end);
        }
    }
    // Replace the paragraphs.
    for p in doc.children_named(body, "p").collect::<Vec<_>>() {
        doc.detach(p);
    }
    let ext = doc.child(body, "extLst");
    for p in written {
        match ext {
            Some(e) => doc.insert_before(e, p),
            None => {
                let index = doc.children(body).len();
                doc.insert(body, index, p);
            }
        }
    }
    write_body_properties(doc, body, old, new)
}

/// Update only changed bodyPr attributes, preserving unknown attributes/children.
fn write_body_properties(doc: &mut XmlDocument, body: NodeId, old: &TextBody, new: &TextBody) -> Result<(), Error> {
    let props = match doc.child(body, "bodyPr") {
        Some(props) => props,
        None => {
            let x = Prefixes::at(doc, body);
            let props = x.import(doc, &format!("<{}bodyPr/>", x.a))?;
            doc.insert(body, 0, props);
            props
        }
    };
    if old.font_scale != new.font_scale || old.autofit != new.autofit {
        if !(1_000..=100_000).contains(&new.font_scale) { return Err(unsupported("文字縮小率の範囲")); }
        let fits: Vec<_> = doc.elements(props).filter(|&n| doc.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main") && matches!(doc.local_name(n), "normAutofit" | "noAutofit" | "spAutoFit")).collect();
        if fits.is_empty() {
            let x = Prefixes::at(doc, props);
            let name = match new.autofit {
                ibis_doc_model::TextAutofit::None => {
                    if new.font_scale != 100_000 { format!("{}normAutofit", x.a) } else { format!("{}noAutofit", x.a) }
                }
                ibis_doc_model::TextAutofit::Shrink => format!("{}normAutofit", x.a),
                ibis_doc_model::TextAutofit::ResizeShape => format!("{}spAutoFit", x.a),
            };
            let fit = doc.append_element(props, &name);
            if (new.autofit == ibis_doc_model::TextAutofit::Shrink || new.autofit == ibis_doc_model::TextAutofit::None) && new.font_scale != 100_000 {
                doc.set_attr(fit, "fontScale", &new.font_scale.to_string());
            }
        } else if fits.len() == 1 {
            let fit = fits[0];
            let x = Prefixes::at(doc, fit);
            match new.autofit {
                ibis_doc_model::TextAutofit::ResizeShape => {
                    doc.rename(fit, &format!("{}spAutoFit", x.a));
                    doc.remove_attr(fit, "fontScale");
                }
                ibis_doc_model::TextAutofit::Shrink => {
                    doc.rename(fit, &format!("{}normAutofit", x.a));
                    if new.font_scale != 100_000 {
                        doc.set_attr(fit, "fontScale", &new.font_scale.to_string());
                    } else {
                        doc.remove_attr(fit, "fontScale");
                    }
                }
                ibis_doc_model::TextAutofit::None => {
                    if new.font_scale != 100_000 {
                        doc.rename(fit, &format!("{}normAutofit", x.a));
                        doc.set_attr(fit, "fontScale", &new.font_scale.to_string());
                    } else {
                        doc.rename(fit, &format!("{}noAutofit", x.a));
                        doc.remove_attr(fit, "fontScale");
                    }
                }
            }
        } else {
            return Err(unsupported("重複した文字自動調整の変更"));
        }
    }
    if old.anchor != new.anchor {
        let value = match new.anchor { ibis_doc_model::VerticalAnchor::Top => "t", ibis_doc_model::VerticalAnchor::Middle => "ctr", ibis_doc_model::VerticalAnchor::Bottom => "b" };
        doc.set_attr(props, "anchor", value);
    }
    for (name, a, b) in [("lIns", old.insets.left, new.insets.left), ("tIns", old.insets.top, new.insets.top), ("rIns", old.insets.right, new.insets.right), ("bIns", old.insets.bottom, new.insets.bottom)] {
        if a != b { doc.set_attr(props, name, &b.to_string()); }
    }
    if old.wrap != new.wrap { doc.set_attr(props, "wrap", if new.wrap { "square" } else { "none" }); }
    if (old.vertical, old.vertical_kind, old.sideways) != (new.vertical, new.vertical_kind, new.sideways) {
        let value = if new.vertical { new.vertical_kind.ooxml() } else { match new.sideways { 5_400_000 => "vert", 16_200_000 => "vert270", _ => "horz" } };
        doc.set_attr(props, "vert", value);
    }
    if old.rotation != new.rotation { doc.set_attr(props, "rot", &new.rotation.to_string()); }
    if old.upright != new.upright { doc.set_attr(props, "upright", if new.upright { "1" } else { "0" }); }
    if old.columns != new.columns {
        if new.columns <= 1 {
            doc.remove_attr(props, "numCol");
        } else {
            doc.set_attr(props, "numCol", &new.columns.to_string());
        }
    }
    if old.column_spacing != new.column_spacing {
        if new.column_spacing <= 0 {
            doc.remove_attr(props, "spcCol");
        } else {
            doc.set_attr(props, "spcCol", &new.column_spacing.to_string());
        }
    }
    if old.warp != new.warp {
        let warps: Vec<_> = doc.elements(props).filter(|&n| doc.namespace(n) == Some("http://schemas.openxmlformats.org/drawingml/2006/main") && doc.local_name(n) == "prstTxWarp").collect();
        match &new.warp {
            Some(warp) => {
                if warps.is_empty() {
                    let x = Prefixes::at(doc, props);
                    let a = &x.a;
                    let warp_node = x.import(doc, &format!("<{a}prstTxWarp prst=\"{warp}\"><{a}avLst/></{a}prstTxWarp>"))?;
                    doc.insert(props, 0, warp_node);
                } else {
                    for &w in &warps {
                        doc.set_attr(w, "prst", warp);
                    }
                }
            }
            None => {
                for &w in &warps {
                    doc.detach(w);
                }
            }
        }
    }
    Ok(())
}

/// `a:rPr` for a look, every format written out.
fn run_props(x: &Prefixes, look: &Look, text: &str) -> String {
    let a = &x.a;
    let s = &look.style;
    let lang_attr = match &look.lang {
        Some(l) => format!(" lang=\"{l}\""),
        None => String::new(),
    };
    let mut out = format!("<{a}rPr{lang_attr} altLang=\"en-US\" sz=\"{}\" b=\"{}\" i=\"{}\"", s.font_size, s.bold as u8, s.italic as u8);
    if look.underline {
        out.push_str(&format!(" u=\"{}\"", look.underline_kind.value()));
    }
    if look.baseline != 0 {
        out.push_str(&format!(" baseline=\"{}\"", look.baseline));
    }
    if look.spacing != 0 {
        out.push_str(&format!(" spc=\"{}\"", look.spacing));
    }
    if look.strike != ibis_doc_model::Strike::None {
        out.push_str(&format!(" strike=\"{}\"", strike_value(look.strike)));
    }
    if look.caps != ibis_doc_model::Caps::None {
        out.push_str(&format!(" cap=\"{}\"", caps_value(look.caps)));
    }
    out.push_str(" dirty=\"0\">");
    if let Some((rgb, width)) = look.outline {
        out.push_str(&format!("<{a}ln w=\"{width}\"><{a}solidFill>{}</{a}solidFill></{a}ln>", new::color_xml(x, ibis_doc_model::Color::opaque(rgb))));
    }
    out.push_str(&format!("<{a}solidFill>{}</{a}solidFill>", new::color_xml(x, ibis_doc_model::Color::opaque(s.color))));
    if let Some(sh) = look.shadow {
        out.push_str(&format!(
            "<{a}effectLst><{a}outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"{}\">{}</{a}outerShdw></{a}effectLst>",
            sh.blur, sh.distance, sh.direction, sh.align.name(),
            new::color_xml(x, sh.color),
            a = x.a
        ));
    }
    if let Some(rgb) = look.highlight {
        out.push_str(&format!("<{a}highlight>{}</{a}highlight>", new::color_xml(x, ibis_doc_model::Color::opaque(rgb))));
    }
    out.push_str(&new::typefaces_xml(x, s.font.as_deref(), s.fallback_font.as_deref(), text));
    out.push_str(&format!("</{a}rPr>"));
    out
}

fn strike_value(strike: ibis_doc_model::Strike) -> &'static str {
    match strike {
        ibis_doc_model::Strike::None => "noStrike",
        ibis_doc_model::Strike::Single => "sngStrike",
        ibis_doc_model::Strike::Double => "dblStrike",
    }
}

fn caps_value(caps: ibis_doc_model::Caps) -> &'static str {
    match caps {
        ibis_doc_model::Caps::None => "none",
        ibis_doc_model::Caps::Small => "small",
        ibis_doc_model::Caps::All => "all",
    }
}

/// The order of the children of `a:rPr`, by local name.
fn rpr_rank(local: &str) -> usize {
    match local {
        "ln" => 0,
        "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill" => 1,
        "effectLst" | "effectDag" => 2,
        "highlight" => 3,
        "uLnTx" | "uLn" | "uFillTx" | "uFill" => 4,
        "latin" => 5,
        "ea" => 6,
        "cs" => 7,
        "sym" => 8,
        "hlinkClick" | "hlinkMouseOver" => 9,
        "rtl" => 10,
        _ => 11,
    }
}

fn insert_ordered(doc: &mut XmlDocument, parent: NodeId, node: NodeId) {
    let r = rpr_rank(doc.local_name(node));
    let next = doc.elements(parent).find(|&c| rpr_rank(doc.local_name(c)) > r);
    match next {
        Some(n) => doc.insert_before(n, node),
        None => {
            let index = doc.children(parent).len();
            doc.insert(parent, index, node);
        }
    }
}

/// Write into run (or paragraph mark) `node` the formats of `want` that
/// differ from `have`, which it shows now.
fn write_run_format(doc: &mut XmlDocument, node: NodeId, x: &Prefixes, have: &Look, want: &Look, text: &str) -> Result<(), Error> {
    // The properties element: the node itself for a paragraph mark.
    let rpr = if matches!(doc.local_name(node), "r" | "fld") {
        match doc.child(node, "rPr") {
            Some(r) => r,
            None => {
                let r = doc.insert_element(node, 0, &format!("{}rPr", x.a));
                if let Some(l) = &want.lang {
                    doc.set_attr(r, "lang", l);
                }
                r
            }
        }
    } else {
        node
    };
    if have.lang != want.lang {
        match &want.lang {
            Some(l) => doc.set_attr(rpr, "lang", l),
            None => doc.remove_attr(rpr, "lang"),
        }
    }
    let (h, w) = (&have.style, &want.style);
    if h.font_size != w.font_size {
        doc.set_attr(rpr, "sz", &w.font_size.to_string());
    }
    if h.bold != w.bold {
        doc.set_attr(rpr, "b", if w.bold { "1" } else { "0" });
    }
    if h.italic != w.italic {
        doc.set_attr(rpr, "i", if w.italic { "1" } else { "0" });
    }
    if have.underline != want.underline || (want.underline && have.underline_kind != want.underline_kind) {
        doc.set_attr(rpr, "u", if want.underline { want.underline_kind.value() } else { "none" });
    }
    if have.baseline != want.baseline {
        doc.set_attr(rpr, "baseline", &want.baseline.to_string());
    }
    if have.spacing != want.spacing {
        doc.set_attr(rpr, "spc", &want.spacing.to_string());
    }
    if have.strike != want.strike {
        doc.set_attr(rpr, "strike", strike_value(want.strike));
    }
    if have.caps != want.caps {
        doc.set_attr(rpr, "cap", caps_value(want.caps));
    }
    if have.outline != want.outline {
        if let Some((rgb, width)) = want.outline {
            let ln = match doc.child(rpr, "ln") {
                Some(ln) => ln,
                None => {
                    let ln = doc.append_element(rpr, &format!("{}ln", x.a));
                    doc.detach(ln); insert_ordered(doc, rpr, ln); ln
                }
            };
            doc.set_attr(ln, "w", &width.to_string());
            for child in doc.elements(ln).filter(|&n| rpr_rank(doc.local_name(n)) == 1).collect::<Vec<_>>() { doc.detach(child); }
            let fill = x.import(doc, &format!("<{a}solidFill>{}</{a}solidFill>", new::color_xml(x, ibis_doc_model::Color::opaque(rgb)), a = x.a))?;
            doc.insert(ln, 0, fill);
        }
    }
    // Clearing character formatting also removes an existing outline.
    if have.outline.is_some() && want.outline.is_none() {
        for c in doc.elements(rpr).filter(|&c| doc.local_name(c) == "ln").collect::<Vec<_>>() {
            doc.detach(c);
        }
    }
    // 蛍光ペン (`a:highlight`).
    if have.highlight != want.highlight {
        for c in doc.elements(rpr).filter(|&c| doc.local_name(c) == "highlight").collect::<Vec<_>>() {
            doc.detach(c);
        }
        if let Some(rgb) = want.highlight {
            let element = x.import(doc, &format!("<{a}highlight>{}</{a}highlight>", new::color_xml(x, ibis_doc_model::Color::opaque(rgb)), a = x.a))?;
            insert_ordered(doc, rpr, element);
        }
    }
    // 文字の影 (`a:effectLst` / `a:outerShdw`).
    if have.shadow != want.shadow {
        for c in doc.elements(rpr).filter(|&c| matches!(doc.local_name(c), "effectLst" | "outerShdw")).collect::<Vec<_>>() {
            doc.detach(c);
        }
        if let Some(sh) = want.shadow {
            let element = x.import(doc, &format!(
                "<{a}effectLst><{a}outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"{}\">{}</{a}outerShdw></{a}effectLst>",
                sh.blur, sh.distance, sh.direction, sh.align.name(),
                new::color_xml(x, sh.color),
                a = x.a
            ))?;
            insert_ordered(doc, rpr, element);
        }
    }
    if h.color != w.color {
        for c in doc.elements(rpr).filter(|&c| rpr_rank(doc.local_name(c)) == 1).collect::<Vec<_>>() {
            doc.detach(c);
        }
        let fill = x.import(doc, &format!("<{a}solidFill>{}</{a}solidFill>", new::color_xml(x, ibis_doc_model::Color::opaque(w.color)), a = x.a))?;
        insert_ordered(doc, rpr, fill);
    }
    // ハイパーリンク.
    if have.link != want.link || have.link_tooltip != want.link_tooltip {
        for c in doc.elements(rpr).filter(|&c| doc.local_name(c) == "hlinkClick").collect::<Vec<_>>() {
            doc.detach(c);
        }
        if let Some(link) = &want.link {
            let xml = super::pptx_links::hlink_xml(x, link, want.link_tooltip.as_deref()).ok_or_else(|| unsupported("このハイパーリンク"))?;
            let element = x.import(doc, &xml)?;
            insert_ordered(doc, rpr, element);
        }
    }
    if h.font != w.font || h.fallback_font != w.fallback_font {
        let (latin, ea) = new::typefaces(w.font.as_deref(), w.fallback_font.as_deref(), text);
        for (name, face) in [("latin", latin), ("ea", ea)] {
            let Some(face) = face else { continue };
            let e = match doc.child(rpr, name) {
                Some(e) => e,
                None => {
                    let e = doc.append_element(rpr, &format!("{}{name}", x.a));
                    doc.detach(e);
                    insert_ordered(doc, rpr, e);
                    e
                }
            };
            doc.set_attr(e, "typeface", &face);
        }
    }
    Ok(())
}

/// Set the `a:t` of a run.
fn set_run_text(doc: &mut XmlDocument, r: NodeId, x: &Prefixes, text: &str) {
    let t = match doc.child(r, "t") {
        Some(t) => t,
        None => doc.append_element(r, &format!("{}t", x.a)),
    };
    doc.set_text(t, text);
}

/// The order of the children of `a:pPr`, by local name.
fn ppr_rank(local: &str) -> usize {
    match local {
        "lnSpc" => 0,
        "spcBef" => 1,
        "spcAft" => 2,
        "buClrTx" | "buClr" => 3,
        "buSzTx" | "buSzPct" | "buSzPts" => 4,
        "buFontTx" | "buFont" => 5,
        "buNone" | "buAutoNum" | "buChar" | "buBlip" => 6,
        "tabLst" => 7,
        "defRPr" => 8,
        _ => 9,
    }
}

/// Replace the children of `ppr` named `names` with `xml` (if any).
fn replace_ppr(doc: &mut XmlDocument, ppr: NodeId, x: &Prefixes, names: &[&str], xml: Option<String>) -> Result<(), Error> {
    for c in doc.elements(ppr).filter(|&c| names.contains(&doc.local_name(c))).collect::<Vec<_>>() {
        doc.detach(c);
    }
    if let Some(xml) = xml {
        let element = x.import(doc, &xml)?;
        let r = ppr_rank(doc.local_name(element));
        let next = doc.elements(ppr).find(|&c| ppr_rank(doc.local_name(c)) > r);
        match next {
            Some(n) => doc.insert_before(n, element),
            None => {
                let index = doc.children(ppr).len();
                doc.insert(ppr, index, element);
            }
        }
    }
    Ok(())
}

/// Write the paragraph formats of `want` that differ from `have` into `p`'s `a:pPr`.
fn write_paragraph_format(doc: &mut XmlDocument, p: NodeId, x: &Prefixes, have: &Paragraph, want: &Paragraph, reduction: u32) -> Result<(), Error> {
    if have == want {
        return Ok(());
    }
    let rest = |q: &Paragraph| Paragraph {
        align: None,
        font_align: ibis_doc_model::FontAlign::Baseline,
        margin_left: 0,
        indent: 0,
        level: 0,
        space_before: 0,
        // How a space before was written (percent or points) is a reading of it, not something the save writes.
        space_before_exact: false,
        space_after: 0,
        line_spacing: None,
        bullet: None,
        line_break: false,
        tabs: Vec::new(),
        ..q.clone()
    };
    if rest(have) != rest(want) {
        if std::env::var("IBIS_DEBUG_TEXT").is_ok() {
            eprintln!("HAVE {:?}\nWANT {:?}", rest(have), rest(want));
        }
        return Err(unsupported("段落の書式（文字の揃え方など）の変更"));
    }
    let ppr = match doc.child(p, "pPr") {
        Some(ppr) => ppr,
        None => doc.insert_element(p, 0, &format!("{}pPr", x.a)),
    };
    let a = &x.a;
    if have.align != want.align {
        let v = match want.align.unwrap_or_default() {
            TextAlign::Left => "l",
            TextAlign::Center => "ctr",
            TextAlign::Right => "r",
            TextAlign::Justify => "just",
            TextAlign::Distributed => "dist",
        };
        doc.set_attr(ppr, "algn", v);
    }
    if have.font_align != want.font_align {
        let value = match want.font_align { ibis_doc_model::FontAlign::Auto => "auto", ibis_doc_model::FontAlign::Baseline => "base", ibis_doc_model::FontAlign::Top => "t", ibis_doc_model::FontAlign::Center => "ctr", ibis_doc_model::FontAlign::Bottom => "b" };
        doc.set_attr(ppr, "fontAlgn", value);
    }
    if have.margin_left != want.margin_left {
        doc.set_attr(ppr, "marL", &want.margin_left.to_string());
    }
    if have.level != want.level {
        doc.set_attr(ppr, "lvl", &want.level.to_string());
    }
    if have.indent != want.indent {
        doc.set_attr(ppr, "indent", &want.indent.to_string());
    }
    if have.line_spacing != want.line_spacing {
        let xml = want.line_spacing.map(|l| match l {
            LineSpacing::Percent(v) => format!("<{a}lnSpc><{a}spcPct val=\"{}\"/></{a}lnSpc>", v + reduction),
            LineSpacing::Points(v) => format!("<{a}lnSpc><{a}spcPts val=\"{v}\"/></{a}lnSpc>"),
        });
        replace_ppr(doc, ppr, x, &["lnSpc"], xml)?;
    }
    if have.space_before != want.space_before {
        replace_ppr(doc, ppr, x, &["spcBef"], Some(format!("<{a}spcBef><{a}spcPts val=\"{}\"/></{a}spcBef>", want.space_before)))?;
    }
    if have.space_after != want.space_after {
        replace_ppr(doc, ppr, x, &["spcAft"], Some(format!("<{a}spcAft><{a}spcPts val=\"{}\"/></{a}spcAft>", want.space_after)))?;
    }
    if have.bullet != want.bullet {
        const MARKS: [&str; 4] = ["buNone", "buAutoNum", "buChar", "buBlip"];
        match &want.bullet {
            None => replace_ppr(doc, ppr, x, &MARKS, Some(format!("<{a}buNone/>")))?,
            Some(b) if b.picture.is_some() => return Err(unsupported("画像の箇条書きへの変更")),
            Some(b) => {
                // Its colour and size, written out (the list style may set others).
                let color = match b.color {
                    Some(rgb) => format!("<{a}buClr>{}</{a}buClr>", new::color_xml(x, ibis_doc_model::Color::opaque(rgb))),
                    None => format!("<{a}buClrTx/>"),
                };
                replace_ppr(doc, ppr, x, &["buClr", "buClrTx"], Some(color))?;
                replace_ppr(doc, ppr, x, &["buSzTx", "buSzPct", "buSzPts"], Some(format!("<{a}buSzPct val=\"{}\"/>", b.size)))?;
                let numbered = b.auto.clone().or_else(|| {
                    (b.text.ends_with('.') && b.text[..b.text.len() - 1].chars().all(|c| c.is_ascii_digit()) && b.text.len() > 1)
                        .then(|| ibis_doc_model::text::AutoNumber { scheme: "arabicPeriod".into(), start_at: 1 })
                });
                if let Some(auto) = numbered {
                    replace_ppr(doc, ppr, x, &["buFont", "buFontTx"], Some(format!("<{a}buFont typeface=\"+mj-lt\"/>")))?;
                    let start = if auto.start_at > 1 { format!(" startAt=\"{}\"", auto.start_at) } else { String::new() };
                    replace_ppr(doc, ppr, x, &MARKS, Some(format!("<{a}buAutoNum type=\"{}\"{start}/>", new::escape(&auto.scheme))))?;
                } else {
                    replace_ppr(doc, ppr, x, &["buFont", "buFontTx"], Some(format!("<{a}buFont typeface=\"Arial\" panose=\"020B0604020202020204\" pitchFamily=\"34\" charset=\"0\"/>")))?;
                    let c: String = b.text.chars().take(1).collect();
                    replace_ppr(doc, ppr, x, &MARKS, Some(format!("<{a}buChar char=\"{}\"/>", new::escape(&c))))?;
                }
            }
        }
    }
    if have.tabs != want.tabs {
        let xml = if want.tabs.is_empty() {
            None
        } else {
            let mut s = format!("<{a}tabLst>");
            for tab in &want.tabs {
                let algn = match tab.align {
                    ibis_doc_model::TextTabAlign::Center => " algn=\"c\"",
                    ibis_doc_model::TextTabAlign::Right => " algn=\"r\"",
                    ibis_doc_model::TextTabAlign::Decimal => " algn=\"dec\"",
                    ibis_doc_model::TextTabAlign::Left => "",
                };
                s.push_str(&format!("<{a}tab pos=\"{}\"{algn}/>", tab.position));
            }
            s.push_str(&format!("</{a}tabLst>"));
            Some(s)
        };
        replace_ppr(doc, ppr, x, &["tabLst"], xml)?;
    }
    Ok(())
}
