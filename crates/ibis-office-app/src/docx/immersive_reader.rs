// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DOCX-ACCESS-010: イマーシブ リーダー（Immersive Reader / 学習ツール）
//!
//! Word 標準の「表示 > イマーシブ リーダー」（Immersive Reader）をクリーンルーム実装。
//! 読書や文章の把握、学習・アクセシビリティを支援するための専用閲覧モード。
//! - 列の幅（非常に狭い / 狭い / 標準 / 広い）
//! - ページの色（白 / セピア / 反転・ダーク / 淡黄色 / 淡青色）
//! - テキストの間隔（文字・単語・行間を広げて視認性向上）
//! - 音節（Syllables: 英単語等を音節ごとに「·」で区切って表示）
//! - 行フォーカス（Line Focus: 1行 / 3行 / 5行 のフォーカス行ハイライトと周囲減光）
//! - 閉じるボタン（Esc キーまたは専用ボタンで通常編集画面へワンクリック復帰）
//!
//! ドキュメントの OOXML データ自体は一切変更せず、表示専用の読書支援として動作する。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 列の幅（Column Width）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ColumnWidth {
    VeryNarrow, // 非常に狭い (約 380px)
    Narrow,     // 狭い (約 520px)
    #[default]
    Moderate,   // 標準 (約 680px)
    Wide,       // 広い (制限なし)
}

impl ColumnWidth {
    pub(crate) fn max_width_px(&self) -> Option<f32> {
        match self {
            Self::VeryNarrow => Some(380.0),
            Self::Narrow => Some(520.0),
            Self::Moderate => Some(680.0),
            Self::Wide => None,
        }
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub(crate) fn label_key(&self) -> &'static str {
        match self {
            Self::VeryNarrow => "docx.immersive.width.very-narrow",
            Self::Narrow => "docx.immersive.width.narrow",
            Self::Moderate => "docx.immersive.width.moderate",
            Self::Wide => "docx.immersive.width.wide",
        }
    }
}

/// ページの色（Page Color / 背景・文字の配色）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ImmersivePageColor {
    #[default]
    None,       // 白 (通常)
    Sepia,      // セピア (目に優しい暖色)
    Inverse,    // 反転・ダーク (暗い背景・明るい文字)
    SoftYellow, // 淡黄色 (パステルイエロー)
    SoftBlue,   // 淡青色 (パステルブルー)
}

impl ImmersivePageColor {
    pub(crate) fn bg_color(&self) -> Rgba {
        match self {
            Self::None => rgb(0xf3f3f3),
            Self::Sepia => rgb(0xf5eedb),
            Self::Inverse => rgb(0x1a1a1a),
            Self::SoftYellow => rgb(0xfdf8d8),
            Self::SoftBlue => rgb(0xe6f2fa),
        }
    }

    pub(crate) fn paper_color(&self) -> Rgba {
        match self {
            Self::None => rgb(0xffffff),
            Self::Sepia => rgb(0xfbf0d9),
            Self::Inverse => rgb(0x252525),
            Self::SoftYellow => rgb(0xfff9c4),
            Self::SoftBlue => rgb(0xecf6fc),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn text_color(&self) -> Rgba {
        match self {
            Self::None => rgb(0x111111),
            Self::Sepia => rgb(0x5f4b32),
            Self::Inverse => rgb(0xe2e2e2),
            Self::SoftYellow => rgb(0x28261e),
            Self::SoftBlue => rgb(0x0a3b66),
        }
    }

    pub(crate) fn label_key(&self) -> &'static str {
        match self {
            Self::None => "docx.immersive.color.none",
            Self::Sepia => "docx.immersive.color.sepia",
            Self::Inverse => "docx.immersive.color.inverse",
            Self::SoftYellow => "docx.immersive.color.soft-yellow",
            Self::SoftBlue => "docx.immersive.color.soft-blue",
        }
    }
}

/// 行フォーカス（Line Focus）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LineFocus {
    #[default]
    Off,
    OneLine,
    ThreeLines,
    FiveLines,
}

