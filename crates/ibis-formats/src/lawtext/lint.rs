// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Static legislative drafting linter (法制執務 Lint) for [`super::LawDoc`]:
//! - Dangling internal references and ripple-effect checks across renumbered/deleted articles
//! - Duplicate or out-of-order article numbers
//! - Conjunction hierarchy (`及び`/`並びに`, `又は`/`若しくは`)
//! - Duplicate abbreviation definitions (`以下「…」という。`)

use std::collections::HashMap;

use serde_json::{Value, json};

use super::diff::{ArticleChangeKind, diff_laws};
use super::refs::extract_references;
use super::{LawArticle, LawDoc, LawNode, LawSubitem};

/// Severity of a legislative lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

impl LintSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// A single diagnostic finding from [`lint_law`] or [`lint_amendment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    pub code: String,
    pub severity: LintSeverity,
    pub location: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl LintFinding {
    pub fn to_json_value(&self) -> Value {
        json!({
            "code": self.code,
            "severity": self.severity.as_str(),
            "location": self.location,
            "message": self.message,
            "suggestion": self.suggestion,
        })
    }
}

/// Summary report returned by [`lint_law`] and [`lint_amendment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintReport {
    pub findings: Vec<LintFinding>,
    pub error_count: usize,
    pub warning_count: usize,
}

impl LintReport {
    fn from_findings(findings: Vec<LintFinding>) -> Self {
        let error_count = findings
            .iter()
            .filter(|f| f.severity == LintSeverity::Error)
            .count();
        let warning_count = findings
            .iter()
            .filter(|f| f.severity == LintSeverity::Warning)
            .count();
        Self {
            findings,
            error_count,
            warning_count,
        }
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "error_count": self.error_count,
            "warning_count": self.warning_count,
            "findings": self.findings.iter().map(LintFinding::to_json_value).collect::<Vec<_>>(),
        })
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# 法制執務 Lint レポート\n\n");
        if self.findings.is_empty() {
            out.push_str("問題は検出されませんでした。\n");
            return out;
        }
        out.push_str(&format!(
            "- エラー: {} 件 / 警告: {} 件\n\n",
            self.error_count, self.warning_count
        ));
        for f in &self.findings {
            let badge = match f.severity {
                LintSeverity::Error => "ERROR",
                LintSeverity::Warning => "WARN",
                LintSeverity::Info => "INFO",
            };
            out.push_str(&format!(
                "- **[{badge}]** `{}` ({}): {}\n",
                f.code, f.location, f.message
            ));
            if let Some(sug) = &f.suggestion {
                out.push_str(&format!("  - 修正候補: {sug}\n"));
            }
        }
        out
    }
}

/// Lint a single [`LawDoc`] for structural, reference, conjunction, and abbreviation issues.
pub fn lint_law(doc: &LawDoc) -> LintReport {
    let mut findings = Vec::new();
    let articles = collect_articles(&doc.main_provision);
    check_article_numbering(&articles, &mut findings);
    check_internal_references(doc, &articles, &mut findings);
    check_conjunctions_and_abbreviations(&articles, &mut findings);
    LintReport::from_findings(findings)
}

