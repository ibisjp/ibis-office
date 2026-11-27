// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 5点セット（新旧対照表・改め文・参照条文・要綱・理由）の統合オーケストレーションおよび
//! ibisOffice 内 AI Assistant 連携用コンテキスト・プロンプト生成モジュール。

use std::fmt::Write as _;

use serde_json::{Value, json};

use super::LawDoc;
use super::aratame::{AratameDraft, VerifyResult, generate_aratame_draft, verify_aratame};
use super::diff::{
    AmendmentIr, ArticleChangeKind, ShinkyuOptions, diff_laws,
    generate_shinkyu_markdown_with_options,
};
use super::lint::{LintReport, lint_amendment};
use super::refs::{LawReference, extract_references, generate_sansho_markdown_with_sources};

/// Deterministic outputs + AI Assistant prompt package for the 5-piece legislative set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legislation5SetPackage {
    pub ir: AmendmentIr,
    pub shinkyu_markdown: String,
    pub aratame_draft: AratameDraft,
    pub aratame_verification: VerifyResult,
    pub sansho_markdown: String,
    pub references: Vec<LawReference>,
    pub lint_report: LintReport,
    pub bill_markdown: String,
    pub youkou_markdown: String,
    pub riyuu_markdown: String,
    pub youkou_prompt: String,
    pub riyuu_prompt: String,
    pub aratame_polish_prompt: String,
}

impl Legislation5SetPackage {
    /// Render the five drafts and review notes as one Markdown document.
    pub fn to_markdown(&self) -> String {
        let mut out = [
            self.youkou_markdown.trim_end(),
            self.bill_markdown.trim_end(),
            self.riyuu_markdown.trim_end(),
            self.shinkyu_markdown.trim_end(),
            self.sansho_markdown.trim_end(),
        ]
        .join("\n\n---\n\n");
        out.push_str("\n\n---\n\n# 確認結果\n\n");
        if self.aratame_verification.is_exact_match {
            out.push_str("改め文の逆適用結果は、解析した改正後の条文構造と一致しました。\n\n");
        } else {
            out.push_str("改め文の逆適用結果に要確認事項があります。\n\n");
            for diagnostic in &self.aratame_verification.diagnostics {
                let _ = writeln!(out, "- {diagnostic}");
            }
            out.push('\n');
        }
        out.push_str(&self.lint_report.to_markdown());
        out
    }

    /// Serialize the entire 5-piece package into JSON for the in-app AI Assistant
    /// or Cloudflare Worker backend.
    pub fn to_json_value(&self) -> Value {
        json!({
            "law_title": self.ir.law_title,
            "amendment_ir": self.ir.to_json_value(),
            "deterministic_outputs": {
                "shinkyu_markdown": self.shinkyu_markdown,
                "aratame_draft": self.aratame_draft.to_json_value(),
                "aratame_verification": self.aratame_verification.to_json_value(),
                "sansho_markdown": self.sansho_markdown,
                "references": self.references.iter().map(LawReference::to_json_value).collect::<Vec<_>>(),
                "lint_report": self.lint_report.to_json_value(),
            },
            "documents": {
                "youkou": self.youkou_markdown,
                "bill": self.bill_markdown,
                "riyuu": self.riyuu_markdown,
                "shinkyu": self.shinkyu_markdown,
                "sansho": self.sansho_markdown,
            },
            "assistant_prompts": {
                "youkou": self.youkou_prompt,
                "riyuu": self.riyuu_prompt,
                "aratame_polish": self.aratame_polish_prompt,
            }
        })
    }
}

/// Build a [`Legislation5SetPackage`] from `old` and `new` [`LawDoc`]s plus an optional
/// policy background memo (`policy_memo`) provided by the user in the AI Assistant.
pub fn build_5set_package(
    old: &LawDoc,
    new: &LawDoc,
    policy_memo: &str,
    shinkyu_opts: &ShinkyuOptions,
) -> Legislation5SetPackage {
    build_5set_package_with_sources(old, new, policy_memo, shinkyu_opts, &[])
}

