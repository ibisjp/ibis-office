// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rule-based 改め文 (amendment text) draft generator and reverse-application
//! verification engine for [`super::LawDoc`].

use serde_json::{Value, json};

use super::diff::{
    AmendmentIr, ArticleChange, ArticleChangeKind, DiffOp, ItemChange, NodeChangeKind, diff_laws,
};
use super::{
    LawArticle, LawDoc, LawItem, LawNode, LawParagraph, parse_article_header,
    parse_numbered_paragraph, to_fullwidth_digits, to_lawtext,
};
use crate::Error;

/// A single structured clause in a generated 改め文.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AratameClause {
    pub target: String,
    pub text: String,
}

/// Generated 改め文 draft from `old` and `new` [`LawDoc`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AratameDraft {
    pub header: String,
    pub clauses: Vec<AratameClause>,
    pub full_text: String,
}

impl AratameDraft {
    /// Render the 改め文 draft as Markdown ready to open in ibisOffice DOCX/PPTX.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {} 改正案（改め文ドラフト）\n\n", self.header));
        out.push_str(&self.full_text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// Serialize to JSON for AI Assistant context injection.
    pub fn to_json_value(&self) -> Value {
        json!({
            "header": self.header,
            "clauses": self.clauses.iter().map(|c| json!({
                "target": c.target,
                "text": c.text,
            })).collect::<Vec<_>>(),
            "full_text": self.full_text,
        })
    }
}

/// Result of applying a 改め文 to `old` and verifying against `expected_new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub is_exact_match: bool,
    pub applied_ops: usize,
    pub diagnostics: Vec<String>,
    pub simulated_new: LawDoc,
    pub residual_diff: AmendmentIr,
}

impl VerifyResult {
    /// Serialize verification outcome to JSON for AI Assistant feedback loops.
    pub fn to_json_value(&self) -> Value {
        json!({
            "is_exact_match": self.is_exact_match,
            "applied_ops": self.applied_ops,
            "diagnostics": self.diagnostics,
            "residual_diff": self.residual_diff.to_json_value(),
        })
    }

    pub fn to_json_string(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(&self.to_json_value())
            .map_err(|e| Error::new(format!("VerifyResult の JSON 変換に失敗しました: {e}")))
    }
}

