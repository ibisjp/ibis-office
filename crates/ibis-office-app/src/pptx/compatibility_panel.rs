// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 互換性チェック (Check Compatibility: PPTX-FILE-007):
//! プレゼンテーション全体をスキャンし、旧バージョンの PowerPoint (97-2003 / 2007) で
//! サポートされていない機能（SVG 画像、埋め込みメディア、特殊グラフ、ワードアート変形、
//! カスタム図形、高度な視覚効果等）を検出し、
//! 重大な機能低下と軽微な精度の低下に分類して該当スライド・図形へのジャンプや再チェックを提供する。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{ChartType, Geometry, Presentation, ShapeId, ShapeKind, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 互換性問題の重大度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompatibilitySeverity {
    /// 重大な機能低下 (Significant loss of functionality: 表示不可、再生不可、編集不可等)
    SignificantLoss,
    /// 軽微な精度の低下 (Minor loss of fidelity: 効果の簡略化、フォールバック表示等)
    MinorLoss,
}

/// 検出された互換性の問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompatibilityIssue {
    pub(crate) severity: CompatibilitySeverity,
    pub(crate) slide_id: SlideId,
    pub(crate) slide_number: usize,
    pub(crate) shape_id: Option<ShapeId>,
    pub(crate) shape_name: Option<String>,
    pub(crate) title: String,
    pub(crate) description: String,
}

/// プレゼンテーションの互換性検査結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompatibilityReport {
    pub(crate) significant: Vec<CompatibilityIssue>,
    pub(crate) minor: Vec<CompatibilityIssue>,
}

impl CompatibilityReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.significant.is_empty() && self.minor.is_empty()
    }

    pub(crate) fn total_issues(&self) -> usize {
        self.significant.len() + self.minor.len()
    }

    pub(crate) fn all_issues(&self) -> Vec<CompatibilityIssue> {
        let mut all = self.significant.clone();
        all.extend(self.minor.clone());
        all
    }
}

