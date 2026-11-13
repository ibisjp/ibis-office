// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DOCX-OPTIONS-001: ミニ ツールバー（Mini Toolbar / クイック書式設定ツールバー）
//!
//! Word 標準の「選択時にミニ ツールバーを表示する」（Show Mini Toolbar on selection）をクリーンルーム実装。
//! テキストを選択した際に、選択範囲のすぐ上にフローティング書式ツールバーを表示する。
//! - フォントサイズの拡大 / 縮小（A▲ / A▼）
//! - 太字（Bold）、斜体（Italic）、下線（Underline）、取り消し線（Strikethrough）
//! - 蛍光ペン（Highlight）、フォントの色（Font Color）
//! - 箇条書き（Bullets）、段落番号（Numbering）
//! - 書式のコピー（Format Painter）
//! - 表示メニュー「表示 > ミニ ツールバー」（`docx.view.mini-toolbar`）でのトグル設定対応。

use gpui_kit::component::h_flex;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::Rgb;

use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::ext::*;

/// ミニツールバーの状態管理
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MiniToolbarState {
    /// 選択時にミニツールバーを表示するかどうか（Word オプション: 既定 true）
    pub(crate) enabled: bool,
}

impl Default for MiniToolbarState {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[ibis_office_macros::editor_ext(MiniToolbar)]
impl SlideEditor {
    /// ミニツールバーが表示有効かどうか
    pub(crate) fn is_mini_toolbar_enabled(&self) -> bool {
        self.doc_state()
            .and_then(|s| s.mini_toolbar.as_ref())
            .map_or(true, |st| st.enabled)
    }

