// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Documents (Word): laying the text out into pages, the pages and the
//! ruler, and the document's editing — the document担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

pub(crate) mod doc_ui;
pub(crate) mod editing;
pub(crate) mod text_format;
pub(crate) mod block_tree;
pub(crate) mod clipboard;
pub(crate) mod kinsoku_panel;
pub(crate) mod spelling_panel;
pub(crate) mod chart_insert;
pub(crate) mod find;
pub(crate) mod wildcard;
pub(crate) mod comments;
pub(crate) mod link_panel;
pub(crate) mod index_fields;
pub(crate) mod toa_fields;
pub(crate) mod bibliography;
pub(crate) mod merge_panel;
pub(crate) mod outline_ops;
pub(crate) mod backup;
pub(crate) mod control_panel;
pub(crate) mod draw_table;
pub(crate) mod ruby_panel;
pub(crate) mod fit_text_panel;
pub(crate) mod text_box_panel;
pub(crate) mod text_box_effects;
pub(crate) mod symbol_panel;
pub(crate) mod style_panel;
pub(crate) mod paragraph_panel;
pub(crate) mod tab_panel;
pub(crate) mod color_panel;
pub(crate) mod font_panel;
pub(crate) mod paragraph_decorations;
pub(crate) mod picture_panel;
pub(crate) mod table_size_panel;
pub(crate) mod table_formula_panel;
pub(crate) mod help_panels;
pub(crate) mod list_formats;
pub(crate) mod list_style_panel;
mod style_new;
pub(crate) mod cell_borders;
pub(crate) mod cell_structure;
pub(crate) mod table_convert;
pub(crate) mod art_border;
pub(crate) mod flow_layout;
pub(crate) mod input;
pub(crate) mod page_view;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/docx/scenarios.rs"]
pub(crate) mod scenarios;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "../tests/docx/corpus_report.rs"]
pub(crate) mod corpus_report;
#[cfg(test)]
#[path = "../tests/docx/layout_tests.rs"]
mod layout_tests;
#[cfg(test)]
#[path = "../tests/docx/layout_tests_grid.rs"]
mod layout_tests_grid;
#[cfg(test)]
#[path = "../tests/docx/probe.rs"]
mod probe;
pub(crate) mod toolbar;
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
use crate::shell::format::{FormatEditor, FormatState};