impl LineFocus {
    pub(crate) fn line_count(&self) -> usize {
        match self {
            Self::Off => 0,
            Self::OneLine => 1,
            Self::ThreeLines => 3,
            Self::FiveLines => 5,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label_key(&self) -> &'static str {
        match self {
            Self::Off => "docx.immersive.focus.off",
            Self::OneLine => "docx.immersive.focus.one-line",
            Self::ThreeLines => "docx.immersive.focus.three-lines",
            Self::FiveLines => "docx.immersive.focus.five-lines",
        }
    }
}

/// イマーシブ リーダーの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImmersiveReaderState {
    pub(crate) enabled: bool,
    pub(crate) column_width: ColumnWidth,
    pub(crate) page_color: ImmersivePageColor,
    pub(crate) text_spacing: bool,
    pub(crate) syllables: bool,
    pub(crate) line_focus: LineFocus,
    pub(crate) focus_line: usize,
}

impl Default for ImmersiveReaderState {
    fn default() -> Self {
        Self {
            enabled: false,
            column_width: ColumnWidth::Moderate,
            page_color: ImmersivePageColor::None,
            text_spacing: false,
            syllables: false,
            line_focus: LineFocus::Off,
            focus_line: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// 音節（Syllables）分割アルゴリズム
// ---------------------------------------------------------------------------

/// 英単語を音節（syllable）に分解し、区切り記号「·」を挟んだ文字列を返す。
/// クリーンルーム実装：母音クラスタ、二重子音、接頭辞/接尾辞に基づくルールベース分割。
pub(crate) fn split_word_syllables(word: &str) -> String {
    if word.chars().count() <= 3 {
        return word.to_string();
    }

    // 句読点等の前後の非英文字を分離
    let chars: Vec<char> = word.chars().collect();
    let start_idx = chars.iter().position(|c| c.is_alphabetic()).unwrap_or(0);
    let end_idx = chars.iter().rposition(|c| c.is_alphabetic()).map(|i| i + 1).unwrap_or(chars.len());

    if start_idx >= end_idx {
        return word.to_string();
    }

    let prefix_str: String = chars[..start_idx].iter().collect();
    let suffix_str: String = chars[end_idx..].iter().collect();
    let letters = &chars[start_idx..end_idx];

    let is_vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');

    let mut result = String::new();
    let len = letters.len();
    let mut i = 0;

    // 一般的な接頭辞（prefix）の簡易検出
    let lower: String = letters.iter().map(|c| c.to_ascii_lowercase()).collect();
    let known_prefixes = ["un", "in", "im", "dis", "pre", "sub", "mis", "non", "over"];
    for p in known_prefixes {
        if lower.starts_with(p) && len > p.len() + 2 {
            for j in 0..p.len() {
                result.push(letters[j]);
            }
            result.push('·');
            i = p.len();
            break;
        }
    }

    // 一般的な接尾辞（suffix）の簡易検出（例: read·ing, move·ment）
    let known_suffixes = ["tion", "ment", "ness", "less", "able", "ful", "ing"];
    let mut suffix_cut = None;
    for s in known_suffixes {
        if lower.ends_with(s) && len >= s.len() + 3 {
            suffix_cut = Some(len - s.len());
            break;
        }
    }

    while i < len {
        result.push(letters[i]);

        if i + 1 < len {
            // 接尾辞境界のチェック
            if suffix_cut == Some(i + 1) {
                result.push('·');
            } else {
                let c1 = letters[i];
                let c2 = letters[i + 1];
                let blends = [
                    "th", "sh", "ch", "ph", "wh", "ck", "qu", "ng",
                    "bl", "cl", "fl", "gl", "pl", "br", "cr", "dr", "fr", "gr", "pr", "tr",
                ];

                // 1. 同一の二重子音の分割（例: 'tt' -> 't·t', 'll' -> 'l·l', 'pp' -> 'p·p'）
                if !is_vowel(c1) && !is_vowel(c2) && c1.to_ascii_lowercase() == c2.to_ascii_lowercase() {
                    if i > 0 && is_vowel(letters[i - 1]) && i + 2 < len {
                        result.push('·');
                    }
                }
                // 2. VCCV パターン（母音-子音-子音-母音）の分割（例: 'doc-u-ment' の 'ct' など）
                else if i > 0 && is_vowel(letters[i - 1]) && !is_vowel(c1) && !is_vowel(c2) && i + 2 < len && is_vowel(letters[i + 2]) {
                    let pair = format!("{}{}", c1.to_ascii_lowercase(), c2.to_ascii_lowercase());
                    if !blends.contains(&pair.as_str()) {
                        result.push('·');
                    }
                }
                // 3. V + 子音ブレンド + V（例: 'syl·la·bles'）または VCV（例: 'o·pen', 'com·pu·ter'）
                else if is_vowel(c1) && !is_vowel(c2) {
                    if i + 2 < len {
                        let c3 = letters[i + 2];
                        let pair = format!("{}{}", c2.to_ascii_lowercase(), c3.to_ascii_lowercase());
                        if blends.contains(&pair.as_str()) && i + 3 < len && is_vowel(letters[i + 3]) {
                            result.push('·');
                        } else if is_vowel(c3) && i > 1 && !is_vowel(letters[i - 1]) {
                            result.push('·');
                        }
                    }
                }
                // 4. CVC + 接尾辞直前の単母音（例: 'doc·u·ment'）
                else if i >= 2
                    && !is_vowel(letters[i - 2])
                    && is_vowel(letters[i - 1])
                    && !is_vowel(c1)
                    && is_vowel(c2)
                    && suffix_cut == Some(i + 2)
                {
                    result.push('·');
                }
            }
        }
        i += 1;
    }

    // 末尾や先頭の余分な中黒を除去
    let trimmed = result.trim_matches('·');
    format!("{}{}{}", prefix_str, trimmed, suffix_str)
}

/// テキスト全体の英単語に音節分割を適用する
pub(crate) fn split_text_syllables(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 5 / 4);
    let mut current_word = String::new();

    for c in text.chars() {
        if c.is_alphabetic() {
            current_word.push(c);
        } else {
            if !current_word.is_empty() {
                out.push_str(&split_word_syllables(&current_word));
                current_word.clear();
            }
            out.push(c);
        }
    }
    if !current_word.is_empty() {
        out.push_str(&split_word_syllables(&current_word));
    }
    out
}

// ---------------------------------------------------------------------------
// SlideEditor 実装
// ---------------------------------------------------------------------------

impl SlideEditor {
    /// イマーシブ リーダーが現在有効かどうか
    pub(crate) fn is_immersive_reader_enabled(&self) -> bool {
        self.doc_state()
            .and_then(|s| s.immersive_reader.as_ref())
            .map_or(false, |st| st.enabled)
    }

    /// イマーシブ リーダーの状態参照
    pub(crate) fn immersive_reader_state(&self) -> Option<&ImmersiveReaderState> {
        self.doc_state().and_then(|s| s.immersive_reader.as_ref())
    }

    /// イマーシブ リーダーの状態可変参照
    pub(crate) fn immersive_reader_state_mut(&mut self) -> Option<&mut ImmersiveReaderState> {
        self.doc_state_mut().and_then(|s| s.immersive_reader.as_mut())
    }

    /// イマーシブ リーダーを開く / 有効化
    pub(crate) fn open_immersive_reader(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(s) = self.doc_state_mut() {
            if let Some(st) = s.immersive_reader.as_mut() {
                st.enabled = true;
            } else {
                s.immersive_reader = Some(ImmersiveReaderState {
                    enabled: true,
                    ..Default::default()
                });
            }
            // 読書に集中するためサイドバーを閉じる
            self.sidebar_open = false;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// イマーシブ リーダーを閉じる / 無効化
    pub(crate) fn close_immersive_reader(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.enabled = false;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// イマーシブ リーダーのトグル切り替え
    pub(crate) fn toggle_immersive_reader(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.is_immersive_reader_enabled();
        if current {
            self.close_immersive_reader(window, cx);
        } else {
            self.open_immersive_reader(window, cx);
        }
    }

    /// 列の幅を設定
    pub(crate) fn set_immersive_column_width(&mut self, width: ColumnWidth, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.column_width = width;
        }
        cx.notify();
    }

    /// ページの色を設定
    pub(crate) fn set_immersive_page_color(&mut self, color: ImmersivePageColor, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.page_color = color;
        }
        cx.notify();
    }

    /// テキストの間隔（文字・単語・行間）をトグル
    pub(crate) fn toggle_immersive_text_spacing(&mut self, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.text_spacing = !st.text_spacing;
        }
        cx.notify();
    }

    /// 音節（Syllables）表示をトグル
    pub(crate) fn toggle_immersive_syllables(&mut self, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.syllables = !st.syllables;
        }
        cx.notify();
    }

    /// 行フォーカスを設定
    pub(crate) fn set_immersive_line_focus(&mut self, focus: LineFocus, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            st.line_focus = focus;
        }
        cx.notify();
    }

