// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "scenarios"), allow(dead_code))]
//! Documents (Word): laying the text out into pages, the pages and the
//! ruler, and the document's editing — the document担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

// The crates below, under the names this crate's code uses (as in ibis-office-app and ibis-office-shell): the shell is a crate of
// its own (tasks/doc/crate-split-plan.md, 12), and what it took down from the app's root is under the same names here.
pub(crate) use ibis_office_base::clock;
pub(crate) use clock::debug_timing;
pub(crate) use ibis_office_render as render;
pub(crate) use ibis_office_shell as shell;
pub(crate) use shell::actions::*;
#[cfg(feature = "scenarios")]
pub(crate) use shell::app::*;
#[cfg(all(not(target_family = "wasm"), feature = "scenarios"))]
pub(crate) use shell::read_file::*;
pub(crate) use shell::{grammar, native_shape_clipboard, spelling};

pub(crate) mod doc_ui;
/// The extension traits of this folder's `impl SlideEditor` blocks, written by `scripts/gen-editor-ext.py`.
pub mod ext;
/// The document's own lines of the menus and its button of the toolbar's left part.
pub(crate) mod menu_items;
/// The screen's rows of the sidebar table.
pub(crate) mod sidebars;
pub(crate) mod editing;
pub(crate) mod text_format;
pub(crate) mod block_tree;
pub(crate) mod clipboard;
pub(crate) mod kinsoku_panel;
pub(crate) mod spelling_panel;
pub(crate) mod chart_insert;
pub mod find;
pub(crate) mod wildcard;
pub mod comments;
pub(crate) mod link_panel;
pub(crate) mod picture_crop;
pub(crate) mod view_mode;
pub(crate) mod replace_fonts;
pub(crate) mod view_state;
pub(crate) mod prefs;
pub(crate) mod index_fields;
pub(crate) mod toa_fields;
pub(crate) mod bibliography;
pub mod merge_panel;
pub(crate) mod outline_ops;
pub use crate::shell::backup_copy as backup;
pub(crate) mod control_panel;
pub(crate) mod draw_table;
pub(crate) mod ruby_panel;
pub(crate) mod fit_text_panel;
pub(crate) mod tate_chu_yoko_panel;
pub(crate) mod text_box_panel;
pub(crate) mod text_box_effects;
pub(crate) mod style_panel;
pub(crate) mod paragraph_panel;
pub(crate) mod tab_panel;
pub(crate) mod color_panel;
pub(crate) mod font_panel;
pub(crate) mod paragraph_decorations;
pub(crate) mod picture_panel;
pub(crate) mod table_size_panel;
pub(crate) mod table_formula_panel;
pub mod help_panels;
pub mod list_formats;
pub(crate) mod list_style_panel;
mod style_new;
pub(crate) mod cell_borders;
pub(crate) mod cell_structure;
pub(crate) mod table_convert;
pub mod art_border;
pub mod flow_layout;
pub(crate) mod input;
pub mod page_view;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/scenarios.rs"]
pub mod scenarios;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "tests/corpus_report.rs"]
pub(crate) mod corpus_report;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/typeset68.rs"]
pub(crate) mod typeset68;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/fallback_carry.rs"]
pub(crate) mod fallback_carry;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/crop_mode.rs"]
pub(crate) mod crop_mode;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/tate_chu_yoko_layout_tests.rs"]
pub(crate) mod tate_chu_yoko_layout_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/vertical_underline_tests.rs"]
pub(crate) mod vertical_underline_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/shy_hang_tests.rs"]
pub(crate) mod shy_hang_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/kinsoku_across_space_tests.rs"]
pub(crate) mod kinsoku_across_space_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/math_color_tests.rs"]
pub(crate) mod math_color_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/autospace_letters_tests.rs"]
pub(crate) mod autospace_letters_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/ms_fixed_advance_tests.rs"]
pub(crate) mod ms_fixed_advance_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/deleted_marks_tests.rs"]
pub(crate) mod deleted_marks_tests;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/ivs_tests.rs"]
pub(crate) mod ivs_tests;
pub(crate) mod toolbar;
mod view_menu;
pub(crate) mod ruler_panel;
pub(crate) mod accessibility;
pub(crate) mod goto;
pub(crate) mod compress_panel;
pub(crate) mod shape_picture_tools;
pub(crate) mod math_panel;
pub(crate) mod quick_parts;
pub(crate) mod insert_objects;
pub(crate) mod inspect_panel;
pub(crate) mod compatibility_panel;
pub(crate) mod print_panel;
pub(crate) mod source_manager;
pub(crate) mod thesaurus;
pub(crate) mod immersive_reader;
pub(crate) mod mini_toolbar;
pub(crate) mod screentip;
pub(crate) mod drag_select;
pub(crate) mod repair;
pub(crate) mod template;
pub(crate) mod ink;
pub(crate) mod convert_panel;

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{FlowDocument, FlowPos, InsertFormat, Presentation, SlideId};

use crate::shell::chrome::{Menus, ToolItem};
use crate::shell::editor::{SlideEditor, WorkspaceContent, zoom_scale};
use crate::shell::format::{FormatEditor, FormatState, impl_format_state};

pub use editing::key_bindings;
use crate::ext::*;

/// The formatting captured by the format painter brush.
#[derive(Clone, Debug)]
pub(crate) struct FormatBrush {
    pub(crate) run: ibis_doc_model::RunFormat,
    pub(crate) para: Option<ibis_doc_model::ParaFormat>,
    pub(crate) persistent: bool,
}

/// The tabs in Word's Navigation Pane (見出し・ページ・結果).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum NavigationTab {
    #[default]
    Headings,
    Pages,
    Results,
}

