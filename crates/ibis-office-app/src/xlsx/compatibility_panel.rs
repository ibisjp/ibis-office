// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 互換性チェック (Check Compatibility: XLSX-FILE-013):
//! ワークブック全体をスキャンし、旧バージョンの Excel (97-2003 / BIFF8) で
//! サポートされていない機能（グリッドサイズ超過、新関数、拡張条件付き書式、
//! テーブル、スパークライン、グラデーション塗りつぶし等）を検出・一覧表示し、
//! 重大な機能低下と軽微な精度の低下に分類して該当セルへのジャンプや再チェックを提供する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CfKind, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 互換性問題の重大度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompatibilitySeverity {
    /// 重大な機能低下 (Significant loss of functionality: データ消失、未対応関数、評価されない書式等)
    SignificantLoss,
    /// 軽微な精度の低下 (Minor loss of fidelity: テーブルの通常範囲化、スパークライン非表示、色変換等)
    MinorLoss,
}

/// 検出された互換性の問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompatibilityIssue {
    pub(crate) severity: CompatibilitySeverity,
    pub(crate) sheet_id: SlideId,
    pub(crate) sheet_index: usize,
    pub(crate) sheet_name: String,
    pub(crate) cell: Option<(u32, u32)>,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) occurrences: usize,
}

/// ワークブックの互換性検査結果
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

    pub(crate) fn all_issues(&self) -> Vec<CompatibilityIssue> {
        self.significant.iter().chain(self.minor.iter()).cloned().collect()
    }
}

/// 関数名が Excel 97-2003 で未対応（Excel 2007以降、または動的配列/365関数）であるかを判定する
fn is_unsupported_function(name: &str) -> bool {
    let upper = name.to_uppercase();
    let stripped = upper
        .trim_start_matches("_XLFN.")
        .trim_start_matches("_XLWS.")
        .trim_start_matches("_XLPM.");

    // 1. ibis-calc の関数情報テーブルで prefix が設定されているか（Excel 2007 より新しい関数）
    if let Some(info) = ibis_calc::functions::table::function_info(stripped) {
        if !info.prefix.is_empty() {
            return true;
        }
    }

    // 2. Excel 2007/2010/2013/365 で追加された主要関数・動的配列関数
    matches!(
        stripped,
        "XLOOKUP"
            | "XMATCH"
            | "FILTER"
            | "SORT"
            | "SORTBY"
            | "UNIQUE"
            | "SEQUENCE"
            | "RANDARRAY"
            | "IFS"
            | "SWITCH"
            | "CONCAT"
            | "TEXTJOIN"
            | "TEXTSPLIT"
            | "TEXTBEFORE"
            | "TEXTAFTER"
            | "CHOOSECOLS"
            | "CHOOSEROWS"
            | "DROP"
            | "TAKE"
            | "EXPAND"
            | "TOCOL"
            | "TOROW"
            | "WRAPCOLS"
            | "WRAPROWS"
            | "LAMBDA"
            | "MAP"
            | "REDUCE"
            | "SCAN"
            | "BYROW"
            | "BYCOL"
            | "ISOMITTED"
            | "LET"
            | "SINGLE"
            | "ANCHORARRAY"
            | "VALUETOTEXT"
            | "ARRAYTOTEXT"
            | "FORMULATEXT"
            | "IFERROR"
            | "IFNA"
            | "SUMIFS"
            | "COUNTIFS"
            | "AVERAGEIF"
            | "AVERAGEIFS"
            | "MAXIFS"
            | "MINIFS"
    )
}