pub(crate) use editing::key_bindings;

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
pub(crate) struct State {
    /// 入力オートコレクト is off (it is on by default).
    pub(crate) autocorrect_off: bool,
    /// 罫線を引いて表を作成: the mode is on, and the box being drawn.
    pub(crate) draw_table_mode: bool,
    /// 校閲 > 比較の設定.
    pub(crate) compare_options: ibis_doc_model::compare::CompareOptions,
    pub(crate) draw_box: Option<draw_table::DrawBox>,
    /// 挿入 > 図形: the shape the next drag draws.
    pub(crate) draw_shape: Option<draw_table::DrawKind>,
    /// オートコレクトのオプション's panel, while it is the one shown.
    pub(crate) autocorrect: Option<crate::shell::autocorrect::AutoCorrectPanel>,
    /// The characters of the text, for the status bar.
    pub(crate) characters: usize,
    /// A page to scroll to once the workspace is laid out.
    pub(crate) pending_page: Option<usize>,
    /// The caret, and the other end of the selection when there is one.
    pub(crate) caret: Option<FlowPos>,
    pub(crate) anchor: Option<FlowPos>,
    /// The other ranges of a selection made of several (⌘-click or ⌘-drag adds one;
    /// the caret and anchor are the last), each in document order (start, end).
    pub(crate) extra_selections: Vec<(FlowPos, FlowPos)>,
    /// Where ↑ / ↓ aim sideways (points), kept over a run of them.
    pub(crate) goal_x: Option<f32>,
    /// Formatting chosen with no selection, for the text typed next.
    pub(crate) pending: Option<InsertFormat>,
    /// Supported resolved character/paragraph properties captured by the brush.
    pub(crate) format_brush: Option<FormatBrush>,
    /// Text the input method is composing, shown at the caret.
    pub(crate) marked: Option<String>,
    /// The document the pages were last laid out from.
    pub(crate) laid_out: Option<Arc<FlowDocument>>,
    /// A drag selecting text is in progress.
    pub(crate) dragging: bool,
    /// 検索と置換's panel, once opened (and while it is the one shown).
    pub(crate) find: Option<find::FindState>,
    /// The link panel, while it is the one shown.
    pub(crate) link: Option<link_panel::LinkState>,
    /// スタイルの変更's panel, while it is the one shown.
    pub(crate) style_edit: Option<style_panel::StyleEditState>,
    pub(crate) cell_border_edge: Option<usize>,
    pub(crate) paragraph_border_edge: Option<usize>,
    pub(crate) font_edit: Option<font_panel::FontEditState>,
    pub(crate) color_edit: Option<color_panel::ColorEditState>,
    pub(crate) paragraph_edit: Option<paragraph_panel::ParagraphEditState>,
    pub(crate) tab_edit: Option<tab_panel::TabEditState>,
    pub(crate) picture_edit: Option<picture_panel::PictureEditState>,
    pub(crate) compress_panel: Option<compress_panel::DocxCompressState>,
    pub(crate) table_size: Option<table_size_panel::TableSizeState>,
    pub(crate) table_formula: Option<table_formula_panel::TableFormulaState>,
    /// The comments' panel, while it is the one shown.
    pub(crate) comments: Option<comments::CommentState>,
    /// ルビ's panel, once opened.
    pub(crate) ruby: Option<ruby_panel::RubyState>,
    pub(crate) fit_text_edit: Option<fit_text_panel::FitTextState>,
    /// コンテンツ コントロールのプロパティ's panel, while it is the one shown.
    pub(crate) control: Option<control_panel::ControlState>,
    /// テキスト ボックスの文字's panel, while it is the one shown.
    pub(crate) text_box: Option<text_box_panel::TextBoxState>,
    /// 差し込み印刷's panel and its data (宛先一覧).
    pub(crate) merge: Option<merge_panel::MergeState>,
    /// A floating text box whose text is being edited in place.
    pub(crate) box_edit: Option<text_box_panel::BoxEdit>,
    /// A floating text box being dragged.
    pub(crate) shape_drag: Option<text_box_panel::ShapeDrag>,
    /// Whether the floating shape at the caret is currently selected (showing handles/outline).
    pub(crate) shape_selected: bool,
    /// Additional shapes selected via Shift+Click (alongside the primary shape at `caret`).
    pub(crate) selected_shapes: Vec<FlowPos>,
    /// 記号と特殊文字's panel, while it is the one shown.
    pub(crate) symbol: Option<symbol_panel::SymbolState>,
    /// 数式 (Math) panel, while it is the one shown.
    pub(crate) docx_math: Option<math_panel::DocxMathState>,
    /// クイックパーツ (Quick Parts) panel, while it is the one shown.
    pub(crate) quick_parts: Option<quick_parts::QuickPartsState>,
    /// ドキュメントの検査 (Document Inspector) panel, while it is the one shown.
    pub(crate) inspect_panel: Option<inspect_panel::DocInspectState>,
    /// 互換性チェック (Compatibility Checker) panel, while it is the one shown.
    pub(crate) compatibility_panel: Option<compatibility_panel::DocCompatibilityState>,
    /// 印刷の設定 (Print Settings: DOCX-SEC-013, 015, 016)
    pub(crate) print_settings: print_panel::DocxPrintSettings,
    pub(crate) print_panel: Option<print_panel::DocxPrintPanelState>,
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
    pub(crate) template_panel: Option<template::TemplateAttachPanel>,
    /// The fix担当's panels (ページ設定).
    pub(crate) helper: help_panels::HelpPanels,
    /// Selection Pane (オブジェクトの選択と表示).
    pub(crate) selection_pane: bool,
    pub(crate) selection_reorder: std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
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
    /// コメント追加時にテキスト選択がない場合の情報ダイアログ表示フラグ。
    pub(crate) comment_alert_no_selection: bool,
    /// コメント削除時の確認ダイアログ（削除対象の comment ID）。
    pub(crate) comment_delete_confirm: Option<u32>,
    /// 新しいリスト スタイルの定義 (DOCX-LIST-011)
    pub(crate) list_style: Option<list_style_panel::ListStyleState>,
    /// 文書テーマ・テーマ色・テーマフォントのカスタマイズ (DOCX-STYLE-012, DOCX-STYLE-013)
    pub(crate) theme_panel: Option<editing::DocxThemePanel>,
    /// 折返し点の編集モード (DOCX-FLOAT-006)
    pub(crate) editing_wrap_points: bool,
    /// マスター文書（サブ文書管理）サイドバーパネル (DOCX-TOC-016)
    pub(crate) master_doc_panel: Option<editing::MasterDocPanelState>,
    /// 手描き注釈・インク描画・インク エディター (DOCX-IMAGE-019, DOCX-INK-001〜007, DOCX-INK-010)
    pub(crate) docx_ink: Option<ink::DocxInkState>,
    /// スクリーンリーダーの操作・ランドマーク移動 (DOCX-ACCESS-007)
    pub(crate) screen_reader: Option<accessibility::DocxScreenReaderState>,
    /// 読み上げ (DOCX-ACCESS-009)
    pub(crate) read_aloud: Option<accessibility::DocxReadAloudState>,
    /// リボンとクイックアクセスのカスタマイズ (DOCX-CLOUD-020)
    pub(crate) toolbar_customize: Option<accessibility::DocxToolbarCustomizeState>,
}

