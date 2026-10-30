// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 互換性チェック (Check Compatibility: DOCX-SEC-018):
//! 文書全体をスキャンし、旧バージョンの Word (Word 97-2003 / 2007) で
//! サポートされていない機能（OMML 数式、SmartArt、コンテンツ コントロール、
//! 高度なフォント効果・OpenType 機能、図形の高度な視覚効果等）を検出し、
//! 重大な機能低下と軽微な精度の低下に分類して該当箇所へのジャンプや再チェックを提供する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow::{FlowDocument, FlowRunKind, Ligatures, NumberForm, NumberSpacing};
use ibis_doc_model::{FlowPos, Placeholder, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 互換性問題の重大度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompatibilitySeverity {
    /// 重大な機能低下 (Significant loss of functionality: 画像化、編集不可等)
    SignificantLoss,
    /// 軽微な精度の低下 (Minor loss of fidelity: 効果の簡略化、フォールバック等)
    MinorLoss,
}

/// 検出された互換性の問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompatibilityIssue {
    pub(crate) severity: CompatibilitySeverity,
    pub(crate) pos: FlowPos,
    pub(crate) para_id: u32,
    pub(crate) title: String,
    pub(crate) description: String,
}

/// 文書の互換性検査結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompatibilityReport {
    pub(crate) significant: Vec<CompatibilityIssue>,
    pub(crate) minor: Vec<CompatibilityIssue>,
}

impl CompatibilityReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.significant.is_empty() && self.minor.is_empty()
    }

    #[allow(dead_code)]
    pub(crate) fn total_issues(&self) -> usize {
        self.significant.len() + self.minor.len()
    }

    #[allow(dead_code)]
    pub(crate) fn all_issues(&self) -> Vec<CompatibilityIssue> {
        let mut all = self.significant.clone();
        all.extend(self.minor.clone());
        all
    }
}

