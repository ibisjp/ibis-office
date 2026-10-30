// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アクセシビリティ チェック (Check Accessibility: PPTX-ACCESS-001):
//! プレゼンテーション全体のアクセシビリティ問題（代替テキストの不足、
//! スライドタイトルの欠落、表の見出し行の未設定等）をスキャン・検出し、
//! 該当オブジェクトへのフォーカスや修正を支援する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{ShapeId, ShapeKind, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// アクセシビリティの問題の種類
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum IssueSeverity {
    Error,
    Warning,
    Tip,
}

/// 検出されたアクセシビリティの問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessibilityIssue {
    pub(crate) severity: IssueSeverity,
    pub(crate) slide_id: SlideId,
    pub(crate) slide_index: usize,
    pub(crate) shape_id: Option<ShapeId>,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) fix_type: IssueFixType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum IssueFixType {
    AddAltText,
    MarkDecorative,
    AddSlideTitle,
    AddHiddenTitle,
    MakeTitleUnique,
    EnableTableHeader,
    UseMeaningfulLinkText,
    CheckReadingOrder,
    FixContrast,
    AddMediaCaptions,
}

/// プレゼンテーションのアクセシビリティ検査結果
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

/// スライドのタイトル文字列を取得する
pub(crate) fn slide_title(slide: &ibis_doc_model::Slide) -> Option<String> {
    slide
        .shapes
        .iter()
        .find_map(|s| {
            let is_title_ph = slide.placeholders.get(&s.id).is_some_and(|ph| ph.placeholder.normalized() == "title");
            if is_title_ph {
                s.text_body().and_then(|tb| {
                    let t = tb.text().trim().to_string();
                    if !t.is_empty() { Some(t) } else { None }
                })
            } else {
                None
            }
        })
        .or_else(|| {
            slide.name.as_ref().and_then(|n| {
                let t = n.trim().to_string();
                if !t.is_empty() { Some(t) } else { None }
            })
        })
}

/// リンクの表示テキストが説明的（意味のあるテキスト）かどうかを判定する (PPTX-ACCESS-008):
/// URLそのものや、「ここをクリック」「詳細」などの曖昧・非説明的な文言はスクリーンリーダー
/// 利用者にとって目的が判別できないため、false を返す。
pub(crate) fn is_meaningful_link_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();

    // 1. URLそのものが表示テキストになっている場合 (Raw URL)
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("www.")
    {
        return false;
    }

    // 2. 曖昧・非説明的な語句 (Vague / Non-descriptive phrases)
    const VAGUE_PHRASES: &[&str] = &[
        "ここをクリック", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "ここをクリックしてください", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "こちらをクリック", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "こちら", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "ここ", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "詳細", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "詳細はこちら", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "リンク", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "ハイパーリンク", // i18n-ok（検査に使う語句：文書のリンク文字列と比べる）
        "ウェブサイト", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "サイト", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "ホームページ", // i18n-ok（曖昧なリンク文言の照合語（判定に使う）。VAGUE_PHRASES。ソースはその）
        "click here",
        "click",
        "here",
        "link",
        "hyperlink",
        "more",
        "more info",
        "details",
        "website",
        "read more",
        "learn more",
        "page",
    ];

    if VAGUE_PHRASES.iter().any(|v| lower == *v) {
        return false;
    }

    true
}