    /// 行フォーカスの位置を上下に移動
    pub(crate) fn move_immersive_line_focus(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(st) = self.immersive_reader_state_mut() {
            if delta < 0 {
                st.focus_line = st.focus_line.saturating_sub((-delta) as usize);
            } else {
                st.focus_line = st.focus_line.saturating_add(delta as usize);
            }
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // UI レンダリング
    // -----------------------------------------------------------------------

    /// イマーシブ リーダーの専用ツールバー（Immersive Toolbar）を描画
    pub(crate) fn render_immersive_reader_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (current_width, current_color, text_spacing, syllables, line_focus, _focus_line) = {
            let st = self.immersive_reader_state().cloned().unwrap_or_default();
            (st.column_width, st.page_color, st.text_spacing, st.syllables, st.line_focus, st.focus_line)
        };

        let theme = cx.theme().clone();
        let bar_bg = theme.background;
        let bar_border = theme.border;

        // 1. 列の幅（非常に狭い / 狭い / 標準 / 広い）
        let width_button = |label_key: &'static str, w: ColumnWidth, active: bool| {
            crate::shell::chrome::segment_control::SegmentItem::label(
                SharedString::from(format!("imm-width-{}",label_key)),text(label_key),active)
                .on_click(cx.listener(move |e,_,_,cx|e.set_immersive_column_width(w,cx)))
        };

        let width_group = h_flex()
            .gap_1()
            .items_center()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(text("docx.immersive.column-width")),
            )
            .child(
                crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                    .child(width_button("docx.immersive.width.very-narrow", ColumnWidth::VeryNarrow, current_width == ColumnWidth::VeryNarrow))
                    .child(width_button("docx.immersive.width.narrow", ColumnWidth::Narrow, current_width == ColumnWidth::Narrow))
                    .child(width_button("docx.immersive.width.moderate", ColumnWidth::Moderate, current_width == ColumnWidth::Moderate))
                    .child(width_button("docx.immersive.width.wide", ColumnWidth::Wide, current_width == ColumnWidth::Wide)),
            );

