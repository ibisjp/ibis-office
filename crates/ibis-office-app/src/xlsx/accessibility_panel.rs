// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アクセシビリティ チェック (Check Accessibility: XLSX-SEC-021):
//! ワークブック全体のアクセシビリティ問題（図形・画像・グラフの代替テキスト不足、
//! 既定のシート名、表の見出し行の未設定、不明瞭なハイパーリンク等）をスキャン・検出し、
//! 該当セルやオブジェクトへのフォーカスや修正を支援する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{ShapeId, ShapeKind, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// アクセシビリティ問題の重大度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssueSeverity {
    Error,
    Warning,
    Tip,
}

/// 検出されたアクセシビリティの問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessibilityIssue {
    pub(crate) severity: IssueSeverity,
    pub(crate) sheet_id: SlideId,
    pub(crate) sheet_index: usize,
    pub(crate) sheet_name: String,
    pub(crate) shape_id: Option<ShapeId>,
    pub(crate) cell: Option<(u32, u32)>,
    pub(crate) title: String,
    pub(crate) description: String,
}

/// ワークブックのアクセシビリティ検査結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityReport {
    pub(crate) errors: Vec<AccessibilityIssue>,
    pub(crate) warnings: Vec<AccessibilityIssue>,
    pub(crate) tips: Vec<AccessibilityIssue>,
}

impl AccessibilityReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    #[allow(dead_code)]
    pub(crate) fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len() + self.tips.len()
    }
}