/// プレゼンテーションをスキャンして互換性問題を検査する
pub(crate) fn check_presentation_compatibility(p: &Presentation) -> CompatibilityReport {
    let mut significant = Vec::new();
    let mut minor = Vec::new();

    for (slide_idx, slide) in p.slides.iter().enumerate() {
        let slide_num = slide_idx + 1;

        for shape in &slide.shapes {
            let shape_name = slide.shape_names.get(&shape.id).cloned();

            match &shape.kind {
                ShapeKind::Picture(pic) => {
                    // 1. 動画・音声: 埋め込みは PowerPoint 2010 以降で再生できる（`p14:media`）ので問題にしない。
                    // ファイルへのリンクは、そのファイルが無い PC では再生できない。
                    if pic.clip.as_ref().is_some_and(|c| c.link.is_some()) {
                        minor.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::MinorLoss,
                            slide_id: slide.id,
                            slide_number: slide_num,
                            shape_id: Some(shape.id),
                            shape_name: shape_name.clone(),
                            title: text("pptx.compat.media-title").to_string(),
                            description: text("pptx.compat.media-desc").to_string(),
                        });
                    }

                    // 2. SVG 形式の画像
                    if let Some(media) = p.media(pic.media) {
                        if media.format == ibis_doc_model::MediaFormat::Svg {
                            significant.push(CompatibilityIssue {
                                severity: CompatibilitySeverity::SignificantLoss,
                                slide_id: slide.id,
                                slide_number: slide_num,
                                shape_id: Some(shape.id),
                                shape_name: shape_name.clone(),
                                title: text("pptx.compat.svg-title").to_string(),
                                description: text("pptx.compat.svg-desc").to_string(),
                            });
                        }
                    }

                    // 画像の反射・ソフトエッジ等の高度な効果
                    if pic.reflection.is_some() || pic.soft_edge.is_some() {
                        minor.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::MinorLoss,
                            slide_id: slide.id,
                            slide_number: slide_num,
                            shape_id: Some(shape.id),
                            shape_name: shape_name.clone(),
                            title: text("pptx.compat.effects-title").to_string(),
                            description: text("pptx.compat.effects-desc").to_string(),
                        });
                    }
                }
                ShapeKind::Chart(chart) => {
                    // 3. 特殊グラフ種別 (Stock, Radar, Surface 等)
                    let has_advanced = chart.groups.iter().any(|g| matches!(
                        g.kind,
                        ChartType::Stock { .. } | ChartType::Radar { .. } | ChartType::Surface { .. }
                    ));
                    if has_advanced {
                        significant.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::SignificantLoss,
                            slide_id: slide.id,
                            slide_number: slide_num,
                            shape_id: Some(shape.id),
                            shape_name: shape_name.clone(),
                            title: text("pptx.compat.advanced-chart-title").to_string(),
                            description: text("pptx.compat.advanced-chart-desc").to_string(),
                        });
                    }
                }
                ShapeKind::Auto(auto) => {
                    // 4. カスタム図形 (Formula / custGeom: preset なし)
                    if let Geometry::Formula(f) = &auto.geometry {
                        if f.preset.is_none() {
                            minor.push(CompatibilityIssue {
                                severity: CompatibilitySeverity::MinorLoss,
                                slide_id: slide.id,
                                slide_number: slide_num,
                                shape_id: Some(shape.id),
                                shape_name: shape_name.clone(),
                                title: text("pptx.compat.cust-geom-title").to_string(),
                                description: text("pptx.compat.cust-geom-desc").to_string(),
                            });
                        }
                    }

                    // 5. 高度な視覚効果 (3D 面取り・反射・ソフトエッジ・3D傾き)
                    let has_effects = auto.reflection.is_some()
                        || auto.soft_edge.is_some()
                        || auto.bevel.is_some()
                        || auto.tilt.is_some();
                    if has_effects {
                        minor.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::MinorLoss,
                            slide_id: slide.id,
                            slide_number: slide_num,
                            shape_id: Some(shape.id),
                            shape_name: shape_name.clone(),
                            title: text("pptx.compat.effects-title").to_string(),
                            description: text("pptx.compat.effects-desc").to_string(),
                        });
                    }
                }
                _ => {}
            }

            // 6. ワードアートの文字変形 (Text Warp)
            if shape.text_body().and_then(|tb| tb.warp.as_ref()).is_some() {
                minor.push(CompatibilityIssue {
                    severity: CompatibilitySeverity::MinorLoss,
                    slide_id: slide.id,
                    slide_number: slide_num,
                    shape_id: Some(shape.id),
                    shape_name: shape_name.clone(),
                    title: text("pptx.compat.text-warp-title").to_string(),
                    description: text("pptx.compat.text-warp-desc").to_string(),
                });
            }
        }
    }

    CompatibilityReport { significant, minor }
}

/// 互換性チェックサイドバーパネルの状態
pub(crate) struct CompatibilityPanel {
    pub(crate) report: CompatibilityReport,
    pub(crate) message: Option<String>,
}

impl SlideEditor {
    /// 互換性チェックパネルを開く
    pub(crate) fn open_pptx_compatibility_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let report = check_presentation_compatibility(self.presentation());
        if let Some(pptx) = self.pptx_mut() {
            pptx.compatibility_panel = Some(CompatibilityPanel {
                report,
                message: None,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 互換性チェックパネルを閉じる
    pub(crate) fn close_pptx_compatibility_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            pptx.compatibility_panel = None;
        }
        cx.notify();
    }

    /// 互換性を再検査する
    pub(crate) fn recheck_pptx_compatibility(&mut self, cx: &mut Context<Self>) {
        let report = check_presentation_compatibility(self.presentation());
        let count = report.total_issues();
        if let Some(pptx) = self.pptx_mut() {
            if let Some(panel) = &mut pptx.compatibility_panel {
                panel.report = report;
                panel.message = Some(crate::shell::i18n::format("_pptx.checked-again-count-issues", &[("count", &count)]));
            }
        }
        cx.notify();
    }

    /// 該当の互換性問題の箇所へジャンプする
    pub(crate) fn jump_to_pptx_compatibility_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let (slide_id, shape_id) = {
            let Some(pptx) = self.pptx() else { return };
            let Some(panel) = &pptx.compatibility_panel else { return };
            let all = panel.report.all_issues();
            let Some(issue) = all.get(issue_idx) else { return };
            (issue.slide_id, issue.shape_id)
        };

        self.current_slide = slide_id;
        self.selection = shape_id;
        if let Some(sh_id) = shape_id {
            if let Some(s) = self.pptx_mut() {
                s.selection = vec![sh_id];
            }
        } else if let Some(s) = self.pptx_mut() {
            s.selection.clear();
        }
        cx.notify();
    }