/// 数式文字列から未対応の関数名を抽出する
fn find_unsupported_functions(formula_text: &str) -> Vec<String> {
    let mut unsupported = Vec::new();
    let Ok(tokens) = ibis_calc::formula::lexer::tokenize(formula_text) else {
        return unsupported;
    };
    for (i, t) in tokens.iter().enumerate() {
        if let ibis_calc::formula::lexer::Token::Word(w) = &t.token {
            let is_call = matches!(
                tokens.get(i + 1).map(|next| &next.token),
                Some(ibis_calc::formula::lexer::Token::LParen)
            );
            if is_call && is_unsupported_function(w) {
                let clean_name = w
                    .trim_start_matches("_xlfn.")
                    .trim_start_matches("_xlws.")
                    .trim_start_matches("_xlpm.")
                    .to_uppercase();
                if !unsupported.contains(&clean_name) {
                    unsupported.push(clean_name);
                }
            }
        }
    }
    unsupported
}

/// ワークブック全体をスキャンして Excel 97-2003 との互換性問題を検査する
pub(crate) fn check_compatibility(p: &ibis_doc_model::Presentation) -> CompatibilityReport {
    let mut significant = Vec::new();
    let mut minor = Vec::new();

    // 1. 各ワークシートの検査
    for (idx, slide) in p.slides.iter().enumerate() {
        let sheet_name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", idx + 1));
        let Some(sheet) = sheet_of_slide(slide) else { continue };

        // (A) グリッドサイズの上限チェック (Excel 97-2003: 最大 65,536 行、256 列)
        let mut out_of_bounds = Vec::new();
        for cell in &sheet.cells {
            if cell.row >= 65536 || cell.column >= 256 {
                out_of_bounds.push((cell.row, cell.column));
            }
        }
        if !out_of_bounds.is_empty() {
            let count = out_of_bounds.len();
            let (first_row, first_col) = out_of_bounds[0];
            let cell_ref = format!("{}{}", super::sheet_ui::column_letters(first_col), first_row + 1);
            significant.push(CompatibilityIssue {
                severity: CompatibilitySeverity::SignificantLoss,
                sheet_id: slide.id,
                sheet_index: idx,
                sheet_name: sheet_name.clone(),
                cell: Some((first_row, first_col)),
                title: crate::shell::i18n::format("_xlsx.sheet-name-cell-range-exceeds-the-older-version-", &[("sheet_name", &sheet_name), ("count", &count)]),
                description: crate::shell::i18n::format("_xlsx.this-sheet-has-count-cells-beyond-the-excel-97-2", &[("count", &count), ("cell_ref", &cell_ref)]),
                occurrences: count,
            });
        }

        // (B) 数式内の未対応関数チェック
        for cell in &sheet.cells {
            if let Some(formula_text) = &cell.formula {
                let funcs = find_unsupported_functions(formula_text);
                for func in funcs {
                    let cell_ref = format!("{}{}", super::sheet_ui::column_letters(cell.column), cell.row + 1);
                    significant.push(CompatibilityIssue {
                        severity: CompatibilitySeverity::SignificantLoss,
                        sheet_id: slide.id,
                        sheet_index: idx,
                        sheet_name: sheet_name.clone(),
                        cell: Some((cell.row, cell.column)),
                        title: crate::shell::i18n::format("_xlsx.sheet-name-cell-ref-contains-unsupported-functio", &[("sheet_name", &sheet_name), ("cell_ref", &cell_ref), ("func", &func)]),
                        description: crate::shell::i18n::format("_xlsx.function-func-is-not-supported-in-excel-97-2003-", &[("func", &func)]),
                        occurrences: 1,
                    });
                }
            }
        }

        // (C) 拡張条件付き書式のチェック
        for cf in &sheet.conditional {
            let first_cell = cf.ranges.first().map(|r| (r.first_row, r.first_column));
            let cell_info = first_cell.map(|(r, c)| format!(" ({}{})", super::sheet_ui::column_letters(c), r + 1)).unwrap_or_default();

            // 4つ以上のルール（Excel 97-2003 は最大3ルールまでしか評価しない）
            if cf.rules.len() > 3 {
                significant.push(CompatibilityIssue {
                    severity: CompatibilitySeverity::SignificantLoss,
                    sheet_id: slide.id,
                    sheet_index: idx,
                    sheet_name: sheet_name.clone(),
                    cell: first_cell,
                    title: crate::shell::i18n::format("_xlsx.sheet-name-cell-info-four-or-more-conditional-fo", &[("sheet_name", &sheet_name), ("cell_info", &cell_info)]),
                    description: crate::shell::i18n::text("_xlsx.in-excel-97-2003-only-the-first-three-rules-are-").into(),
                    occurrences: cf.rules.len(),
                });
            }

            for rule in &cf.rules {
                match &rule.kind {
                    CfKind::DataBar { .. } => {
                        significant.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::SignificantLoss,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            cell: first_cell,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-cell-info-data-bar-conditional-format", &[("sheet_name", &sheet_name), ("cell_info", &cell_info)]),
                            description: crate::shell::i18n::text("_xlsx.data-bars-are-not-supported-in-excel-97-2003-and").into(),
                            occurrences: 1,
                        });
                    }
                    CfKind::ColorScale(..) => {
                        significant.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::SignificantLoss,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            cell: first_cell,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-cell-info-color-scale-conditional-for", &[("sheet_name", &sheet_name), ("cell_info", &cell_info)]),
                            description: crate::shell::i18n::text("_xlsx.color-scales-are-not-supported-in-excel-97-2003-").into(),
                            occurrences: 1,
                        });
                    }
                    CfKind::IconSet { .. } => {
                        significant.push(CompatibilityIssue {
                            severity: CompatibilitySeverity::SignificantLoss,
                            sheet_id: slide.id,
                            sheet_index: idx,
                            sheet_name: sheet_name.clone(),
                            cell: first_cell,
                            title: crate::shell::i18n::format("_xlsx.sheet-name-cell-info-icon-set-conditional-format", &[("sheet_name", &sheet_name), ("cell_info", &cell_info)]),
                            description: crate::shell::i18n::text("_xlsx.icon-sets-are-not-supported-in-excel-97-2003-and").into(),
                            occurrences: 1,
                        });
                    }
                    _ => {}
                }
            }
        }

        // (D) Excel テーブルのチェック (Minor loss of fidelity)
        for table in &sheet.tables {
            let tbl_name = if table.name.is_empty() { crate::shell::i18n::text("_xlsx.table").to_string() } else { table.name.clone() };
            minor.push(CompatibilityIssue {
                severity: CompatibilitySeverity::MinorLoss,
                sheet_id: slide.id,
                sheet_index: idx,
                sheet_name: sheet_name.clone(),
                cell: Some((table.range.first_row, table.range.first_column)),
                title: crate::shell::i18n::format("_xlsx.sheet-name-table-tbl-name-will-be-converted-to-a", &[("sheet_name", &sheet_name), ("tbl_name", &tbl_name)]),
                description: crate::shell::i18n::text("_xlsx.in-excel-97-2003-tables-are-converted-to-normal-").into(),
                occurrences: 1,
            });
        }

        // (E) スパークラインのチェック (Minor loss of fidelity)
        if !sheet.sparklines.is_empty() {
            let total_sparklines: usize = sheet.sparklines.iter().map(|g| g.sparklines.len()).sum();
            let first_cell = sheet.sparklines.first().and_then(|g| g.sparklines.first()).map(|s| (s.row, s.column));
            let cell_info = first_cell.map(|(r, c)| format!(" ({}{})", super::sheet_ui::column_letters(c), r + 1)).unwrap_or_default();
            minor.push(CompatibilityIssue {
                severity: CompatibilitySeverity::MinorLoss,
                sheet_id: slide.id,
                sheet_index: idx,
                sheet_name: sheet_name.clone(),
                cell: first_cell,
                title: crate::shell::i18n::format("_xlsx.sheet-name-cell-info-sparklines-are-not-displaye", &[("sheet_name", &sheet_name), ("cell_info", &cell_info), ("total_sparklines", &total_sparklines)]),
                description: crate::shell::i18n::text("_xlsx.sparklines-were-introduced-in-excel-2010-so-they").into(),
                occurrences: total_sparklines,
            });
        }

        // (F) グラデーション塗りつぶしのチェック (Minor loss of fidelity)
        let mut gradient_cells = Vec::new();
        for cell in &sheet.cells {
            if let Some(style) = sheet.styles.get(cell.style as usize) {
                if style.gradient.is_some() {
                    gradient_cells.push((cell.row, cell.column));
                }
            }
        }
        if !gradient_cells.is_empty() {
            let count = gradient_cells.len();
            let (first_row, first_col) = gradient_cells[0];
            let cell_ref = format!("{}{}", super::sheet_ui::column_letters(first_col), first_row + 1);
            minor.push(CompatibilityIssue {
                severity: CompatibilitySeverity::MinorLoss,
                sheet_id: slide.id,
                sheet_index: idx,
                sheet_name: sheet_name.clone(),
                cell: Some((first_row, first_col)),
                title: crate::shell::i18n::format("_xlsx.sheet-name-gradient-fills-will-be-converted-to-s", &[("sheet_name", &sheet_name), ("count", &count)]),
                description: crate::shell::i18n::format("_xlsx.this-sheet-has-count-cells-with-gradient-fills-e", &[("count", &count), ("cell_ref", &cell_ref)]),
                occurrences: count,
            });
        }
    }

    // 2. ブック全体の定義名に含まれる未対応関数のチェック
    if let Some(wb) = p.workbook.as_ref() {
        for name in &wb.names {
            let funcs = find_unsupported_functions(&name.formula);
            for func in funcs {
                let default_slide_id = p.slides.first().map(|s| s.id).unwrap_or(SlideId(0));
                let sheet_id = name.sheet.and_then(|si| p.slides.get(si).map(|s| s.id)).unwrap_or(default_slide_id);
                let s_name = name.sheet.and_then(|si| p.slides.get(si).and_then(|s| s.name.clone())).unwrap_or_else(|| crate::shell::i18n::text("xlsx.print-what.entire-workbook").to_string());
                significant.push(CompatibilityIssue {
                    severity: CompatibilitySeverity::SignificantLoss,
                    sheet_id,
                    sheet_index: name.sheet.unwrap_or(0),
                    sheet_name: s_name,
                    cell: None,
                    title: crate::shell::i18n::format("_xlsx.name-name-contains-unsupported-function-func", &[("name", &name.name), ("func", &func)]),
                    description: crate::shell::i18n::format("_xlsx.the-function-func-in-this-defined-name-s-formula", &[("func", &func)]),
                    occurrences: 1,
                });
            }
        }
    }

    CompatibilityReport { significant, minor }
}

