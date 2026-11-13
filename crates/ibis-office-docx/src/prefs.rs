// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a document's window keeps over the files loaded into it: the view settings and the options that are the
//! window's and not the file's (the rulers, the formatting marks, the white space between pages, how text is pasted,
//! the user's name, the print options). The shell makes one with the window's editor (`FormatEditor::new_prefs`) and
//! never looks into it. What the open document itself has is `State`, which is new with every file; nothing here is
//! written to the settings file.
use crate::shell::editor::SlideEditor;

/// Word Options > 詳細設定 > 切り取り、コピー、貼り付け: 貼付の既定動作 (DOCX-OPTIONS-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum DefaultPasteMode {
    /// 元の書式を保持 (Keep Source Formatting, Word既定値).
    #[default]
    KeepSourceFormatting,
    /// 書式を統合 (Merge Formatting / 貼り付け先の書式に合わせる).
    MergeFormatting,
    /// テキストのみ保持 (Keep Text Only).
    KeepTextOnly,
}

/// 編集記号の表示設定 (DOCX-OPTIONS-008: Word Options > 表示 > 常に画面に表示する編集記号).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct FormattingMarksOptions {
    /// 常に表示する: タブ (→)
    pub(crate) show_tabs: bool,
    /// 常に表示する: スペース (·, □)
    pub(crate) show_spaces: bool,
    /// 常に表示する: 段落記号 (¶)
    pub(crate) show_paragraph_marks: bool,
    /// 常に表示する: 任意指定の改行 / 手動改行 (↵)
    pub(crate) show_line_breaks: bool,
    /// 常に表示する: 隠し文字 (下線点線)
    pub show_hidden_text: bool,
}

impl FormattingMarksOptions {
    pub(crate) fn is_tabs_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs
    }
    pub(crate) fn is_spaces_visible(&self, all_open: bool) -> bool {
        all_open || self.show_spaces
    }
    pub(crate) fn is_paragraph_marks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_paragraph_marks
    }
    pub(crate) fn is_line_breaks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_line_breaks
    }
    pub(crate) fn is_hidden_text_visible(&self, all_open: bool) -> bool {
        all_open || self.show_hidden_text
    }
    pub(crate) fn any_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs || self.show_spaces || self.show_paragraph_marks || self.show_line_breaks || self.show_hidden_text
    }
}

/// The document's settings of one window. They stay when another file is loaded into the window, of whatever kind.
pub struct Prefs {
    /// The ruler (ルーラー) is shown.
    pub ruler_open: bool,
    /// The vertical ruler (垂直ルーラー) is shown (DOCX-VIEW-012).
    pub vertical_ruler_open: bool,
    /// The formatting marks (編集記号) are shown.
    pub(crate) formatting_marks_open: bool,
    /// 常に画面に表示する編集記号オプション (DOCX-OPTIONS-008).
    pub(crate) formatting_marks_options: FormattingMarksOptions,
    /// ページ間の余白の表示 (DOCX-VIEW-017). True displays normal page gaps and margins; false collapses page gaps into a thin dividing line.
    pub(crate) show_white_space: bool,
    /// 貼付の既定動作 (DOCX-OPTIONS-006).
    pub(crate) default_paste_mode: DefaultPasteMode,
    /// ユーザー名設定 (DOCX-OPTIONS-003).
    pub custom_user_name: Option<String>,
    /// ユーザー頭文字設定 (DOCX-OPTIONS-003).
    pub custom_user_initials: Option<String>,
    /// 印刷前にリンクされたデータを更新する (DOCX-OPTIONS-009).
    pub(crate) update_links_before_printing: bool,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    pub(crate) print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    pub(crate) print_hidden_text: bool,
    /// 印刷オプション: 変更履歴とコメントを印刷する (DOCX-SEC-014).
    pub(crate) print_markup: bool,
    /// ヘッダー/フッター編集時に文書内のテキストを表示する (Word Header & Footer Tools > オプション > 文書内のテキストを表示).
    pub(crate) show_document_text: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            ruler_open: true,
            vertical_ruler_open: true,
            formatting_marks_open: false,
            formatting_marks_options: FormattingMarksOptions::default(),
            show_white_space: true,
            default_paste_mode: DefaultPasteMode::default(),
            custom_user_name: None,
            custom_user_initials: None,
            update_links_before_printing: false,
            print_backgrounds: false,
            print_hidden_text: false,
            print_markup: true,
            show_document_text: true,
        }
    }
}

#[ibis_office_macros::editor_ext(PrefsExt)]
impl SlideEditor {
    /// The document's settings of this window (there whatever kind of file is open).
    pub fn doc_prefs(&self) -> &Prefs {
        self.format_prefs::<Prefs>()
    }

    pub(crate) fn doc_prefs_mut(&mut self) -> &mut Prefs {
        self.format_prefs_mut::<Prefs>()
    }
}
