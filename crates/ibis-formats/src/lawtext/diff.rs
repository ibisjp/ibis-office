// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Structured legal diff IR (`AmendmentIr`) and 新旧対照表 (side-by-side
//! amendment comparison) generator for [`super::LawDoc`].

use serde_json::{Value, json};

use super::{
    DivisionKind, LawAppdxTable, LawArticle, LawDoc, LawItem, LawNode, LawParagraph, LawSubitem,
    SupplProvision, to_fullwidth_digits,
};
use crate::Error;

/// Options controlling 新旧対照表 Markdown generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShinkyuOptions {
    /// Put `改正後` in the left column and `現行` in the right column (default `true`).
    /// Set to `false` for `現行 | 改正後` ordering.
    pub new_on_left: bool,
    /// Include `SupplProvision` (附則) diffs when present (default `true`).
    pub include_suppl: bool,
    /// Collapse unchanged paragraphs / items / subitems into `（略）` inside
    /// modified articles (default `true`).
    pub omit_unchanged_items: bool,
}

impl Default for ShinkyuOptions {
    fn default() -> Self {
        Self {
            new_on_left: true,
            include_suppl: true,
            omit_unchanged_items: true,
        }
    }
}

/// Token-level diff operation for structured IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    Equal(String),
    Delete(String),
    Insert(String),
}

impl DiffOp {
    fn to_json(&self) -> Value {
        match self {
            Self::Equal(s) => json!({ "op": "equal", "text": s }),
            Self::Delete(s) => json!({ "op": "delete", "text": s }),
            Self::Insert(s) => json!({ "op": "insert", "text": s }),
        }
    }
}

/// Change kind for a division header (編・章・節・款・目).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DivisionChangeKind {
    Added,
    Deleted,
    Renamed,
}

/// Structural division change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivisionChange {
    pub kind: DivisionKind,
    pub num: String,
    pub change: DivisionChangeKind,
    pub old_title: Option<String>,
    pub new_title: Option<String>,
}

impl DivisionChange {
    fn to_json(&self) -> Value {
        json!({
            "division_kind": self.kind.xml_tag(),
            "num": self.num,
            "change": match self.change {
                DivisionChangeKind::Added => "added",
                DivisionChangeKind::Deleted => "deleted",
                DivisionChangeKind::Renamed => "renamed",
            },
            "old_title": self.old_title,
            "new_title": self.new_title,
        })
    }
}

/// Change to a sub-item (`Subitem1`..`Subitem10`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubitemChange {
    pub depth: u8,
    pub kind: NodeChangeKind,
    pub old_title: Option<String>,
    pub new_title: Option<String>,
    pub old_sentence: Option<String>,
    pub new_sentence: Option<String>,
    pub sentence_diff: Vec<DiffOp>,
    pub children: Vec<SubitemChange>,
}