/// サイドバーに表示する互換性チェック パネル
pub(crate) struct CompatibilityPanel {
    pub(crate) report: CompatibilityReport,
    pub(crate) selected_issue: Option<usize>,
    pub(crate) message: String,
}

impl SlideEditor {
    /// 互換性チェック パネルを開く
    pub(crate) fn open_xlsx_compatibility_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let report = check_compatibility(self.presentation());
        let panel = CompatibilityPanel {
            report,
            selected_issue: None,
            message: String::new(),
        };

        let state = self.wb_mut();
        state.close_settings();
        state.panels.compatibility = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 互換性チェック パネルを閉じる
    pub(crate) fn close_xlsx_compatibility_panel(&mut self, cx: &mut Context<Self>) {
        self.wb_mut().panels.compatibility = None;
        self.sidebar_open = false;
        cx.notify();
    }

    /// 再検査を実行
    pub(crate) fn recheck_xlsx_compatibility(&mut self, cx: &mut Context<Self>) {
        let report = check_compatibility(self.presentation());
        if let Some(p) = self.wb_mut().panels.compatibility.as_mut() {
            p.report = report;
            p.selected_issue = None;
            p.message = text("xlsx.compatibility.recheck-done").to_string();
            cx.notify();
        }
    }

    /// 問題のあるシートおよびセルにジャンプ（フォーカス）
    pub(crate) fn jump_to_xlsx_compatibility_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let issue = {
            let Some(p) = self.wb().panels.compatibility.as_ref() else { return };
            let all = p.report.all_issues();
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

        if let Some(p) = self.wb_mut().panels.compatibility.as_mut() {
            p.selected_issue = Some(issue_idx);
        }

        cx.notify();
    }