/// Generate a rule-based 改め文 draft from `old` and `new` [`LawDoc`]s.
pub fn generate_aratame_draft(old: &LawDoc, new: &LawDoc) -> AratameDraft {
    let ir = diff_laws(old, new);
    let law_title = if !old.law_title.is_empty() {
        old.law_title.clone()
    } else if !ir.law_title.is_empty() {
        ir.law_title.clone()
    } else {
        "対象法令".to_string()
    };
    let header = if !ir.old_law_num.is_empty() {
        format!("{}（{}）の一部を次のように改正する。", law_title, ir.old_law_num)
    } else {
        format!("{}の一部を次のように改正する。", law_title)
    };

    let mut clauses = Vec::new();

    if let Some((old_t, new_t)) = &ir.title_change {
        clauses.push(AratameClause {
            target: "題名".to_string(),
            text: format!("題名中「{}」を「{}」に改める。", old_t, new_t),
        });
    }

    // Collect renumbered articles first, ordered properly:
    // - Shifting down (old < new, e.g. 2 -> 3): process from back to front.
    // - Shifting up (old > new, e.g. 3 -> 2): process from front to back.
    let mut shift_down: Vec<&ArticleChange> = Vec::new();
    let mut shift_up: Vec<&ArticleChange> = Vec::new();
    let mut other_articles: Vec<&ArticleChange> = Vec::new();

    for ac in &ir.article_changes {
        match ac.kind {
            ArticleChangeKind::Renumbered => {
                let on = parse_art_num_key(ac.old_num.as_deref().unwrap_or("0"));
                let nn = parse_art_num_key(ac.new_num.as_deref().unwrap_or("0"));
                if nn > on {
                    shift_down.push(ac);
                } else {
                    shift_up.push(ac);
                }
            }
            _ => other_articles.push(ac),
        }
    }

    // Process shift_down in reverse order
    shift_down.reverse();
    for ac in &shift_down {
        if let (Some(ot), Some(nt)) = (ac.old_title.as_deref(), ac.new_title.as_deref()) {
            clauses.push(AratameClause {
                target: ot.to_string(),
                text: format!("{ot}を{nt}とする。"),
            });
            let body_ops = render_article_body_ops(nt, ac, new);
            for text in body_ops {
                clauses.push(AratameClause {
                    target: nt.to_string(),
                    text,
                });
            }
        }
    }

    for ac in &shift_up {
        if let (Some(ot), Some(nt)) = (ac.old_title.as_deref(), ac.new_title.as_deref()) {
            clauses.push(AratameClause {
                target: ot.to_string(),
                text: format!("{ot}を{nt}とする。"),
            });
            let body_ops = render_article_body_ops(nt, ac, new);
            for text in body_ops {
                clauses.push(AratameClause {
                    target: nt.to_string(),
                    text,
                });
            }
        }
    }

    for ac in &other_articles {
        match ac.kind {
            ArticleChangeKind::Modified => {
                let art_title = ac
                    .new_title
                    .as_deref()
                    .or(ac.old_title.as_deref())
                    .unwrap_or("第○条");
                for text in render_article_body_ops(art_title, ac, new) {
                    clauses.push(AratameClause {
                        target: art_title.to_string(),
                        text,
                    });
                }
            }
            ArticleChangeKind::Deleted => {
                let art_title = ac.old_title.as_deref().unwrap_or("第○条");
                clauses.push(AratameClause {
                    target: art_title.to_string(),
                    text: format!("{art_title}を削る。"),
                });
            }
            ArticleChangeKind::Added => {
                let new_num = ac.new_num.as_deref().unwrap_or("");
                let new_art = find_article_by_num(&new.main_provision, new_num);
                let prev_title = find_prev_article_title(&new.main_provision, new_num);
                let art_title = ac.new_title.as_deref().unwrap_or("第○条");
                let body = new_art.map(format_article_for_aratame).unwrap_or_default();
                let text = if let Some(prev) = prev_title {
                    format!("{prev}の次に次の一条を加える。\n{body}")
                } else {
                    format!("{art_title}を次のように加える。\n{body}")
                };
                clauses.push(AratameClause {
                    target: art_title.to_string(),
                    text,
                });
            }
            ArticleChangeKind::Renumbered => {}
        }
    }

    for at in &ir.appdx_table_changes {
        match at.kind {
            NodeChangeKind::Modified => {
                let mut parts = Vec::new();
                for rc in &at.row_changes {
                    if rc.kind == NodeChangeKind::Modified {
                        let cols = rc.old_cells.len().max(rc.new_cells.len());
                        for c in 0..cols {
                            let oc = rc.old_cells.get(c).map(String::as_str).unwrap_or("");
                            let nc = rc.new_cells.get(c).map(String::as_str).unwrap_or("");
                            if oc != nc && !oc.is_empty() && !nc.is_empty() {
                                parts.push(format!("「{oc}」を「{nc}」に"));
                            }
                        }
                    }
                }
                if !parts.is_empty() {
                    let joined = join_aratame_parts(&parts, "改め、", "改める。");
                    clauses.push(AratameClause {
                        target: at.title.clone(),
                        text: format!("{}中{joined}", at.title),
                    });
                }
            }
            NodeChangeKind::Added => {
                clauses.push(AratameClause {
                    target: at.title.clone(),
                    text: format!("附則の次に次の別表を加える。\n{}", at.title),
                });
            }
            NodeChangeKind::Deleted => {
                clauses.push(AratameClause {
                    target: at.title.clone(),
                    text: format!("{}を削る。", at.title),
                });
            }
            _ => {}
        }
    }

    let mut full_text = String::new();
    full_text.push_str(&header);
    full_text.push('\n');
    for c in &clauses {
        full_text.push_str("  ");
        full_text.push_str(&c.text.replace('\n', "\n  "));
        full_text.push('\n');
    }

    AratameDraft {
        header,
        clauses,
        full_text,
    }
}