impl SubitemChange {
    fn to_json(&self) -> Value {
        json!({
            "depth": self.depth,
            "kind": self.kind.as_str(),
            "old_title": self.old_title,
            "new_title": self.new_title,
            "old_sentence": self.old_sentence,
            "new_sentence": self.new_sentence,
            "sentence_diff": self.sentence_diff.iter().map(DiffOp::to_json).collect::<Vec<_>>(),
            "children": self.children.iter().map(SubitemChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Change to an item (`Item`: 一, 二, 三...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemChange {
    pub kind: NodeChangeKind,
    pub old_title: Option<String>,
    pub new_title: Option<String>,
    pub old_sentence: Option<String>,
    pub new_sentence: Option<String>,
    pub sentence_diff: Vec<DiffOp>,
    pub subitem_changes: Vec<SubitemChange>,
}

impl ItemChange {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "old_title": self.old_title,
            "new_title": self.new_title,
            "old_sentence": self.old_sentence,
            "new_sentence": self.new_sentence,
            "sentence_diff": self.sentence_diff.iter().map(DiffOp::to_json).collect::<Vec<_>>(),
            "subitem_changes": self.subitem_changes.iter().map(SubitemChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Change to a paragraph (`Paragraph`: 項).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphChange {
    pub kind: NodeChangeKind,
    pub old_num: Option<String>,
    pub new_num: Option<String>,
    pub old_sentence: Option<String>,
    pub new_sentence: Option<String>,
    pub sentence_diff: Vec<DiffOp>,
    pub item_changes: Vec<ItemChange>,
}

impl ParagraphChange {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "old_num": self.old_num,
            "new_num": self.new_num,
            "old_sentence": self.old_sentence,
            "new_sentence": self.new_sentence,
            "sentence_diff": self.sentence_diff.iter().map(DiffOp::to_json).collect::<Vec<_>>(),
            "item_changes": self.item_changes.iter().map(ItemChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Generic change kind for paragraphs, items, and subitems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeChangeKind {
    Unchanged,
    Added,
    Deleted,
    Modified,
    Renumbered,
}

impl NodeChangeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Modified => "modified",
            Self::Renumbered => "renumbered",
        }
    }
}

/// Change kind for an article (`Article`: 条).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArticleChangeKind {
    Added,
    Deleted,
    Modified,
    Renumbered,
}

impl ArticleChangeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Modified => "modified",
            Self::Renumbered => "renumbered",
        }
    }
}

/// Change to a single article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleChange {
    pub kind: ArticleChangeKind,
    pub old_num: Option<String>,
    pub new_num: Option<String>,
    pub old_title: Option<String>,
    pub new_title: Option<String>,
    pub old_caption: Option<String>,
    pub new_caption: Option<String>,
    pub caption_diff: Vec<DiffOp>,
    pub paragraph_changes: Vec<ParagraphChange>,
}

impl ArticleChange {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "old_num": self.old_num,
            "new_num": self.new_num,
            "old_title": self.old_title,
            "new_title": self.new_title,
            "old_caption": self.old_caption,
            "new_caption": self.new_caption,
            "caption_diff": self.caption_diff.iter().map(DiffOp::to_json).collect::<Vec<_>>(),
            "paragraph_changes": self.paragraph_changes.iter().map(ParagraphChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Change inside a `SupplProvision` (附則).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplChange {
    pub label: String,
    pub amend_law_num: Option<String>,
    pub article_changes: Vec<ArticleChange>,
    pub paragraph_changes: Vec<ParagraphChange>,
}

impl SupplChange {
    fn is_empty(&self) -> bool {
        self.article_changes.is_empty() && self.paragraph_changes.is_empty()
    }

    fn to_json(&self) -> Value {
        json!({
            "label": self.label,
            "amend_law_num": self.amend_law_num,
            "article_changes": self.article_changes.iter().map(ArticleChange::to_json).collect::<Vec<_>>(),
            "paragraph_changes": self.paragraph_changes.iter().map(ParagraphChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Change to a row inside an appended table (`AppdxTable`: 別表).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppdxRowChange {
    pub kind: NodeChangeKind,
    pub row_index: usize,
    pub old_cells: Vec<String>,
    pub new_cells: Vec<String>,
}

impl AppdxRowChange {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "row_index": self.row_index,
            "old_cells": self.old_cells,
            "new_cells": self.new_cells,
        })
    }
}

/// Change to an appended table (`AppdxTable`: 別表).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppdxTableChange {
    pub title: String,
    pub kind: NodeChangeKind,
    pub row_changes: Vec<AppdxRowChange>,
}

impl AppdxTableChange {
    fn to_json(&self) -> Value {
        json!({
            "title": self.title,
            "kind": self.kind.as_str(),
            "row_changes": self.row_changes.iter().map(AppdxRowChange::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Structured intermediate representation of differences between two [`LawDoc`]s.
///
/// Designed as the shared backbone for 新旧対照表 generation, 改め文 drafting/verification,
/// and AI Assistant context injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendmentIr {
    pub law_title: String,
    pub old_law_num: String,
    pub new_law_num: String,
    pub title_change: Option<(String, String)>,
    pub division_changes: Vec<DivisionChange>,
    pub article_changes: Vec<ArticleChange>,
    pub suppl_changes: Vec<SupplChange>,
    pub appdx_table_changes: Vec<AppdxTableChange>,
}

impl AmendmentIr {
    /// Serialize the structured diff IR into a [`serde_json::Value`].
    pub fn to_json_value(&self) -> Value {
        json!({
            "law_title": self.law_title,
            "old_law_num": self.old_law_num,
            "new_law_num": self.new_law_num,
            "title_change": self.title_change.as_ref().map(|(o, n)| json!({
                "old": o,
                "new": n,
                "diff": diff_ops(o, n).iter().map(DiffOp::to_json).collect::<Vec<_>>(),
            })),
            "division_changes": self.division_changes.iter().map(DivisionChange::to_json).collect::<Vec<_>>(),
            "article_changes": self.article_changes.iter().map(ArticleChange::to_json).collect::<Vec<_>>(),
            "suppl_changes": self.suppl_changes.iter().map(SupplChange::to_json).collect::<Vec<_>>(),
            "appdx_table_changes": self.appdx_table_changes.iter().map(AppdxTableChange::to_json).collect::<Vec<_>>(),
        })
    }

    /// Serialize the structured diff IR into pretty-printed JSON.
    pub fn to_json_string(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(&self.to_json_value())
            .map_err(|e| Error::new(format!("AmendmentIr の JSON 変換に失敗しました: {e}")))
    }
}

/// Compute a structured [`AmendmentIr`] between `old` and `new` [`LawDoc`]s.
pub fn diff_laws(old: &LawDoc, new: &LawDoc) -> AmendmentIr {
    let law_title = if !new.law_title.is_empty() {
        new.law_title.clone()
    } else {
        old.law_title.clone()
    };
    let title_change = if !old.law_title.is_empty()
        && !new.law_title.is_empty()
        && old.law_title != new.law_title
    {
        Some((old.law_title.clone(), new.law_title.clone()))
    } else {
        None
    };

    let division_changes = diff_divisions(&old.main_provision, &new.main_provision);
    let old_articles = collect_articles(&old.main_provision);
    let new_articles = collect_articles(&new.main_provision);
    let article_changes = diff_article_lists(&old_articles, &new_articles);
    let suppl_changes = diff_suppl_provisions(&old.suppl_provisions, &new.suppl_provisions);
    let appdx_table_changes = diff_appdx_tables(&old.appdx_tables, &new.appdx_tables);

    AmendmentIr {
        law_title,
        old_law_num: old.law_num.clone(),
        new_law_num: new.law_num.clone(),
        title_change,
        division_changes,
        article_changes,
        suppl_changes,
        appdx_table_changes,
    }
}

/// Generate a 新旧対照表 (side-by-side comparison) in Markdown from `old` and `new` [`LawDoc`]s.
///
/// Changed/added spans are highlighted with `++...++` (underline / 傍線 in ibisOffice Markdown)
/// and deleted spans with `~~...~~` inside a 2-column GFM table (`改正後` | `現行`),
/// ready to open or export as DOCX, PPTX, or XLSX.
pub fn generate_shinkyu_markdown(old: &LawDoc, new: &LawDoc) -> String {
    generate_shinkyu_markdown_with_options(old, new, &ShinkyuOptions::default())
}

/// Generate a 新旧対照表 (side-by-side comparison) in Markdown with explicit [`ShinkyuOptions`].
pub fn generate_shinkyu_markdown_with_options(
    old: &LawDoc,
    new: &LawDoc,
    opts: &ShinkyuOptions,
) -> String {
    let ir = diff_laws(old, new);
    let old_articles = collect_articles(&old.main_provision);
    let new_articles = collect_articles(&new.main_provision);

    let mut out = String::new();
    out.push_str(&format!("# {} 新旧対照表\n\n", ir.law_title));
    out.push_str("（傍線部分は改正部分）\n\n");
    if opts.new_on_left {
        out.push_str("| 改正後 | 現行 |\n| :--- | :--- |\n");
    } else {
        out.push_str("| 現行 | 改正後 |\n| :--- | :--- |\n");
    }

    if let Some((old_t, new_t)) = &ir.title_change {
        let (dn, do_) = diff_clause_tokens(old_t, new_t);
        push_table_row(&mut out, &dn, &do_, opts.new_on_left);
    }

    for dc in &ir.division_changes {
        let (new_cell, old_cell) = match dc.change {
            DivisionChangeKind::Added => (
                format!("++{}++", dc.new_title.as_deref().unwrap_or("")),
                "（新設）".to_string(),
            ),
            DivisionChangeKind::Deleted => (
                "（削る）".to_string(),
                format!("~~{}~~", dc.old_title.as_deref().unwrap_or("")),
            ),
            DivisionChangeKind::Renamed => diff_clause_tokens(
                dc.old_title.as_deref().unwrap_or(""),
                dc.new_title.as_deref().unwrap_or(""),
            ),
        };
        push_table_row(&mut out, &new_cell, &old_cell, opts.new_on_left);
    }

    for ac in &ir.article_changes {
        let (new_cell, old_cell) = render_article_change(ac, &old_articles, &new_articles, opts);
        push_table_row(&mut out, &new_cell, &old_cell, opts.new_on_left);
    }

    if opts.include_suppl {
        for sc in &ir.suppl_changes {
            if sc.is_empty() {
                continue;
            }
            let header = format!("**{}**", sc.label);
            push_table_row(&mut out, &header, &header, opts.new_on_left);
            let old_suppl_arts: Vec<&LawArticle> = old
                .suppl_provisions
                .iter()
                .filter(|s| s.label == sc.label)
                .flat_map(|s| collect_articles(&s.nodes))
                .collect();
            let new_suppl_arts: Vec<&LawArticle> = new
                .suppl_provisions
                .iter()
                .filter(|s| s.label == sc.label)
                .flat_map(|s| collect_articles(&s.nodes))
                .collect();
            for ac in &sc.article_changes {
                let (new_cell, old_cell) =
                    render_article_change(ac, &old_suppl_arts, &new_suppl_arts, opts);
                push_table_row(&mut out, &new_cell, &old_cell, opts.new_on_left);
            }
            if !sc.paragraph_changes.is_empty() {
                let (mut new_parts, mut old_parts) = (Vec::new(), Vec::new());
                render_paragraph_changes(
                    &sc.paragraph_changes,
                    None,
                    None,
                    opts.omit_unchanged_items,
                    &mut new_parts,
                    &mut old_parts,
                );
                push_table_row(
                    &mut out,
                    &new_parts.join("<br>"),
                    &old_parts.join("<br>"),
                    opts.new_on_left,
                );
            }
        }
    }

    for at in &ir.appdx_table_changes {
        let header = format!("**{}**", at.title);
        let mut new_lines = vec![header.clone()];
        let mut old_lines = vec![header];
        for rc in &at.row_changes {
            match rc.kind {
                NodeChangeKind::Added => {
                    new_lines.push(format!("++{}++", rc.new_cells.join(" | ")));
                    old_lines.push("（新設）".to_string());
                }
                NodeChangeKind::Deleted => {
                    new_lines.push("（削る）".to_string());
                    old_lines.push(format!("~~{}~~", rc.old_cells.join(" | ")));
                }
                NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                    let cols = rc.old_cells.len().max(rc.new_cells.len());
                    let mut nc = Vec::new();
                    let mut oc = Vec::new();
                    for c in 0..cols {
                        let o = rc.old_cells.get(c).map(String::as_str).unwrap_or("");
                        let n = rc.new_cells.get(c).map(String::as_str).unwrap_or("");
                        let (dn, do_) = diff_clause_tokens(o, n);
                        nc.push(dn);
                        oc.push(do_);
                    }
                    new_lines.push(nc.join(" / "));
                    old_lines.push(oc.join(" / "));
                }
                NodeChangeKind::Unchanged => {}
            }
        }
        push_table_row(
            &mut out,
            &new_lines.join("<br>"),
            &old_lines.join("<br>"),
            opts.new_on_left,
        );
    }

    out
}

fn push_table_row(out: &mut String, new_cell: &str, old_cell: &str, new_on_left: bool) {
    if new_on_left {
        out.push_str(&format!(
            "| {} | {} |\n",
            escape_md_cell(new_cell),
            escape_md_cell(old_cell)
        ));
    } else {
        out.push_str(&format!(
            "| {} | {} |\n",
            escape_md_cell(old_cell),
            escape_md_cell(new_cell)
        ));
    }
}

fn collect_divisions<'a>(nodes: &'a [LawNode]) -> Vec<(DivisionKind, &'a str, &'a str)> {
    let mut out = Vec::new();
    for n in nodes {
        if let LawNode::Division {
            kind,
            num,
            title,
            children,
        } = n
        {
            out.push((*kind, num.as_str(), title.as_str()));
            out.extend(collect_divisions(children));
        }
    }
    out
}

fn diff_divisions(old_nodes: &[LawNode], new_nodes: &[LawNode]) -> Vec<DivisionChange> {
    let old_divs = collect_divisions(old_nodes);
    let new_divs = collect_divisions(new_nodes);
    let mut changes = Vec::new();
    let mut old_used = vec![false; old_divs.len()];

    for (nk, nnum, ntitle) in &new_divs {
        if let Some(pos) = old_divs
            .iter()
            .enumerate()
            .position(|(i, (ok, onum, _))| !old_used[i] && ok == nk && onum == nnum)
        {
            old_used[pos] = true;
            let otitle = old_divs[pos].2;
            if otitle != *ntitle {
                changes.push(DivisionChange {
                    kind: *nk,
                    num: nnum.to_string(),
                    change: DivisionChangeKind::Renamed,
                    old_title: Some(otitle.to_string()),
                    new_title: Some(ntitle.to_string()),
                });
            }
        } else {
            changes.push(DivisionChange {
                kind: *nk,
                num: nnum.to_string(),
                change: DivisionChangeKind::Added,
                old_title: None,
                new_title: Some(ntitle.to_string()),
            });
        }
    }

    for (i, (ok, onum, otitle)) in old_divs.iter().enumerate() {
        if !old_used[i] {
            changes.push(DivisionChange {
                kind: *ok,
                num: onum.to_string(),
                change: DivisionChangeKind::Deleted,
                old_title: Some(otitle.to_string()),
                new_title: None,
            });
        }
    }

    changes
}

fn collect_articles<'a>(nodes: &'a [LawNode]) -> Vec<&'a LawArticle> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            LawNode::Division { children, .. } => out.extend(collect_articles(children)),
            LawNode::Article(a) => out.push(a),
            LawNode::Paragraph(_) => {}
        }
    }
    out
}

fn collect_suppl_paragraphs<'a>(nodes: &'a [LawNode]) -> Vec<&'a LawParagraph> {
    let mut out = Vec::new();
    for n in nodes {
        if let LawNode::Paragraph(p) = n {
            out.push(p);
        }
    }
    out
}

fn diff_article_lists(old_articles: &[&LawArticle], new_articles: &[&LawArticle]) -> Vec<ArticleChange> {
    // Pass 1: anchor exact `num` matches that also share compatible caption/content.
    // This prevents an inserted article (e.g. new 第2条) from stealing the anchor
    // of a pushed-down article (old 第2条 -> new 第3条).
    let mut old_matched: Vec<Option<usize>> = vec![None; old_articles.len()];
    let mut new_matched: Vec<Option<usize>> = vec![None; new_articles.len()];

    let mut old_cursor = 0usize;
    for (ni, new_art) in new_articles.iter().enumerate() {
        if let Some(rel) = old_articles[old_cursor..].iter().position(|a| {
            a.num == new_art.num && is_compatible_same_num(a, new_art)
        }) {
            let oi = old_cursor + rel;
            old_matched[oi] = Some(ni);
            new_matched[ni] = Some(oi);
            old_cursor = oi + 1;
        }
    }

    // Pass 2: detect renumbered articles (条ズレ) among unmatched pairs by similarity.
    let mut renumber_pairs: Vec<(usize, usize)> = Vec::new();
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (oi, old_art) in old_articles.iter().enumerate() {
        if old_matched[oi].is_some() {
            continue;
        }
        for (ni, new_art) in new_articles.iter().enumerate() {
            if new_matched[ni].is_some() {
                continue;
            }
            let sim = article_similarity(old_art, new_art);
            if sim >= 0.45 {
                candidates.push((sim, oi, ni));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_sim, oi, ni) in candidates {
        if old_matched[oi].is_none() && new_matched[ni].is_none() {
            old_matched[oi] = Some(ni);
            new_matched[ni] = Some(oi);
            if old_articles[oi].num != new_articles[ni].num {
                renumber_pairs.push((oi, ni));
            }
        }
    }

    // Pass 3: pair any remaining unmatched articles that share the exact same `num`
    // (full rewrite of an article at the same number).
    let mut old_cursor3 = 0usize;
    for (ni, new_art) in new_articles.iter().enumerate() {
        if new_matched[ni].is_some() {
            continue;
        }
        if let Some(rel) = old_articles[old_cursor3..]
            .iter()
            .enumerate()
            .position(|(k, a)| old_matched[old_cursor3 + k].is_none() && a.num == new_art.num)
        {
            let oi = old_cursor3 + rel;
            old_matched[oi] = Some(ni);
            new_matched[ni] = Some(oi);
            old_cursor3 = oi + 1;
        }
    }

    // Emit changes in document order.
    let mut changes = Vec::new();
    let mut oi = 0usize;
    for (ni, new_art) in new_articles.iter().enumerate() {
        match new_matched[ni] {
            Some(matched_oi) => {
                while oi < matched_oi {
                    if old_matched[oi].is_none() {
                        changes.push(make_deleted_article(old_articles[oi]));
                    }
                    oi += 1;
                }
                let old_art = old_articles[matched_oi];
                let is_renumbered = renumber_pairs.contains(&(matched_oi, ni));
                if is_renumbered || old_art != *new_art {
                    changes.push(make_modified_article(old_art, new_art, is_renumbered));
                }
                if oi == matched_oi {
                    oi += 1;
                }
            }
            None => {
                changes.push(make_added_article(new_art));
            }
        }
    }
    while oi < old_articles.len() {
        if old_matched[oi].is_none() {
            changes.push(make_deleted_article(old_articles[oi]));
        }
        oi += 1;
    }
    changes
}

fn is_compatible_same_num(old: &LawArticle, new: &LawArticle) -> bool {
    if let (Some(oc), Some(nc)) = (&old.caption, &new.caption) {
        if !oc.is_empty() && !nc.is_empty() {
            return oc == nc || article_similarity(old, new) >= 0.75;
        }
    }
    article_similarity(old, new) >= 0.35 || (old.paragraphs.is_empty() && new.paragraphs.is_empty())
}

fn article_similarity(old: &LawArticle, new: &LawArticle) -> f64 {
    let mut score = 0.0;
    if let (Some(oc), Some(nc)) = (&old.caption, &new.caption) {
        if !oc.is_empty() && oc == nc {
            score += 0.55;
        }
    }
    let old_text: String = old
        .paragraphs
        .iter()
        .map(|p| p.sentence.as_str())
        .collect::<Vec<_>>()
        .join("");
    let new_text: String = new
        .paragraphs
        .iter()
        .map(|p| p.sentence.as_str())
        .collect::<Vec<_>>()
        .join("");
    let a = tokenize_legal_clauses(&old_text);
    let b = tokenize_legal_clauses(&new_text);
    if !a.is_empty() && !b.is_empty() {
        let mut common = 0usize;
        let mut used = vec![false; b.len()];
        for tok in &a {
            if let Some(pos) = b.iter().enumerate().position(|(j, t)| !used[j] && t == tok) {
                used[pos] = true;
                common += 1;
            }
        }
        let union = a.len() + b.len() - common;
        if union > 0 {
            let jaccard = common as f64 / union as f64;
            score += jaccard * 0.85;
        }
    }
    score.min(1.0)
}

fn make_added_article(art: &LawArticle) -> ArticleChange {
    ArticleChange {
        kind: ArticleChangeKind::Added,
        old_num: None,
        new_num: Some(art.num.clone()),
        old_title: None,
        new_title: Some(art.title.clone()),
        old_caption: None,
        new_caption: art.caption.clone(),
        caption_diff: Vec::new(),
        paragraph_changes: art
            .paragraphs
            .iter()
            .map(|p| ParagraphChange {
                kind: NodeChangeKind::Added,
                old_num: None,
                new_num: Some(p.num.clone()),
                old_sentence: None,
                new_sentence: Some(p.sentence.clone()),
                sentence_diff: vec![DiffOp::Insert(p.sentence.clone())],
                item_changes: Vec::new(),
            })
            .collect(),
    }
}

fn make_deleted_article(art: &LawArticle) -> ArticleChange {
    ArticleChange {
        kind: ArticleChangeKind::Deleted,
        old_num: Some(art.num.clone()),
        new_num: None,
        old_title: Some(art.title.clone()),
        new_title: None,
        old_caption: art.caption.clone(),
        new_caption: None,
        caption_diff: Vec::new(),
        paragraph_changes: art
            .paragraphs
            .iter()
            .map(|p| ParagraphChange {
                kind: NodeChangeKind::Deleted,
                old_num: Some(p.num.clone()),
                new_num: None,
                old_sentence: Some(p.sentence.clone()),
                new_sentence: None,
                sentence_diff: vec![DiffOp::Delete(p.sentence.clone())],
                item_changes: Vec::new(),
            })
            .collect(),
    }
}

fn make_modified_article(
    old: &LawArticle,
    new: &LawArticle,
    is_renumbered: bool,
) -> ArticleChange {
    let kind = if is_renumbered {
        ArticleChangeKind::Renumbered
    } else {
        ArticleChangeKind::Modified
    };
    let caption_diff = match (&old.caption, &new.caption) {
        (Some(oc), Some(nc)) if oc != nc => diff_ops(oc, nc),
        (None, Some(nc)) => vec![DiffOp::Insert(nc.clone())],
        (Some(oc), None) => vec![DiffOp::Delete(oc.clone())],
        _ => Vec::new(),
    };
    let old_paras: Vec<&LawParagraph> = old.paragraphs.iter().collect();
    let new_paras: Vec<&LawParagraph> = new.paragraphs.iter().collect();
    let paragraph_changes = diff_paragraph_lists(&old_paras, &new_paras);

    ArticleChange {
        kind,
        old_num: Some(old.num.clone()),
        new_num: Some(new.num.clone()),
        old_title: Some(old.title.clone()),
        new_title: Some(new.title.clone()),
        old_caption: old.caption.clone(),
        new_caption: new.caption.clone(),
        caption_diff,
        paragraph_changes,
    }
}

fn diff_paragraph_lists(
    old_paras: &[&LawParagraph],
    new_paras: &[&LawParagraph],
) -> Vec<ParagraphChange> {
    let mut changes = Vec::new();
    let mut old_idx = 0usize;
    for np in new_paras {
        if let Some(pos) = old_paras[old_idx..].iter().position(|op| op.num == np.num) {
            for skipped in &old_paras[old_idx..old_idx + pos] {
                changes.push(ParagraphChange {
                    kind: NodeChangeKind::Deleted,
                    old_num: Some(skipped.num.clone()),
                    new_num: None,
                    old_sentence: Some(skipped.sentence.clone()),
                    new_sentence: None,
                    sentence_diff: vec![DiffOp::Delete(skipped.sentence.clone())],
                    item_changes: Vec::new(),
                });
            }
            let op = old_paras[old_idx + pos];
            if op == *np {
                changes.push(ParagraphChange {
                    kind: NodeChangeKind::Unchanged,
                    old_num: Some(op.num.clone()),
                    new_num: Some(np.num.clone()),
                    old_sentence: Some(op.sentence.clone()),
                    new_sentence: Some(np.sentence.clone()),
                    sentence_diff: Vec::new(),
                    item_changes: Vec::new(),
                });
            } else {
                let sentence_diff = if op.sentence != np.sentence {
                    diff_ops(&op.sentence, &np.sentence)
                } else {
                    Vec::new()
                };
                let item_changes = diff_item_lists(&op.items, &np.items);
                changes.push(ParagraphChange {
                    kind: NodeChangeKind::Modified,
                    old_num: Some(op.num.clone()),
                    new_num: Some(np.num.clone()),
                    old_sentence: Some(op.sentence.clone()),
                    new_sentence: Some(np.sentence.clone()),
                    sentence_diff,
                    item_changes,
                });
            }
            old_idx = old_idx + pos + 1;
        } else {
            changes.push(ParagraphChange {
                kind: NodeChangeKind::Added,
                old_num: None,
                new_num: Some(np.num.clone()),
                old_sentence: None,
                new_sentence: Some(np.sentence.clone()),
                sentence_diff: vec![DiffOp::Insert(np.sentence.clone())],
                item_changes: Vec::new(),
            });
        }
    }
    for remaining in &old_paras[old_idx..] {
        changes.push(ParagraphChange {
            kind: NodeChangeKind::Deleted,
            old_num: Some(remaining.num.clone()),
            new_num: None,
            old_sentence: Some(remaining.sentence.clone()),
            new_sentence: None,
            sentence_diff: vec![DiffOp::Delete(remaining.sentence.clone())],
            item_changes: Vec::new(),
        });
    }
    changes
}

fn diff_item_lists(old_items: &[LawItem], new_items: &[LawItem]) -> Vec<ItemChange> {
    let mut changes = Vec::new();
    let mut old_idx = 0usize;
    for ni in new_items {
        if let Some(pos) = old_items[old_idx..]
            .iter()
            .position(|oi| oi.title == ni.title)
        {
            for skipped in &old_items[old_idx..old_idx + pos] {
                changes.push(ItemChange {
                    kind: NodeChangeKind::Deleted,
                    old_title: Some(skipped.title.clone()),
                    new_title: None,
                    old_sentence: Some(skipped.sentence.clone()),
                    new_sentence: None,
                    sentence_diff: vec![DiffOp::Delete(skipped.sentence.clone())],
                    subitem_changes: Vec::new(),
                });
            }
            let oi = &old_items[old_idx + pos];
            if oi == ni {
                changes.push(ItemChange {
                    kind: NodeChangeKind::Unchanged,
                    old_title: Some(oi.title.clone()),
                    new_title: Some(ni.title.clone()),
                    old_sentence: Some(oi.sentence.clone()),
                    new_sentence: Some(ni.sentence.clone()),
                    sentence_diff: Vec::new(),
                    subitem_changes: Vec::new(),
                });
            } else {
                let sentence_diff = if oi.sentence != ni.sentence {
                    diff_ops(&oi.sentence, &ni.sentence)
                } else {
                    Vec::new()
                };
                let subitem_changes = diff_subitem_lists(&oi.subitems, &ni.subitems);
                changes.push(ItemChange {
                    kind: NodeChangeKind::Modified,
                    old_title: Some(oi.title.clone()),
                    new_title: Some(ni.title.clone()),
                    old_sentence: Some(oi.sentence.clone()),
                    new_sentence: Some(ni.sentence.clone()),
                    sentence_diff,
                    subitem_changes,
                });
            }
            old_idx = old_idx + pos + 1;
        } else {
            changes.push(ItemChange {
                kind: NodeChangeKind::Added,
                old_title: None,
                new_title: Some(ni.title.clone()),
                old_sentence: None,
                new_sentence: Some(ni.sentence.clone()),
                sentence_diff: vec![DiffOp::Insert(ni.sentence.clone())],
                subitem_changes: Vec::new(),
            });
        }
    }
    for remaining in &old_items[old_idx..] {
        changes.push(ItemChange {
            kind: NodeChangeKind::Deleted,
            old_title: Some(remaining.title.clone()),
            new_title: None,
            old_sentence: Some(remaining.sentence.clone()),
            new_sentence: None,
            sentence_diff: vec![DiffOp::Delete(remaining.sentence.clone())],
            subitem_changes: Vec::new(),
        });
    }
    changes
}

fn diff_subitem_lists(old_subs: &[LawSubitem], new_subs: &[LawSubitem]) -> Vec<SubitemChange> {
    let mut changes = Vec::new();
    let mut old_idx = 0usize;
    for ns in new_subs {
        if let Some(pos) = old_subs[old_idx..]
            .iter()
            .position(|os| os.title == ns.title)
        {
            for skipped in &old_subs[old_idx..old_idx + pos] {
                changes.push(SubitemChange {
                    depth: skipped.depth,
                    kind: NodeChangeKind::Deleted,
                    old_title: Some(skipped.title.clone()),
                    new_title: None,
                    old_sentence: Some(skipped.sentence.clone()),
                    new_sentence: None,
                    sentence_diff: vec![DiffOp::Delete(skipped.sentence.clone())],
                    children: Vec::new(),
                });
            }
            let os = &old_subs[old_idx + pos];
            if os == ns {
                changes.push(SubitemChange {
                    depth: ns.depth,
                    kind: NodeChangeKind::Unchanged,
                    old_title: Some(os.title.clone()),
                    new_title: Some(ns.title.clone()),
                    old_sentence: Some(os.sentence.clone()),
                    new_sentence: Some(ns.sentence.clone()),
                    sentence_diff: Vec::new(),
                    children: Vec::new(),
                });
            } else {
                let sentence_diff = if os.sentence != ns.sentence {
                    diff_ops(&os.sentence, &ns.sentence)
                } else {
                    Vec::new()
                };
                let children = diff_subitem_lists(&os.children, &ns.children);
                changes.push(SubitemChange {
                    depth: ns.depth,
                    kind: NodeChangeKind::Modified,
                    old_title: Some(os.title.clone()),
                    new_title: Some(ns.title.clone()),
                    old_sentence: Some(os.sentence.clone()),
                    new_sentence: Some(ns.sentence.clone()),
                    sentence_diff,
                    children,
                });
            }
            old_idx = old_idx + pos + 1;
        } else {
            changes.push(SubitemChange {
                depth: ns.depth,
                kind: NodeChangeKind::Added,
                old_title: None,
                new_title: Some(ns.title.clone()),
                old_sentence: None,
                new_sentence: Some(ns.sentence.clone()),
                sentence_diff: vec![DiffOp::Insert(ns.sentence.clone())],
                children: Vec::new(),
            });
        }
    }
    for remaining in &old_subs[old_idx..] {
        changes.push(SubitemChange {
            depth: remaining.depth,
            kind: NodeChangeKind::Deleted,
            old_title: Some(remaining.title.clone()),
            new_title: None,
            old_sentence: Some(remaining.sentence.clone()),
            new_sentence: None,
            sentence_diff: vec![DiffOp::Delete(remaining.sentence.clone())],
            children: Vec::new(),
        });
    }
    changes
}

fn diff_appdx_tables(
    old_tables: &[LawAppdxTable],
    new_tables: &[LawAppdxTable],
) -> Vec<AppdxTableChange> {
    let mut changes = Vec::new();
    let mut old_used = vec![false; old_tables.len()];

    for nt in new_tables {
        if let Some(pos) = old_tables
            .iter()
            .enumerate()
            .position(|(i, ot)| !old_used[i] && ot.title == nt.title)
        {
            old_used[pos] = true;
            let ot = &old_tables[pos];
            if ot != nt {
                let mut row_changes = Vec::new();
                let max_r = ot.rows.len().max(nt.rows.len());
                for r in 0..max_r {
                    match (ot.rows.get(r), nt.rows.get(r)) {
                        (Some(orow), Some(nrow)) if orow != nrow => {
                            row_changes.push(AppdxRowChange {
                                kind: NodeChangeKind::Modified,
                                row_index: r,
                                old_cells: orow.clone(),
                                new_cells: nrow.clone(),
                            });
                        }
                        (None, Some(nrow)) => {
                            row_changes.push(AppdxRowChange {
                                kind: NodeChangeKind::Added,
                                row_index: r,
                                old_cells: Vec::new(),
                                new_cells: nrow.clone(),
                            });
                        }
                        (Some(orow), None) => {
                            row_changes.push(AppdxRowChange {
                                kind: NodeChangeKind::Deleted,
                                row_index: r,
                                old_cells: orow.clone(),
                                new_cells: Vec::new(),
                            });
                        }
                        _ => {}
                    }
                }
                if !row_changes.is_empty() {
                    changes.push(AppdxTableChange {
                        title: nt.title.clone(),
                        kind: NodeChangeKind::Modified,
                        row_changes,
                    });
                }
            }
        } else {
            let row_changes = nt
                .rows
                .iter()
                .enumerate()
                .map(|(r, nrow)| AppdxRowChange {
                    kind: NodeChangeKind::Added,
                    row_index: r,
                    old_cells: Vec::new(),
                    new_cells: nrow.clone(),
                })
                .collect();
            changes.push(AppdxTableChange {
                title: nt.title.clone(),
                kind: NodeChangeKind::Added,
                row_changes,
            });
        }
    }

    for (i, ot) in old_tables.iter().enumerate() {
        if !old_used[i] {
            let row_changes = ot
                .rows
                .iter()
                .enumerate()
                .map(|(r, orow)| AppdxRowChange {
                    kind: NodeChangeKind::Deleted,
                    row_index: r,
                    old_cells: orow.clone(),
                    new_cells: Vec::new(),
                })
                .collect();
            changes.push(AppdxTableChange {
                title: ot.title.clone(),
                kind: NodeChangeKind::Deleted,
                row_changes,
            });
        }
    }

    changes
}

fn diff_suppl_provisions(
    old_suppls: &[SupplProvision],
    new_suppls: &[SupplProvision],
) -> Vec<SupplChange> {
    let mut out = Vec::new();
    let max_len = old_suppls.len().max(new_suppls.len());
    for i in 0..max_len {
        let old_s = old_suppls.get(i);
        let new_s = new_suppls.get(i);
        match (old_s, new_s) {
            (Some(os), Some(ns)) if os != ns => {
                let old_arts = collect_articles(&os.nodes);
                let new_arts = collect_articles(&ns.nodes);
                let old_paras = collect_suppl_paragraphs(&os.nodes);
                let new_paras = collect_suppl_paragraphs(&ns.nodes);
                let sc = SupplChange {
                    label: ns.label.clone(),
                    amend_law_num: ns.amend_law_num.clone(),
                    article_changes: diff_article_lists(&old_arts, &new_arts),
                    paragraph_changes: diff_paragraph_lists(&old_paras, &new_paras),
                };
                if !sc.is_empty() {
                    out.push(sc);
                }
            }
            (None, Some(ns)) => {
                let new_arts = collect_articles(&ns.nodes);
                let new_paras = collect_suppl_paragraphs(&ns.nodes);
                let sc = SupplChange {
                    label: ns.label.clone(),
                    amend_law_num: ns.amend_law_num.clone(),
                    article_changes: new_arts.into_iter().map(make_added_article).collect(),
                    paragraph_changes: diff_paragraph_lists(&[], &new_paras),
                };
                if !sc.is_empty() {
                    out.push(sc);
                }
            }
            (Some(os), None) => {
                let old_arts = collect_articles(&os.nodes);
                let old_paras = collect_suppl_paragraphs(&os.nodes);
                let sc = SupplChange {
                    label: os.label.clone(),
                    amend_law_num: os.amend_law_num.clone(),
                    article_changes: old_arts.into_iter().map(make_deleted_article).collect(),
                    paragraph_changes: diff_paragraph_lists(&old_paras, &[]),
                };
                if !sc.is_empty() {
                    out.push(sc);
                }
            }
            _ => {}
        }
    }
    out
}

fn render_article_change(
    ac: &ArticleChange,
    old_articles: &[&LawArticle],
    new_articles: &[&LawArticle],
    opts: &ShinkyuOptions,
) -> (String, String) {
    match ac.kind {
        ArticleChangeKind::Added => {
            let new_art = ac
                .new_num
                .as_deref()
                .and_then(|num| new_articles.iter().find(|a| a.num == num).copied());
            let new_cell = match new_art {
                Some(art) => format_article_plain(art, Some("++")),
                None => String::new(),
            };
            (new_cell, "（新設）".to_string())
        }
        ArticleChangeKind::Deleted => {
            let old_art = ac
                .old_num
                .as_deref()
                .and_then(|num| old_articles.iter().find(|a| a.num == num).copied());
            let old_cell = match old_art {
                Some(art) => format_article_plain(art, Some("~~")),
                None => String::new(),
            };
            ("（削る）".to_string(), old_cell)
        }
        ArticleChangeKind::Modified | ArticleChangeKind::Renumbered => {
            let mut new_parts = Vec::new();
            let mut old_parts = Vec::new();

            if ac.old_caption.is_some() || ac.new_caption.is_some() {
                let oc = ac.old_caption.as_deref().unwrap_or("");
                let nc = ac.new_caption.as_deref().unwrap_or("");
                if oc != nc {
                    let (dn, do_) = diff_clause_tokens(oc, nc);
                    new_parts.push(if nc.is_empty() { "（削る）".to_string() } else { dn });
                    old_parts.push(if oc.is_empty() { "（新設）".to_string() } else { do_ });
                } else if !nc.is_empty() {
                    new_parts.push(nc.to_string());
                    old_parts.push(oc.to_string());
                }
            }

            render_paragraph_changes(
                &ac.paragraph_changes,
                ac.old_title.as_deref(),
                ac.new_title.as_deref(),
                opts.omit_unchanged_items,
                &mut new_parts,
                &mut old_parts,
            );
            (new_parts.join("<br>"), old_parts.join("<br>"))
        }
    }
}

fn render_paragraph_changes(
    changes: &[ParagraphChange],
    old_art_title: Option<&str>,
    new_art_title: Option<&str>,
    omit_unchanged: bool,
    new_parts: &mut Vec<String>,
    old_parts: &mut Vec<String>,
) {
    let any_changed = changes.iter().any(|c| c.kind != NodeChangeKind::Unchanged);
    let mut skipped_unchanged = false;

    for (idx, pc) in changes.iter().enumerate() {
        if pc.kind == NodeChangeKind::Unchanged && omit_unchanged && any_changed {
            if idx == 0 {
                if let (Some(nt), Some(ot)) = (new_art_title, old_art_title) {
                    let (dn, do_) = if nt != ot {
                        diff_clause_tokens(ot, nt)
                    } else {
                        (nt.to_string(), ot.to_string())
                    };
                    new_parts.push(format!("{dn} （略）"));
                    old_parts.push(format!("{do_} （略）"));
                } else {
                    new_parts.push("（略）".to_string());
                    old_parts.push("（略）".to_string());
                }
                skipped_unchanged = false;
            } else if !skipped_unchanged {
                new_parts.push("（略）".to_string());
                old_parts.push("（略）".to_string());
                skipped_unchanged = true;
            }
            continue;
        }
        skipped_unchanged = false;

        let new_prefix = if idx == 0 {
            new_art_title
                .map(|t| format!("{t} "))
                .unwrap_or_default()
        } else {
            pc.new_num
                .as_deref()
                .map(|n| format!("{} ", to_fullwidth_digits(n)))
                .unwrap_or_default()
        };
        let old_prefix = if idx == 0 {
            old_art_title
                .map(|t| format!("{t} "))
                .unwrap_or_default()
        } else {
            pc.old_num
                .as_deref()
                .map(|n| format!("{} ", to_fullwidth_digits(n)))
                .unwrap_or_default()
        };

        let title_prefix_diff = if idx == 0 {
            match (old_art_title, new_art_title) {
                (Some(ot), Some(nt)) if ot != nt => {
                    let (dn, do_) = diff_clause_tokens(ot, nt);
                    Some((format!("{dn} "), format!("{do_} ")))
                }
                _ => None,
            }
        } else {
            None
        };
        let (np_str, op_str) = title_prefix_diff.unwrap_or((new_prefix, old_prefix));

        match pc.kind {
            NodeChangeKind::Added => {
                new_parts.push(format!(
                    "++{}{}++",
                    pc.new_num
                        .as_deref()
                        .map(|n| format!("{} ", to_fullwidth_digits(n)))
                        .unwrap_or_default(),
                    pc.new_sentence.as_deref().unwrap_or("")
                ));
                old_parts.push("（新設）".to_string());
            }
            NodeChangeKind::Deleted => {
                new_parts.push("（削る）".to_string());
                old_parts.push(format!(
                    "~~{}{}~~",
                    pc.old_num
                        .as_deref()
                        .map(|n| format!("{} ", to_fullwidth_digits(n)))
                        .unwrap_or_default(),
                    pc.old_sentence.as_deref().unwrap_or("")
                ));
            }
            NodeChangeKind::Unchanged => {
                let ns = pc.new_sentence.as_deref().unwrap_or("");
                let os = pc.old_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("{np_str}{ns}"));
                old_parts.push(format!("{op_str}{os}"));
            }
            NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                let os = pc.old_sentence.as_deref().unwrap_or("");
                let ns = pc.new_sentence.as_deref().unwrap_or("");
                let (dn, do_) = if os != ns {
                    diff_clause_tokens(os, ns)
                } else {
                    (ns.to_string(), os.to_string())
                };
                new_parts.push(format!("{np_str}{dn}"));
                old_parts.push(format!("{op_str}{do_}"));
                render_item_changes(&pc.item_changes, omit_unchanged, new_parts, old_parts);
            }
        }
    }
}

fn render_item_changes(
    changes: &[ItemChange],
    omit_unchanged: bool,
    new_parts: &mut Vec<String>,
    old_parts: &mut Vec<String>,
) {
    let any_changed = changes.iter().any(|c| c.kind != NodeChangeKind::Unchanged);
    let mut skipped_unchanged = false;

    for ic in changes {
        if ic.kind == NodeChangeKind::Unchanged && omit_unchanged && any_changed {
            if !skipped_unchanged {
                new_parts.push("　（略）".to_string());
                old_parts.push("　（略）".to_string());
                skipped_unchanged = true;
            }
            continue;
        }
        skipped_unchanged = false;

        match ic.kind {
            NodeChangeKind::Added => {
                let nt = ic.new_title.as_deref().unwrap_or("");
                let ns = ic.new_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("　++{nt} {ns}++"));
                old_parts.push("　（新設）".to_string());
            }
            NodeChangeKind::Deleted => {
                let ot = ic.old_title.as_deref().unwrap_or("");
                let os = ic.old_sentence.as_deref().unwrap_or("");
                new_parts.push("　（削る）".to_string());
                old_parts.push(format!("　~~{ot} {os}~~"));
            }
            NodeChangeKind::Unchanged => {
                let nt = ic.new_title.as_deref().unwrap_or("");
                let ns = ic.new_sentence.as_deref().unwrap_or("");
                let ot = ic.old_title.as_deref().unwrap_or("");
                let os = ic.old_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("　{nt} {ns}"));
                old_parts.push(format!("　{ot} {os}"));
            }
            NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                let nt = ic.new_title.as_deref().unwrap_or("");
                let ot = ic.old_title.as_deref().unwrap_or("");
                let ns = ic.new_sentence.as_deref().unwrap_or("");
                let os = ic.old_sentence.as_deref().unwrap_or("");
                let (dn, do_) = if os != ns {
                    diff_clause_tokens(os, ns)
                } else {
                    (ns.to_string(), os.to_string())
                };
                new_parts.push(format!("　{nt} {dn}"));
                old_parts.push(format!("　{ot} {do_}"));
                render_subitem_changes(&ic.subitem_changes, omit_unchanged, 2, new_parts, old_parts);
            }
        }
    }
}