    /// サイドバーの互換性チェック パネル UI 要素
    pub(crate) fn xlsx_compatibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.wb().panels.compatibility.as_ref()?;
        let report = panel.report.clone();
        let selected_idx = panel.selected_issue;
        let msg = panel.message.clone();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("xlsx.compatibility.desc").to_string()),
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
                        .child(text("xlsx.compatibility.passed").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("xlsx.compatibility.passed-sub").to_string()),
                )
        });

        let mut issues_list = v_flex().gap_3();

        let all_issues = report.all_issues();

        // 1. 重大な機能低下セクション
        if !report.significant.is_empty() {
            let mut sig_section = v_flex().gap_2();
            sig_section = sig_section.child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("{} ({})", text("xlsx.compatibility.significant"), report.significant.len())),
            );

            for (ix, issue) in report.significant.iter().enumerate() {
                let overall_ix = ix;
                let is_selected = selected_idx == Some(overall_ix);
                let title = issue.title.clone();
                let desc = issue.description.clone();

                let border_color = if is_selected {
                    cx.theme().primary
                } else {
                    cx.theme().danger.opacity(0.4)
                };

                let bg_color = if is_selected {
                    cx.theme().primary.opacity(0.08)
                } else {
                    cx.theme().danger.opacity(0.04)
                };

                let item = v_flex()
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
                                    .text_sm()
                                    .text_color(cx.theme().danger)
                                    .child(title),
                            )
                            .child(
                                crate::shell::chrome::action_button(("jump-compat-issue", overall_ix))
                                    .label(text("_shell.view"))
                                    .small()
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.jump_to_xlsx_compatibility_issue(overall_ix, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(desc),
                    );

                sig_section = sig_section.child(item);
            }

            issues_list = issues_list.child(sig_section);
        }

        // 2. 軽微な精度の低下セクション
        if !report.minor.is_empty() {
            let offset = report.significant.len();
            let mut minor_section = v_flex().gap_2();
            minor_section = minor_section.child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(cx.theme().warning)
                    .child(format!("{} ({})", text("xlsx.compatibility.minor"), report.minor.len())),
            );

            for (ix, issue) in report.minor.iter().enumerate() {
                let overall_ix = offset + ix;
                let is_selected = selected_idx == Some(overall_ix);
                let title = issue.title.clone();
                let desc = issue.description.clone();

                let border_color = if is_selected {
                    cx.theme().primary
                } else {
                    cx.theme().warning.opacity(0.4)
                };

                let bg_color = if is_selected {
                    cx.theme().primary.opacity(0.08)
                } else {
                    cx.theme().background
                };

                let item = v_flex()
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
                                    .text_sm()
                                    .text_color(cx.theme().warning)
                                    .child(title),
                            )
                            .child(
                                crate::shell::chrome::action_button(("jump-compat-issue", overall_ix))
                                    .label(text("_shell.view"))
                                    .small()
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.jump_to_xlsx_compatibility_issue(overall_ix, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(desc),
                    );

                minor_section = minor_section.child(item);
            }

            issues_list = issues_list.child(minor_section);
        }

        let bottom_actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("compat-recheck", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("xlsx.compatibility.recheck"))
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_xlsx_compatibility(cx))),
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
            .when(!all_issues.is_empty(), |el| el.child(issues_list))
            .child(div().h_2())
            ;

        let panel_terminal_actions = bottom_actions;
        Some(SidebarPanel::with_footer(text("xlsx.compatibility.title"), content.into_any_element(), panel_terminal_actions))
    }
}