fn render_article_body_ops(art_title: &str, ac: &ArticleChange, new_doc: &LawDoc) -> Vec<String> {
    let mut ops = Vec::new();
    let multi_para = ac.paragraph_changes.len() > 1;
    let new_art = ac
        .new_num
        .as_deref()
        .and_then(|n| find_article_by_num(&new_doc.main_provision, n));

    for (idx, pc) in ac.paragraph_changes.iter().enumerate() {
        match pc.kind {
            NodeChangeKind::Unchanged => {}
            NodeChangeKind::Added => {
                let p_num = pc.new_num.as_deref().unwrap_or("2");
                let body = new_art
                    .and_then(|a| a.paragraphs.iter().find(|p| p.num == p_num))
                    .map(format_paragraph_for_aratame)
                    .unwrap_or_else(|| {
                        format!(
                            "{} {}",
                            to_fullwidth_digits(p_num),
                            pc.new_sentence.as_deref().unwrap_or("")
                        )
                    });
                ops.push(format!("{art_title}に次の一項を加える。\n{body}"));
            }
            NodeChangeKind::Deleted => {
                let p_num = pc.old_num.as_deref().unwrap_or("2");
                ops.push(format!(
                    "{art_title}第{}項を削る。",
                    to_kanji_or_num(p_num)
                ));
            }
            NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                let para_loc = if !multi_para && idx == 0 {
                    art_title.to_string()
                } else {
                    let p_num = pc.new_num.as_deref().or(pc.old_num.as_deref()).unwrap_or("1");
                    format!("{art_title}第{}項", to_kanji_or_num(p_num))
                };

                if !pc.sentence_diff.is_empty() {
                    let has_items = !pc.item_changes.is_empty();
                    let loc = if has_items {
                        format!("{para_loc}各号列記以外の部分")
                    } else {
                        para_loc.clone()
                    };
                    let old_s = pc.old_sentence.as_deref().unwrap_or("");
                    let replacements = extract_unique_replacements(old_s, &pc.sentence_diff);
                    if !replacements.is_empty() {
                        let parts: Vec<String> = replacements
                            .into_iter()
                            .map(|(o, n)| format!("「{o}」を「{n}」に"))
                            .collect();
                        let joined = join_aratame_parts(&parts, "改め、", "改める。");
                        ops.push(format!("{loc}中{joined}"));
                    }
                }

                for ic in &pc.item_changes {
                    render_item_op(&para_loc, ic, &mut ops);
                }
            }
        }
    }
    ops
}

fn render_item_op(para_loc: &str, ic: &ItemChange, ops: &mut Vec<String>) {
    match ic.kind {
        NodeChangeKind::Unchanged => {}
        NodeChangeKind::Added => {
            let it = ic.new_title.as_deref().unwrap_or("");
            let is = ic.new_sentence.as_deref().unwrap_or("");
            ops.push(format!("{para_loc}に次の一号を加える。\n{it} {is}"));
        }
        NodeChangeKind::Deleted => {
            let it = ic.old_title.as_deref().unwrap_or("");
            ops.push(format!("{para_loc}第{it}号を削る。"));
        }
        NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
            let it = ic.new_title.as_deref().or(ic.old_title.as_deref()).unwrap_or("");
            let item_loc = format!("{para_loc}第{it}号");
            if !ic.sentence_diff.is_empty() {
                let old_s = ic.old_sentence.as_deref().unwrap_or("");
                let replacements = extract_unique_replacements(old_s, &ic.sentence_diff);
                if !replacements.is_empty() {
                    let parts: Vec<String> = replacements
                        .into_iter()
                        .map(|(o, n)| format!("「{o}」を「{n}」に"))
                        .collect();
                    let joined = join_aratame_parts(&parts, "改め、", "改める。");
                    ops.push(format!("{item_loc}中{joined}"));
                }
            }
            for sc in &ic.subitem_changes {
                match sc.kind {
                    NodeChangeKind::Added => {
                        let st = sc.new_title.as_deref().unwrap_or("");
                        let ss = sc.new_sentence.as_deref().unwrap_or("");
                        ops.push(format!("{item_loc}に次のように加える。\n{st} {ss}"));
                    }
                    NodeChangeKind::Deleted => {
                        let st = sc.old_title.as_deref().unwrap_or("");
                        ops.push(format!("{item_loc}{st}を削る。"));
                    }
                    NodeChangeKind::Modified | NodeChangeKind::Renumbered => {
                        let st = sc.new_title.as_deref().or(sc.old_title.as_deref()).unwrap_or("");
                        let old_s = sc.old_sentence.as_deref().unwrap_or("");
                        let replacements = extract_unique_replacements(old_s, &sc.sentence_diff);
                        if !replacements.is_empty() {
                            let parts: Vec<String> = replacements
                                .into_iter()
                                .map(|(o, n)| format!("「{o}」を「{n}」に"))
                                .collect();
                            let joined = join_aratame_parts(&parts, "改め、", "改める。");
                            ops.push(format!("{item_loc}{st}中{joined}"));
                        }
                    }
                    NodeChangeKind::Unchanged => {}
                }
            }
        }
    }
}

fn join_aratame_parts(parts: &[String], mid_suffix: &str, end_suffix: &str) -> String {
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        out.push_str(p);
        if i + 1 < parts.len() {
            out.push_str(mid_suffix);
        } else {
            out.push_str(end_suffix);
        }
    }
    out
}