    /// 互換性問題の要因となっている視覚効果（3D面取り・反射・ソフトエッジ・3D傾き）や文字変形（WordArt Warp）を解除して互換性を回復する
    pub(crate) fn resolve_pptx_compatibility_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        self.jump_to_pptx_compatibility_issue(issue_idx, cx);
        let (slide_id, Some(shape_id), issue_title) = ({
            let Some(pptx) = self.pptx() else { return };
            let Some(panel) = &pptx.compatibility_panel else { return };
            let all = panel.report.all_issues();
            let Some(issue) = all.get(issue_idx) else { return };
            (issue.slide_id, issue.shape_id, issue.title.clone())
        }) else {
            return;
        };

        let Some(mut slide) = self.presentation().slide(slide_id).cloned() else {
            return;
        };
        let Some(shape) = slide.shapes.iter_mut().find(|s| s.id == shape_id) else {
            return;
        };

        let effects_title = text("pptx.compat.effects-title").to_string();
        let warp_title = text("pptx.compat.text-warp-title").to_string();
        let cust_geom_title = text("pptx.compat.cust-geom-title").to_string();

        if issue_title == effects_title {
            match &mut shape.kind {
                ShapeKind::Auto(auto) => {
                    auto.reflection = None;
                    auto.soft_edge = None;
                    auto.bevel = None;
                    auto.tilt = None;
                }
                ShapeKind::Picture(pic) => {
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                _ => {}
            }
            self.run(vec![ibis_doc_model::Command::SetSlide { slide }], cx);
        } else if issue_title == warp_title {
            if let Some(tb) = shape.text_body_mut() {
                tb.warp = None;
            }
            self.run(vec![ibis_doc_model::Command::SetSlide { slide }], cx);
        } else if issue_title == cust_geom_title {
            if let ShapeKind::Auto(auto) = &mut shape.kind {
                auto.geometry = Geometry::Rect;
            }
            self.run(vec![ibis_doc_model::Command::SetSlide { slide }], cx);
        } else {
            self.run(
                vec![ibis_doc_model::Command::RemoveShape {
                    slide: slide_id,
                    shape: shape_id,
                }],
                cx,
            );
        }
        self.recheck_pptx_compatibility(cx);
    }

    /// 互換性問題の対象図形を削除して再検査する
    pub(crate) fn remove_pptx_compatibility_issue_shape(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let (slide_id, Some(shape_id)) = ({
            let Some(pptx) = self.pptx() else { return };
            let Some(panel) = &pptx.compatibility_panel else { return };
            let all = panel.report.all_issues();
            let Some(issue) = all.get(issue_idx) else { return };
            (issue.slide_id, issue.shape_id)
        }) else {
            return;
        };
        self.current_slide = slide_id;
        self.run(
            vec![ibis_doc_model::Command::RemoveShape {
                slide: slide_id,
                shape: shape_id,
            }],
            cx,
        );
        self.recheck_pptx_compatibility(cx);
    }

    /// Context menu entries for compatibility issue `issue_idx` (PPTX-FILE-007).
    pub(crate) fn compatibility_issue_context_menu_entries(
        &self,
        issue_idx: usize,
    ) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(panel) = self.pptx().and_then(|s| s.compatibility_panel.as_ref()) else {
            return Vec::new();
        };
        let all = panel.report.all_issues();
        let Some(issue) = all.get(issue_idx) else {
            return Vec::new();
        };
        let has_shape = issue.shape_id.is_some();