/// ワークブック全体をスキャンしてアクセシビリティ問題を検査する
pub(crate) fn check_accessibility(p: &ibis_doc_model::Presentation) -> AccessibilityReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut tips = Vec::new();

    for (idx, slide) in p.slides.iter().enumerate() {
        let sheet_name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", idx + 1));

        // 1. 既定のシート名チェック (Warning)
        let is_default_name = {
            let s = sheet_name.trim();
            (s.starts_with("Sheet") && s.len() > 5 && s[5..].chars().all(|c| c.is_ascii_digit()))
                || (s.starts_with("シート") && s.len() > 9 && s[9..].chars().all(|c| c.is_ascii_digit())) // i18n-ok（既定のシート名の照合に使う語（日本語の既定名 シート1 など））
                || (s.starts_with("Feuil") && s.len() > 5 && s[5..].chars().all(|c| c.is_ascii_digit()))
                || (s.starts_with("Tabelle") && s.len() > 7 && s[7..].chars().all(|c| c.is_ascii_digit()))
        };

        if is_default_name {
            warnings.push(AccessibilityIssue {
                severity: IssueSeverity::Warning,
                sheet_id: slide.id,
                sheet_index: idx,
                sheet_name: sheet_name.clone(),
                shape_id: None,
                cell: None,
                title: crate::shell::i18n::format("_xlsx.sheet-name-default-sheet-name", &[("sheet_name", &sheet_name)]),
                description: text("_xlsx.consider-renaming-the-sheet-to-a-specific-name-t").to_string(),
            });
        }

        // 2. 図形・画像・グラフの代替テキストチェック (Error)
        for shape in &slide.shapes {
            let shape_name = slide.shape_names.get(&shape.id).cloned().unwrap_or_else(|| text("_xlsx.object").to_string());
            let is_decorative = slide.decorative_shapes.contains(&shape.id);
            let has_alt = slide.shape_alt_texts.get(&shape.id).is_some_and(|a| {
                !a.description.trim().is_empty() || !a.title.trim().is_empty()
            });

            match &shape.kind {
                ShapeKind::Picture(_) | ShapeKind::Chart(_) => {
                    if !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            shape_id: Some(shape.id),
                            cell: None,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-missing-alternative-text-shape-name", &[("sheet_name", &sheet_name), ("shape_name", &shape_name)]),
                            description: text("_xlsx.add-alternative-text-so-screen-reader-users-can").to_string(),
                        });
                    }
                }
                ShapeKind::Auto(auto) => {
                    let has_text = auto.text.as_ref().is_some_and(|tb| !tb.text().trim().is_empty());
                    if !has_text && !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            shape_id: Some(shape.id),
                            cell: None,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-missing-alternative-text-shape-name", &[("sheet_name", &sheet_name), ("shape_name", &shape_name)]),
                            description: text("_xlsx.add-alternative-text-to-the-shape-or-mark-it-as").to_string(),
                        });
                    }
                }
                ShapeKind::TextBox(tb) => {
                    let has_text = !tb.text().trim().is_empty();
                    if !has_text && !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            shape_id: Some(shape.id),
                            cell: None,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-missing-alternative-text-shape-name", &[("sheet_name", &sheet_name), ("shape_name", &shape_name)]),
                            description: text("_xlsx.add-alternative-text-to-the-text-box-or-mark-it").to_string(),
                        });
                    }
                }
                _ => {}
            }
        }

        // 3. シート内のテーブル・ハイパーリンクのチェック
        if let Some(sheet) = sheet_of_slide(slide) {
            // テーブルの見出し行チェック (Warning)
            for table in &sheet.tables {
                if table.header_rows == 0 {
                    let tbl_name = if table.name.is_empty() { text("_xlsx.table").to_string() } else { table.name.clone() };
                    warnings.push(AccessibilityIssue {
                        severity: IssueSeverity::Warning,
                        sheet_id: slide.id,
                        sheet_index: idx,
                        sheet_name: sheet_name.clone(),
                        shape_id: None,
                        cell: Some((table.range.first_row, table.range.first_column)),
                        title: crate::shell::i18n::format("_xlsx.sheet-name-table-tbl-name-has-no-header-row", &[("sheet_name", &sheet_name), ("tbl_name", &tbl_name)]),
                        description: text("_xlsx.set-a-header-row-so-screen-readers-can-correctly").to_string(),
                    });
                }
            }

            // 分かりにくいハイパーリンク文字列チェック (Tip)
            for hl in &sheet.hyperlinks {
                let row = hl.range.first_row;
                let col = hl.range.first_column;
                let cell_text = sheet.cell(row, col).map(|c| c.text.as_str()).unwrap_or("");
                let target = hl.target.as_deref().unwrap_or(hl.location.as_deref().unwrap_or(""));
                let text_lower = cell_text.to_lowercase();
                let is_unclear_link = text_lower == "ここをクリック" // i18n-ok（リンク文字列の判定に使う語（日本語の文書のリンク文字列との照合）。迷った：英語の）
                    || text_lower == "クリック" // i18n-ok（リンク文字列の判定に使う語（照合）。迷った：英語の文書は別途）
                    || text_lower == "リンク" // i18n-ok（リンク文字列の判定に使う語（照合）。迷った：英語の文書は別途）
                    || text_lower == "click here"
                    || text_lower == "link"
                    || text_lower == "here"
                    || text_lower.starts_with("http://")
                    || text_lower.starts_with("https://")
                    || (!cell_text.is_empty() && cell_text == target);

                if is_unclear_link {
                    let cell_ref = format!("{}{}", super::sheet_ui::column_letters(col), row + 1);
                    tips.push(AccessibilityIssue {
                        severity: IssueSeverity::Tip,
                        sheet_id: slide.id,
                        sheet_index: idx,
                        sheet_name: sheet_name.clone(),
                        shape_id: None,
                        cell: Some((row, col)),
                        title: crate::shell::i18n::format("_xlsx.sheet-name-cell-ref-unclear-hyperlink-text", &[("sheet_name", &sheet_name), ("cell_ref", &cell_ref)]),
                        description: text("_xlsx.use-descriptive-text-that-explains-what-the-link").to_string(),
                    });
                }
            }
        }
    }

    AccessibilityReport { errors, warnings, tips }
}

/// サイドバーに表示するアクセシビリティ チェック パネル
pub(crate) struct AccessibilityPanel {
    pub(crate) report: AccessibilityReport,
    pub(crate) selected_issue: Option<usize>,
    pub(crate) message: String,
}

impl SlideEditor {
    /// アクセシビリティ チェック パネルを開く
    pub(crate) fn open_xlsx_accessibility_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let report = check_accessibility(self.presentation());
        let panel = AccessibilityPanel {
            report,
            selected_issue: None,
            message: String::new(),
        };