/// Extract `(old_phrase, new_phrase)` pairs from `DiffOp`s, expanding with minimal
/// surrounding `Equal` context whenever `old_phrase` is empty or occurs multiple times
/// in `old_sentence`.
fn extract_unique_replacements(old_sentence: &str, ops: &[DiffOp]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut i = 0usize;
    while i < ops.len() {
        if matches!(ops[i], DiffOp::Equal(_)) {
            i += 1;
            continue;
        }
        let mut del = String::new();
        let mut ins = String::new();
        let start = i;
        while i < ops.len() && !matches!(ops[i], DiffOp::Equal(_)) {
            match &ops[i] {
                DiffOp::Delete(s) => del.push_str(s),
                DiffOp::Insert(s) => ins.push_str(s),
                DiffOp::Equal(_) => {}
            }
            i += 1;
        }
        let prev_eq = if start > 0 {
            match &ops[start - 1] {
                DiffOp::Equal(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        };
        let next_eq = if i < ops.len() {
            match &ops[i] {
                DiffOp::Equal(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        };

        let mut old_p = del;
        let mut new_p = ins;
        if (old_p.is_empty() || count_occurrences(old_sentence, &old_p) != 1) && prev_eq.is_some()
        {
            if let Some(p) = prev_eq {
                let tail: String = p.chars().rev().take(6).collect::<String>().chars().rev().collect();
                old_p = format!("{tail}{old_p}");
                new_p = format!("{tail}{new_p}");
            }
        }
        if (old_p.is_empty() || count_occurrences(old_sentence, &old_p) != 1) && next_eq.is_some()
        {
            if let Some(n) = next_eq {
                let head: String = n.chars().take(6).collect();
                old_p = format!("{old_p}{head}");
                new_p = format!("{new_p}{head}");
            }
        }
        if !old_p.is_empty() || !new_p.is_empty() {
            result.push((old_p, new_p));
        }
    }
    result
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.match_indices(needle).count()
}

/// Apply a 改め文 text to `old` [`LawDoc`] and verify the result against `expected_new`.
pub fn verify_aratame(old: &LawDoc, aratame_text: &str, expected_new: &LawDoc) -> VerifyResult {
    let (simulated_new, applied_ops, mut diagnostics) = apply_aratame(old, aratame_text);
    let residual_diff = diff_laws(&simulated_new, expected_new);
    for (scope, matches) in [
        ("題名", simulated_new.law_title == expected_new.law_title),
        ("制定文", simulated_new.enact_statement == expected_new.enact_statement),
        ("前文", simulated_new.preamble == expected_new.preamble),
        ("本則", simulated_new.main_provision == expected_new.main_provision),
        ("附則", simulated_new.suppl_provisions == expected_new.suppl_provisions),
        ("別表", simulated_new.appdx_tables == expected_new.appdx_tables),
    ] {
        if !matches {
            diagnostics.push(format!("{scope}に逆適用後の差分が残っています。"));
        }
    }
    if !expected_new.law_num.is_empty()
        && !simulated_new.law_num.is_empty()
        && simulated_new.law_num != expected_new.law_num
    {
        diagnostics.push("現行と改正後の法令番号が一致しません。".into());
    }
    let is_exact_match = diagnostics.is_empty();

    VerifyResult {
        is_exact_match,
        applied_ops,
        diagnostics,
        simulated_new,
        residual_diff,
    }
}

/// Apply a 改め文 text to `old` [`LawDoc`], returning `(new_doc, applied_ops, diagnostics)`.
pub fn apply_aratame(old: &LawDoc, aratame_text: &str) -> (LawDoc, usize, Vec<String>) {
    let mut doc = old.clone();
    let mut applied = 0usize;
    let mut diagnostics = Vec::new();

    let lines: Vec<&str> = aratame_text.lines().collect();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx].trim();
        idx += 1;
        if line.is_empty() || line.ends_with("の一部を次のように改正する。") {
            continue;
        }

        // 1. 題名改正: 題名中「X」を「Y」に改める。
        if let Some(rest) = line.strip_prefix("題名中") {
            let pairs = parse_quote_replacements(rest);
            for (o, n) in pairs {
                if apply_replace_in_string(&mut doc.law_title, &o, &n, "題名", &mut diagnostics) {
                    applied += 1;
                }
            }
            continue;
        }

        // 2. 条の移動: 第○条を第△条とする。
        if let Some((from_art, to_art)) = parse_article_renumber(line) {
            if let Some(art) = find_article_mut_by_title(&mut doc.main_provision, &from_art) {
                art.title = to_art.clone();
                if let Some((_, num, _)) = parse_article_header(&to_art) {
                    art.num = num;
                }
                applied += 1;
            } else {
                diagnostics.push(format!("移動対象の条文「{from_art}」が見つかりません"));
            }
            continue;
        }

        // 3. 条の削除: 第○条を削る。
        if let Some(art_title) = line.strip_suffix("を削る。") {
            if !art_title.contains('項') && !art_title.contains('号') {
                if remove_article_by_title(&mut doc.main_provision, art_title) {
                    applied += 1;
                } else {
                    diagnostics.push(format!("削除対象の条文「{art_title}」が見つかりません"));
                }
                continue;
            }
        }

        // 4. 条の追加: 第○条の次に次の一条を加える。
        if let Some(prev_title) = line.strip_suffix("の次に次の一条を加える。") {
            let mut block_lines = Vec::new();
            while idx < lines.len() {
                let next_l = lines[idx].trim();
                if next_l.is_empty() || is_aratame_command_line(next_l) {
                    break;
                }
                block_lines.push(next_l);
                idx += 1;
            }
            if let Some(new_art) = parse_inserted_article_block(&block_lines) {
                if insert_article_after(&mut doc.main_provision, prev_title, new_art) {
                    applied += 1;
                } else {
                    diagnostics.push(format!("挿入基準の条文「{prev_title}」が見つかりません"));
                }
            } else {
                diagnostics.push(format!("「{prev_title}の次に次の一条を加える」の後続条文を解析できません"));
            }
            continue;
        }

        // 5. 項の追加: 第○条に次の一項を加える。
        if let Some(art_title) = line.strip_suffix("に次の一項を加える。") {
            let mut block_lines = Vec::new();
            while idx < lines.len() {
                let next_l = lines[idx].trim();
                if next_l.is_empty() || is_aratame_command_line(next_l) {
                    break;
                }
                block_lines.push(next_l);
                idx += 1;
            }
            if let Some(new_para) = parse_inserted_paragraph_block(&block_lines) {
                if let Some(art) = find_article_mut_by_title(&mut doc.main_provision, art_title) {
                    art.paragraphs.push(new_para);
                    applied += 1;
                } else {
                    diagnostics.push(format!("項追加対象の条文「{art_title}」が見つかりません"));
                }
            } else {
                diagnostics.push(format!("「{art_title}に次の一項を加える」の後続項を解析できません"));
            }
            continue;
        }

        // 6. 号の追加: 第○条(第○項)に次の一号を加える。
        if let Some(target) = line.strip_suffix("に次の一号を加える。") {
            let mut block_lines = Vec::new();
            while idx < lines.len() {
                let next_l = lines[idx].trim();
                if next_l.is_empty() || is_aratame_command_line(next_l) {
                    break;
                }
                block_lines.push(next_l);
                idx += 1;
            }
            if let Some((art_t, para_n)) = parse_art_para_target(target) {
                if let Some(first_l) = block_lines.first() {
                    if let Some((it_title, it_sent)) = super::split_leading_token(first_l) {
                        if let Some(art) = find_article_mut_by_title(&mut doc.main_provision, &art_t)
                        {
                            if let Some(para) = find_para_mut(art, para_n.as_deref()) {
                                let num = (para.items.len() + 1).to_string();
                                para.items.push(LawItem {
                                    num,
                                    title: it_title.to_string(),
                                    sentence: it_sent.to_string(),
                                    subitems: Vec::new(),
                                });
                                applied += 1;
                            } else {
                                diagnostics.push(format!("対象項が見つかりません: {target}"));
                            }
                        } else {
                            diagnostics.push(format!("対象条文が見つかりません: {target}"));
                        }
                    }
                }
            }
            continue;
        }

        // 7. 字句置換: <target>中「X」を「Y」に改め(る)。
        if let Some((target, rest)) = line.split_once("中「") {
            let pairs = parse_quote_replacements(&format!("「{rest}"));
            if !pairs.is_empty() {
                if target.starts_with("別表") {
                    if let Some(tbl) = doc.appdx_tables.iter_mut().find(|t| t.title == target) {
                        for (o, n) in pairs {
                            let mut replaced = false;
                            for row in &mut tbl.rows {
                                for cell in row.iter_mut() {
                                    if cell.contains(&o) {
                                        *cell = cell.replacen(&o, &n, 1);
                                        applied += 1;
                                        replaced = true;
                                        break;
                                    }
                                }
                                if replaced {
                                    break;
                                }
                            }
                            if !replaced {
                                diagnostics.push(format!("{target} に語句「{o}」が見つかりません"));
                            }
                        }
                    } else {
                        diagnostics.push(format!("対象の別表が見つかりません: {target}"));
                    }
                    continue;
                }
                if let Some((art_t, para_n, item_t, sub_t)) = parse_full_target(target) {
                    if let Some(art) = find_article_mut_by_title(&mut doc.main_provision, &art_t) {
                        if let Some(para) = find_para_mut(art, para_n.as_deref()) {
                            let target_str_opt: Option<&mut String> =
                                match (item_t.as_deref(), sub_t.as_deref()) {
                                    (Some(it), Some(st)) => para
                                        .items
                                        .iter_mut()
                                        .find(|i| i.title == it)
                                        .and_then(|i| {
                                            i.subitems.iter_mut().find(|s| s.title == st)
                                        })
                                        .map(|s| &mut s.sentence),
                                    (Some(it), None) => para
                                        .items
                                        .iter_mut()
                                        .find(|i| i.title == it)
                                        .map(|i| &mut i.sentence),
                                    (None, _) => Some(&mut para.sentence),
                                };
                            if let Some(s) = target_str_opt {
                                for (o, n) in pairs {
                                    if apply_replace_in_string(
                                        s,
                                        &o,
                                        &n,
                                        target,
                                        &mut diagnostics,
                                    ) {
                                        applied += 1;
                                    }
                                }
                            } else {
                                diagnostics.push(format!("対象の号/イロハが見つかりません: {target}"));
                            }
                        } else {
                            diagnostics.push(format!("対象の項が見つかりません: {target}"));
                        }
                    } else {
                        diagnostics.push(format!("対象の条文が見つかりません: {target}"));
                    }
                }
            }
        }
    }

    (doc, applied, diagnostics)
}

fn apply_replace_in_string(
    target: &mut String,
    old: &str,
    new: &str,
    label: &str,
    diagnostics: &mut Vec<String>,
) -> bool {
    let count = count_occurrences(target, old);
    if count == 0 {
        diagnostics.push(format!("{label} に語句「{old}」が見つかりません"));
        false
    } else if count > 1 {
        diagnostics.push(format!(
            "{label} に語句「{old}」が {count} 箇所あり一意に特定できません"
        ));
        false
    } else {
        *target = target.replacen(old, new, 1);
        true
    }
}

fn parse_quote_replacements(s: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut rest = s;
    while let Some(o_start) = rest.find('「') {
        let after_o = &rest[o_start + '「'.len_utf8()..];
        let Some(o_end) = after_o.find('」') else {
            break;
        };
        let old_p = &after_o[..o_end];
        let after_mid = &after_o[o_end + '」'.len_utf8()..];
        let Some(n_start) = after_mid.find('「') else {
            break;
        };
        let after_n = &after_mid[n_start + '「'.len_utf8()..];
        let Some(n_end) = after_n.find('」') else {
            break;
        };
        let new_p = &after_n[..n_end];
        pairs.push((old_p.to_string(), new_p.to_string()));
        rest = &after_n[n_end + '」'.len_utf8()..];
    }
    pairs
}

fn parse_article_renumber(line: &str) -> Option<(String, String)> {
    let body = line.strip_suffix("とする。")?;
    let (from, to) = body.split_once("を")?;
    if from.starts_with('第') && from.contains('条') && to.starts_with('第') && to.contains('条')
    {
        Some((from.trim().to_string(), to.trim().to_string()))
    } else {
        None
    }
}

fn is_aratame_command_line(line: &str) -> bool {
    line.ends_with("改める。")
        || line.ends_with("削る。")
        || line.ends_with("とする。")
        || line.ends_with("加える。")
}

fn parse_art_para_target(target: &str) -> Option<(String, Option<String>)> {
    let clean = target.trim().trim_end_matches("各号列記以外の部分");
    if let Some(t_end) = clean.find('条') {
            let mut art_end = t_end + '条'.len_utf8();
            let rem = &clean[art_end..];
            if let Some(after_no) = rem.strip_prefix('の') {
                let digits_len: usize = after_no
                    .chars()
                    .take_while(|c| super::is_kanji_digit(*c))
                    .map(char::len_utf8)
                    .sum();
                if digits_len > 0 {
                    art_end += 'の'.len_utf8() + digits_len;
                }
            }
            let art_t = clean[..art_end].to_string();
            let tail = &clean[art_end..];
            if let Some(p_start) = tail.find('第') {
                if let Some(p_end) = tail[p_start..].find('項') {
                    let p_kanji = &tail[p_start + '第'.len_utf8()..p_start + p_end];
                    let p_num = super::kanji_num_to_ascii(p_kanji).unwrap_or_else(|| p_kanji.to_string());
                    return Some((art_t, Some(p_num)));
                }
            }
            return Some((art_t, None));
    }
    None
}

fn parse_full_target(target: &str) -> Option<(String, Option<String>, Option<String>, Option<String>)> {
    let target = target.trim().trim_end_matches("各号列記以外の部分");
    let (art_t, para_n) = parse_art_para_target(target)?;
    let mut item_t = None;
    let mut sub_t = None;
    if let Some(pos) = target.rfind('第') {
        let tail = &target[pos..];
        if let Some(g_pos) = tail.find('号') {
            let inner = &tail['第'.len_utf8()..g_pos];
            item_t = Some(inner.to_string());
            let after_g = tail[g_pos + '号'.len_utf8()..].trim();
            if !after_g.is_empty() && !after_g.starts_with("各号") {
                sub_t = Some(after_g.to_string());
            }
        }
    }
    Some((art_t, para_n, item_t, sub_t))
}

fn parse_inserted_article_block(lines: &[&str]) -> Option<LawArticle> {
    let mut caption = None;
    let mut idx = 0usize;
    if let Some(first) = lines.first() {
        if first.starts_with('（') && first.ends_with('）') {
            caption = Some(first.to_string());
            idx = 1;
        }
    }
    let art_line = lines.get(idx)?;
    let (title, num, rest) = parse_article_header(art_line)?;
    let mut paragraphs = Vec::new();
    if !rest.is_empty() {
        paragraphs.push(LawParagraph {
            num: "1".to_string(),
            sentence: rest.to_string(),
            items: Vec::new(),
        });
    }
    for l in &lines[idx + 1..] {
        if let Some((p_num, p_rest)) = parse_numbered_paragraph(l) {
            paragraphs.push(LawParagraph {
                num: p_num,
                sentence: p_rest.to_string(),
                items: Vec::new(),
            });
        }
    }
    Some(LawArticle {
        num,
        caption,
        title,
        paragraphs,
    })
}

fn parse_inserted_paragraph_block(lines: &[&str]) -> Option<LawParagraph> {
    let first = lines.first()?;
    let (p_num, rest) = parse_numbered_paragraph(first).unwrap_or(("2".to_string(), *first));
    Some(LawParagraph {
        num: p_num,
        sentence: rest.to_string(),
        items: Vec::new(),
    })
}

fn find_article_by_num<'a>(nodes: &'a [LawNode], num: &str) -> Option<&'a LawArticle> {
    for n in nodes {
        match n {
            LawNode::Division { children, .. } => {
                if let Some(a) = find_article_by_num(children, num) {
                    return Some(a);
                }
            }
            LawNode::Article(a) if a.num == num => return Some(a),
            _ => {}
        }
    }
    None
}