fn render_subitem_changes(
    changes: &[SubitemChange],
    omit_unchanged: bool,
    indent_level: usize,
    new_parts: &mut Vec<String>,
    old_parts: &mut Vec<String>,
) {
    let any_changed = changes.iter().any(|c| c.kind != NodeChangeKind::Unchanged);
    let mut skipped_unchanged = false;
    let indent = "　".repeat(indent_level);

    for sc in changes {
        if sc.kind == NodeChangeKind::Unchanged && omit_unchanged && any_changed {
            if !skipped_unchanged {
                new_parts.push(format!("{indent}（略）"));
                old_parts.push(format!("{indent}（略）"));
                skipped_unchanged = true;
            }
            continue;
        }
        skipped_unchanged = false;

        match sc.kind {
            NodeChangeKind::Added => {
                let nt = sc.new_title.as_deref().unwrap_or("");
                let ns = sc.new_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("{indent}++{nt} {ns}++"));
                old_parts.push(format!("{indent}（新設）"));
            }
            NodeChangeKind::Deleted => {
                let ot = sc.old_title.as_deref().unwrap_or("");
                let os = sc.old_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("{indent}（削る）"));
                old_parts.push(format!("{indent}~~{ot} {os}~~"));
            }
            NodeChangeKind::Unchanged => {
                let nt = sc.new_title.as_deref().unwrap_or("");
                let ns = sc.new_sentence.as_deref().unwrap_or("");
                let ot = sc.old_title.as_deref().unwrap_or("");
                let os = sc.old_sentence.as_deref().unwrap_or("");
                new_parts.push(format!("{indent}{nt} {ns}"));
                old_parts.push(format!("{indent}{ot} {os}"));
            }
            NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                let nt = sc.new_title.as_deref().unwrap_or("");
                let ot = sc.old_title.as_deref().unwrap_or("");
                let ns = sc.new_sentence.as_deref().unwrap_or("");
                let os = sc.old_sentence.as_deref().unwrap_or("");
                let (dn, do_) = if os != ns {
                    diff_clause_tokens(os, ns)
                } else {
                    (ns.to_string(), os.to_string())
                };
                new_parts.push(format!("{indent}{nt} {dn}"));
                old_parts.push(format!("{indent}{ot} {do_}"));
                render_subitem_changes(
                    &sc.children,
                    omit_unchanged,
                    indent_level + 1,
                    new_parts,
                    old_parts,
                );
            }
        }
    }
}