/// What only an open document has.
#[derive(Default)]
pub struct State {
    /// 入力オートコレクト is off (it is on by default).
    pub(crate) autocorrect_off: bool,
    /// 罫線を引いて表を作成: the mode is on, and the box being drawn.
    pub draw_table_mode: bool,
    /// 校閲 > 比較の設定.
    pub(crate) compare_options: ibis_doc_model::compare::CompareOptions,
    pub(crate) draw_box: Option<draw_table::DrawBox>,
    /// 挿入 > 図形: the shape the next drag draws.
    pub draw_shape: Option<draw_table::DrawKind>,
    /// オートコレクトのオプション's panel, while it is the one shown.
    pub autocorrect: Option<crate::shell::autocorrect::AutoCorrectPanel>,
    /// The characters of the text, for the status bar.
    pub characters: usize,
    /// A page to scroll to once the workspace is laid out.
    pub(crate) pending_page: Option<usize>,
    /// The caret, and the other end of the selection when there is one.
    pub caret: Option<FlowPos>,
    pub anchor: Option<FlowPos>,
    /// The other ranges of a selection made of several (⌘-click or ⌘-drag adds one;
    /// the caret and anchor are the last), each in document order (start, end).
    pub(crate) extra_selections: Vec<(FlowPos, FlowPos)>,
    /// Where the caret was in the body before the header or footer was entered (ヘッダーとフッターを閉じる goes back there).
    pub(crate) hf_return: Option<FlowPos>,
    /// Where ↑ / ↓ aim sideways (points), kept over a run of them.
    pub(crate) goal_x: Option<f32>,
    /// Formatting chosen with no selection, for the text typed next.
    pub pending: Option<InsertFormat>,
    /// Supported resolved character/paragraph properties captured by the brush.
    pub(crate) format_brush: Option<FormatBrush>,
    /// Text the input method is composing, shown at the caret.
    pub marked: Option<String>,
    /// How the input method underlines each clause of `marked` (UTF-16 units of it; empty when the platform does not say).
    pub marked_clauses: Vec<ImeClause>,
    /// The document the pages were last laid out from.
    pub(crate) laid_out: Option<Arc<FlowDocument>>,
    /// A drag selecting text is in progress.
    pub(crate) dragging: bool,
    /// 検索と置換's panel, once opened (and while it is the one shown).
    pub find: Option<find::FindState>,
    /// The link panel, while it is the one shown.
    pub link: Option<link_panel::LinkState>,
    /// スタイルの変更's panel, while it is the one shown.
    pub(crate) style_edit: Option<style_panel::StyleEditState>,
    pub(crate) cell_border_edge: Option<usize>,
    pub(crate) paragraph_border_edge: Option<usize>,
    pub font_edit: Option<font_panel::FontEditState>,
    pub color_edit: Option<color_panel::ColorEditState>,
    pub paragraph_edit: Option<paragraph_panel::ParagraphEditState>,
    pub tab_edit: Option<tab_panel::TabEditState>,
    pub(crate) picture_edit: Option<picture_panel::PictureEditState>,
    pub compress_panel: Option<compress_panel::DocxCompressState>,
    pub(crate) table_size: Option<table_size_panel::TableSizeState>,
    pub(crate) table_formula: Option<table_formula_panel::TableFormulaState>,
    /// The comments' panel, while it is the one shown.
    pub(crate) comments: Option<comments::CommentState>,
    /// ルビ's panel, once opened.
    pub ruby: Option<ruby_panel::RubyState>,
    pub fit_text_edit: Option<fit_text_panel::FitTextState>,
    pub(crate) tate_edit: Option<tate_chu_yoko_panel::TateChuYokoState>,
    /// コンテンツ コントロールのプロパティ's panel, while it is the one shown.
    pub(crate) control: Option<control_panel::ControlState>,
    /// テキスト ボックスの文字's panel, while it is the one shown.
    pub text_box: Option<text_box_panel::TextBoxState>,
    /// 差し込み印刷's panel and its data (宛先一覧).
    pub merge: Option<merge_panel::MergeState>,
    /// A floating text box whose text is being edited in place.
    pub box_edit: Option<text_box_panel::BoxEdit>,
    /// A floating text box being dragged.
    pub shape_drag: Option<text_box_panel::ShapeDrag>,
    /// Whether the floating shape at the caret is currently selected (showing handles/outline).
    pub shape_selected: bool,
    /// Additional shapes selected via Shift+Click (alongside the primary shape at `caret`).
    pub(crate) selected_shapes: Vec<FlowPos>,
    /// 数式 (Math) panel, while it is the one shown.
    pub docx_math: Option<math_panel::DocxMathState>,
    /// クイックパーツ (Quick Parts) panel, while it is the one shown.
    pub(crate) quick_parts: Option<quick_parts::QuickPartsState>,
    /// ドキュメントの検査 (Document Inspector) panel, while it is the one shown.
    pub(crate) inspect_panel: Option<inspect_panel::DocInspectState>,
    /// 互換性チェック (Compatibility Checker) panel, while it is the one shown.
    pub(crate) compatibility_panel: Option<compatibility_panel::DocCompatibilityState>,
    /// 印刷の設定 (Print Settings: DOCX-SEC-013, 015, 016)
    pub(crate) print_settings: print_panel::DocxPrintSettings,
    pub print_panel: Option<print_panel::DocxPrintPanelState>,
    /// 資料文献の管理 (Source Manager: DOCX-REF-016)
    pub(crate) source_manager: Option<source_manager::SourceManagerState>,
    /// 類義語辞典 (Thesaurus: DOCX-REVIEW-019)
    pub(crate) thesaurus: Option<thesaurus::ThesaurusState>,
    /// イマーシブ リーダー (Immersive Reader: DOCX-ACCESS-010)
    pub(crate) immersive_reader: Option<immersive_reader::ImmersiveReaderState>,
    /// ミニ ツールバー (Mini Toolbar: DOCX-OPTIONS-001)
    pub(crate) mini_toolbar: Option<mini_toolbar::MiniToolbarState>,
    /// 画面のヒントのスタイル (ScreenTip Style: DOCX-OPTIONS-002)
    pub(crate) screentip: Option<screentip::ScreenTipState>,
    /// ドラッグ編集と選択方法 (DOCX-OPTIONS-005)
    pub(crate) drag_select: Option<drag_select::DragSelectOptionsState>,
    /// 破損文書の開いて修復 (DOCX-FILE-020)
    pub(crate) repair: Option<repair::RepairDocumentPanel>,
    /// テンプレートの添付とスタイルの更新 (DOCX-STYLE-017, DOCX-FILE-009)
    pub template_panel: Option<template::TemplateAttachPanel>,
    /// The fix担当's panels (ページ設定).
    pub helper: help_panels::HelpPanels,
    /// Selection Pane (オブジェクトの選択と表示).
    pub(crate) selection_pane: bool,
    pub(crate) selection_reorder: crate::shell::chrome::data_table::ListState,
    /// Accessibility Checker (アクセシビリティ チェック).
    pub(crate) accessibility_panel: bool,
    /// Word's Navigation Pane active tab (見出し / ページ / 結果).
    pub(crate) navigation_tab: NavigationTab,
    /// Word's Navigation Pane search query.
    pub(crate) nav_query: String,
    /// 読み取り専用の推奨の情報バーで「編集を有効にする」を押したか。
    pub(crate) read_only_dismissed: bool,
    /// 上書き入力モード (Word's Overtype mode).
    pub(crate) overtype: bool,
    /// 新しいリスト スタイルの定義 (DOCX-LIST-011)
    pub(crate) list_style: Option<list_style_panel::ListStyleState>,
    /// 文書テーマ・テーマ色・テーマフォントのカスタマイズ (DOCX-STYLE-012, DOCX-STYLE-013)
    pub theme_panel: Option<crate::shell::theme_panel::ThemePanel>,
    /// 折返し点の編集モード (DOCX-FLOAT-006)
    pub(crate) editing_wrap_points: bool,
    /// マスター文書（サブ文書管理）サイドバーパネル (DOCX-TOC-016)
    pub(crate) master_doc_panel: Option<editing::MasterDocPanelState>,
    /// 手描き注釈・インク描画・インク エディター (DOCX-IMAGE-019, DOCX-INK-001〜007, DOCX-INK-010)
    pub docx_ink: Option<ink::DocxInkState>,
    /// スクリーンリーダーの操作・ランドマーク移動 (DOCX-ACCESS-007)
    pub(crate) screen_reader: Option<accessibility::DocxScreenReaderState>,
    /// 読み上げ (DOCX-ACCESS-009)
    pub(crate) read_aloud: Option<accessibility::DocxReadAloudState>,
    /// リボンとクイックアクセスのカスタマイズ (DOCX-CLOUD-020)
    pub(crate) toolbar_customize: Option<accessibility::DocxToolbarCustomizeState>,
    /// ウィンドウの分割比率 (DOCX-VIEW-019). None = 分割なし, Some(ratio) = 上下分割 (例: 0.5).
    pub(crate) split: Option<f32>,
    /// 分割時の上部ペインの独立スクロールハンドル (DOCX-VIEW-019).
    pub(crate) split_scroll: ScrollHandle,
    /// 分割境界バーのドラッグ中フラグ (DOCX-VIEW-019).
    pub(crate) split_dragging: bool,
    /// 分割時に直近でアクティブになったペイン (DOCX-VIEW-019: true = 上部, false = 下部).
    pub(crate) split_active: bool,
    /// 閲覧モード (DOCX-VIEW-005).
    pub(crate) read_mode: bool,
    /// フォーカス表示モード (DOCX-VIEW-006).
    pub(crate) focus_mode: bool,
    /// ページの左右移動モード (DOCX-VIEW-008). false = 縦方向スクロール (既定), true = 左右ページ移動 (Side to Side).
    pub(crate) page_movement_side_to_side: bool,
}

/// A document's editor: its pages are laid out from the text and shown
/// stacked in one scrolling column under a ruler, like Word's print layout.
pub struct Document;

mod dialog_hooks;
mod event_hooks;
mod infobar_hooks;
impl_format_state!(State, ibis_pptx::Kind::Document);

#[ibis_office_macros::editor_ext(Docx)]
impl SlideEditor {
    /// A document's state, when a document is open.
    pub fn doc_state(&self) -> Option<&State> {
        self.format_state::<State>()
    }

    pub fn doc_state_mut(&mut self) -> Option<&mut State> {
        self.format_state_mut::<State>()
    }
}

/// The context menus of a document's modes and of what the caret is in (`FormatInfo::outer_contexts`).
fn outer_contexts(state: &crate::shell::menu_context::ContextState) -> Vec<crate::shell::menu_context::ContextMenu> {
    use crate::shell::menu_context::ContextMenu::*;
    let mut out = Vec::new();
    if state.header_footer {
        out.push(HeaderFooter);
    }
    if state.outline_view {
        out.push(Outline);
    }
    if state.caret_in_table {
        out.extend([TableDesign, TableLayout]);
    }
    if state.in_equation {
        out.push(Equation);
    }
    out
}