fn find_prev_article_title(nodes: &[LawNode], target_num: &str) -> Option<String> {
    let mut prev: Option<String> = None;
    fn walk(nodes: &[LawNode], target_num: &str, prev: &mut Option<String>) -> Option<String> {
        for n in nodes {
            match n {
                LawNode::Division { children, .. } => {
                    if let Some(found) = walk(children, target_num, prev) {
                        return Some(found);
                    }
                }
                LawNode::Article(a) => {
                    if a.num == target_num {
                        return prev.clone();
                    }
                    *prev = Some(a.title.clone());
                }
                LawNode::Paragraph(_) => {}
            }
        }
        None
    }
    walk(nodes, target_num, &mut prev)
}

fn find_article_mut_by_title<'a>(
    nodes: &'a mut [LawNode],
    title: &str,
) -> Option<&'a mut LawArticle> {
    for n in nodes {
        match n {
            LawNode::Division { children, .. } => {
                if let Some(a) = find_article_mut_by_title(children, title) {
                    return Some(a);
                }
            }
            LawNode::Article(a) if a.title == title => return Some(a),
            _ => {}
        }
    }
    None
}

fn find_para_mut<'a>(art: &'a mut LawArticle, para_num: Option<&str>) -> Option<&'a mut LawParagraph> {
    match para_num {
        Some(n) => art.paragraphs.iter_mut().find(|p| p.num == n),
        None => art.paragraphs.first_mut(),
    }
}

