// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DOCX-OPTIONS-002: 画面のヒントのスタイル（ScreenTip Style）
//!
//! Word 標準の「画面のヒントのスタイル」（ScreenTip Style）をクリーンルーム実装。
//! Word オプション > 全般 > ユーザー インターフェイスのオプション:
//! 1. 「画面のヒントに機能の説明を表示する」（Show feature descriptions in ScreenTips: 既定）
//! 2. 「画面のヒントに機能の説明を表示しない」（Don't show feature descriptions in ScreenTips）
//! 3. 「画面のヒントを表示しない」（Don't show ScreenTips）
//!
//! 表示メニュー「表示 > 画面のヒント」（`docx.view.screentips`）での動的切り替えに対応。

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::ext::*;

/// 画面のヒントの表示スタイル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScreenTipStyle {
    /// 画面のヒントに機能の説明を表示する（Word 既定値）
    #[default]
    ShowFeatureDescriptions,
    /// 画面のヒントに機能の説明を表示しない（機能名とショートカットのみ）
    DontShowFeatureDescriptions,
    /// 画面のヒントを表示しない（ツールチップ自体を非表示）
    DontShowScreenTips,
}

impl ScreenTipStyle {
    #[allow(dead_code)]
    pub(crate) fn ooxml_str(self) -> &'static str {
        match self {
            Self::ShowFeatureDescriptions => "showFeatureDescriptions",
            Self::DontShowFeatureDescriptions => "dontShowFeatureDescriptions",
            Self::DontShowScreenTips => "dontShowScreenTips",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_ooxml_str(s: &str) -> Option<Self> {
        match s {
            "showFeatureDescriptions" => Some(Self::ShowFeatureDescriptions),
            "dontShowFeatureDescriptions" => Some(Self::DontShowFeatureDescriptions),
            "dontShowScreenTips" => Some(Self::DontShowScreenTips),
            _ => None,
        }
    }
}

/// 画面のヒントの状態管理
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ScreenTipState {
    pub(crate) style: ScreenTipStyle,
}

#[ibis_office_macros::editor_ext(Screentip)]
impl SlideEditor {
    /// 現在の画面のヒントスタイルを取得
    pub(crate) fn screentip_style(&self) -> ScreenTipStyle {
        self.doc_state()
            .and_then(|s| s.screentip.as_ref())
            .map_or(ScreenTipStyle::default(), |st| st.style)
    }

    /// 画面のヒントスタイルを設定
    pub(crate) fn set_screentip_style(&mut self, style: ScreenTipStyle, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            if let Some(st) = s.screentip.as_mut() {
                st.style = style;
            } else {
                s.screentip = Some(ScreenTipState { style });
            }
        }
        cx.notify();
    }

    /// 画面のヒント（ツールチップ）のテキストをスタイルに応じて整形して生成する
    /// - DontShowScreenTips の場合は None を返す（ツールチップ非表示）
    /// - DontShowFeatureDescriptions の場合は タイトル [+ (ショートカット)] のみ返す
    /// - ShowFeatureDescriptions の場合は タイトル [+ (ショートカット)] + 説明文を返す
    pub(crate) fn format_screentip(
        &self,
        title_key: &str,
        desc_key: Option<&str>,
        shortcut: Option<&str>,
    ) -> Option<SharedString> {
        match self.screentip_style() {
            ScreenTipStyle::DontShowScreenTips => None,
            ScreenTipStyle::DontShowFeatureDescriptions => {
                let title = text(title_key);
                if let Some(sc) = shortcut {
                    Some(format!("{title} ({sc})").into())
                } else {
                    Some(title)
                }
            }
            ScreenTipStyle::ShowFeatureDescriptions => {
                let title = text(title_key);
                let head = if let Some(sc) = shortcut {
                    format!("{title} ({sc})")
                } else {
                    title.to_string()
                };
                if let Some(dk) = desc_key {
                    let desc = text(dk);
                    if !desc.is_empty() {
                        Some(format!("{head}\n\n{desc}").into())
                    } else {
                        Some(head.into())
                    }
                } else {
                    Some(head.into())
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::ScreenTipStyle;

    #[test]
    fn test_screentip_style_default() {
        assert_eq!(ScreenTipStyle::default(), ScreenTipStyle::ShowFeatureDescriptions);
    }

    #[test]
    fn test_screentip_style_ooxml_roundtrip() {
        for style in [
            ScreenTipStyle::ShowFeatureDescriptions,
            ScreenTipStyle::DontShowFeatureDescriptions,
            ScreenTipStyle::DontShowScreenTips,
        ] {
            let s = style.ooxml_str();
            assert_eq!(ScreenTipStyle::from_ooxml_str(s), Some(style));
        }
        assert_eq!(ScreenTipStyle::from_ooxml_str("unknown"), None);
    }
}