fn format_article_plain(art: &LawArticle, wrap: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(cap) = art.caption.as_deref().filter(|s| !s.is_empty()) {
        parts.push(cap.to_string());
    }
    for (idx, p) in art.paragraphs.iter().enumerate() {
        let line = if idx == 0 {
            format!("{} {}", art.title, p.sentence)
        } else {
            format!("{} {}", to_fullwidth_digits(&p.num), p.sentence)
        };
        parts.push(wrap_str(&line, wrap));
        for item in &p.items {
            let iline = format!("　{} {}", item.title, item.sentence);
            parts.push(wrap_str(&iline, wrap));
            for sub in &item.subitems {
                format_subitem_plain(sub, 2, wrap, &mut parts);
            }
        }
    }
    parts.join("<br>")
}

fn format_subitem_plain(
    sub: &LawSubitem,
    indent_level: usize,
    wrap: Option<&str>,
    parts: &mut Vec<String>,
) {
    let indent = "　".repeat(indent_level);
    let line = format!("{indent}{} {}", sub.title, sub.sentence);
    parts.push(wrap_str(&line, wrap));
    for child in &sub.children {
        format_subitem_plain(child, indent_level + 1, wrap, parts);
    }
}

fn wrap_str(s: &str, wrap: Option<&str>) -> String {
    match wrap {
        Some(w) => format!("{w}{s}{w}"),
        None => s.to_string(),
    }
}