fn remove_article_by_title(nodes: &mut Vec<LawNode>, title: &str) -> bool {
    if let Some(pos) = nodes
        .iter()
        .position(|n| matches!(n, LawNode::Article(a) if a.title == title))
    {
        nodes.remove(pos);
        return true;
    }
    for n in nodes.iter_mut() {
        if let LawNode::Division { children, .. } = n {
            if remove_article_by_title(children, title) {
                return true;
            }
        }
    }
    false
}

fn insert_article_after(nodes: &mut Vec<LawNode>, prev_title: &str, new_art: LawArticle) -> bool {
    if let Some(pos) = nodes
        .iter()
        .position(|n| matches!(n, LawNode::Article(a) if a.title == prev_title))
    {
        nodes.insert(pos + 1, LawNode::Article(new_art));
        return true;
    }
    for n in nodes.iter_mut() {
        if let LawNode::Division { children, .. } = n {
            if insert_article_after(children, prev_title, new_art.clone()) {
                return true;
            }
        }
    }
    false
}

fn format_article_for_aratame(art: &LawArticle) -> String {
    let mut doc = LawDoc::default();
    doc.main_provision.push(LawNode::Article(art.clone()));
    to_lawtext(&doc).trim().to_string()
}

fn format_paragraph_for_aratame(para: &LawParagraph) -> String {
    let mut out = format!("{} {}", to_fullwidth_digits(&para.num), para.sentence);
    for it in &para.items {
        out.push_str(&format!("\n  {} {}", it.title, it.sentence));
    }
    out
}