/// 文書をスキャンして互換性問題を検査する
pub(crate) fn check_docx_compatibility(doc: &FlowDocument) -> CompatibilityReport {
    let mut significant = Vec::new();
    let mut minor = Vec::new();

    let paras = ibis_doc_model::flow_command::all_paragraphs_everywhere(doc);
    for p in paras {
        let mut offset = 0;
        for r in &p.runs {
            let len = r.len();
            let pos = FlowPos::new(p.id, offset);
            match &r.kind {
                FlowRunKind::Math { .. } => {
                    significant.push(CompatibilityIssue {
                        severity: CompatibilitySeverity::SignificantLoss,
                        pos,
                        para_id: p.id,
                        title: text("docx.compat.math-title").to_string(),
                        description: text("docx.compat.math-desc").to_string(),
                    });
                }
                FlowRunKind::Shape(shape) => {
                    if matches!(shape.kind, ShapeKind::Placeholder(Placeholder::Diagram)) {
                        significant.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::SignificantLoss,
                            pos,
                            para_id: p.id,
                            title: text("docx.compat.smartart-title").to_string(),
                            description: text("docx.compat.smartart-desc").to_string(),
                        });
                    }
                    let has_effects = match &shape.kind {
                        ShapeKind::Auto(auto) => {
                            auto.reflection.is_some() || auto.soft_edge.is_some() || auto.bevel.is_some() || auto.tilt.is_some()
                        }
                        ShapeKind::Picture(pic) => {
                            pic.reflection.is_some() || pic.soft_edge.is_some()
                        }
                        _ => false,
                    };
                    if has_effects {
                        minor.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::MinorLoss,
                            pos,
                            para_id: p.id,
                            title: text("docx.compat.shape-effects-title").to_string(),
                            description: text("docx.compat.shape-effects-desc").to_string(),
                        });
                    }
                }
                FlowRunKind::FormField(_) => {
                    minor.push(CompatibilityIssue {
                        severity: CompatibilitySeverity::MinorLoss,
                        pos,
                        para_id: p.id,
                        title: text("docx.compat.content-control-title").to_string(),
                        description: text("docx.compat.content-control-desc").to_string(),
                    });
                }
                FlowRunKind::CheckBox { .. }
                | FlowRunKind::DropDownList { .. }
                | FlowRunKind::DatePicker { .. }
                | FlowRunKind::RichText { .. }
                | FlowRunKind::PlainText { .. }
                | FlowRunKind::PictureControl { .. }
                | FlowRunKind::BuildingBlockGallery { .. }
                | FlowRunKind::RepeatingSection { .. }
                | FlowRunKind::Group { .. } => {
                    minor.push(CompatibilityIssue {
                        severity: CompatibilitySeverity::MinorLoss,
                        pos,
                        para_id: p.id,
                        title: text("docx.compat.content-control-title").to_string(),
                        description: text("docx.compat.content-control-desc").to_string(),
                    });
                }
                _ => {}
            }

            if r.control.is_some()
                && !matches!(
                    r.kind,
                    FlowRunKind::CheckBox { .. }
                        | FlowRunKind::DropDownList { .. }
                        | FlowRunKind::DatePicker { .. }
                        | FlowRunKind::RichText { .. }
                        | FlowRunKind::PlainText { .. }
                        | FlowRunKind::PictureControl { .. }
                        | FlowRunKind::BuildingBlockGallery { .. }
                        | FlowRunKind::RepeatingSection { .. }
                        | FlowRunKind::Group { .. }
                )
            {
                minor.push(CompatibilityIssue {
                    severity: CompatibilitySeverity::MinorLoss,
                    pos,
                    para_id: p.id,
                    title: text("docx.compat.content-control-title").to_string(),
                    description: text("docx.compat.content-control-desc").to_string(),
                });
            }

            let has_opentype = r.style.ligatures != Ligatures::None
                || r.style.num_form != NumberForm::Default
                || r.style.num_spacing != NumberSpacing::Default
                || r.style.contextual_alternates;
            let has_font_effects = r.style.outline || r.style.shadow || r.style.emboss || r.style.imprint;
            if has_opentype || has_font_effects {
                minor.push(CompatibilityIssue {
                    severity: CompatibilitySeverity::MinorLoss,
                    pos,
                    para_id: p.id,
                    title: text("docx.compat.text-effects-title").to_string(),
                    description: text("docx.compat.text-effects-desc").to_string(),
                });
            }

            offset += len;
        }
    }

    CompatibilityReport { significant, minor }
}

/// サイドバーに表示する互換性チェックパネル状態
pub(crate) struct DocCompatibilityState {
    pub(crate) report: CompatibilityReport,
    pub(crate) message: Option<String>,
}

impl SlideEditor {
    /// 互換性チェックパネルを開く
    pub(crate) fn open_doc_compatibility_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(doc) = self.flow() else { return };
        let report = check_docx_compatibility(doc);
        let panel = DocCompatibilityState {
            report,
            message: None,
        };

        if let Some(s) = self.doc_state_mut() {
            s.compatibility_panel = Some(panel);
            s.inspect_panel = None;
            s.docx_math = None;
            s.quick_parts = None;
            s.table_formula = None;
            s.table_size = None;
            s.picture_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 互換性チェックパネルを閉じる
    pub(crate) fn close_doc_compatibility_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.compatibility_panel = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 互換性を再チェック
    pub(crate) fn recheck_doc_compatibility(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let report = check_docx_compatibility(doc);
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.compatibility_panel.as_mut()) {
            panel.report = report;
            panel.message = Some(text("pptx.compat.recheck").to_string());
        }
        cx.notify();
    }

    /// 互換性の問題箇所へ移動
    pub(crate) fn jump_to_docx_compatibility_issue(&mut self, pos: FlowPos, cx: &mut Context<Self>) {
        self.set_caret(pos, false, cx);
        self.scroll_to_caret(cx);
        cx.notify();
    }