/// Tokenize Japanese legal text by script runs (kanji, hiragana, katakana, ASCII, punctuation)
/// so LCS diff highlights coherent words/clauses rather than stray single characters.
pub(crate) fn tokenize_legal_clauses(text: &str) -> Vec<&str> {
    #[derive(PartialEq, Eq, Clone, Copy)]
    enum Class {
        Kanji,
        Hira,
        Kata,
        Ascii,
        Other,
    }
    let classify = |c: char| -> Class {
        match c {
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '々' => Class::Kanji,
            '\u{3041}'..='\u{309F}' => Class::Hira,
            '\u{30A1}'..='\u{30FF}' => Class::Kata,
            'a'..='z' | 'A'..='Z' | '0'..='9' | '０'..='９' => Class::Ascii,
            _ => Class::Other,
        }
    };
    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut prev: Option<Class> = None;
    let mut idx = 0usize;
    while idx < text.len() {
        let rest = &text[idx..];
        if let Some(conj) = ["並びに", "若しくは", "及び", "又は"]
            .into_iter()
            .find(|c| rest.starts_with(c))
        {
            if start < idx {
                tokens.push(&text[start..idx]);
            }
            tokens.push(&text[idx..idx + conj.len()]);
            idx += conj.len();
            start = idx;
            prev = None;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        let cls = classify(ch);
        if let Some(p) = prev {
            if cls != p || cls == Class::Other {
                tokens.push(&text[start..idx]);
                start = idx;
            }
        }
        prev = Some(cls);
        idx += ch.len_utf8();
    }
    if start < text.len() {
        tokens.push(&text[start..]);
    }
    tokens
}

pub(crate) fn diff_ops(old_text: &str, new_text: &str) -> Vec<DiffOp> {
    let a = tokenize_legal_clauses(old_text);
    let b = tokenize_legal_clauses(new_text);
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut ops: Vec<DiffOp> = Vec::new();
    let push_op = |ops: &mut Vec<DiffOp>, kind: u8, tok: &str| match (ops.last_mut(), kind) {
        (Some(DiffOp::Equal(s)), 0) => s.push_str(tok),
        (Some(DiffOp::Delete(s)), 1) => s.push_str(tok),
        (Some(DiffOp::Insert(s)), 2) => s.push_str(tok),
        (_, 0) => ops.push(DiffOp::Equal(tok.to_string())),
        (_, 1) => ops.push(DiffOp::Delete(tok.to_string())),
        _ => ops.push(DiffOp::Insert(tok.to_string())),
    };

    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        if a[i] == b[j] {
            push_op(&mut ops, 0, b[j]);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            push_op(&mut ops, 1, a[i]);
            i += 1;
        } else {
            push_op(&mut ops, 2, b[j]);
            j += 1;
        }
    }
    while i < m {
        push_op(&mut ops, 1, a[i]);
        i += 1;
    }
    while j < n {
        push_op(&mut ops, 2, b[j]);
        j += 1;
    }
    ops
}