        vec![
            MenuItem::tr("_shell.view")
                .icon("visibility")
                .on(move |e, _, cx| e.jump_to_pptx_compatibility_issue(issue_idx, cx))
                .into(),
            MenuItem::tr("_shell.clear-formatting")
                .icon("format_clear")
                .disabled(!has_shape)
                .on(move |e, _, cx| e.resolve_pptx_compatibility_issue(issue_idx, cx))
                .into(),
            MenuItem::tr("edit.delete")
                .icon("delete")
                .disabled(!has_shape)
                .on(move |e, _, cx| e.remove_pptx_compatibility_issue_shape(issue_idx, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.compat.recheck")
                .icon("refresh")
                .on(|e, _, cx| e.recheck_pptx_compatibility(cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_pptx_compatibility_panel(cx))
                .into(),
        ]
    }

    /// Open context menu for compatibility issue `issue_idx` at pointer `position`.
    pub(crate) fn open_compatibility_issue_context_menu(
        &mut self,
        issue_idx: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_pptx_compatibility_issue(issue_idx, cx);
        let entries = self.compatibility_issue_context_menu_entries(issue_idx);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    /// 互換性チェックパネルの UI を生成する
    pub(crate) fn pptx_compatibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let pptx = self.pptx()?;
        let panel = pptx.compatibility_panel.as_ref()?;
        let report = panel.report.clone();
        let msg = panel.message.clone().unwrap_or_default();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("pptx.compat.summary-hint").to_string()),
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
                        .child(text("pptx.compat.no-issues").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("pptx.compat.clean-desc").to_string()),
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
                    text("pptx.compat.significant-loss"),
                    report.significant.len(),
                    text("_pptx.items")
                ));
            issues_list = issues_list.child(sig_header);

            for (ix, issue) in report.significant.iter().enumerate() {
                let slide_num = issue.slide_number;
                let shape_name = issue.shape_name.clone().unwrap_or_else(|| crate::shell::i18n::text("insert.shape").to_string());
                let jump_label = crate::shell::i18n::format("_pptx.slide-slide-num-shape-name", &[("slide_num", &slide_num), ("shape_name", &shape_name)]);

                let card = v_flex()
                    .id(("compat-issue-card", ix))
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.3))
                    .bg(cx.theme().danger.opacity(0.04))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_to_pptx_compatibility_issue(ix, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            this.open_compatibility_issue_context_menu(ix, event.position, w, cx);
                        }),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(issue.title.clone()),
                            )
                            .child(
                                crate::shell::chrome::action_button(("jump-compat-sig", ix))
                                    .xsmall()
                                    .outline()
                                    .label(jump_label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.jump_to_pptx_compatibility_issue(ix, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(issue.description.clone()),
                    );

                issues_list = issues_list.child(card);
            }
        }

        // 2. 軽微な精度の低下
        if !report.minor.is_empty() {
            let minor_header = div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().accent)
                .child(format!(
                    "{} ({} {})",
                    text("pptx.compat.minor-loss"),
                    report.minor.len(),
                    text("_pptx.items")
                ));
            issues_list = issues_list.child(minor_header);

            let offset = report.significant.len();
            for (ix, issue) in report.minor.iter().enumerate() {
                let global_ix = offset + ix;
                let slide_num = issue.slide_number;
                let shape_name = issue.shape_name.clone().unwrap_or_else(|| crate::shell::i18n::text("insert.shape").to_string());
                let jump_label = crate::shell::i18n::format("_pptx.slide-slide-num-shape-name", &[("slide_num", &slide_num), ("shape_name", &shape_name)]);

                let card = v_flex()
                    .id(("compat-issue-card", global_ix))
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().accent.opacity(0.3))
                    .bg(cx.theme().accent.opacity(0.04))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_to_pptx_compatibility_issue(global_ix, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            this.open_compatibility_issue_context_menu(global_ix, event.position, w, cx);
                        }),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(issue.title.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        crate::shell::chrome::action_button(("fix-compat-minor", ix))
                                            .xsmall()

                                            .label(text("_shell.clear-formatting"))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.resolve_pptx_compatibility_issue(global_ix, cx);
                                            })),
                                    )
                                    .child(
                                        crate::shell::chrome::action_button(("jump-compat-minor", ix))
                                            .xsmall()
                                            .outline()
                                            .label(jump_label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.jump_to_pptx_compatibility_issue(global_ix, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(issue.description.clone()),
                    );

                issues_list = issues_list.child(card);
            }
        }

        let bottom_actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("compat-recheck", text("pptx.compat.recheck"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_pptx_compatibility(cx))),
            )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));

        let content = v_flex()
            .p_4()
            .gap_3()
            .p_0().child(header)
            .when(!msg.is_empty(), |el| {
                el.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().primary.opacity(0.1))
                        .text_sm()
                        .text_color(cx.theme().primary)
                        .child(msg),
                )
            })
            .children(clean_box)
            .child(issues_list)
            .child(div().h_2())
            ;

        let panel_terminal_actions = bottom_actions;
        Some(SidebarPanel::with_footer(text("pptx.compatibility-check"), content.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests;