impl FormatEditor for Document {
    fn note_open_failed(path: &std::path::Path) {
        repair::note_open_failed(path);
    }
    fn note_opened(path: &std::path::Path) {
        repair::note_opened(path);
    }
    const INFO: &'static crate::shell::format::FormatInfo = &crate::shell::format::FormatInfo {
        order: 0,
        extension: "docx",
        new_file_name: || crate::shell::i18n::text("_shell.document-1-docx"),
        untitled_key: "_shell.document-1-docx",
        office: crate::shell::format::OfficeApp {
            name: "Word",
            setting_key: "office_path.word",
            #[cfg(target_os = "macos")]
            bundle: "Microsoft Word.app",
            #[cfg(target_os = "windows")]
            exe: "WINWORD.EXE",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            open_key: "window.open-in-word",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            logo_svg: include_bytes!("../logo/open_in_word.svg"),
        },
        new_command: "file.new.document",
        command_menus: {
            use crate::shell::chrome::MenuId as M;
            &[M::File, M::Edit, M::View, M::Insert, M::Format, M::Tools, M::Window, M::Help]
        },
        icon: "description",
        color: 0x2f6fd6,
        context_tones: [
            crate::shell::menu_context::ContextTone { fg: 0x1f56b8, bg: 0x2f6fd6, bg_alpha: 0.12 },
            crate::shell::menu_context::ContextTone { fg: 0x8fb4ff, bg: 0x8fb4ff, bg_alpha: 0.16 },
        ],
        menus: &crate::shell::menu_layout::DOCUMENT_MENUS,
        fold_order: {
            use crate::shell::chrome::MenuId;
            &[MenuId::Mailings, MenuId::References, MenuId::Design, MenuId::Layout, MenuId::Review]
        },
        sidebars: sidebars::SIDEBARS,
        mask: crate::shell::commands::KindMask::DOCX,
        several_windows: Some(crate::shell::format::SeveralWindows { compare_left_right: true, follow_shape_edits: false, number_in_title_bar: false }),
        shapes: crate::shell::format::ShapeRules { in_shell: false, several: false, try_first: true, cmyk: false, break_is_typed: true },
        outer_contexts,
        font_substitutes: false,
        drawing_canvas: true,
        new_badge_color: 0x2f6fd6,
        search_history_key: "menu-search.history.document",
        px_per_inch_at_100: crate::shell::editor::PX_PER_INCH_AT_100,
        #[cfg(not(target_family = "wasm"))]
        save_formats: &[("file.save-format.docx", "docx"), ("file.save-format.dotx", "dotx"), ("file.save-format.odt", "odt")],
    };

    fn box_edit_active(editor: &SlideEditor) -> bool {
        editor.doc_state().is_some_and(|s| s.box_edit.is_some())
    }

    fn box_text_edit(editor: &SlideEditor) -> Option<&crate::shell::text_edit::TextEditing> {
        editor.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| &b.edit)
    }

    fn box_text_edit_mut(editor: &mut SlideEditor) -> Option<&mut crate::shell::text_edit::TextEditing> {
        editor.doc_state_mut().and_then(|s| s.box_edit.as_mut()).map(|b| &mut b.edit)
    }

    fn box_edit_body(editor: &SlideEditor) -> Option<&ibis_doc_model::TextBody> {
        let at = editor.doc_state().and_then(|s| s.box_edit.as_ref())?.at;
        editor.shape_text_body_at(at)
    }

    fn box_edit_commit(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.commit_box_edit(window, cx);
    }

    fn box_edit_splice(editor: &mut SlideEditor, start: usize, remove: usize, insert: &str, typing: bool, cx: &mut Context<SlideEditor>) -> Option<bool> {
        editor.docx_box_edit_splice(start, remove, insert, typing, cx)
    }

    fn box_edit_changed(editor: &mut SlideEditor) {
        editor.sync_docx_edited_box_resource();
    }

    fn box_edit_positions(editor: &SlideEditor) -> Option<crate::render::rich_text::TextPositions> {
        editor.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| b.positions.clone())
    }

    fn box_edit_goal_x(editor: &SlideEditor) -> Option<(usize, f32)> {
        editor.doc_state().and_then(|s| s.box_edit.as_ref()).and_then(|b| b.goal_x)
    }

    fn box_edit_set_goal_x(editor: &mut SlideEditor, goal: (usize, f32)) {
        if let Some(b) = editor.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
            b.goal_x = Some(goal);
        }
    }

    fn new_file_shown(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.apply_docx_default_design(cx);
    }

    fn theme_panel(editor: &SlideEditor) -> Option<&crate::shell::theme_panel::ThemePanel> {
        editor.doc_state()?.theme_panel.as_ref()
    }

    fn theme_panel_open(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.open_docx_theme_panel(window, cx)
    }

    fn theme_panel_apply(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_docx_theme_panel(window, cx)
    }

    fn theme_palettes() -> Vec<(&'static str, &'static str, Vec<(&'static str, ibis_doc_model::Rgb)>)> {
        editing::DOCX_THEME_COLORS_PRESETS.iter().map(|p| (p.id, p.label_key, p.colors.to_vec())).collect()
    }

    fn theme_palette_apply(editor: &mut SlideEditor, id: &str, _window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_docx_theme_colors(id, cx)
    }

    fn open_print_panel(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.commit_edit(window, cx);
        editor.open_doc_print_panel(window, cx);
        true
    }

    fn prepare_print_or_export(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.prepare_print_or_export(cx);
    }

    fn export_args(editor: &SlideEditor) -> Vec<&'static str> {
        // 印刷オプション (DOCX-OPTIONS-010, DOCX-SEC-014): the exporting run prints as the window's settings say.
        let prefs = editor.doc_prefs();
        let mut args = Vec::new();
        if prefs.print_backgrounds {
            args.push("--print-backgrounds");
        }
        if prefs.print_hidden_text {
            args.push("--print-hidden-text");
        }
        if !prefs.print_markup {
            args.push("--no-print-markup");
        }
        args
    }

    /// A document as .odt (OpenDocument) and as a template (.dotx, .dotm).
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(editor: &SlideEditor, path: &std::path::Path, source: &ibis_pptx::Source, copy: bool) -> Option<Result<crate::shell::format::FileBytes, String>> {
        use crate::shell::format::FileBytes;
        let extension = path.extension()?;
        if extension.eq_ignore_ascii_case("odt") {
            // Saved, the file's password encrypts it; a copy is left as it is (as the shell did before the formats were cut apart).
            return Some(ibis_pptx::docx_convert::export_docx_to_odt(editor.presentation()).map(|bytes| FileBytes { bytes, protect: !copy, note: None }).map_err(|e| e.to_string()));
        }
        if extension.eq_ignore_ascii_case("dotx") || extension.eq_ignore_ascii_case("dotm") {
            return Some(source.save_template(editor.presentation(), ibis_pptx::SaveOptions::default()).map(|bytes| FileBytes { bytes, protect: true, note: None }).map_err(|e| e.to_string()));
        }
        None
    }

    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(editor: &SlideEditor, printing: bool) -> Result<crate::shell::format::PdfJob, String> {
        let bytes = editor.current_bytes()?;
        if !printing {
            return Ok(crate::shell::format::PdfJob { bytes, slides: None, format: "pdf", args: None });
        }
        let (slides, args) = editor.docx_print_job_args();
        Ok(match args {
            Some(args) => {
                let format = if args.iter().any(|a| a == "pdf-image") { "pdf-image" } else { "pdf" };
                let filtered: Vec<String> = args.into_iter().filter(|a| a != "--export" && a != "pdf-image").collect();
                crate::shell::format::PdfJob { bytes, slides, format, args: Some(filtered) }
            }
            None => crate::shell::format::PdfJob { bytes, slides, format: "pdf", args: None },
        })
    }

    fn ruler_panel(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        editor.ruler_panel(cx)
    }

    fn extra_dialogs(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Vec<crate::shell::format::ExtraDialog> {
        use crate::shell::dialog::DialogMode;
        use crate::shell::format::ExtraDialog;
        let mut dialogs = Vec::new();
        if let Some((panel, key)) = editor.merge_workflow_dialog(cx) {
            dialogs.push(ExtraDialog { panel, key, mode: DialogMode::Modal, footer_from_sidebar: false, resets_panel_mode: false });
        }
        if let Some(panel) = editor.tab_row_dialog(cx) {
            dialogs.push(ExtraDialog { panel, key: "tab_stop_editor", mode: DialogMode::Modal, footer_from_sidebar: false, resets_panel_mode: false });
        }
        dialogs
    }

    fn relayout_if_changed(editor: &mut SlideEditor) {
        editor.relayout_if_changed();
    }

    fn view_from_model(editor: &mut SlideEditor) {
        editor.sync_document_view_mode();
    }

    /// The other window's edit: the view mode follows and the pages are laid out again.
    fn model_from_peer(editor: &mut SlideEditor) {
        editor.sync_document_view_mode();
        editor.relayout_if_changed();
    }

    /// フォントの置換: the fonts of the document's text runs.
    fn used_fonts(editor: &SlideEditor) -> Vec<String> {
        editor.flow().map(|d| replace_fonts::document_used_fonts(d)).unwrap_or_default()
    }

    fn replace_fonts(editor: &mut SlideEditor, target: &str, replacement: &str, cx: &mut Context<SlideEditor>) -> usize {
        let Some((commands, count)) = editor.flow().map(|d| replace_fonts::document_replace_commands(d, target, replacement)) else { return 0 };
        if count > 0 {
            editor.run_flow(commands, cx);
        }
        count
    }

    /// The document's own interval of the autorecover snapshots (its save frequency).
    fn autorecover_minutes(editor: &SlideEditor) -> Option<u32> {
        editor.flow().and_then(|f| f.save_frequency)
    }

    fn set_autorecover_minutes(editor: &mut SlideEditor, minutes: u32, cx: &mut Context<SlideEditor>) {
        let freq = Some(minutes);
        // Setting the interval is not an edit: on an unedited document it does not start an undo history
        // (the document stays unmodified, so the timer has nothing to save).
        if editor.document.can_undo() {
            editor.run_flow(vec![ibis_doc_model::FlowCommand::SetSaveFrequency(freq)], cx);
        } else if let Some(flow) = editor.document.ids().flow.as_mut() {
            std::sync::Arc::make_mut(flow).save_frequency = freq;
        }
    }

    /// A window's view over a reload: the caret and the other end of the selection (`view_state.rs`).
    fn view_kept(editor: &SlideEditor) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(editor.keep_doc_view()))
    }

    fn view_put_back(editor: &mut SlideEditor, kept: &dyn std::any::Any, _: &mut Window, cx: &mut Context<SlideEditor>) {
        if let Some(view) = kept.downcast_ref::<view_state::KeptView>() {
            editor.put_doc_view_back(view, cx);
        }
    }

    fn set_font_size(editor: &mut SlideEditor, pt: f32, _: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.docx_set_font_size(pt, cx);
    }

    fn searchable_menu_entries(editor: &SlideEditor, id: &str) -> Option<Vec<crate::shell::chrome::MenuEntry>> {
        editor.doc_font_menu_entries(id)
    }

    fn crop_fields_synced(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.sync_picture_crop_fields(window, cx);
    }

    // ⌘B ⌘I ⌘U in a text box: the box's text is the document's runs.
    fn toggle_text_characters(editor: &mut SlideEditor, toggle: crate::shell::text_keys::CharacterToggle, cx: &mut Context<SlideEditor>) -> bool {
        editor.toggle_box_edit_characters(toggle, cx);
        true
    }

    // Escape, after the window's dialogs: a drag in progress is dropped.
    fn cancel_tool_late(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.cancel_pointer_drag(cx)
    }

    fn register_actions(root: Stateful<Div>, cx: &mut Context<SlideEditor>) -> Stateful<Div> {
        root
            .on_action(cx.listener(|this, _: &crate::ScrollLineUp, _, cx| this.scroll_lines(-1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollLineDown, _, cx| this.scroll_lines(1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToStart, _, cx| this.scroll_to_end(false, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToEnd, _, cx| this.scroll_to_end(true, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::split_axis::ToggleSplit, _, cx| this.toggle_document_split(cx)))
    }

    fn infobar_kind(editor: &SlideEditor) -> Option<crate::shell::infobar::InfoBarKind> {
        editor.docx_infobar_kind()
    }

    fn infobar_action(editor: &mut SlideEditor, kind: crate::shell::infobar::InfoBarKind, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.docx_infobar_action(kind, window, cx);
    }

    fn infobar_title_suffix(editor: &SlideEditor, kind: crate::shell::infobar::InfoBarKind) -> Option<String> {
        (kind == crate::shell::infobar::InfoBarKind::Compatibility).then(|| editor.docx_compatibility_mode_name().to_string())
    }

    fn close_dialog(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.docx_close_dialog(window, cx)
    }

    fn discard_dialog(editor: &mut SlideEditor, key: &'static str) {
        editor.discard_docx_dialog(key);
    }

    /// The symbols panel opens (`shell/symbols.rs`): the document's own panels give way to it.
    fn panel_opening(editor: &mut SlideEditor, panel: crate::shell::shape_state::ShapePanel) {
        if panel != crate::shell::shape_state::ShapePanel::Symbols {
            return;
        }
        let Some(state) = editor.doc_state_mut() else { return };
        state.find = None;
        state.link = None;
        state.style_edit = None;
        state.font_edit = None;
        state.color_edit = None;
        state.paragraph_edit = None;
        state.picture_edit = None;
        state.table_size = None;
        state.comments = None;
        state.ruby = None;
        state.text_box = None;
        state.merge = None;
    }

    fn autocorrect_dialog_parent_open(editor: &SlideEditor) -> bool {
        editor.doc_state().and_then(|s| s.autocorrect.as_ref()).is_some_and(|p| !p.entry_open)
    }

    fn autocorrect_panel_mut(editor: &mut SlideEditor) -> Option<&mut crate::shell::autocorrect::AutoCorrectPanel> {
        editor.doc_state_mut().and_then(|s| s.autocorrect.as_mut())
    }

    /// The chart's data grid is kept with the document's chart panel (`HelpPanels::chart_data`), and goes when it does.
    fn chart_data_window(editor: &SlideEditor) -> Option<&crate::shell::chart_data::DataWindow> {
        editor.doc_state().and_then(|s| s.helper.chart_data.as_ref())
    }

    fn chart_data_slot(editor: &mut SlideEditor) -> Option<&mut Option<crate::shell::chart_data::DataWindow>> {
        editor.doc_state_mut().map(|s| &mut s.helper.chart_data)
    }

    /// A document's chart is in its text: found by its id alone.
    fn read_chart_data(editor: &SlideEditor, source: &ibis_pptx::Source, _slide: ibis_doc_model::SlideId, shape: ibis_doc_model::ShapeId) -> Result<ibis_pptx::ChartData, ibis_pptx::Error> {
        source.doc_chart_data(editor.presentation(), shape)
    }

    fn apply_chart_data(editor: &mut SlideEditor, _slide: ibis_doc_model::SlideId, shape: ibis_doc_model::ShapeId, book: &Presentation, data: ibis_pptx::ChartData, cx: &mut Context<SlideEditor>) {
        editor.apply_doc_chart_data(shape, book, data, cx)
    }

    fn chart_panel_apply(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_doc_chart_panel(window, cx)
    }

    /// 常にバックアップ コピーを作成する (a document's, `shell/backup_copy.rs`).
    #[cfg(not(target_family = "wasm"))]
    fn backup_before_write(_editor: &SlideEditor, path: &std::path::Path) {
        if let Ok(Some(copy)) = crate::shell::backup_copy::make_backup(path) {
            log::info!("Created backup copy at: {}", copy.display());
        }
    }

    fn prepare_model(presentation: &mut Presentation, resources: &crate::render::shape_view::Resources, _: &TextSystem, file_name: Option<&str>, reloaded: bool) {
        if reloaded {
            flow_layout::update_fields_on_open(presentation, resources);
        }
        flow_layout::expand_pages(presentation, resources, file_name);
    }

    const PAGE_NOUN: &'static str = "docx.mod.page";
    const FIT_TOOLTIP: &'static str = "docx.mod.zoom-page-width";
    /// The pages are laid out from the text; no text box is put on them.
    const SHAPES: bool = false;

    fn new_prefs() -> Box<dyn std::any::Any> {
        Box::new(prefs::Prefs::default())
    }

    /// A document saved in the outline view opens with the navigation pane.
    fn document_loaded(editor: &mut SlideEditor) {
        if editor.presentation().flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Outline) {
            editor.outline_open = true;
        }
    }

    fn resources_renewed(editor: &mut SlideEditor) {
        // The window's own display of hidden text goes on with the next document (the ¶ button is the window's).
        let prefs = editor.doc_prefs();
        let show_hidden = prefs.formatting_marks_options.is_hidden_text_visible(prefs.formatting_marks_open);
        editor.resources.markup_view.show_hidden_text = show_hidden;
    }

    fn new_state(presentation: &Presentation, _: &mut Window, _: &mut Context<SlideEditor>) -> Box<dyn FormatState> {
        let characters = presentation.flow.as_deref().map_or(0, doc_ui::count_characters);
        // The caret starts at the beginning, as in Word.
        let caret = presentation.flow.as_deref().and_then(|d| {
            d.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
        });
        // A document saved in 閲覧モード opens in it.
        let read_mode = presentation.flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Reading);
        Box::new(State { characters, caret, laid_out: presentation.flow.clone(), read_mode, ..State::default() })
    }

    /// At 141% by default (#154).
    fn initial_zoom(_: &SlideEditor) -> Option<f32> {
        Some(1.41)
    }

    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        doc_ui::flow_content(workspace, &editor.page_sizes(), editor.zoom.map(zoom_scale), editor.page_gap(), editor.is_page_movement_side_to_side())
    }

    /// The page in view keeps its place as the zoom changes (and when the
    /// pages are fitted to the width).
    fn zoom_scroll_y(editor: &SlideEditor, before: &WorkspaceContent, after: &WorkspaceContent, old: Pixels, view: Pixels, anchor: Pixels) -> Option<Pixels> {
        Some(editor.flow_zoom_offset(before, after, old, view, anchor))
    }

    /// PageDown / PageUp scroll to the next or previous page.
    fn step_page(editor: &mut SlideEditor, forward: bool, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.step_page(forward, cx);
        true
    }

    /// フォーカス (DOCX-VIEW-006): only the pages are shown.
    fn chrome_hidden(editor: &SlideEditor) -> bool {
        editor.is_focus_mode()
    }

    fn link_under_pointer(editor: &SlideEditor, position: Point<Pixels>) -> bool {
        editor.doc_link_under_pointer(position).is_some()
    }

    fn link_pressed(editor: &mut SlideEditor, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.handle_document_link_pointer(event, window, cx)
    }

    /// トリミング works on the picture at the caret, on its page (`docx/picture_crop.rs`).
    fn crop_target(editor: &SlideEditor) -> Option<(crate::shell::crop_mode::CropTarget, ibis_doc_model::EmuRect, ibis_doc_model::Crop)> {
        editor.flow_crop_target()
    }

    fn crop_keep(editor: &mut SlideEditor, target: &crate::shell::crop_mode::CropTarget, full: ibis_doc_model::EmuRect, crop: ibis_doc_model::Crop, cx: &mut Context<SlideEditor>) {
        editor.keep_flow_crop(target, full, crop, cx);
    }

    fn crop_pointer(editor: &SlideEditor, target: &crate::shell::crop_mode::CropTarget, position: Point<Pixels>) -> Option<(f64, f64)> {
        editor.flow_crop_pointer(target, position)
    }

    /// Pasted shapes go into the text at the caret.
    fn paste_shapes_in_text(editor: &mut SlideEditor, shapes: Vec<ibis_doc_model::Shape>, cx: &mut Context<SlideEditor>) -> bool {
        editor.paste_shapes_at_caret(shapes, cx)
    }

    /// フォント… and 段落… of the right-click menu of the text: the panels the Home toolbar's font and line-spacing menus reach.
    fn body_popup_lines(_: &SlideEditor) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::MenuItem;
        vec![
            MenuItem::tr("docx.context.font").icon("text_format").on(|e, w, cx| e.open_font_edit(w, cx)).into(),
            MenuItem::tr("docx.context.paragraph").icon("format_line_spacing").on(|e, w, cx| e.open_paragraph_edit(None, w, cx)).into(),
        ]
    }

    /// The panes of the sidebar table a document opens without a command of its own.
    fn open_sidebar(editor: &mut SlideEditor, id: &'static str, window: &mut Window, cx: &mut Context<SlideEditor>) {
        match id {
            "sidebar.spelling" => editor.open_doc_spelling(window, cx),
            "sidebar.thesaurus" => editor.open_thesaurus_panel(window, cx),
            "sidebar.screen-reader" => editor.open_docx_screen_reader_panel(cx),
            "sidebar.read-aloud" => editor.open_docx_read_aloud(cx),
            "sidebar.ink" => editor.open_docx_ink_panel(cx),
            "sidebar.customize-toolbar" => editor.open_docx_customize_toolbar_panel(cx),
            _ => {}
        }
    }

    /// The context menus' state: the table and the equation the caret is in, the header or footer being edited, the outline
    /// view, and the picture, chart or shape selected.
    fn selection_context(editor: &SlideEditor, state: &mut crate::shell::menu_context::ContextState) {
        state.caret_in_table = editor.table_at_caret().is_some();
        state.in_equation = editor.docx_math_at_caret().is_some();
        state.header_footer = editor.is_header_footer_editing();
        state.outline_view = editor.document_view_mode() == ibis_doc_model::DocumentViewMode::Outline;
        state.objects = editor.selected_document_objects();
    }

    /// The pages are in one column: scroll to the page.
    fn show_page(editor: &mut SlideEditor, slide: SlideId, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        if let Some(index) = editor.presentation().slide_index(slide) {
            editor.current_slide = slide;
            editor.selection = None;
            editor.scroll_to_page(index, cx);
        }
        true
    }

    /// The title row's 「ヘッダーとフッターを閉じる」 (the shell's red pill, as the slide master's) while the header or footer is
    /// edited: a click puts the caret back in the body.
    fn title_mode_button(editor: &SlideEditor, weak: &WeakEntity<SlideEditor>, _icon_only: bool) -> Option<AnyElement> {
        if !editor.is_header_footer_editing() {
            return None;
        }
        let close: crate::shell::chrome::Handler = std::rc::Rc::new(|this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.close_header_footer(cx));
        Some(crate::shell::chrome::close_mode_button("docx-header-footer-close", "docx.header-footer.close", weak, &close))
    }

    /// The page in view is the current one (for the status bar).
    fn before_render(editor: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>) {
        editor.relayout_if_changed();
        let index = editor.visible_page();
        if let Some(slide) = editor.presentation().slides.get(index) {
            editor.current_slide = slide.id;
        }
    }

    /// Word's print layout: the ruler over the pages, and no page list.
    fn render_body(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        let is_immersive = editor.is_immersive_reader_enabled();
        let is_focus = editor.is_focus_mode() || is_immersive;
        let mut pages = div().flex().flex_col().flex_1().min_w_0().min_h_0();
        if is_immersive {
            pages = pages.child(editor.render_immersive_reader_toolbar(cx));
        }
        if let Some(bar) = editor.render_infobar(cx) {
            pages = pages.child(bar);
        }
        if let Some(bar) = editor.render_recovery_banner(cx) {
            pages = pages.child(bar);
        }
        if editor.is_repaired {
            pages = pages.child(editor.render_docx_repair_banner(cx));
        }
        let show_v_ruler = !is_focus && editor.doc_prefs().vertical_ruler_open;
        let theme = cx.theme().clone();
        if !is_focus && editor.doc_prefs().ruler_open {
            if show_v_ruler {
                let corner = SlideEditor::render_ruler_corner(theme.border, editor.ruler_unit)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle_ruler_unit(cx)));
                let h_ruler = editor.render_ruler(theme.border, theme.font_family.clone());
                let ruler_row = div()
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .w_full()
                    .child(corner)
                    .child(div().flex_1().min_w_0().child(h_ruler));
                pages = pages.child(ruler_row);
            } else {
                pages = pages.child(editor.render_ruler(theme.border, theme.font_family.clone()));
            }
        }
        let mut content_row = div().flex().flex_1().min_w_0().min_h_0().relative();
        if show_v_ruler {
            content_row = content_row.flex_row().child(editor.render_vertical_ruler(theme.border, theme.font_family.clone()));
        } else {
            content_row = content_row.flex_col();
        }

        let (imm_bg, imm_max_w, imm_overlay) = if is_immersive {
            (
                editor.immersive_reader_state().map(|s| s.page_color.bg_color()),
                editor.immersive_reader_state().and_then(|s| s.column_width.max_width_px()),
                editor.render_immersive_line_focus_overlay(cx),
            )
        } else {
            (None, None, None)
        };
        if let Some(bg) = imm_bg {
            pages = pages.bg(bg);
        }

        let doc_ws = editor.render_document_workspace(cx);
        if let Some(max_w) = imm_max_w {
            content_row = content_row.items_center().child(div().w_full().max_w(px(max_w)).flex_1().min_h_0().child(doc_ws));
        } else {
            content_row = content_row.child(doc_ws);
        }
        if let Some(overlay) = imm_overlay {
            content_row = content_row.child(overlay);
        }

        let pages = pages.child(content_row);
        // Word's Navigation Pane (ナビゲーション ウィンドウ):
        // left of the pages: Headings (見出し), Pages (ページ), Results (検索結果).
        let outline = (!is_focus && editor.outline_open).then(|| {
            let tab = editor.doc_state().map_or(NavigationTab::Headings, |s| s.navigation_tab);
            let nav_query = editor.doc_state().map(|s| s.nav_query.clone()).unwrap_or_default();

            let tab_btn = |id_str: &'static str, label: &'static str, active: bool, target: NavigationTab| {
                let mut b = div()
                    .id(id_str)
                    .px(px(8.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .rounded_md();
                if active {
                    b = b.bg(rgb(0xe0e0e0)).font_weight(FontWeight::SEMIBOLD);
                } else {
                    b = b.hover(|h| h.bg(rgb(0xf0f0f0)));
                }
                b.child(crate::shell::i18n::text(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_navigation_tab(target, cx);
                    }))
            };

            let tabs = div()
                .flex()
                .flex_row()
                .gap_1()
                .px(px(12.))
                .pb(px(8.))
                .child(tab_btn("nav-tab-headings", "docx.navigation.headings", tab == NavigationTab::Headings, NavigationTab::Headings))
                .child(tab_btn("nav-tab-pages", "docx.navigation.pages", tab == NavigationTab::Pages, NavigationTab::Pages))
                .child(tab_btn("nav-tab-results", "docx.navigation.results", tab == NavigationTab::Results, NavigationTab::Results));

            let content: AnyElement = match tab {
                NavigationTab::Headings => {
                    let items = editor.outline();
                    if items.is_empty() {
                        div().px(px(16.)).py(px(8.)).text_size(px(12.)).text_color(rgb(0x888888))
                            .child(crate::shell::i18n::text("docx.navigation.no-headings"))
                            .into_any_element()
                    } else {
                        let elements: Vec<AnyElement> = items
                            .into_iter()
                            .enumerate()
                            .map(|(i, (level, text, id))| {
                                // ▾ open / ▸ closed, for a heading with something under it.
                                let has_content = editor.heading_has_content(id);
                                let collapsed = editor.heading_collapsed(id);
                                let marker = div()
                                    .id(("outline-fold", i))
                                    .w(px(14.))
                                    .text_size(px(11.));
                                let marker = if has_content {
                                    marker.cursor_pointer().child(if collapsed { "▸" } else { "▾" }).on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let now = this.heading_collapsed(id);
                                        this.set_heading_collapsed(id, !now, cx);
                                    }))
                                } else {
                                    marker
                                };
                                div()
                                    .id(("outline", i))
                                    .flex()
                                    .flex_row()
                                    .pl(px(6.0 + 14.0 * (level.saturating_sub(1)) as f32))
                                    .pr(px(8.))
                                    .py(px(4.))
                                    .text_size(px(13.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0xe8eaed)))
                                    .child(marker)
                                    .child(SharedString::from(text.clone()))
                                    .on_click(cx.listener(move |this, _, _, cx| this.jump_to(id, cx)))
                                    // The heading dragged onto another goes before it, with what is under it.
                                    .on_drag(outline_ops::DraggedHeading { id, text: text.clone() }, |dragged, _, _, cx| cx.new(|_| outline_ops::DraggedHeadingView(dragged.text.clone())))
                                    .drag_over::<outline_ops::DraggedHeading>(|style, _, _, cx| style.border_t_2().border_color(cx.theme().primary))
                                    .on_drop(cx.listener(move |this, dragged: &outline_ops::DraggedHeading, _, cx| {
                                        if dragged.id != id {
                                            this.move_heading_to(dragged.id, id, cx);
                                        }
                                    }))
                                    .into_any_element()
                            })
                            .collect();
                        // Promote, demote and move the heading under the caret.
                        let op = |id: &'static str, glyph: &'static str, f: fn(&mut SlideEditor, &mut Context<SlideEditor>)| {
                            div().id(id).px(px(8.)).py(px(2.)).rounded_md().cursor_pointer().hover(|s| s.bg(rgb(0xe8eaed))).child(glyph)
                                .on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
                        };
                        let ops = div().flex().flex_row().gap_1().px(px(12.)).pb(px(4.))
                            .child(op("outline-promote", "←", |e, cx| { e.change_heading_level(-1, cx); }))
                            .child(op("outline-demote", "→", |e, cx| { e.change_heading_level(1, cx); }))
                            .child(op("outline-up", "↑", |e, cx| { e.move_heading(true, cx); }))
                            .child(op("outline-down", "↓", |e, cx| { e.move_heading(false, cx); }));
                        div().child(ops).children(elements).into_any_element()
                    }
                }
                NavigationTab::Pages => {
                    let total_pages = editor.page_sizes().len().max(1);
                    let page_cards: Vec<AnyElement> = (0..total_pages)
                        .map(|page_idx| {
                            let label = crate::shell::i18n::format("_docx.page-page", &[("page", &(page_idx + 1))]);
                            div()
                                .id(("nav-page", page_idx))
                                .mx(px(12.))
                                .my(px(4.))
                                .p(px(8.))
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(0xdddddd))
                                .bg(rgb(0xffffff))
                                .hover(|s| s.bg(rgb(0xf5f5f5)).border_color(rgb(0x1a73e8)))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(label)
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.scroll_to_page(page_idx, cx);
                                }))
                                .into_any_element()
                        })
                        .collect();
                    div().children(page_cards).into_any_element()
                }
                NavigationTab::Results => {
                    let flow = editor.flow();
                    let matches = flow.map(|f| find::find_all(&editor.resources.markup_view, f, &nav_query)).unwrap_or_default();
                    let count_text = if nav_query.is_empty() {
                        crate::shell::i18n::text("docx.navigation.search-prompt").to_string()
                    } else {
                        crate::shell::i18n::format("_docx.count-results", &[("count", &matches.len().to_string())])
                    };

                    let result_cards: Vec<AnyElement> = matches
                        .into_iter()
                        .enumerate()
                        .take(50)
                        .map(|(i, (from, to))| {
                            let snippet = flow.and_then(|f| {
                                for b in &f.body {
                                    if let ibis_doc_model::Block::Paragraph(p) = b {
                                        if p.id == from.para {
                                            let s: String = p.runs.iter().map(|r| match &r.kind {
                                                ibis_doc_model::FlowRunKind::Text(t) => t.as_str(),
                                                ibis_doc_model::FlowRunKind::RichText { text, .. }
                                                | ibis_doc_model::FlowRunKind::PlainText { text, .. }
                                                | ibis_doc_model::FlowRunKind::BuildingBlockGallery { text, .. }
                                                | ibis_doc_model::FlowRunKind::Group { text, .. } => text.as_str(),
                                                ibis_doc_model::FlowRunKind::RepeatingSection { items, selected, .. } => items.get(*selected).map(|s| s.as_str()).unwrap_or(""),
                                                ibis_doc_model::FlowRunKind::FormField(field) => match &field.kind {
                                                    ibis_doc_model::FormFieldKind::TextInput { value, .. } => value.as_str(),
                                                    ibis_doc_model::FormFieldKind::DropDown { items, selected, .. } => items.get(*selected).map(|s| s.as_str()).unwrap_or(""),
                                                    ibis_doc_model::FormFieldKind::CheckBox { checked, .. } => if *checked { "☒" } else { "☐" },
                                                },
                                                _ => "",
                                            }).collect();
                                            return Some(s);
                                        }
                                    }
                                }
                                None
                            }).unwrap_or_default();

                            let display_text = if snippet.len() > 60 {
                                format!("{}...", &snippet[..snippet.floor_char_boundary(60)])
                            } else {
                                snippet
                            };

                            div()
                                .id(("nav-match", i))
                                .mx(px(12.))
                                .my(px(3.))
                                .p(px(6.))
                                .rounded_sm()
                                .border_1()
                                .border_color(rgb(0xeeeeee))
                                .hover(|s| s.bg(rgb(0xe8f0fe)).border_color(rgb(0x1a73e8)))
                                .cursor_pointer()
                                .child(div().text_size(px(12.)).child(display_text))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to_range(from, to, cx);
                                }))
                                .into_any_element()
                        })
                        .collect();

                    div()
                        .child(
                            div()
                                .px(px(12.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .text_color(rgb(0x666666))
                                .child(count_text)
                        )
                        .children(result_cards)
                        .into_any_element()
                }
            };

            div()
                .id("docx-outline")
                .w(px(240.))
                .h_full()
                .flex_shrink_0()
                .overflow_y_scroll()
                .py(px(8.))
                .border_r_1()
                .border_color(rgb(0xe0e0e0))
                .bg(rgb(0xfafafa))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .px(px(12.))
                        .pb(px(6.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::BOLD)
                                .child(crate::shell::i18n::text("docx.navigation.title"))
                        )
                        .child(
                            div()
                                .id("docx-nav-close")
                                .text_size(px(12.))
                                .cursor_pointer()
                                .text_color(rgb(0x888888))
                                .hover(|s| s.text_color(rgb(0x000000)))
                                .child("✕")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.outline_open = false;
                                    cx.notify();
                                }))
                        )
                )
                .child(tabs)
                .child(content)
        });
        let exit_focus_btn = if is_focus {
            Some(
                div()
                    .id("docx-focus-mode-exit")
                    .absolute()
                    .top(px(16.))
                    .right(px(24.))
                    .px(px(14.))
                    .py(px(6.))
                    .rounded_md()
                    .bg(hsla(0.0, 0.0, 0.1, 0.75))
                    .hover(|h| h.bg(hsla(0.0, 0.0, 0.1, 0.9)))
                    .text_size(px(12.))
                    .text_color(crate::shell::palette::doc(0xffffff))
                    .cursor_pointer()
                    .child(crate::shell::i18n::text("docx.focus.exit"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_focus_mode(false, window, cx);
                    })),
            )
        } else {
            None
        };
        div().flex().flex_row().flex_1().min_h_0().relative().children(outline).child(pages).children(exit_focus_btn).into_any_element()
    }

    /// Typing and the input method go to the document while it has a caret.
    fn wants_text_input(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.doc_accepts_text_input(window, cx)
    }

    fn text_for_range(editor: &mut SlideEditor, range: Range<usize>, adjusted: &mut Option<Range<usize>>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<String> {
        editor.doc_text_for_range(range, adjusted, window, cx)
    }

    fn selected_text_range(editor: &mut SlideEditor, ignore_disabled_input: bool, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        editor.doc_selected_text_range(ignore_disabled_input, window, cx)
    }

    fn marked_text_range(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<Range<usize>> {
        editor.doc_marked_text_range(window, cx)
    }

    fn set_ime_clauses(editor: &mut SlideEditor, clauses: Vec<ImeClause>, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.doc_set_ime_clauses(clauses, window, cx)
    }

    fn unmark_text(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.doc_unmark_text(window, cx)
    }

    fn replace_text_in_range(editor: &mut SlideEditor, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
        let auto_started = if !text.is_empty() && editor.doc_state().is_some_and(|s| s.shape_selected && s.box_edit.is_none()) {
            editor.text_box_at_caret().filter(|i| i.has_text).is_some_and(|info| editor.start_box_edit_at(info.at, window, cx))
        } else {
            false
        };
        editor.doc_replace_text_in_range(if auto_started { None } else { range }, text, window, cx)
    }

    fn replace_and_mark_text_in_range(editor: &mut SlideEditor, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<SlideEditor>) {
        let auto_started = if !text.is_empty() && editor.doc_state().is_some_and(|s| s.shape_selected && s.box_edit.is_none()) {
            editor.text_box_at_caret().filter(|i| i.has_text).is_some_and(|info| editor.start_box_edit_at(info.at, window, cx))
        } else {
            false
        };
        editor.doc_replace_and_mark_text_in_range(if auto_started { None } else { range }, text, selected, window, cx)
    }

    fn bounds_for_range(editor: &mut SlideEditor, range: Range<usize>, element: Bounds<Pixels>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        editor.doc_bounds_for_range(range, element, window, cx)
    }

    /// A character picked in the symbols panel is typed at the caret (over the selection).
    fn insert_symbol(editor: &mut SlideEditor, c: char, _: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.type_text(&c.to_string(), cx);
    }

    fn text_vertical(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        editor.doc_text_vertical()
    }

    fn character_index_for_point(editor: &mut SlideEditor, point: Point<Pixels>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<usize> {
        editor.doc_character_index_for_point(point, window, cx)
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn ignored_scenarios() -> Vec<(&'static str, fn())> {
        let mut out = typeset68::ignored();
        out.push(("docx::fallback_carry_count_corpus", fallback_carry::count_corpus));
        #[cfg(target_os = "macos")]
        out.extend(corpus_report::ignored());
        out
    }

    /// The sidebar: 検索と置換's panel when it is open.
    fn sidebar(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        editor.font_panel(cx).map(|panel| panel.modal_named("font_panel")).or_else(|| editor.color_panel(cx).map(|panel| panel.modal_named("color_panel"))).or_else(|| editor.table_size_panel(cx).map(|panel| panel.modal_named("table_size_panel"))).or_else(|| editor.table_formula_panel(cx).map(|panel| panel.modal_named("table_formula_panel"))).or_else(|| editor.picture_panel(cx)).or_else(|| editor.docx_compress_panel(cx).map(|panel| panel.modal_named("docx_compress_panel"))).or_else(|| editor.paragraph_panel(cx).map(|panel| panel.modal_named("paragraph_panel"))).or_else(|| editor.tab_panel(cx)).or_else(|| editor.comments_panel(cx)).or_else(|| editor.style_panel(cx).map(|panel| panel.modal_named("style_panel"))).or_else(|| editor.link_panel(cx).map(|panel| panel.modal_named("link_panel"))).or_else(|| editor.find_panel(cx)).or_else(|| editor.tate_chu_yoko_panel(cx).map(|panel| panel.modal_named("tate_chu_yoko_panel"))).or_else(|| editor.ruby_panel(cx).map(|panel| panel.modal_named("ruby_panel")).or_else(||editor.fit_text_panel(cx).map(|p|p.modal_named("fit_text_panel")))).or_else(|| editor.text_box_panel(cx)).or_else(|| editor.merge_panel(cx)).or_else(|| editor.doc_page_setup_panel(cx).map(|panel| panel.modal_named("doc_page_setup_panel"))).or_else(|| editor.notes_panel(cx).map(|panel| panel.modal_named("notes_panel"))).or_else(|| editor.bookmark_panel(cx).map(|panel| panel.modal_named("bookmark_panel"))).or_else(|| editor.watermark_panel(cx).map(|panel| panel.modal_named("watermark_panel"))).or_else(|| editor.new_style_panel(cx).map(|panel| panel.modal_named("new_style_panel"))).or_else(|| editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel"))).or_else(|| editor.word_count_panel(cx)).or_else(|| editor.field_panel(cx).map(|panel| panel.modal_named("field_panel"))).or_else(|| editor.caption_panel(cx).map(|panel| panel.modal_named("caption_panel"))).or_else(|| editor.doc_properties_panel(cx).map(|panel| panel.modal_named("doc_properties_panel"))).or_else(|| editor.cross_reference_panel(cx).map(|panel| panel.modal_named("cross_reference_panel"))).or_else(|| editor.selection_pane_panel(cx)).or_else(|| editor.accessibility_panel(cx)).or_else(|| editor.update_toc_panel(cx).map(|panel| panel.modal_named("update_toc_panel"))).or_else(|| editor.user_info_panel(cx).map(|panel| panel.modal_named("user_info_panel"))).or_else(|| editor.kinsoku_panel(cx).map(|panel| panel.modal_named("kinsoku_panel"))).or_else(|| editor.go_to_panel(cx).map(|panel| panel.modeless_named("go_to_panel"))).or_else(|| editor.doc_spelling_panel(cx)).or_else(|| editor.doc_chart_panel(cx)).or_else(|| editor.control_panel(cx).map(|panel| panel.modal_named("control_panel"))).or_else(|| editor.doc_autocorrect_panel(cx).map(|panel| panel.modal_named("doc_autocorrect_panel"))).or_else(|| editor.autorecover_settings_panel(cx).map(|panel| panel.modal_named("autorecover_settings_panel"))).or_else(|| editor.recover_unsaved_panel(cx).map(|panel| panel.modal_named("recover_unsaved_panel"))).or_else(|| editor.docx_math_panel(cx).map(|panel| panel.modal_named("docx_math_panel"))).or_else(|| editor.quick_parts_panel(cx)).or_else(|| editor.doc_inspect_panel(cx).map(|panel| panel.modal_named("doc_inspect_panel"))).or_else(|| editor.doc_compatibility_panel(cx).map(|panel| panel.modal_named("doc_compatibility_panel"))).or_else(|| editor.doc_print_panel(cx).map(|panel| panel.modal_named("doc_print_panel"))).or_else(|| editor.doc_source_manager_panel(cx).map(|panel| panel.modal_named("doc_source_manager_panel"))).or_else(|| editor.doc_thesaurus_panel(cx)).or_else(|| editor.doc_repair_panel(cx).map(|panel| panel.modal_named("doc_repair_panel"))).or_else(|| editor.doc_template_panel(cx).map(|panel| panel.modal_named("doc_template_panel"))).or_else(|| editor.list_style_panel(cx).map(|panel| panel.modal_named("list_style_panel"))).or_else(|| editor.docx_theme_panel(cx).map(|panel| panel.modal_named("docx_theme_panel"))).or_else(|| editor.master_doc_panel(cx)).or_else(|| editor.docx_ink_panel(cx)).or_else(|| editor.docx_screen_reader_panel(cx)).or_else(|| editor.docx_read_aloud_panel(cx)).or_else(|| editor.docx_customize_toolbar_panel(cx))
    }

    /// The title bar's コメントを追加: a comment on the selection.
    fn add_comment(_: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        Some(std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.add_new_comment(window, cx)))
    }

    /// The title bar's navigation pane button (ナビゲーション ウィンドウ), filled while the pane is open.
    fn title_buttons(editor: &SlideEditor) -> Vec<crate::shell::format::TitleButton> {
        vec![crate::shell::format::TitleButton {
            id: "outline",
            icon: "dock_to_right",
            on: editor.outline_open,
            tooltip: "docx.navigation.title",
            handler: std::rc::Rc::new(|this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| {
                this.outline_open = !this.outline_open;
                cx.notify();
            }),
        }]
    }

    /// The title bar's コメントを表示: the comments' panel shown or hidden.
    fn ink_panel_open(editor: &SlideEditor) -> bool {
        editor.sidebar_open && editor.doc_state().is_some_and(|s| s.docx_ink.is_some())
    }

    fn close_ink_panel(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        if let Some(s) = editor.doc_state_mut() {
            s.docx_ink = None;
        }
        cx.notify();
    }

    fn show_comments(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let shown = editor.sidebar_open && editor.doc_state().is_some_and(|s| s.comments.is_some());
        Some((
            shown,
            std::rc::Rc::new(move |this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| {
                if shown {
                    if let Some(state) = this.doc_state_mut() {
                        state.comments = None;
                    }
                    cx.notify();
                } else {
                    this.open_comments(window, cx);
                }
            }),
        ))
    }

    fn menus(editor: &SlideEditor, menus: &mut Menus) {
        view_menu::connect(editor, menus);
        toolbar::menus(editor, menus);
    }

    fn format_paint(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let on = editor.doc_state()?.format_brush.is_some();
        Some((on, std::rc::Rc::new(|this, _, cx| this.toggle_doc_format_brush(cx))))
    }

    fn menu_items(editor: &SlideEditor, part: crate::shell::chrome::MenuPart) -> Vec<crate::shell::chrome::MenuEntry> {
        menu_items::menu_items(editor, part)
    }

    fn toolbar_left(_: &SlideEditor) -> Vec<ToolItem> {
        menu_items::toolbar_left()
    }

    fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
        toolbar::toolbar_right(editor)
    }

    fn status_items(editor: &SlideEditor) -> Vec<SharedString> {
        let characters = editor.doc_state().map_or(0, |s| s.characters);
        let mode = if editor.is_overtype() {
            crate::shell::i18n::text("docx.status.overtype")
        } else {
            crate::shell::i18n::text("docx.status.insert")
        };
        let mut items = vec![
            crate::shell::i18n::format("docx.mod.n-characters", &[("n", &crate::shell::ribbon::thousands(characters))]).into(),
            mode,
        ];
        match editor.interaction_mode() {
            ibis_doc_model::DocumentInteractionMode::Reviewing => {
                items.push(crate::shell::i18n::text("docx.status.interaction-reviewing"));
            }
            ibis_doc_model::DocumentInteractionMode::Viewing => {
                items.push(crate::shell::i18n::text("docx.status.interaction-viewing"));
            }
            ibis_doc_model::DocumentInteractionMode::Editing => {}
        }
        match editor.document_view_mode() {
            ibis_doc_model::DocumentViewMode::Reading => items.push(crate::shell::i18n::text("docx.status.read-mode")),
            ibis_doc_model::DocumentViewMode::Web => items.push(crate::shell::i18n::text("docx.status.web-layout")),
            ibis_doc_model::DocumentViewMode::Outline => items.push(crate::shell::i18n::text("docx.status.outline")),
            ibis_doc_model::DocumentViewMode::Draft => items.push(crate::shell::i18n::text("docx.status.draft")),
            ibis_doc_model::DocumentViewMode::Print => {}
        }
        items
    }
}

#[ibis_office_macros::editor_ext(Docx2)]
impl SlideEditor {
    /// Lay the pages out again now (after a change of how they are shown).
    pub(crate) fn relayout_now(&mut self, cx: &mut Context<SlideEditor>) {
        if let Some(state) = self.doc_state_mut() {
            state.laid_out = None;
        }
        self.relayout_if_changed();
        cx.notify();
    }

    /// After an edit (or undo), lay the pages out again and keep the caret
    /// in the document.
    pub(crate) fn relayout_if_changed(&mut self) {
        let Some(flow) = self.presentation().flow.clone() else { return };
        if self.doc_state().is_none_or(|s| s.laid_out.as_ref().is_some_and(|l| Arc::ptr_eq(l, &flow))) {
            return;
        }
        let resources = &self.resources;
        let started = crate::debug_timing::start();
        flow_layout::expand_pages(self.document.ids(), resources, None);
        crate::debug_timing::report("layout", started);
        if self.presentation().slide(self.current_slide).is_none() {
            self.current_slide = self.presentation().slides[0].id;
        }
        let characters = doc_ui::count_characters(&flow);
        let valid = |pos: Option<FlowPos>| {
            let pos = pos?;
            let p = ibis_doc_model::flow_command::paragraph(&flow, pos.para)?;
            Some(FlowPos::new(pos.para, pos.offset.min(p.len())))
        };
        let first = flow.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None });
        if let Some(state) = self.doc_state_mut() {
            state.laid_out = Some(flow.clone());
            state.characters = characters;
            state.caret = valid(state.caret).or(first);
            state.anchor = valid(state.anchor);
            state.selected_shapes.retain(|&p| valid(Some(p)).is_some());
        }
        self.normalize_visible_selection();
    }

    /// Word's Navigation Pane: switch active tab (Headings / Pages / Results).
    pub(crate) fn set_navigation_tab(&mut self, tab: NavigationTab, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.navigation_tab = tab;
        }
        cx.notify();
    }

    /// Word's Navigation Pane: get active tab.
    #[allow(dead_code)]
    pub(crate) fn navigation_tab(&self) -> NavigationTab {
        self.doc_state().map_or(NavigationTab::Headings, |s| s.navigation_tab)
    }

    /// Word's Navigation Pane: set search query.
    #[allow(dead_code)]
    pub(crate) fn set_nav_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.nav_query = query.to_string();
        }
        cx.notify();
    }

    /// Word's Navigation Pane: get search query.
    #[allow(dead_code)]
    pub(crate) fn nav_query(&self) -> String {
        self.doc_state().map(|s| s.nav_query.clone()).unwrap_or_default()
    }

    /// Jump to a range (selecting from..to) and scroll to it.
    pub(crate) fn jump_to_range(&mut self, from: FlowPos, to: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(from);
            state.caret = Some(to);
        }
        self.scroll_to_caret(cx);
        cx.notify();
    }

    /// 新規コメント追加（選択範囲がない場合は情報メッセージボックスを表示）
    pub(crate) fn add_new_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_selection = self.doc_state().map_or(false, |s| {
            if let (Some(a), Some(c)) = (s.anchor, s.caret) {
                a != c
            } else {
                false
            }
        });
        if !has_selection {
            // The common message (the same box as every other format's).
            self.show_panel_message(crate::shell::i18n::text("shell.comments.no-selection"), cx);
            return;
        }
        self.open_comments(window, cx);
    }
}