/// Build five legislative drafts using explicit snapshots of cited external laws.
pub fn build_5set_package_with_sources(
    old: &LawDoc,
    new: &LawDoc,
    policy_memo: &str,
    shinkyu_opts: &ShinkyuOptions,
    sources: &[LawDoc],
) -> Legislation5SetPackage {
    let ir = diff_laws(old, new);
    let shinkyu_markdown = generate_shinkyu_markdown_with_options(old, new, shinkyu_opts);
    let aratame_draft = generate_aratame_draft(old, new);
    let aratame_verification = verify_aratame(old, &aratame_draft.full_text, new);
    let references = extract_references(new);
    let sansho_markdown = generate_sansho_markdown_with_sources(old, new, sources);
    let lint_report = lint_amendment(old, new);

    let subject = if !old.law_title.is_empty() {
        &old.law_title
    } else if !ir.law_title.is_empty() {
        &ir.law_title
    } else {
        "対象法令"
    };
    let bill_title = format!("{subject}の一部を改正する法律案");
    let ir_summary = ir.to_json_string().unwrap_or_default();
    let memo_section = if policy_memo.trim().is_empty() {
        "（担当者メモなし。改正趣旨・政策判断は要確認として残してください。）".to_string()
    } else {
        policy_memo.trim().to_string()
    };

    let youkou_prompt = format!(
        "あなたは日本の法制執務を支援するアシスタントです。\n\
         以下の法令改正の構造化差分（AmendmentIr）と担当者メモに基づき、「{bill_title}要綱」のドラフトをMarkdownで作成してください。\n\
         体裁は日本の標準的な要綱文体（「第一 目的」「第二 〇〇に関する事項」「一 〜すること。」の体言止め）に従ってください。メモで裏付けられない政策上の記述は要確認としてください。\n\n\
         ## 担当者メモ\n{}\n\n## 構造化差分 (AmendmentIr)\n```json\n{}\n```",
        memo_section, ir_summary
    );

    let riyuu_prompt = format!(
        "あなたは日本の法制執務を支援するアシスタントです。\n\
         以下の構造化差分と担当者メモに基づき、法律案の「理由」（提案理由）を起案してください。\n\
         末尾は標準的な「〜する必要がある。これが、この法律案を提出する理由である。」の形式で結んでください。メモで裏付けられない必要性は要確認としてください。\n\n\
         ## 担当者メモ\n{}\n\n## 構造化差分 (AmendmentIr)\n```json\n{}\n```",
        memo_section, ir_summary
    );

    let aratame_polish_prompt = format!(
        "あなたは日本の法制執務を支援するアシスタントです。\n\
         以下の自動生成された改め文ドラフトについて、法制執務上の表現（連用形接続や文言の自然さ）を整え、\
         附則の施行期日・経過措置は担当者が提示した内容に従い、未提示の事項は要確認として残してください。\n\
         ただし、本則の改め文は機械検算（逆適用検証）に通るよう、引用語句「〜」を勝手に変更しないでください。\n\n\
         ## 現在の検算結果\n- exact_match: {}\n- diagnostics: {:?}\n\n\
         ## 改め文ドラフト\n```\n{}\n```",
        aratame_verification.is_exact_match,
        aratame_verification.diagnostics,
        aratame_draft.full_text
    );

    let mut bill_markdown = format!("# {bill_title}（草案）\n\n");
    if !aratame_verification.is_exact_match {
        bill_markdown
            .push_str("> 逆適用に未確認の差分があります。確認結果を参照してください。\n\n");
    }
    if aratame_draft.clauses.is_empty() {
        bill_markdown.push_str("［自動起案できる改め文なし。改正内容を確認］\n");
    } else {
        bill_markdown.push_str(&aratame_draft.full_text);
    }
    bill_markdown.push_str("\n\n## 附則\n\n［施行期日・経過措置の要否と内容を確認］\n");

    let youkou_markdown = outline(&bill_title, &ir);
    let mut riyuu_markdown = format!("# 理由（草案）\n\n対象法案　{bill_title}\n\n");
    if policy_memo.trim().is_empty() {
        riyuu_markdown.push_str("［改正の背景と必要性を記入］\n\n");
    } else {
        riyuu_markdown.push_str("## 起案の根拠（担当者メモ）\n\n");
        riyuu_markdown.push_str(policy_memo.trim());
        riyuu_markdown.push_str("\n\n［上記の根拠に基づき、改正の必要性を記述］\n\n");
    }
    riyuu_markdown.push_str("これが、この法律案を提出する理由である。\n");

    Legislation5SetPackage {
        ir,
        shinkyu_markdown,
        aratame_draft,
        aratame_verification,
        sansho_markdown,
        references,
        lint_report,
        bill_markdown,
        youkou_markdown,
        riyuu_markdown,
        youkou_prompt,
        riyuu_prompt,
        aratame_polish_prompt,
    }
}