/// sRGB のコンポーネント (0..=255) を線形輝度に変換
fn linear_srgb_component(c: u8) -> f64 {
    let s = c as f64 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// 相対輝度（Relative Luminance: W3C WCAG 2.1 基準）
pub(crate) fn relative_luminance(rgb: ibis_doc_model::Rgb) -> f64 {
    0.2126 * linear_srgb_component(rgb.r()) + 0.7152 * linear_srgb_component(rgb.g()) + 0.0722 * linear_srgb_component(rgb.b())
}

/// 2色のコントラスト比（Contrast Ratio: 1.0..=21.0）
pub(crate) fn contrast_ratio(c1: ibis_doc_model::Rgb, c2: ibis_doc_model::Rgb) -> f64 {
    let l1 = relative_luminance(c1);
    let l2 = relative_luminance(c2);
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// プレゼンテーション全体をスキャンしてアクセシビリティ問題を検査する
pub(crate) fn check_accessibility(p: &ibis_doc_model::Presentation) -> AccessibilityReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut tips = Vec::new();

    // スライド タイトルの収集（一意性・重複検査用: PPTX-ACCESS-002）
    let mut title_occurrences: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();

    for (idx, slide) in p.slides.iter().enumerate() {
        let slide_num = idx + 1;

        // 1. スライド タイトルの欠落チェック
        if let Some(t) = slide_title(slide) {
            title_occurrences.entry(t).or_default().push(idx);
        } else {
            errors.push(AccessibilityIssue {
                severity: IssueSeverity::Error,
                slide_id: slide.id,
                slide_index: idx,
                shape_id: None,
                title: crate::shell::i18n::format("_pptx.slide-slide-num-missing-slide-title", &[("slide_num", &slide_num)]),
                description: crate::shell::i18n::text("_pptx.give-each-slide-a-title-so-screen-reader-users-c").into(),
                fix_type: IssueFixType::AddSlideTitle,
            });
        }

        // 2. 図形・画像・表のチェック
        for shape in &slide.shapes {
            let shape_name = slide.shape_names.get(&shape.id).cloned().unwrap_or_else(|| crate::shell::i18n::text("insert.shape").into());

            match &shape.kind {
                ShapeKind::Picture(pic) => {
                    // 装飾用としてマークされているか確認
                    let is_decorative = slide.decorative_shapes.contains(&shape.id);
                    let has_alt = slide.shape_alt_texts.get(&shape.id).is_some_and(|a| !a.description.trim().is_empty() || !a.title.trim().is_empty());

                    if !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            slide_id: slide.id,
                            slide_index: idx,
                            shape_id: Some(shape.id),
                            title: crate::shell::i18n::format("_pptx.slide-slide-num-missing-alternative-text-shape-n", &[("slide_num", &slide_num), ("shape_name", &shape_name)]),
                            description: crate::shell::i18n::text("_pptx.add-alternative-text-so-people-with-low-vision-c").into(),
                            fix_type: IssueFixType::AddAltText,
                        });
                    }

                    // メディアクリップ（動画・音声）の字幕チェック (PPTX-ACCESS-010)
                    if let Some(clip) = &pic.clip {
                        if !clip.options.has_captions() && !is_decorative {
                            warnings.push(AccessibilityIssue {
                                severity: IssueSeverity::Warning,
                                slide_id: slide.id,
                                slide_index: idx,
                                shape_id: Some(shape.id),
                                title: crate::shell::i18n::format("_pptx.slide-slide-num-media-is-missing-captions-shape", &[("slide_num", &slide_num), ("shape_name", &shape_name)]),
                                description: crate::shell::i18n::text("_pptx.add-closed-captions-webvtt-so-people-who-are-dea").into(),
                                fix_type: IssueFixType::AddMediaCaptions,
                            });
                        }
                    }
                }
                ShapeKind::Chart(_) => {
                    let is_decorative = slide.decorative_shapes.contains(&shape.id);
                    let has_alt = slide.shape_alt_texts.get(&shape.id).is_some_and(|a| !a.description.trim().is_empty() || !a.title.trim().is_empty());

                    if !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            slide_id: slide.id,
                            slide_index: idx,
                            shape_id: Some(shape.id),
                            title: crate::shell::i18n::format("_pptx.slide-slide-num-missing-alternative-text-shape-n", &[("slide_num", &slide_num), ("shape_name", &shape_name)]),
                            description: crate::shell::i18n::text("_pptx.add-alternative-text-so-people-with-low-vision-c").into(),
                            fix_type: IssueFixType::AddAltText,
                        });
                    }
                }
                ShapeKind::Auto(auto) => {
                    // テキストを持たない意味のある図形で、代替テキストがない場合
                    let has_text = auto.text.as_ref().is_some_and(|tb| !tb.text().trim().is_empty());
                    let is_decorative = slide.decorative_shapes.contains(&shape.id);
                    let has_alt = slide.shape_alt_texts.get(&shape.id).is_some_and(|a| !a.description.trim().is_empty() || !a.title.trim().is_empty());

                    if !has_text && !is_decorative && !has_alt {
                        errors.push(AccessibilityIssue {
                            severity: IssueSeverity::Error,
                            slide_id: slide.id,
                            slide_index: idx,
                            shape_id: Some(shape.id),
                            title: crate::shell::i18n::format("_pptx.slide-slide-num-missing-alternative-text-shape-n", &[("slide_num", &slide_num), ("shape_name", &shape_name)]),
                            description: crate::shell::i18n::text("_pptx.add-alternative-text-to-the-shape-or-mark-it-as").into(),
                            fix_type: IssueFixType::AddAltText,
                        });
                    }
                }
                ShapeKind::Table(table) => {
                    // 表の見出し行チェック
                    if !table.style_options.first_row {
                        warnings.push(AccessibilityIssue {
                            severity: IssueSeverity::Warning,
                            slide_id: slide.id,
                            slide_index: idx,
                            shape_id: Some(shape.id),
                            title: crate::shell::i18n::format("_pptx.slide-slide-num-table-header-row-is-not-set", &[("slide_num", &slide_num)]),
                            description: crate::shell::i18n::text("_pptx.turn-on-the-header-row-option-so-screen-readers").into(),
                            fix_type: IssueFixType::EnableTableHeader,
                        });
                    }
                }
                _ => {}
            }

            // 2-d. ハイパーリンク テキストの意味・わかりやすさの検査 (PPTX-ACCESS-008)
            // (1) テキスト内のリンク
            if let Some(tb) = shape.text_body() {
                for p in &tb.paragraphs {
                    for run in &p.runs {
                        if run.style.link.is_some() && !is_meaningful_link_text(&run.text) {
                            let raw_text = run.text.trim();
                            let display_label = if raw_text.chars().count() > 25 {
                                format!("{}...", raw_text.chars().take(22).collect::<String>())
                            } else {
                                raw_text.to_string()
                            };
                            warnings.push(AccessibilityIssue {
                                severity: IssueSeverity::Warning,
                                slide_id: slide.id,
                                slide_index: idx,
                                shape_id: Some(shape.id),
                                title: crate::shell::i18n::format("_pptx.slide-slide-num-use-descriptive-link-text-displa", &[("slide_num", &slide_num), ("display_label", &display_label)]),
                                description: crate::shell::i18n::text("_pptx.instead-of-showing-a-url-or-using-general-phrase").into(),
                                fix_type: IssueFixType::UseMeaningfulLinkText,
                            });
                        }
                    }
                }
            }

            // (2) 図形自体のリンク
            if slide.shape_links.contains_key(&shape.id) {
                let text = shape.text_body().map(|tb| tb.text()).unwrap_or_default();
                if !is_meaningful_link_text(&text) {
                    let display_label = if text.trim().is_empty() {
                        shape_name.clone()
                    } else if text.trim().chars().count() > 25 {
                        format!("{}...", text.trim().chars().take(22).collect::<String>())
                    } else {
                        text.trim().to_string()
                    };
                    warnings.push(AccessibilityIssue {
                        severity: IssueSeverity::Warning,
                        slide_id: slide.id,
                        slide_index: idx,
                        shape_id: Some(shape.id),
                        title: crate::shell::i18n::format("_pptx.slide-slide-num-use-descriptive-link-text-displa", &[("slide_num", &slide_num), ("display_label", &display_label)]),
                        description: crate::shell::i18n::text("_pptx.instead-of-showing-a-url-or-using-general-phrase").into(),
                        fix_type: IssueFixType::UseMeaningfulLinkText,
                    });
                }
            }

            // 2-c. テキストのコントラスト比検査 (Check Text Contrast: PPTX-ACCESS-011)
            if let Some(tb) = shape.text_body() {
                let shape_fill_rgb = match &shape.kind {
                    ShapeKind::Auto(auto) => match &auto.fill {
                        ibis_doc_model::Fill::Solid(c) => Some(c.rgb),
                        _ => None,
                    },
                    _ => None,
                };
                let bg_rgb = shape_fill_rgb.unwrap_or_else(|| {
                    match &slide.background {
                        ibis_doc_model::Fill::Solid(c) => c.rgb,
                        _ => ibis_doc_model::Rgb::WHITE,
                    }
                });

                let mut min_contrast: Option<(f64, f64, ibis_doc_model::Rgb, ibis_doc_model::Rgb)> = None;

                for p in &tb.paragraphs {
                    for run in &p.runs {
                        let text = run.text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        let look = tb.style.with_run(&run.style);
                        let text_rgb = look.color;
                        let is_large = look.font_size >= 1800 || (look.bold && look.font_size >= 1400);
                        let required_ratio = if is_large { 3.0 } else { 4.5 };
                        let ratio = contrast_ratio(text_rgb, bg_rgb);

                        if ratio < required_ratio {
                            let is_worse = match min_contrast {
                                Some((min_r, _, _, _)) => ratio < min_r,
                                None => true,
                            };
                            if is_worse {
                                min_contrast = Some((ratio, required_ratio, text_rgb, bg_rgb));
                            }
                        }
                    }
                }

                if let Some((ratio, req, text_rgb, bg_rgb)) = min_contrast {
                    warnings.push(AccessibilityIssue {
                        severity: IssueSeverity::Warning,
                        slide_id: slide.id,
                        slide_index: idx,
                        shape_id: Some(shape.id),
                        title: crate::shell::i18n::format("_pptx.slide-slide-num-text-contrast-is-insufficient-sh", &[("slide_num", &slide_num), ("shape_name", &shape_name)]),
                        description: crate::shell::i18n::format(
                            "_pptx.the-contrast-between-the-text-color-text-and-the",
                            &[
                                ("text", &format!("{:06X}", text_rgb.0)),
                                ("background", &format!("{:06X}", bg_rgb.0)),
                                ("ratio", &format!("{:.1}", ratio)),
                                ("recommended", &format!("{:.1}", req)),
                            ],
                        ),
                        fix_type: IssueFixType::FixContrast,
                    });
                }
            }
        }

        // 2-e. 読み上げ順序の確認推奨（Check Reading Order: PPTX-ACCESS-004）
        let content_count = slide.shapes.iter().filter(|s| !slide.decorative_shapes.contains(&s.id)).count();
        if content_count >= 2 {
            tips.push(AccessibilityIssue {
                severity: IssueSeverity::Tip,
                slide_id: slide.id,
                slide_index: idx,
                shape_id: None,
                title: crate::shell::i18n::format("_pptx.slide-slide-num-check-the-reading-order", &[("slide_num", &slide_num)]),
                description: crate::shell::i18n::text("pptx.accessibility-panel.check-reading-order-pane-ensure").to_string(),
                fix_type: IssueFixType::CheckReadingOrder,
            });
        }
    }

    // 3. スライドの一意なタイトル（重複チェック: PPTX-ACCESS-002）
    for (title, indices) in title_occurrences {
        if indices.len() > 1 {
            for idx in indices {
                let slide = &p.slides[idx];
                let slide_num = idx + 1;
                warnings.push(AccessibilityIssue {
                    severity: IssueSeverity::Warning,
                    slide_id: slide.id,
                    slide_index: idx,
                    shape_id: None,
                    title: crate::shell::i18n::format("_pptx.slide-slide-num-duplicate-slide-title-title", &[("slide_num", &slide_num), ("title", &title)]),
                    description: crate::shell::i18n::text("_pptx.give-each-slide-a-unique-title-such-as-a-number").into(),
                    fix_type: IssueFixType::MakeTitleUnique,
                });
            }
        }
    }

    // スライド順にソートして一貫性のある一覧にする
    warnings.sort_by_key(|w| (w.slide_index, w.shape_id.map(|s| s.0)));

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
    pub(crate) fn open_pptx_accessibility_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let report = check_accessibility(self.presentation());
        let panel = AccessibilityPanel {
            report,
            selected_issue: None,
            message: String::new(),
        };

        if let Some(s) = self.pptx_mut() {
            s.accessibility_panel = Some(panel);
            s.inspect_panel = None;
            s.properties_panel = None;
            s.alt_text_panel = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// アクセシビリティ チェック パネルを閉じる
    pub(crate) fn close_pptx_accessibility_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.accessibility_panel = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 再検査を実行
    pub(crate) fn recheck_pptx_accessibility(&mut self, cx: &mut Context<Self>) {
        let report = check_accessibility(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.accessibility_panel.as_mut()) {
            p.report = report;
            p.selected_issue = None;
            p.message = text("pptx.inspect-panel.reinspection-complete").to_string();
            cx.notify();
        }
    }

    /// 問題のあるスライドおよび図形にジャンプ（フォーカス）
    pub(crate) fn jump_to_pptx_accessibility_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let issue = {
            let Some(p) = self.pptx().and_then(|s| s.accessibility_panel.as_ref()) else { return };
            let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).chain(p.report.tips.iter()).cloned().collect();
            if issue_idx < all.len() {
                all[issue_idx].clone()
            } else {
                return;
            }
        };

        // スライドの選択
        self.current_slide = issue.slide_id;

        // 図形の選択
        self.selection = issue.shape_id;
        if let Some(shape_id) = issue.shape_id {
            if let Some(s) = self.pptx_mut() {
                s.selection = vec![shape_id];
            }
        } else if let Some(s) = self.pptx_mut() {
            s.selection.clear();
        }

        if let Some(p) = self.pptx_mut().and_then(|s| s.accessibility_panel.as_mut()) {
            p.selected_issue = Some(issue_idx);
        }

        cx.notify();
    }

    /// 特定のアクセシビリティ問題の図形を装飾用としてマークし、再検査する
    pub(crate) fn mark_accessibility_issue_decorative(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        self.jump_to_pptx_accessibility_issue(issue_idx, cx);
        let slide = self.current_slide;
        if let Some(shape) = self.selection {
            self.run(vec![ibis_doc_model::Command::SetShapeDecorative { slide, shape, decorative: true }], cx);
            self.recheck_pptx_accessibility(cx);
        }
    }

    /// 特定のアクセシビリティ問題の表に対してタイトル行（見出し行）を有効化し、再検査する
    pub(crate) fn enable_accessibility_issue_table_header(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        self.jump_to_pptx_accessibility_issue(issue_idx, cx);
        if !self.table_option_checked(crate::pptx::editing::TableOption::FirstRow) {
            self.toggle_table_option(crate::pptx::editing::TableOption::FirstRow, cx);
        }
        self.recheck_pptx_accessibility(cx);
    }

    /// アクセシビリティ チェック項目カードの右クリック コンテキストメニュー項目 (PPTX-ACCESS-001..011, PPTX-AUDIT-003)
    pub(crate) fn accessibility_issue_context_menu_entries(&self, issue_idx: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(p) = self.pptx().and_then(|s| s.accessibility_panel.as_ref()) else { return Vec::new() };
        let all: Vec<_> = p.report.errors.iter().chain(p.report.warnings.iter()).chain(p.report.tips.iter()).cloned().collect();
        let Some(issue) = all.get(issue_idx).cloned() else { return Vec::new() };
        let s_id = issue.slide_id;
        let sh_id = issue.shape_id;

        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("_shell.view")
                .icon("visibility")
                .on(move |e, _, cx| e.jump_to_pptx_accessibility_issue(issue_idx, cx))
                .into(),
        ];

        match issue.fix_type {
            IssueFixType::AddAltText | IssueFixType::MarkDecorative => {
                entries.push(MenuEntry::Separator);
                entries.push(
                    MenuItem::tr("pptx.alt-text")
                        .icon("description")
                        .on(move |e, w, cx| {
                            e.jump_to_pptx_accessibility_issue(issue_idx, cx);
                            e.open_alt_text_panel(w, cx);
                        })
                        .into(),
                );
                entries.push(
                    MenuItem::tr("pptx.alt-text.decorative")
                        .icon("check_circle")
                        .on(move |e, _, cx| e.mark_accessibility_issue_decorative(issue_idx, cx))
                        .into(),
                );
            }
            IssueFixType::AddSlideTitle | IssueFixType::AddHiddenTitle => {
                entries.push(MenuEntry::Separator);
                entries.push(
                    MenuItem::tr("pptx.accessibility-panel.add-hidden-title")
                        .icon("title")
                        .on(move |e, _, cx| {
                            e.add_hidden_slide_title(s_id, None, cx);
                            e.recheck_pptx_accessibility(cx);
                        })
                        .into(),
                );
            }
            IssueFixType::EnableTableHeader => {
                entries.push(MenuEntry::Separator);
                entries.push(
                    MenuItem::tr("pptx.table.first-row")
                        .icon("table_rows")
                        .on(move |e, _, cx| e.enable_accessibility_issue_table_header(issue_idx, cx))
                        .into(),
                );
            }
            IssueFixType::CheckReadingOrder => {
                entries.push(MenuEntry::Separator);
                entries.push(
                    MenuItem::tr("pptx.accessibility-panel.check-reading-order")
                        .icon("format_list_numbered")
                        .on(move |e, _, cx| {
                            e.current_slide = s_id;
                            e.open_reading_order_pane(cx);
                        })
                        .into(),
                );
            }
            IssueFixType::AddMediaCaptions => {
                entries.push(MenuEntry::Separator);
                entries.push(
                    MenuItem::tr("_pptx.add-captions")
                        .icon("closed_caption")
                        .on(move |e, _, cx| {
                            e.current_slide = s_id;
                            if let Some(sid) = sh_id {
                                e.selection = Some(sid);
                                if let Some(s) = e.pptx_mut() {
                                    s.selection = vec![sid];
                                }
                                e.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx);
                            }
                            e.recheck_pptx_accessibility(cx);
                        })
                        .into(),
                );
            }
            IssueFixType::MakeTitleUnique | IssueFixType::UseMeaningfulLinkText | IssueFixType::FixContrast => {}
        }

        entries.extend([
            MenuEntry::Separator,
            MenuItem::tr("pptx.inspect-panel.reinspect")
                .icon("refresh")
                .on(|e, _, cx| e.recheck_pptx_accessibility(cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_pptx_accessibility_panel(cx))
                .into(),
        ]);
        entries
    }

    /// アクセシビリティ チェック項目カード上の右クリック メニューを開く
    pub(crate) fn open_accessibility_issue_context_menu(&mut self, issue_idx: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_to_pptx_accessibility_issue(issue_idx, cx);
        let entries = self.accessibility_issue_context_menu_entries(issue_idx);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// サイドバーのアクセシビリティ チェック パネル UI 要素
    pub(crate) fn pptx_accessibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.accessibility_panel.as_ref()?;
        let report = panel.report.clone();
        let selected_idx = panel.selected_issue;
        let msg = panel.message.clone();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("pptx.accessibility-panel.check-presentation-accessibility").to_string()),
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
                        .child(text("pptx.accessibility-panel.no-accessibility-issues-found").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text("pptx.accessibility-panel.all-images-have-alternative-text").to_string()),
                )
        });

        let mut issues_list = v_flex().gap_2();

        let all_issues: Vec<_> = report.errors.iter().chain(report.warnings.iter()).chain(report.tips.iter()).cloned().collect();

        for (ix, issue) in all_issues.iter().enumerate() {
            let is_selected = selected_idx == Some(ix);
            let is_error = issue.severity == IssueSeverity::Error;
            let title = issue.title.clone();
            let desc = issue.description.clone();

            let border_color = if is_selected {
                cx.theme().primary
            } else if is_error {
                cx.theme().danger.opacity(0.4)
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

            let item = v_flex()
                .id(("access-issue-card", ix))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(border_color)
                .bg(bg_color)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.jump_to_pptx_accessibility_issue(ix, cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_accessibility_issue_context_menu(ix, event.position, window, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .text_color(if is_error { cx.theme().danger } else { cx.theme().foreground })
                                .child(title),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .when(matches!(issue.fix_type, IssueFixType::AddAltText | IssueFixType::MarkDecorative), |el| {
                                    el.child(
                                        crate::shell::chrome::action_button(("mark-decorative", ix))
                                            .label(text("pptx.alt-text.decorative"))
                                            .small()
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.mark_accessibility_issue_decorative(ix, cx);
                                            })),
                                    )
                                })
                                .when(matches!(issue.fix_type, IssueFixType::AddSlideTitle | IssueFixType::AddHiddenTitle), |el| {
                                    let s_id = issue.slide_id;
                                    el.child(
                                        crate::shell::chrome::action_button(("add-hidden-title", ix))
                                            .label(text("pptx.accessibility-panel.add-hidden-title"))
                                            .small()
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.add_hidden_slide_title(s_id, None, cx);
                                                this.recheck_pptx_accessibility(cx);
                                            })),
                                    )
                                })
                                .when(issue.fix_type == IssueFixType::EnableTableHeader, |el| {
                                    el.child(
                                        crate::shell::chrome::action_button(("enable-table-header", ix))
                                            .label(text("pptx.table.first-row"))
                                            .small()
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.enable_accessibility_issue_table_header(ix, cx);
                                            })),
                                    )
                                })
                                .when(issue.fix_type == IssueFixType::CheckReadingOrder, |el| {
                                    let s_id = issue.slide_id;
                                    el.child(
                                        crate::shell::chrome::action_button(("check-reading-order", ix))
                                            .label(text("pptx.accessibility-panel.check-reading-order"))
                                            .small()
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.current_slide = s_id;
                                                this.open_reading_order_pane(cx);
                                            })),
                                    )
                                })
                                .when(issue.fix_type == IssueFixType::AddMediaCaptions, |el| {
                                    let s_id = issue.slide_id;
                                    let sh_id = issue.shape_id;
                                    el.child(
                                        crate::shell::chrome::action_button(("add-media-captions", ix))
                                            .label(text("_pptx.add-captions"))
                                            .small()
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.current_slide = s_id;
                                                if let Some(sid) = sh_id {
                                                    this.selection = Some(sid);
                                                    if let Some(s) = this.pptx_mut() {
                                                        s.selection = vec![sid];
                                                    }
                                                    this.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx);
                                                }
                                                this.recheck_pptx_accessibility(cx);
                                            })),
                                    )
                                })
                                .child(
                                    crate::shell::chrome::action_button(("jump-issue", ix))
                                        .label(text("_shell.view"))
                                        .small()
                                        .on_click(cx.listener(move |this, _, _window, cx| {
                                            this.jump_to_pptx_accessibility_issue(ix, cx);
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(desc),
                );

            issues_list = issues_list.child(item);
        }

        let is_hc = self.is_high_contrast();
        let bottom_actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("access-toggle-high-contrast", if is_hc {
                        text("pptx.accessibility-panel.color-view")
                    } else {
                        text("pptx.accessibility-panel.high-contrast")
                    }, crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_high_contrast(cx);
                    })),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("access-recheck", text("pptx.inspect-panel.reinspect"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_pptx_accessibility(cx))),
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
        Some(SidebarPanel::with_footer(text("pptx.accessibility-panel.accessibility"), content.into_any_element(), panel_terminal_actions))
    }
}