        // 2. ページの色（白 / セピア / 反転 / 淡黄 / 淡青）
        let color_swatch = |c: ImmersivePageColor, bg: Rgba, border: Rgba, active: bool| {
            let mut swatch = div()
                .id(format!("imm-color-{}", c.label_key()))
                .w(px(20.))
                .h(px(20.))
                .rounded_full()
                .border_2()
                .border_color(if active { rgb(0x0078d4) } else { border })
                .bg(bg)
                .cursor_pointer();
            if active {
                swatch = swatch.shadow_sm();
            }
            swatch.on_click(cx.listener(move |this, _, _, cx| {
                this.set_immersive_page_color(c, cx);
            }))
        };

        let color_group = h_flex()
            .gap_1()
            .items_center()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(text("docx.immersive.page-color")),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .p(px(2.))
                    .child(color_swatch(ImmersivePageColor::None, rgb(0xffffff), rgb(0xcccccc), current_color == ImmersivePageColor::None))
                    .child(color_swatch(ImmersivePageColor::Sepia, rgb(0xfbf0d9), rgb(0xd0bfa2), current_color == ImmersivePageColor::Sepia))
                    .child(color_swatch(ImmersivePageColor::Inverse, rgb(0x222222), rgb(0x555555), current_color == ImmersivePageColor::Inverse))
                    .child(color_swatch(ImmersivePageColor::SoftYellow, rgb(0xfff9c4), rgb(0xe2dc96), current_color == ImmersivePageColor::SoftYellow))
                    .child(color_swatch(ImmersivePageColor::SoftBlue, rgb(0xe1f5fe), rgb(0xbadcf1), current_color == ImmersivePageColor::SoftBlue)),
            );

        // 3. テキストの間隔（Text Spacing トグル）
        let spacing_button = Button::new("imm-text-spacing")
            .small()
            .ghost()
            .label(text("docx.immersive.text-spacing"))
            .when(text_spacing, |b| b.primary())
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_immersive_text_spacing(cx);
            }));

        // 4. 音節（Syllables トグル）
        let syllables_button = Button::new("imm-syllables")
            .small()
            .ghost()
            .label(text("docx.immersive.syllables"))
            .when(syllables, |b| b.primary())
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_immersive_syllables(cx);
            }));

        // 5. 行フォーカス（Line Focus: Off / 1 / 3 / 5 & ナビゲーション）
        let focus_button = |count_key: &'static str, f: LineFocus, active: bool| {
            let mut btn = div()
                .id(format!("imm-focus-{}", count_key))
                .px(px(6.))
                .py(px(2.))
                .text_size(px(11.))
                .rounded_sm()
                .cursor_pointer();
            if active {
                btn = btn.bg(rgb(0x0078d4)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD);
            } else {
                btn = btn.hover(|h| h.bg(rgb(0xe5e5e5))).text_color(rgb(0x333333));
            }
            btn.child(text(count_key))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_immersive_line_focus(f, cx);
                }))
        };

        let is_focus_active = line_focus != LineFocus::Off;
        let focus_group = h_flex()
            .gap_1()
            .items_center()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(text("docx.immersive.line-focus")),
            )
            .child(
                h_flex()
                    .gap_1()
                    .p(px(2.))
                    .bg(rgb(0xf0f0f0))
                    .rounded_md()
                    .child(focus_button("docx.immersive.focus.off", LineFocus::Off, line_focus == LineFocus::Off))
                    .child(focus_button("docx.immersive.focus.one-line", LineFocus::OneLine, line_focus == LineFocus::OneLine))
                    .child(focus_button("docx.immersive.focus.three-lines", LineFocus::ThreeLines, line_focus == LineFocus::ThreeLines))
                    .child(focus_button("docx.immersive.focus.five-lines", LineFocus::FiveLines, line_focus == LineFocus::FiveLines)),
            )
            .when(is_focus_active, |h| {
                h.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            Button::new("imm-focus-up")
                                .ghost()
                                .xsmall()
                                .label("▲")
                                .tooltip(text("docx.immersive.focus.up"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.move_immersive_line_focus(-1, cx);
                                })),
                        )
                        .child(
                            Button::new("imm-focus-down")
                                .ghost()
                                .xsmall()
                                .label("▼")
                                .tooltip(text("docx.immersive.focus.down"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.move_immersive_line_focus(1, cx);
                                })),
                        ),
                )
            });

        // 6. 音節プレビュー案内バナー（音節が有効な場合のステータス）
        let syllables_status = if syllables {
            let sample_doc_text = self.flow().and_then(|d| {
                d.body.iter().find_map(|b| {
                    if let ibis_doc_model::Block::Paragraph(p) = b {
                        let t = p.text();
                        if !t.trim().is_empty() {
                            Some(t)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            }).unwrap_or_else(|| "Immersive Reader syllables learning tool".to_string());

            let preview_words = split_text_syllables(&sample_doc_text.chars().take(40).collect::<String>());

            Some(
                h_flex()
                    .gap_2()
                    .items_center()
                    .px(px(8.))
                    .py(px(2.))
                    .bg(rgb(0xe8f0fe))
                    .rounded_sm()
                    .text_size(px(11.))
                    .text_color(rgb(0x1a73e8))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(text("docx.immersive.syllables-active")))
                    .child(div().child(format!("「{}…」", preview_words))), // i18n-ok（文書の中の名前・全角の区切り・記号）
            )
        } else {
            None
        };

        // 7. 閉じるボタン（✕ イマーシブ リーダーを閉じる）
        let close_button = crate::shell::chrome::primary_button("imm-close-btn")
            .small()
            .label(text("docx.immersive.close"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_immersive_reader(window, cx);
            }));

        // ツールバー全体のレイアウト
        v_flex()
            .w_full()
            .border_b_1()
            .border_color(bar_border)
            .bg(bar_bg)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px(px(12.))
                    .py(px(6.))
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(div().text_size(px(14.)).child("📖"))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(text("docx.immersive.title")),
                                    ),
                            )
                            .child(width_group)
                            .child(color_group)
                            .child(spacing_button)
                            .child(syllables_button)
                            .child(focus_group),
                    )
                    .child(close_button),
            )
            .children(syllables_status)
            .into_any_element()
    }

    /// 行フォーカスが有効な場合に本文領域に減光（ディミング）マスクを描画する
    pub(crate) fn render_immersive_line_focus_overlay(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let st = self.immersive_reader_state()?;
        if !st.enabled || st.line_focus == LineFocus::Off {
            return None;
        }

        let lines = st.line_focus.line_count();
        let focus_index = st.focus_line;

        // 行の高さ（約 24px）、フォーカス領域の計算
        let line_height_px = if st.text_spacing { 36.0 } else { 26.0 };
        let top_dim_height = (focus_index as f32) * line_height_px;
        let window_height = (lines as f32) * line_height_px;

        let dim_color = rgba(0x00000088);

        Some(
            div()
                .id("imm-focus-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .w_full()
                        .h(px(top_dim_height.max(0.0)))
                        .bg(dim_color),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(window_height))
                        .border_y_1()
                        .border_color(rgba(0x0078d4cc)),
                )
                .child(
                    div()
                        .w_full()
                        .flex_1()
                        .bg(dim_color),
                )
                .into_any_element(),
        )
    }
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        ColumnWidth, ImmersivePageColor, ImmersiveReaderState, LineFocus,
        split_text_syllables, split_word_syllables,
    };
    use gpui_kit::rgb;

    #[test]
    fn test_syllables_splitting() {
        assert_eq!(split_word_syllables("read"), "read");
        assert_eq!(split_word_syllables("reading"), "read·ing");
        assert_eq!(split_word_syllables("important"), "im·por·tant");
        assert_eq!(split_word_syllables("syllables"), "syl·la·bles");
        assert_eq!(split_word_syllables("computer"), "com·pu·ter");
        assert_eq!(split_word_syllables("document"), "doc·u·ment");

        let text = "Important document reading.";
        let res = split_text_syllables(text);
        assert!(res.contains("Im·por·tant"), "result: {res}");
        assert!(res.contains("doc·u·ment"), "result: {res}");
    }

    #[test]
    fn test_immersive_reader_state_defaults_and_transitions() {
        let mut state = ImmersiveReaderState::default();
        assert!(!state.enabled);
        assert_eq!(state.column_width, ColumnWidth::Moderate);
        assert_eq!(state.page_color, ImmersivePageColor::None);
        assert_eq!(state.line_focus, LineFocus::Off);
        assert!(!state.text_spacing);
        assert!(!state.syllables);

        state.enabled = true;
        state.column_width = ColumnWidth::Narrow;
        state.page_color = ImmersivePageColor::Sepia;
        state.line_focus = LineFocus::ThreeLines;
        state.text_spacing = true;
        state.syllables = true;
        state.focus_line = 3;

        assert_eq!(state.column_width.max_width_px(), Some(520.0));
        assert_eq!(state.line_focus.line_count(), 3);
        assert_eq!(state.page_color.paper_color(), rgb(0xfbf0d9));
    }
}