fn parse_art_num_key(num: &str) -> (u32, u32) {
    let mut parts = num.split('_');
    let main = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let sub = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    (main, sub)
}

fn to_kanji_or_num(num: &str) -> String {
    match num {
        "1" => "一".to_string(),
        "2" => "二".to_string(),
        "3" => "三".to_string(),
        "4" => "四".to_string(),
        "5" => "五".to_string(),
        "6" => "六".to_string(),
        "7" => "七".to_string(),
        "8" => "八".to_string(),
        "9" => "九".to_string(),
        "10" => "十".to_string(),
        other => to_fullwidth_digits(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lawtext::parse_lawtext;

    #[test]
    fn resolves_paragraph_text_before_enumerated_items() {
        assert_eq!(
            parse_full_target("第二条第一項各号列記以外の部分"),
            Some(("第二条".into(), Some("1".into()), None, None))
        );
    }

    #[test]
    fn generates_and_verifies_aratame_roundtrip() {
        let old_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 この法律は、処分及び届出に関する手続を定める。
（雑則）
第二条 雑則を定める。";
        let new_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 この法律は、処分、行政指導及び届出に関する手続を定める。
（定義）
第二条 この法律において用語の意義を定める。
（雑則）
第三条 雑則を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();

        let draft = generate_aratame_draft(&old_doc, &new_doc);
        assert!(draft.full_text.contains("テスト法（令和七年法律第一号）の一部を次のように改正する。"));
        assert!(draft.full_text.contains("第二条を第三条とする。"));
        assert!(draft.full_text.contains("第一条の次に次の一条を加える。"));

        let verify = verify_aratame(&old_doc, &draft.full_text, &new_doc);
        assert!(
            verify.is_exact_match,
            "expected exact match, diagnostics={:?}, residual={:?}",
            verify.diagnostics,
            verify.residual_diff
        );
    }

    #[test]
    fn detects_mismatch_in_aratame_verification() {
        let old_lt = "\
テスト法（令和七年法律第一号）
第一条 この法律は、処分に関する手続を定める。";
        let new_lt = "\
テスト法（令和七年法律第一号）
第一条 この法律は、処分及び届出に関する手続を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();

        let broken_aratame = "テスト法（令和七年法律第一号）の一部を次のように改正する。\n  第一条中「存在しない語」を「届出」に改める。";
        let verify = verify_aratame(&old_doc, broken_aratame, &new_doc);
        assert!(!verify.is_exact_match);
        assert!(!verify.diagnostics.is_empty());
    }

    #[test]
    fn generates_and_verifies_appdx_table_amendment() {
        let old_lt = "\
手数料法（令和七年法律第一号）
第一条 手数料を定める。

別表第一（第一条関係）

| 区分 | 金額 |
| 電子申請 | 三百円 |";
        let new_lt = "\
手数料法（令和七年法律第一号）
第一条 手数料を定める。

別表第一（第一条関係）

| 区分 | 金額 |
| 電子申請 | 五百円 |";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();

        let draft = generate_aratame_draft(&old_doc, &new_doc);
        assert!(draft.full_text.contains("別表第一中「三百円」を「五百円」に改める。"));
        let verify = verify_aratame(&old_doc, &draft.full_text, &new_doc);
        assert!(verify.is_exact_match, "diagnostics={:?}", verify.diagnostics);
    }
}