/// A document's editor: its pages are laid out from the text and shown
/// stacked in one scrolling column under a ruler, like Word's print layout.
pub(crate) struct Document;

impl FormatEditor for Document {
    const PAGE_NOUN: &'static str = "docx.mod.page";
    const FIT_TOOLTIP: &'static str = "docx.mod.zoom-page-width";
    /// The pages are laid out from the text; no text box is put on them.
    const SHAPES: bool = false;

    fn new_state(presentation: &Presentation, _: &mut Window, _: &mut Context<SlideEditor>) -> FormatState {
        let characters = presentation.flow.as_deref().map_or(0, doc_ui::count_characters);
        // The caret starts at the beginning, as in Word.
        let caret = presentation.flow.as_deref().and_then(|d| {
            d.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
        });
        FormatState::Document(State { characters, caret, laid_out: presentation.flow.clone(), ..State::default() })
    }

    /// At 141% by default (#154).
    fn initial_zoom(_: &SlideEditor) -> Option<f32> {
        Some(1.41)
    }

    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        doc_ui::flow_content(workspace, &editor.page_sizes(), editor.zoom.map(zoom_scale), editor.page_gap(), editor.page_movement_side_to_side)
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

    /// The pages are in one column: scroll to the page.
    fn show_page(editor: &mut SlideEditor, slide: SlideId, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        if let Some(index) = editor.presentation().slide_index(slide) {
            editor.current_slide = slide;
            editor.selection = None;
            editor.scroll_to_page(index, cx);
        }
        true
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
        if let Some(bar) = editor.render_docx_infobar(cx) {
            pages = pages.child(bar);
        }
        if let Some(bar) = editor.render_recovery_banner(cx) {
            pages = pages.child(bar);
        }
        if editor.is_repaired {
            pages = pages.child(editor.render_docx_repair_banner(cx));
        }
        let show_v_ruler = !is_focus && editor.vertical_ruler_open;
        let theme = cx.theme().clone();
        if !is_focus && editor.ruler_open {
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
                    let matches = flow.map(|f| find::find_all(f, &nav_query)).unwrap_or_default();
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
        div().flex().flex_row().flex_1().min_h_0().relative().children(outline).child(pages).children(exit_focus_btn).children(editor.render_docx_comment_dialogs(cx)).into_any_element()
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

    fn character_index_for_point(editor: &mut SlideEditor, point: Point<Pixels>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<usize> {
        editor.doc_character_index_for_point(point, window, cx)
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }

    /// The sidebar: 検索と置換's panel when it is open.
    fn sidebar(editor: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        editor.font_panel(cx).map(|panel| panel.modal_named("font_panel")).or_else(|| editor.color_panel(cx).map(|panel| panel.modal_named("color_panel"))).or_else(|| editor.table_size_panel(cx).map(|panel| panel.modal_named("table_size_panel"))).or_else(|| editor.table_formula_panel(cx).map(|panel| panel.modal_named("table_formula_panel"))).or_else(|| editor.picture_panel(cx)).or_else(|| editor.docx_compress_panel(cx).map(|panel| panel.modal_named("docx_compress_panel"))).or_else(|| editor.paragraph_panel(cx).map(|panel| panel.modal_named("paragraph_panel"))).or_else(|| editor.tab_panel(cx)).or_else(|| editor.comments_panel(cx)).or_else(|| editor.style_panel(cx).map(|panel| panel.modal_named("style_panel"))).or_else(|| editor.link_panel(cx).map(|panel| panel.modal_named("link_panel"))).or_else(|| editor.find_panel(cx)).or_else(|| editor.ruby_panel(cx).map(|panel| panel.modal_named("ruby_panel")).or_else(||editor.fit_text_panel(cx).map(|p|p.modal_named("fit_text_panel")))).or_else(|| editor.text_box_panel(cx)).or_else(|| editor.merge_panel(cx)).or_else(|| editor.doc_page_setup_panel(cx).map(|panel| panel.modal_named("doc_page_setup_panel"))).or_else(|| editor.notes_panel(cx).map(|panel| panel.modal_named("notes_panel"))).or_else(|| editor.bookmark_panel(cx).map(|panel| panel.modal_named("bookmark_panel"))).or_else(|| editor.watermark_panel(cx).map(|panel| panel.modal_named("watermark_panel"))).or_else(|| editor.new_style_panel(cx).map(|panel| panel.modal_named("new_style_panel"))).or_else(|| editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel"))).or_else(|| editor.word_count_panel(cx)).or_else(|| editor.field_panel(cx).map(|panel| panel.modal_named("field_panel"))).or_else(|| editor.caption_panel(cx).map(|panel| panel.modal_named("caption_panel"))).or_else(|| editor.doc_properties_panel(cx).map(|panel| panel.modal_named("doc_properties_panel"))).or_else(|| editor.cross_reference_panel(cx).map(|panel| panel.modal_named("cross_reference_panel"))).or_else(|| editor.selection_pane_panel(cx)).or_else(|| editor.accessibility_panel(cx)).or_else(|| editor.update_toc_panel(cx).map(|panel| panel.modal_named("update_toc_panel"))).or_else(|| editor.user_info_panel(cx).map(|panel| panel.modal_named("user_info_panel"))).or_else(|| editor.kinsoku_panel(cx).map(|panel| panel.modal_named("kinsoku_panel"))).or_else(|| editor.go_to_panel(cx).map(|panel| panel.modeless_named("go_to_panel"))).or_else(|| editor.doc_spelling_panel(cx)).or_else(|| editor.doc_chart_panel(cx)).or_else(|| editor.control_panel(cx).map(|panel| panel.modal_named("control_panel"))).or_else(|| editor.doc_autocorrect_panel(cx).map(|panel| panel.modal_named("doc_autocorrect_panel"))).or_else(|| editor.autorecover_settings_panel(cx).map(|panel| panel.modal_named("autorecover_settings_panel"))).or_else(|| editor.recover_unsaved_panel(cx).map(|panel| panel.modal_named("recover_unsaved_panel"))).or_else(|| editor.docx_math_panel(cx).map(|panel| panel.modal_named("docx_math_panel"))).or_else(|| editor.quick_parts_panel(cx)).or_else(|| editor.doc_inspect_panel(cx).map(|panel| panel.modal_named("doc_inspect_panel"))).or_else(|| editor.doc_compatibility_panel(cx).map(|panel| panel.modal_named("doc_compatibility_panel"))).or_else(|| editor.doc_print_panel(cx).map(|panel| panel.modal_named("doc_print_panel"))).or_else(|| editor.doc_source_manager_panel(cx).map(|panel| panel.modal_named("doc_source_manager_panel"))).or_else(|| editor.doc_thesaurus_panel(cx)).or_else(|| editor.doc_repair_panel(cx).map(|panel| panel.modal_named("doc_repair_panel"))).or_else(|| editor.doc_template_panel(cx).map(|panel| panel.modal_named("doc_template_panel"))).or_else(|| editor.list_style_panel(cx).map(|panel| panel.modal_named("list_style_panel"))).or_else(|| editor.docx_theme_panel(cx).map(|panel| panel.modal_named("docx_theme_panel"))).or_else(|| editor.master_doc_panel(cx)).or_else(|| editor.docx_ink_panel(cx)).or_else(|| editor.docx_screen_reader_panel(cx)).or_else(|| editor.docx_read_aloud_panel(cx)).or_else(|| editor.docx_customize_toolbar_panel(cx))
    }

    /// The title bar's コメントを追加: a comment on the selection.
    fn add_comment(_: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        Some(std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.add_new_comment(window, cx)))
    }

    /// The title bar's コメントを表示: the comments' panel shown or hidden.
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
        toolbar::menus(editor, menus);
    }

    fn format_paint(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let on = editor.doc_state()?.format_brush.is_some();
        Some((on, std::rc::Rc::new(|this, _, cx| this.toggle_doc_format_brush(cx))))
    }

    fn toolbar_right(editor: &SlideEditor) -> Option<Vec<ToolItem>> {
        Some(toolbar::toolbar_right(editor))
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
            if let Some(state) = self.doc_state_mut() {
                state.comment_alert_no_selection = true;
            }
            cx.notify();
            return;
        }
        self.open_comments(window, cx);
    }

    /// コメント関連のダイアログ（選択範囲なし、削除確認）の描画
    pub(crate) fn render_docx_comment_dialogs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.doc_state()?;

        if state.comment_alert_no_selection {
            return Some(
                div()
                    .id("docx-comment-alert-modal")
                    .absolute()
                    .inset_0()
                    .bg(hsla(0.0, 0.0, 0.0, 0.35))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("docx-comment-alert-box")
                            .w(px(360.))
                            .p(px(20.))
                            .rounded_lg()
                            .bg(rgb(0xffffff))
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(rgb(0x222222))
                                    .child(crate::shell::i18n::text("docx.comments.alert-no-selection.message"))
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .child(
                                        div()
                                            .id("docx-comment-alert-ok")
                                            .px(px(16.))
                                            .py(px(6.))
                                            .rounded_md()
                                            .bg(rgb(0x1a73e8))
                                            .text_color(crate::shell::palette::doc(0xffffff))
                                            .text_size(px(13.))
                                            .cursor_pointer()
                                            .child(crate::shell::i18n::text("docx.comments.alert-no-selection.ok"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    s.comment_alert_no_selection = false;
                                                }
                                                cx.notify();
                                            }))
                                    )
                            )
                    )
                    .into_any_element(),
            );
        }

        if let Some(comment_id) = state.comment_delete_confirm {
            return Some(
                div()
                    .id("docx-comment-delete-modal")
                    .absolute()
                    .inset_0()
                    .bg(hsla(0.0, 0.0, 0.0, 0.35))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("docx-comment-delete-box")
                            .w(px(380.))
                            .p(px(20.))
                            .rounded_lg()
                            .bg(rgb(0xffffff))
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(15.))
                                    .text_color(rgb(0x222222))
                                    .child(crate::shell::i18n::text("docx.comments.delete-confirm.title"))
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(0x444444))
                                    .child(crate::shell::i18n::text("docx.comments.delete-confirm.message"))
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("docx-comment-delete-cancel")
                                            .px(px(14.))
                                            .py(px(6.))
                                            .rounded_md()
                                            .bg(rgb(0xf0f0f0))
                                            .text_color(rgb(0x333333))
                                            .text_size(px(13.))
                                            .cursor_pointer()
                                            .hover(|h| h.bg(rgb(0xe0e0e0)))
                                            .child(crate::shell::i18n::text("docx.comments.delete-confirm.cancel"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    s.comment_delete_confirm = None;
                                                }
                                                cx.notify();
                                            }))
                                    )
                                    .child(
                                        div()
                                            .id("docx-comment-delete-ok")
                                            .px(px(14.))
                                            .py(px(6.))
                                            .rounded_md()
                                            .bg(rgb(0xd93025))
                                            .text_color(crate::shell::palette::doc(0xffffff))
                                            .text_size(px(13.))
                                            .cursor_pointer()
                                            .hover(|h| h.bg(rgb(0xb3261e)))
                                            .child(crate::shell::i18n::text("docx.comments.delete-confirm.delete"))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    s.comment_delete_confirm = None;
                                                }
                                                this.delete_comment(comment_id, cx);
                                            }))
                                    )
                            )
                    )
                    .into_any_element(),
            );
        }

        None
    }
}