/// Lint an amended [`LawDoc`] (`new`) together with its baseline (`old`), adding
/// ripple-effect checks for deleted or renumbered articles.
pub fn lint_amendment(old: &LawDoc, new: &LawDoc) -> LintReport {
    let mut findings = lint_law(new).findings;
    let ir = diff_laws(old, new);
    let refs = extract_references(new);

    for ac in &ir.article_changes {
        match ac.kind {
            ArticleChangeKind::Deleted => {
                if let Some(old_num) = &ac.old_num {
                    let old_title = ac.old_title.as_deref().unwrap_or("削除条文");
                    for r in refs.iter().filter(|r| {
                        r.external_law.is_none()
                            && !r.is_relative
                            && r.target_article_num.as_deref() == Some(old_num.as_str())
                    }) {
                        findings.push(LintFinding {
                            code: "RIPPLE_DELETED_REF".to_string(),
                            severity: LintSeverity::Error,
                            location: format!("{} 第{}項", r.source_article, r.source_paragraph),
                            message: format!(
                                "削除された {old_title} を「{}」で参照したままになっています。",
                                r.raw_text
                            ),
                            suggestion: Some("参照を削除または代替条文へ修正してください。".to_string()),
                        });
                    }
                }
            }
            ArticleChangeKind::Renumbered => {
                if let (Some(old_num), Some(new_num)) = (&ac.old_num, &ac.new_num) {
                    let old_title = ac.old_title.as_deref().unwrap_or("");
                    let new_title = ac.new_title.as_deref().unwrap_or("");
                    for r in refs.iter().filter(|r| {
                        r.external_law.is_none()
                            && !r.is_relative
                            && r.target_article_num.as_deref() == Some(old_num.as_str())
                    }) {
                        findings.push(LintFinding {
                            code: "RIPPLE_RENUMBERED_REF".to_string(),
                            severity: LintSeverity::Warning,
                            location: format!("{} 第{}項", r.source_article, r.source_paragraph),
                            message: format!(
                                "{old_title} が {new_title} へ条ズレしていますが、旧条番号「{}」を参照している可能性があります。",
                                r.raw_text
                            ),
                            suggestion: Some(format!("{old_title} → {new_title} への参照更新を確認してください。")),
                        });
                    }
                    let _ = new_num;
                }
            }
            _ => {}
        }
    }

    LintReport::from_findings(findings)
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

fn parse_art_key(num: &str) -> Option<(u32, u32)> {
    let mut parts = num.split('_');
    let main = parts.next()?.parse::<u32>().ok()?;
    let sub = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    Some((main, sub))
}

fn check_article_numbering(articles: &[&LawArticle], findings: &mut Vec<LintFinding>) {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    let mut prev_key: Option<((u32, u32), &str)> = None;

    for art in articles {
        if let Some(first_title) = seen.get(art.num.as_str()) {
            findings.push(LintFinding {
                code: "DUPLICATE_ARTICLE_NUM".to_string(),
                severity: LintSeverity::Error,
                location: art.title.clone(),
                message: format!("条番号「{}」が重複しています（既出: {first_title}）。", art.title),
                suggestion: Some("枝番（第○条の二）または繰り下げを確認してください。".to_string()),
            });
        } else {
            seen.insert(art.num.as_str(), art.title.as_str());
        }

        if let Some(key) = parse_art_key(&art.num) {
            if let Some((pk, prev_title)) = prev_key {
                if key < pk {
                    findings.push(LintFinding {
                        code: "OUT_OF_ORDER_ARTICLE".to_string(),
                        severity: LintSeverity::Warning,
                        location: art.title.clone(),
                        message: format!(
                            "条番号の順序が逆転しています（{prev_title} の後に {}）。",
                            art.title
                        ),
                        suggestion: None,
                    });
                }
            }
            prev_key = Some((key, art.title.as_str()));
        }
    }
}

fn check_internal_references(
    doc: &LawDoc,
    articles: &[&LawArticle],
    findings: &mut Vec<LintFinding>,
) {
    let refs = extract_references(doc);
    for r in refs {
        if r.external_law.is_some() {
            continue;
        }
        let Some(target_num) = &r.target_article_num else {
            continue;
        };
        let target_art = articles.iter().find(|a| a.num == *target_num);
        match target_art {
            None => {
                findings.push(LintFinding {
                    code: "DANGLING_ARTICLE_REF".to_string(),
                    severity: LintSeverity::Error,
                    location: format!("{} 第{}項", r.source_article, r.source_paragraph),
                    message: format!("参照先「{}」に対応する条文が見つかりません。", r.raw_text),
                    suggestion: Some("条番号の誤記または条ズレ・削除漏れの可能性があります。".to_string()),
                });
            }
            Some(art) => {
                if let Some(target_p) = &r.target_paragraph_num {
                    if !art.paragraphs.iter().any(|p| p.num == *target_p) {
                        findings.push(LintFinding {
                            code: "DANGLING_PARAGRAPH_REF".to_string(),
                            severity: LintSeverity::Error,
                            location: format!("{} 第{}項", r.source_article, r.source_paragraph),
                            message: format!(
                                "参照先「{}」に対応する {} 第{}項が見つかりません。",
                                r.raw_text, art.title, target_p
                            ),
                            suggestion: Some("項番号の変更または削除漏れの可能性があります。".to_string()),
                        });
                    }
                }
            }
        }
    }
}

fn check_conjunctions_and_abbreviations(
    articles: &[&LawArticle],
    findings: &mut Vec<LintFinding>,
) {
    let mut abbr_defs: HashMap<String, String> = HashMap::new();

    for art in articles {
        for para in &art.paragraphs {
            let loc = format!("{} 第{}項", art.title, para.num);
            check_sentence_conjunctions(&para.sentence, &loc, findings);
            collect_abbreviations(&para.sentence, &loc, &mut abbr_defs, findings);
            for it in &para.items {
                let iloc = format!("{loc} 第{}号", it.title);
                check_sentence_conjunctions(&it.sentence, &iloc, findings);
                collect_abbreviations(&it.sentence, &iloc, &mut abbr_defs, findings);
                check_subitems_conj_abbr(&it.subitems, &iloc, &mut abbr_defs, findings);
            }
        }
    }
}

fn check_subitems_conj_abbr(
    subs: &[LawSubitem],
    parent_loc: &str,
    abbr_defs: &mut HashMap<String, String>,
    findings: &mut Vec<LintFinding>,
) {
    for sub in subs {
        let loc = format!("{parent_loc} {}", sub.title);
        check_sentence_conjunctions(&sub.sentence, &loc, findings);
        collect_abbreviations(&sub.sentence, &loc, abbr_defs, findings);
        check_subitems_conj_abbr(&sub.children, &loc, abbr_defs, findings);
    }
}

fn check_sentence_conjunctions(
    sentence: &str,
    loc: &str,
    findings: &mut Vec<LintFinding>,
) {
    if sentence.contains("並びに") && !sentence.contains("及び") {
        findings.push(LintFinding {
            code: "CONJUNCTION_NARABINI_WITHOUT_OYOBI".to_string(),
            severity: LintSeverity::Warning,
            location: loc.to_string(),
            message: "「並びに」が使用されていますが、下位の接続詞「及び」が見当たりません。".to_string(),
            suggestion: Some("単層の並列接続には「及び」を使用するのが法制執務の原則です。".to_string()),
        });
    }
    if sentence.contains("若しくは") && !sentence.contains("又は") {
        findings.push(LintFinding {
            code: "CONJUNCTION_MOSHIKUWA_WITHOUT_MATAWA".to_string(),
            severity: LintSeverity::Warning,
            location: loc.to_string(),
            message: "「若しくは」が使用されていますが、上位の接続詞「又は」が見当たりません。".to_string(),
            suggestion: Some("単層の選択接続には「又は」を使用するのが法制執務の原則です。".to_string()),
        });
    }
}

fn collect_abbreviations(
    sentence: &str,
    loc: &str,
    abbr_defs: &mut HashMap<String, String>,
    findings: &mut Vec<LintFinding>,
) {
    let mut rest = sentence;
    while let Some(pos) = rest.find("以下「") {
        let after = &rest[pos + "以下「".len()..];
        let Some(end) = after.find("」という") else {
            break;
        };
        let abbr = &after[..end];
        if !abbr.is_empty() {
            if let Some(first_loc) = abbr_defs.get(abbr) {
                findings.push(LintFinding {
                    code: "DUPLICATE_ABBREVIATION".to_string(),
                    severity: LintSeverity::Warning,
                    location: loc.to_string(),
                    message: format!(
                        "略称「{abbr}」が重複定義されています（初出: {first_loc}）。"
                    ),
                    suggestion: Some("条文ごとに別略称にするか、初出箇所のみ定義してください。".to_string()),
                });
            } else {
                abbr_defs.insert(abbr.to_string(), loc.to_string());
            }
        }
        rest = &after[end + "」という".len()..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lawtext::parse_lawtext;

    #[test]
    fn detects_dangling_and_ripple_references_and_conjunctions() {
        let old_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 この法律は、基本事項を定める。
（雑則）
第二条 第一条に規定する基本事項並びに手続を定める。";
        let new_lt = "\
テスト法（令和七年法律第一号）
（定義）
第一条 この法律において「処分」とは、処分（以下「特定処分」という。）をいう。
（目的）
第二条 この法律は、基本事項及び特定処分（以下「特定処分」という。）を定める。
（雑則）
第三条 第一条並びに第五条に規定する事項を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();

        let report = lint_amendment(&old_doc, &new_doc);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "DANGLING_ARTICLE_REF" && f.message.contains("第五条")),
            "findings: {:?}",
            report.findings
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "CONJUNCTION_NARABINI_WITHOUT_OYOBI"),
            "findings: {:?}",
            report.findings
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "DUPLICATE_ABBREVIATION" && f.message.contains("特定処分")),
            "findings: {:?}",
            report.findings
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "RIPPLE_RENUMBERED_REF"),
            "findings: {:?}",
            report.findings
        );
    }
}