        let state = self.wb_mut();
        state.close_settings();
        state.panels.accessibility = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// アクセシビリティ チェック パネルを閉じる
    pub(crate) fn close_xlsx_accessibility_panel(&mut self, cx: &mut Context<Self>) {
        self.wb_mut().panels.accessibility = None;
        self.sidebar_open = false;
        cx.notify();
    }

    /// 再検査を実行
    pub(crate) fn recheck_xlsx_accessibility(&mut self, cx: &mut Context<Self>) {
        let report = check_accessibility(self.presentation());
        if let Some(p) = self.wb_mut().panels.accessibility.as_mut() {
            p.report = report;
            p.selected_issue = None;
            p.message = text("xlsx.accessibility.recheck-done").to_string();
            cx.notify();
        }
    }

    /// 問題のあるシートおよびセル・図形にジャンプ（フォーカス）
    pub(crate) fn jump_to_xlsx_accessibility_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let issue = {
            let Some(p) = self.wb().panels.accessibility.as_ref() else { return };
            let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).chain(p.report.tips.iter()).cloned().collect();
            if issue_idx < all.len() {
                all[issue_idx].clone()
            } else {
                return;
            }
        };

        // シートの切り替え
        self.current_slide = issue.sheet_id;

        // セルの選択
        if let Some((row, col)) = issue.cell {
            let wb = self.wb_mut();
            wb.cells.insert(issue.sheet_id, (col, row));
            wb.cell_ends.remove(&issue.sheet_id);
        }

        // 図形の選択
        if let Some(shape_id) = issue.shape_id {
            self.selection = Some(shape_id);
        }

        if let Some(p) = self.wb_mut().panels.accessibility.as_mut() {
            p.selected_issue = Some(issue_idx);
        }

        cx.notify();
    }

    /// サイドバーのアクセシビリティ チェック パネル UI 要素
    pub(crate) fn xlsx_accessibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.wb().panels.accessibility.as_ref()?;
        let report = panel.report.clone();
        let selected_idx = panel.selected_issue;
        let msg = panel.message.clone();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("xlsx.accessibility.desc").to_string()),
            );

        let is_clean = report.is_clean();

        let clean_box = is_clean.then(|| {
            v_flex()
                .p_3()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary.opacity(0.3))
                .bg(cx.theme().primary.opacity(0.08))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_sm()
                        .text_color(cx.theme().primary)
                        .child(text("xlsx.accessibility.passed").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("xlsx.accessibility.passed-sub").to_string()),
                )
        });

        let mut issues_list = v_flex().gap_2();

        let all_issues: Vec<_> = report.errors.iter().chain(report.warnings.iter()).chain(report.tips.iter()).cloned().collect();

        for (ix, issue) in all_issues.iter().enumerate() {
            let is_selected = selected_idx == Some(ix);
            let is_error = issue.severity == IssueSeverity::Error;
            let is_warning = issue.severity == IssueSeverity::Warning;
            let title = issue.title.clone();
            let desc = issue.description.clone();

            let border_color = if is_selected {
                cx.theme().primary
            } else if is_error {
                cx.theme().danger.opacity(0.4)
            } else if is_warning {
                cx.theme().warning.opacity(0.4)
            } else {
                cx.theme().border
            };

            let bg_color = if is_selected {
                cx.theme().primary.opacity(0.08)
            } else if is_error {
                cx.theme().danger.opacity(0.04)
            } else {
                cx.theme().background
            };

            let item = v_flex().w_full().min_w_0().overflow_hidden()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(border_color)
                .bg(bg_color)
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .flex_1().min_w_0().truncate().text_sm()
                                .text_color(if is_error {
                                    cx.theme().danger
                                } else if is_warning {
                                    cx.theme().warning
                                } else {
                                    cx.theme().foreground
                                })
                                .child(title),
                        )
                        .child(
                            crate::shell::chrome::action_button(("jump-issue", ix))
                                .label(text("_shell.view"))
                                .small()
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    this.jump_to_xlsx_accessibility_issue(ix, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .min_w_0().overflow_hidden().child(desc),
                );

            issues_list = issues_list.child(item);
        }

        let bottom_actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("access-recheck", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("xlsx.accessibility.recheck"))
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_xlsx_accessibility(cx))),
            );

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
        Some(SidebarPanel::with_footer(text("xlsx.accessibility.title"), content.into_any_element(), panel_terminal_actions))
    }
}