    /// ミニツールバーの表示設定を設定
    pub(crate) fn set_mini_toolbar_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            if let Some(st) = s.mini_toolbar.as_mut() {
                st.enabled = enabled;
            } else {
                s.mini_toolbar = Some(MiniToolbarState { enabled });
            }
        }
        cx.notify();
    }

    /// ミニツールバーの表示設定をトグル
    pub(crate) fn toggle_mini_toolbar(&mut self, cx: &mut Context<Self>) {
        let current = self.is_mini_toolbar_enabled();
        self.set_mini_toolbar_enabled(!current, cx);
    }

    /// ミニツールバーの要素をレンダリング（選択範囲がある場合のみオーバーレイ表示）
    #[allow(dead_code)]
    pub(crate) fn render_mini_toolbar(&self, page_index: usize, k: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_mini_toolbar_enabled() {
            return None;
        }

        // 閲覧モード・イマーシブリーダー中は非表示
        if self.is_read_mode() || self.is_viewing_mode() || self.is_immersive_reader_enabled() {
            return None;
        }

        // テキスト選択があるか確認
        let (start, end) = self.ordered_selection()?;
        if start == end {
            return None;
        }

        // 選択されている行の位置（page_index ページ上）を特定
        let lines = self.flow_lines();
        let target_line = lines.iter().find(|l| l.page == page_index && (l.para == start.para || l.para == end.para))?;

        // 選択範囲の水平・垂直座標を計算
        let lo = if target_line.para == start.para { start.offset } else { 0 };
        let hi = if target_line.para == end.para { end.offset } else { usize::MAX };
        let inside: Vec<f32> = target_line.stops.iter().filter(|(o, _)| *o >= lo && *o <= hi).map(|(_, x)| *x).collect();
        let min_x = inside.iter().copied().reduce(f32::min).unwrap_or(0.0);

        // ミニツールバーの配置座標（選択範囲の少し上、ページ内クランプ）
        let toolbar_left = ((target_line.x + min_x) * k).clamp(12.0, 520.0);
        let toolbar_top = (target_line.y * k - 38.0).max(8.0);

        // 現在の文字書式状態を取得
        let current = self.current_format();
        let style = current.as_ref().map(|(_, s)| s);
        let is_bold = style.as_ref().is_some_and(|s| s.bold);
        let is_italic = style.as_ref().is_some_and(|s| s.italic);
        let is_underline = style.as_ref().is_some_and(|s| s.underline);
        let is_strike = style.as_ref().is_some_and(|s| s.strike);
        let is_highlighted = style.as_ref().and_then(|s| s.highlight).is_some();
        let is_colored = style.as_ref().is_some_and(|s| s.color != Rgb(0x000000));

        let sep = || {
            div()
                .w(px(1.))
                .h(px(14.))
                .mx(px(2.))
                .bg(rgb(0xdddddd))
        };

        let mini_btn = |id: &'static str, label: &'static str, active: bool, _tooltip_text: &'static str| {
            let mut b = div()
                .id(id)
                .px(px(6.))
                .py(px(2.))
                .rounded_sm()
                .text_size(px(11.))
                .cursor_pointer();
            if active {
                b = b.bg(rgb(0x0078d4)).text_color(crate::shell::palette::doc(0xffffff)).font_weight(FontWeight::BOLD);
            } else {
                b = b.hover(|h| h.bg(rgb(0xf0f0f0))).text_color(rgb(0x333333));
            }
            b.child(text(label))
        };

        let toolbar = h_flex()
            .id("docx-mini-toolbar")
            .absolute()
            .left(px(toolbar_left))
            .top(px(toolbar_top))
            .p(px(2.))
            .gap_1()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xcccccc))
            .bg(rgba(0xfffffffa))
            .shadow_md()
            // 1. フォントサイズ変更（A▲ / A▼）
            .child(
                mini_btn("mini-grow-font", "A▲", false, "docx.mini.grow-font")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.step_text_size(true, cx);
                    })),
            )
            .child(
                mini_btn("mini-shrink-font", "A▼", false, "docx.mini.shrink-font")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.step_text_size(false, cx);
                    })),
            )
            .child(sep())
            // 2. 基本書式（B / I / U / S）
            .child(
                mini_btn("mini-bold", "B", is_bold, "docx.mini.bold")
                    .font_weight(FontWeight::BOLD)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_bold(cx);
                    })),
            )
            .child(
                mini_btn("mini-italic", "I", is_italic, "docx.mini.italic")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_italic(cx);
                    })),
            )
            .child(
                mini_btn("mini-underline", "U", is_underline, "docx.mini.underline")
                    .underline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_underline(cx);
                    })),
            )
            .child(
                mini_btn("mini-strike", "S", is_strike, "docx.mini.strikethrough")
                    .line_through()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_strike(cx);
                    })),
            )
            .child(sep())
            // 3. 蛍光ペン（Highlight）
            .child(
                mini_btn("mini-highlight", "🖍", is_highlighted, "docx.mini.highlight")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let new_color = if is_highlighted { None } else { Some(Rgb(0xffff00)) };
                        this.set_highlight(new_color, cx);
                    })),
            )
            // 4. 文字色（Font Color: 赤強調 / 解除）
            .child(
                mini_btn("mini-font-color", "A", is_colored, "docx.mini.font-color")
                    .text_color(if is_colored { rgb(0xffffff) } else { rgb(0xc00000) })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let new_color = if is_colored { Rgb(0x000000) } else { Rgb(0xc00000) };
                        this.set_text_color(new_color, cx);
                    })),
            )
            .child(sep())
            // 5. 箇条書き（•） / 段落番号（1.）
            .child(
                mini_btn("mini-bullet", "•", false, "docx.mini.bullet")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_list(true, cx);
                    })),
            )
            .child(
                mini_btn("mini-numbered", "1.", false, "docx.mini.numbered")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_list(false, cx);
                    })),
            )
            .child(sep())
            // 6. 書式のコピー（Format Painter）
            .child(
                mini_btn("mini-format-painter", "🖌", false, "docx.mini.format-painter")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.capture_doc_format_brush(false, cx);
                    })),
            )
            .into_any_element();

        Some(toolbar)
    }
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::MiniToolbarState;

    #[test]
    fn test_mini_toolbar_state_default() {
        let state = MiniToolbarState::default();
        assert!(state.enabled, "mini toolbar should be enabled by default");
    }
}