    /// 互換性チェックパネルの UI を生成する
    pub(crate) fn doc_compatibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?;
        let panel = state.compatibility_panel.as_ref()?;
        let report = panel.report.clone();
        let msg = panel.message.clone().unwrap_or_default();

        let is_compat = self.is_docx_compatibility_mode();
        let mode_name = self.docx_compatibility_mode_name();
        let mut mode_card = v_flex()
            .p_2()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{}: {}", text("docx.compat.current-mode"), mode_name)),
                    ),
            );
        if is_compat {
            mode_card = mode_card
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("docx.infobar.compat.message").to_string()),
                )
                .child(
                    crate::shell::chrome::action_button("compat-convert-btn")
                        .label(text("docx.infobar.compat.convert"))
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.convert_docx_to_current_format(cx);
                        })),
                );
        }

        let header = v_flex()
            .gap_2()
            .child(mode_card)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("docx.compat.summary-hint").to_string()),
            );

        let clean_box = report.is_clean().then(|| {
            v_flex()
                .p_4()
                .gap_2()
                .items_center()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().primary)
                        .child(text("docx.compat.no-issues").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("docx.compat.clean-desc").to_string()),
                )
        });

        let mut issues_list = v_flex().gap_2();

        // 1. 重大な機能低下
        if !report.significant.is_empty() {
            let sig_header = div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().danger)
                .child(format!(
                    "{} ({} {})",
                    text("docx.compat.significant-loss"),
                    report.significant.len(),
                    text("_docx.items")
                ));
            issues_list = issues_list.child(sig_header);

            for (ix, issue) in report.significant.iter().enumerate() {
                let pos = issue.pos;
                let card = v_flex()
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.3))
                    .bg(cx.theme().danger.opacity(0.05))
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_sm()
                                    .child(issue.title.clone()),
                            )
                            .child(
                                crate::shell::chrome::action_button(("compat-jump-sig", ix))
                                    .label(text("docx.compat.jump"))
                                    .small()
                                    .outline()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.jump_to_docx_compatibility_issue(pos, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(issue.description.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::shell::i18n::format("_docx.para-item", &[("para", &pos.para)])),
                    );

                issues_list = issues_list.child(card);
            }
        }

        // 2. 軽微な精度の低下
        if !report.minor.is_empty() {
            let minor_header = div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().warning)
                .child(format!(
                    "{} ({} {})",
                    text("docx.compat.minor-loss"),
                    report.minor.len(),
                    text("_docx.items")
                ));
            issues_list = issues_list.child(minor_header);

            for (ix, issue) in report.minor.iter().enumerate() {
                let pos = issue.pos;
                let card = v_flex()
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().warning.opacity(0.3))
                    .bg(cx.theme().warning.opacity(0.05))
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_sm()
                                    .child(issue.title.clone()),
                            )
                            .child(
                                crate::shell::chrome::action_button(("compat-jump-minor", ix))
                                    .label(text("docx.compat.jump"))
                                    .small()
                                    .outline()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.jump_to_docx_compatibility_issue(pos, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(issue.description.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::shell::i18n::format("_docx.para-item", &[("para", &pos.para)])),
                    );

                issues_list = issues_list.child(card);
            }
        }

        let message_box = (!msg.is_empty()).then(|| {
            div()
                .p_2()
                .text_sm()
                .rounded_md()
                .bg(cx.theme().primary.opacity(0.1))
                .text_color(cx.theme().primary)
                .child(msg)
        });

        let footer = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("compat-recheck", text("docx.compat.recheck"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_doc_compatibility(cx))),
            )

            .action(crate::shell::chrome::ActionDefinition::new("compat-close", text("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx| e.close_panel_dialog(w,cx))));

        let body = v_flex()
            .p_4()
            .gap_3()
            .p_0().child(header)
            .children(message_box)
            .children(clean_box)
            .child(issues_list)
            ;

        let panel_terminal_actions = footer;
        Some(SidebarPanel::with_footer(text("docx.compat.title"), body.into_any_element(), panel_terminal_actions))
    }
}