fn diff_clause_tokens(old_text: &str, new_text: &str) -> (String, String) {
    let ops = diff_ops(old_text, new_text);
    let mut new_out = String::new();
    let mut old_out = String::new();
    for op in ops {
        match op {
            DiffOp::Equal(s) => {
                new_out.push_str(&s);
                old_out.push_str(&s);
            }
            DiffOp::Delete(s) => {
                old_out.push_str("~~");
                old_out.push_str(&s);
                old_out.push_str("~~");
            }
            DiffOp::Insert(s) => {
                new_out.push_str("++");
                new_out.push_str(&s);
                new_out.push_str("++");
            }
        }
    }
    (new_out, old_out)
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lawtext::parse_lawtext;

    #[test]
    fn table_cells_remain_on_one_markdown_row() {
        assert_eq!(escape_md_cell("甲|乙\r\n丙\n丁"), "甲\\|乙<br>丙<br>丁");
    }

    #[test]
    fn diffs_items_subitems_and_omits_unchanged() {
        let old_lt = "\
テスト法（令和七年法律第一号）
（定義）
第一条 この法律において用語を定める。
  一 行政機関 国の機関をいう。
  二 事業者 法人をいう。
    イ 株式会社
    ロ 合同会社
  三 国民 個人をいう。";
        let new_lt = "\
テスト法（令和七年法律第一号）
（定義）
第一条 この法律において用語を定める。
  一 行政機関 国の機関をいう。
  二 事業者 法人及び個人事業主をいう。
    イ 株式会社
    ロ 合同会社
    ハ 合名会社
  三 国民 個人をいう。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();

        let ir = diff_laws(&old_doc, &new_doc);
        assert_eq!(ir.article_changes.len(), 1);
        assert_eq!(ir.article_changes[0].kind, ArticleChangeKind::Modified);
        let json = ir.to_json_string().unwrap();
        assert!(json.contains("\"item_changes\""));
        assert!(json.contains("\"subitem_changes\""));

        let md = generate_shinkyu_markdown(&old_doc, &new_doc);
        assert!(md.contains("法人++及び個人事業主++をいう。"), "md:\n{md}");
        assert!(md.contains("++ハ 合名会社++"), "md:\n{md}");
        assert!(md.contains("（略）"), "unchanged items should be omitted:\n{md}");
    }

    #[test]
    fn detects_renumbered_articles_and_supports_old_on_left() {
        let old_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 目的を定める。
（雑則）
第二条 雑則を定める。";
        let new_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 目的を定める。
（定義）
第二条 定義を定める。
（雑則）
第三条 雑則及び罰則を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();
        let ir = diff_laws(&old_doc, &new_doc);

        assert_eq!(ir.article_changes.len(), 2);
        assert_eq!(ir.article_changes[0].kind, ArticleChangeKind::Added);
        assert_eq!(ir.article_changes[0].new_num.as_deref(), Some("2"));
        assert_eq!(ir.article_changes[1].kind, ArticleChangeKind::Renumbered);
        assert_eq!(ir.article_changes[1].old_num.as_deref(), Some("2"));
        assert_eq!(ir.article_changes[1].new_num.as_deref(), Some("3"));

        let md_old_left = generate_shinkyu_markdown_with_options(
            &old_doc,
            &new_doc,
            &ShinkyuOptions {
                new_on_left: false,
                ..Default::default()
            },
        );
        assert!(md_old_left.contains("| 現行 | 改正後 |"));
        assert!(md_old_left.contains("（新設）"));
        assert!(md_old_left.contains("++第三条++"));
        assert!(md_old_left.contains("~~第二条~~"));
    }

    #[test]
    fn diffs_suppl_provisions() {
        let old_lt = "\
テスト法（令和七年法律第一号）
第一条 本則。
附則
１ この法律は、公布の日から施行する。";
        let new_lt = "\
テスト法（令和七年法律第一号）
第一条 本則。
附則
１ この法律は、公布の日から起算して六月を超えない範囲内において施行する。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();
        let ir = diff_laws(&old_doc, &new_doc);
        assert_eq!(ir.suppl_changes.len(), 1);
        let md = generate_shinkyu_markdown(&old_doc, &new_doc);
        assert!(md.contains("**附則**"));
        assert!(md.contains("++起算して六月を超えない範囲内において++"));
    }
}