fn outline(bill_title: &str, ir: &AmendmentIr) -> String {
    let mut out = format!("# {bill_title}要綱（草案）\n\n## 第一　改正事項\n\n");
    if ir.title_change.is_none()
        && ir.article_changes.is_empty()
        && ir.division_changes.is_empty()
        && ir.suppl_changes.is_empty()
        && ir.appdx_table_changes.is_empty()
    {
        out.push_str("［構造化差分なし。対象と改正内容を確認］\n");
    }
    if let Some((old, new)) = &ir.title_change {
        let _ = writeln!(out, "- 題名を「{old}」から「{new}」に改めること。");
    }
    for article in &ir.article_changes {
        let old = article.old_title.as_deref().unwrap_or_default();
        let new = article.new_title.as_deref().unwrap_or_default();
        let item = match article.kind {
            ArticleChangeKind::Added => format!("{new}を加えること。"),
            ArticleChangeKind::Deleted => format!("{old}を削ること。"),
            ArticleChangeKind::Modified => format!("{new}の規定を改めること。"),
            ArticleChangeKind::Renumbered => {
                format!("{old}を{new}に移すこと。［本文の変更有無を確認］")
            }
        };
        let _ = writeln!(out, "- {item}");
        for paragraph in &article.paragraph_changes {
            if paragraph.old_sentence == paragraph.new_sentence {
                continue;
            }
            if let (Some(number), Some(sentence)) = (&paragraph.new_num, &paragraph.new_sentence) {
                if !sentence.is_empty() {
                    let _ = writeln!(out, "  - 改正後の第{number}項本文　「{sentence}」");
                }
            }
        }
    }
    if !ir.division_changes.is_empty() {
        out.push_str("- 編・章等の構成を改めること。\n");
    }
    for provision in &ir.suppl_changes {
        let _ = writeln!(out, "- {}の規定を改めること。", provision.label);
    }
    for table in &ir.appdx_table_changes {
        let _ = writeln!(out, "- {}に係る規定を改めること。", table.title);
    }
    out.push_str("\n## 第二　施行期日・経過措置\n\n［担当者が確認・記入］\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lawtext::parse_lawtext;

    #[test]
    fn builds_complete_5set_package() {
        let old_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 この法律は、処分に関する手続を定める。";
        let new_lt = "\
テスト法（令和七年法律第一号）
（目的）
第一条 この法律は、処分及び行政指導に関する手続を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();
        let pkg = build_5set_package(
            &old_doc,
            &new_doc,
            "行政指導の透明性向上のため手続対象に追加する。",
            &ShinkyuOptions::default(),
        );

        assert!(pkg.aratame_verification.is_exact_match);
        assert!(pkg.shinkyu_markdown.contains("新旧対照表"));
        assert!(pkg.youkou_prompt.contains("行政指導の透明性向上"));
        assert!(
            pkg.riyuu_prompt
                .contains("これが、この法律案を提出する理由である")
        );
        let json = pkg.to_json_value();
        assert_eq!(json["documents"].as_object().unwrap().len(), 5);
        assert!(pkg.bill_markdown.contains(&pkg.aratame_draft.full_text));
        assert!(pkg.youkou_markdown.contains("第一条の規定を改めること。"));
        assert!(pkg.riyuu_markdown.contains("行政指導の透明性向上"));
        assert!(pkg.to_markdown().contains("# 確認結果"));
        assert_eq!(
            json["deterministic_outputs"]["aratame_verification"]["is_exact_match"],
            true
        );
    }

    #[test]
    fn reports_unapplied_supplementary_provisions() {
        let old = parse_lawtext("試験法（令和七年法律第一号）\n第一条 旧規定。").unwrap();
        let new = parse_lawtext(
            "試験法（令和七年法律第一号）\n第一条 新規定。\n附則\nこの法律は、公布の日から施行する。",
        ).unwrap();
        let pkg = build_5set_package(&old, &new, "", &ShinkyuOptions::default());
        assert!(!pkg.aratame_verification.is_exact_match);
        assert!(pkg.to_markdown().contains("附則に逆適用後の差分"));
        assert!(pkg.riyuu_markdown.contains("［改正の背景と必要性を記入］"));
    }
}
