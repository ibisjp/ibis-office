// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Presentations (PowerPoint): the slide list, the slide show, and the
//! presentation's editing — the presentation担当's directory.
//!
//! See `shell/format.rs` for how the shell hands over to a format.

pub(crate) mod sidebar_settings;
pub(crate) mod chrome;
pub(crate) mod editing;
pub(crate) mod alt_text_panel;
pub(crate) mod find;
pub(crate) mod insets_panel;
pub(crate) mod animation_ui;
pub(crate) mod chart_panel;
pub(crate) mod chart_data;
pub(crate) mod selection_pane;
pub(crate) mod reading_order_pane;
pub(crate) mod links;
pub(crate) mod clips;
pub(crate) mod clip_bar;
pub(crate) mod show_window;
pub(crate) mod symbols;
pub(crate) mod theme_panel;
pub(crate) mod comments;
pub(crate) mod crop_panel;
pub(crate) mod frame_panel;
pub(crate) mod cell_size_panel;
pub(crate) mod header_footer;
pub(crate) mod timing_panel;
pub(crate) mod show_panel;
pub(crate) mod glow_panel;
pub(crate) mod shadow_panel;
pub(crate) mod gradient_panel;
pub(crate) mod paragraph_panel;
pub(crate) mod interaction;
pub(crate) mod notes;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/pptx/scenarios.rs"]
pub(crate) mod scenarios;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "../tests/pptx/corpus_report.rs"]
pub(crate) mod corpus_report;
pub(crate) mod slide_view;
pub(crate) mod size_panel;
pub(crate) mod section_panel;
pub(crate) mod layout_panel;
pub(crate) mod print_panel;
pub(crate) mod master_view;
pub(crate) mod notes_format;
pub(crate) mod spelling_panel;
mod thesaurus_panel;
pub(crate) mod morph;

#[cfg(not(feature = "export"))]
impl crate::shell::editor::SlideEditor {
    /// 図の圧縮 needs the picture codecs of the `export` feature.
    pub(crate) fn compress_panel(&mut self, _cx: &mut gpui_kit::Context<Self>) -> Option<SidebarPanel> {
        None
    }
}
#[cfg(feature = "export")]
pub(crate) mod compress;
pub(crate) mod edit_points;
pub(crate) mod outline_edit;
pub(crate) mod ink;
pub(crate) mod autocorrect_panel;
pub(crate) mod math_panel;
pub(crate) mod modify_password;
pub(crate) mod path_draw;
pub(crate) mod shape_menu;
pub(crate) mod smartart;
pub use smartart::{SmartArtColorStyle, SmartArtKind};
pub(crate) mod notes_marks_panel;
pub(crate) mod shape_merge;
pub(crate) mod properties_panel;
pub(crate) mod inspect_panel;
pub(crate) mod accessibility_panel;
pub(crate) mod compatibility_panel;
pub(crate) mod slideshow;
pub(crate) mod text_edit;
pub(crate) mod replace_fonts;
pub(crate) mod grid_view;
pub(crate) mod thumbnails;
pub(crate) mod slide_zoom;
pub(crate) mod photo_album;
pub(crate) mod compare;
pub(crate) mod audio_record;
pub(crate) mod media_compress;
pub(crate) use media_compress::MediaQuality;
pub(crate) mod threed_and_nav;

use gpui_kit::*;
use ibis_doc_model::Presentation as Model;

use crate::shell::editor::{SlideEditor, WorkspaceContent};
use crate::shell::chrome::{Menus, SidebarPanel, ToolItem};
use crate::shell::format::{FormatEditor, FormatState};

/// What only an open presentation has: the selection (several shapes),
/// the tool, the drag in progress and the shapes copied.
pub(crate) struct State {
    pub(crate) selection: Vec<ibis_doc_model::ShapeId>,
    pub(crate) tool: editing::Tool,
    pub(crate) drag: Option<slide_view::Drag>,
    /// Shift is held (during a drag).
    pub(crate) shift: bool,
    pub(crate) clipboard: Vec<ibis_doc_model::Shape>,
    pub(crate) clipboard_groups: Vec<ibis_doc_model::ShapeGroup>,
    pub(crate) find: Option<find::FindState>,
    pub(crate) size_panel: Option<size_panel::SizePanel>,
    pub(crate) section_panel: Option<section_panel::SectionPanel>,
    pub(crate) layout_panel: Option<layout_panel::LayoutNamePanel>,
    /// 印刷の設定 panel, while open, and the settings the file prints with.
    pub(crate) print_panel: Option<print_panel::PrintPanel>,
    /// スペルチェック panel, while open.
    pub(crate) spelling: Option<spelling_panel::SpellPanel>,
    pub(crate) thesaurus: Option<thesaurus_panel::PptxThesaurusPanel>,
    /// 図の圧縮 panel, while open.
    #[cfg(feature = "export")]
    pub(crate) compress_panel: Option<compress::CompressPanel>,
    /// 頂点の編集: the shape whose vertices are shown.
    pub(crate) edit_points: Option<edit_points::EditPoints>,
    /// The edited text last checked for spelling, and the words found wrong (as ranges of characters).
    pub(crate) spell_cache: std::cell::RefCell<Option<(String, Vec<std::ops::Range<usize>>)>>,
    /// カスタム軌跡: the path being drawn.
    pub(crate) path_draw: Option<path_draw::PathDraw>,
    /// ノートのヘッダーとフッター panel, while open.
    pub(crate) notes_marks_panel: Option<notes_marks_panel::NotesMarksPanel>,
    pub(crate) print_settings: print_panel::PrintSettings,
    pub(crate) folded_sections: std::collections::HashSet<String>,
    pub(crate) section_selection: Option<(String,ibis_doc_model::SlideId)>,
    pub(crate) paragraph_panel: Option<paragraph_panel::ParagraphPanel>,
    pub(crate) timing_panel: Option<timing_panel::TimingPanel>,
    pub(crate) show_panel: Option<show_panel::ShowPanel>,
    pub(crate) glow_panel: Option<glow_panel::GlowPanel>,
    pub(crate) shadow_panel: Option<shadow_panel::ShadowPanel>,
    pub(crate) gradient_panel: Option<gradient_panel::GradientPanel>,
    pub(crate) crop_panel: Option<crop_panel::CropPanel>,
    pub(crate) frame_panel: Option<frame_panel::FramePanel>,
    pub(crate) cell_size_panel: Option<cell_size_panel::CellSizePanel>,
    /// The comments' panel (コメント).
    pub(crate) comments: Option<comments::CommentsPanel>,
    /// グラフのデータ.
    pub(crate) chart_panel: Option<chart_panel::ChartPanel>,
    /// データの編集's window, while open.
    pub(crate) chart_data: Option<chart_data::DataWindow>,
    /// ヘッダーとフッター.
    pub(crate) header_footer: Option<header_footer::HeaderFooterPanel>,
    /// アニメーション ウィンドウ shown.
    pub(crate) animation_pane: bool,
    /// SmartArt テキスト ウィンドウ shown.
    pub(crate) sidebar_settings: Option<sidebar_settings::SidebarSettings>,
    pub(crate) smartart_text_pane: bool,
    pub(crate) smartart_reorder: std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
    /// アニメーション効果のオプション詳細設定パネル (Effect Options Panel), while open.
    pub(crate) animation_options_panel: Option<animation_ui::AnimationOptionsPanel>,
    /// 選択ウィンドウ, while open.
    pub(crate) selection_pane: Option<selection_pane::SelectionPane>,
    /// 読み上げ順序ペイン, while open.
    pub(crate) reading_order_pane: Option<reading_order_pane::ReadingOrderPane>,
    /// ハイパーリンク (⌘K), while open.
    pub(crate) link_panel: Option<links::LinkPanel>,
    /// The video and audio clips playing.
    pub(crate) clips: Vec<clips::Playing>,
    /// The clip the pointer is over (or whose control bar it is over): its control bar shows (clip_bar.rs).
    pub(crate) clip_hover: Option<ibis_doc_model::ShapeId>,
    /// The clip whose seek bar is being dragged.
    pub(crate) clip_seek: Option<(ibis_doc_model::ShapeId, bool)>,
    /// The bar's volume of each clip (a playback aid of the editor, not the document).
    pub(crate) clip_volume: std::collections::HashMap<ibis_doc_model::ShapeId, clip_bar::BarVolume>,
    /// 挿入 > 記号と特殊文字, while open: the group shown.
    pub(crate) symbols: Option<usize>,
    /// 既定の図形・線に設定: what a new shape or line is made with.
    pub(crate) defaults: ibis_doc_model::ObjectDefaults,
    /// テーマの色とフォント, while open.
    pub(crate) theme_panel: Option<theme_panel::ThemePanel>,
    pub(crate) insets_panel: Option<insets_panel::InsetsPanel>,
    pub(crate) paragraph_format_copy: Option<ibis_doc_model::Paragraph>,
    pub(crate) character_format_copy: Option<ibis_doc_model::CharacterFormat>,
    /// アニメーションの書式設定（Animation Painter）でコピーされたアニメーション群.
    pub(crate) animation_copy: Option<Vec<ibis_doc_model::Animation>>,
    /// The slide the clipboard's shapes were copied from (`None` after a cut).
    pub(crate) clipboard_slide: Option<ibis_doc_model::SlideId>,
    /// How many times they were pasted on that slide.
    pub(crate) pastes: u32,
    /// The text being edited in place.
    pub(crate) edit: Option<text_edit::TextEditing>,
    /// Where the edited text's characters were painted.
    pub(crate) positions: crate::render::rich_text::TextPositions,
    /// The x (the text's own coordinates) ↑↓ keep to, and the caret it
    /// was kept for: pressed again from there, they go down the same column.
    pub(crate) goal_x: Option<(usize, f32)>,
    /// Where the click that started the edit was (window pixels): the caret
    /// goes there once the text has been painted.
    pub(crate) pending_click: Option<Point<Pixels>>,
    /// フォントの置換, while open.
    pub(crate) replace_fonts: Option<replace_fonts::ReplaceFontsState>,
    /// フォトアルバムの設定・作成, while open.
    pub(crate) photo_album: Option<photo_album::PhotoAlbumState>,
    /// The speaker notes field, and whether the notes pane is hidden.
    pub(crate) notes_field: Option<notes::NotesField>,
    pub(crate) notes_hidden: bool,
    /// The width of the slide list (None: the view's own) and the height of the notes pane, as dragged by the dividers (#39); `pane_drag` while one is held.
    pub(crate) pane_width: Option<f32>,
    pub(crate) notes_height: f32,
    pub(crate) pane_drag: Option<notes::PaneDrag>,
    /// 代替テキスト (Alt Text), while open.
    pub(crate) alt_text_panel: Option<alt_text_panel::AltTextPanel>,
    /// スライド一覧（グリッド表示 / Slide Sorter View）.
    pub(crate) grid_view: bool,
    /// アウトライン表示 (Outline View).
    pub(crate) outline_view: bool,
    /// The line of the outline being edited (アウトライン表示).
    pub(crate) outline_edit: Option<outline_edit::OutlineEdit>,
    /// The Draw panel (挿入 > 描画…), while it is open.
    pub(crate) ink: Option<ink::InkPanel>,
    /// ツール > オートコレクトのオプション… (the panel the formats share).
    pub(crate) autocorrect: Option<crate::shell::autocorrect::AutoCorrectPanel>,
    /// 挿入 > 数式… (the panel to type a formula in).
    pub(crate) math_panel: Option<math_panel::MathPanel>,
    /// ノート表示 (Notes Page View).
    pub(crate) notes_page_view: bool,
    /// グリッド線 (Gridlines) 表示.
    pub(crate) gridlines: bool,
    /// ガイド (Guides) 表示.
    pub(crate) guides: bool,
    /// グリッドに合わせる (Snap to Grid).
    pub(crate) snap_to_grid: bool,
    /// 図形に合わせる (Snap to Shape).
    pub(crate) snap_to_shape: bool,
    /// スライドの右クリックメニュー (Slide Context Menu), while open.
    pub(crate) slide_popup: Option<SlidePopup>,
    /// 読み取り専用推奨の情報バーで「編集を有効にする」を押したか.
    pub(crate) read_only_dismissed: bool,
    /// The password to modify the file was given (or set here).
    pub(crate) modify_unlocked: bool,
    /// What the system's clipboard held when shapes were last copied.
    pub(crate) clipboard_stamp: Option<u64>,
    /// The slides selected in the list when there are several (Shift or ⌘ and a click); empty for the one shown.
    pub(crate) slide_selection: Vec<ibis_doc_model::SlideId>,
    /// 配置: several shapes are aligned to the slide (スライドに合わせる), not to one another.
    pub(crate) align_to_slide: bool,
    /// ドキュメント プロパティ パネル (Document Properties Panel), while open.
    pub(crate) properties_panel: Option<properties_panel::PresentationDocPropertiesPanel>,
    /// ドキュメント検査パネル (Document Inspector Panel), while open.
    pub(crate) inspect_panel: Option<inspect_panel::PresentationInspectPanel>,
    /// アクセシビリティ検査パネル (Accessibility Panel), while open.
    pub(crate) accessibility_panel: Option<accessibility_panel::AccessibilityPanel>,
    /// 互換性検査パネル (Compatibility Panel), while open.
    pub(crate) compatibility_panel: Option<compatibility_panel::CompatibilityPanel>,
    /// スライド ズーム挿入ダイアログ (Slide Zoom Dialog), while open.
    pub(crate) slide_zoom_dialog: Option<slide_zoom::SlideZoomDialog>,
    /// セクション ズーム挿入ダイアログ (Section Zoom Dialog), while open.
    pub(crate) section_zoom_dialog: Option<slide_zoom::SectionZoomDialog>,
    /// サマリー ズーム挿入ダイアログ (Summary Zoom Dialog), while open.
    pub(crate) summary_zoom_dialog: Option<slide_zoom::SummaryZoomDialog>,
    /// プレゼンテーション比較・変更統合パネル (Compare Presentations Panel), while open.
    pub(crate) compare_panel: Option<compare::ComparePanel>,
    /// オーディオの録音パネル (Record Audio Panel), while open.
    pub(crate) audio_record: Option<audio_record::AudioRecordPanel>,
    /// 3D モデルの設定パネル (3D Model Settings Panel), while open.
    pub(crate) threed_panel: Option<threed_and_nav::ThreeDModelPanel>,
    /// 録画プレビュー パネル (Recording Preview Panel), while open.
    pub(crate) record_preview: Option<threed_and_nav::RecordPreviewPanel>,
    /// スクリーンリーダー操作パネル (Screen Reader Navigation Panel), while open.
    pub(crate) screen_reader: Option<threed_and_nav::PptxScreenReaderState>,
    /// リボンとクイックアクセスの設定パネル (Ribbon & QAT Customization Panel), while open.
    pub(crate) ribbon_qat: Option<threed_and_nav::PptxRibbonQatState>,
    /// セキュリティ センター パネル (Trust Center Panel), while open.
    pub(crate) trust_center: Option<threed_and_nav::PptxTrustCenterState>,
}

/// The slide's context menu, open.
pub(crate) struct SlidePopup {
    pub(crate) menu: Entity<gpui_kit::component::menu::PopupMenu>,
    pub(crate) position: Point<Pixels>,
    _dismiss: Subscription,
}

impl Default for State {
    fn default() -> Self {
        Self { selection: Vec::new(), tool: editing::Tool::Select, drag: None, shift: false, clipboard: Vec::new(), clipboard_groups: Vec::new(), character_format_copy: None, paragraph_format_copy: None, animation_copy: None, find: None, replace_fonts: None, photo_album: None, section_panel: None, layout_panel: None, print_panel: None, spelling: None, thesaurus: None, #[cfg(feature = "export")] compress_panel: None, edit_points: None, spell_cache: Default::default(), path_draw: None, notes_marks_panel: None, print_settings: Default::default(), folded_sections: Default::default(), section_selection: None, size_panel: None, insets_panel: None, crop_panel: None, frame_panel: None, cell_size_panel: None, alt_text_panel: None, comments: None, chart_panel: None, chart_data: None, header_footer: None, animation_pane: false, sidebar_settings: None, smartart_text_pane: false, smartart_reorder: Default::default(), animation_options_panel: None, selection_pane: None, reading_order_pane: None, link_panel: None, clips: Vec::new(), clip_hover: None, clip_seek: None, clip_volume: Default::default(), symbols: None, defaults: Default::default(), theme_panel: None, timing_panel: None, show_panel: None, glow_panel: None, shadow_panel: None, gradient_panel: None, paragraph_panel: None, clipboard_slide: None, pastes: 0, edit: None, positions: Default::default(), goal_x: None, pending_click: None, notes_field: None, notes_hidden: false, pane_width: None, notes_height: notes::PANE_HEIGHT, pane_drag: None, grid_view: false, outline_view: false, outline_edit: None, ink: None, autocorrect: None, math_panel: None, notes_page_view: false, gridlines: false, guides: false, snap_to_grid: false, snap_to_shape: false, slide_popup: None, read_only_dismissed: false, modify_unlocked: false, clipboard_stamp: None, slide_selection: Vec::new(), align_to_slide: false, properties_panel: None, inspect_panel: None, accessibility_panel: None, compatibility_panel: None, slide_zoom_dialog: None, section_zoom_dialog: None, summary_zoom_dialog: None, compare_panel: None, audio_record: None, threed_panel: None, record_preview: None, screen_reader: None, ribbon_qat: None, trust_center: None }
    }
}

/// A presentation's editor.
pub(crate) struct Presentation;

impl FormatEditor for Presentation {
    const PAGE_NOUN: &'static str = "menu.slide";
    const FIT_TOOLTIP: &'static str = "pptx.mod.fit-slide-window";
    const SLIDES: bool = true;

    fn new_state(_: &Model, _: &mut Window, cx: &mut Context<SlideEditor>) -> FormatState {
        for context in [slide_view::CONTEXT,crate::shell::editor::WORKSPACE_CONTEXT,crate::shell::editor::LIST_CONTEXT] {
            cx.bind_keys([
                KeyBinding::new("ctrl-m",crate::AddPresentationSlide,Some(context)),
                KeyBinding::new("cmd-shift-n",crate::AddPresentationSlide,Some(context)),
                KeyBinding::new("cmd-d",crate::DuplicatePresentationSelection,Some(context)),
            ]);
        }
        // ⌃D is forward-delete while text is edited on a Mac, so it duplicates only off the Mac.
        if !crate::is_mac() {
            for context in [slide_view::CONTEXT,crate::shell::editor::WORKSPACE_CONTEXT,crate::shell::editor::LIST_CONTEXT] {
                cx.bind_keys([KeyBinding::new("ctrl-d",crate::DuplicatePresentationSelection,Some(context))]);
            }
        }
        cx.bind_keys([
            KeyBinding::new(if crate::is_mac() { "cmd-shift-h" } else { "ctrl-h" }, slide_view::OpenFind, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-f" } else { "ctrl-f" }, slide_view::OpenFind, Some(slide_view::CONTEXT)),
            KeyBinding::new("delete", slide_view::DeleteForward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-\\" } else { "ctrl-\\" }, slide_view::ClearFormatting, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-alt-g" } else { "ctrl-alt-g" }, slide_view::GroupShapes, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-shift-alt-g" } else { "ctrl-shift-alt-g" }, slide_view::UngroupShapes, Some(slide_view::CONTEXT)),
            // 上付き ⇧⌘+・下付き ⌘=（文字を編集しているか図形を選んでいるとき。ほかは表示の拡大）.
            KeyBinding::new(if crate::is_mac() { "cmd-+" } else { "ctrl-+" }, slide_view::ToggleSuperscript, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-shift-=" } else { "ctrl-shift-=" }, slide_view::ToggleSuperscript, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-=" } else { "ctrl-=" }, slide_view::ToggleSubscript, Some(slide_view::CONTEXT)),
            // 順序（メニューの表示どおり）: 最前面 ⌥⇧⌘↑・前面へ ⌥⌘↑・背面へ ⌥⌘↓・最背面 ⌥⇧⌘↓.
            KeyBinding::new(if crate::is_mac() { "cmd-shift-alt-up" } else { "ctrl-shift-alt-up" }, slide_view::BringToFront, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-alt-up" } else { "ctrl-alt-up" }, slide_view::BringForward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-alt-down" } else { "ctrl-alt-down" }, slide_view::SendBackward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-shift-alt-down" } else { "ctrl-shift-alt-down" }, slide_view::SendToBack, Some(slide_view::CONTEXT)),
            KeyBinding::new("shift-f9", slide_view::ToggleGridlines, Some(slide_view::CONTEXT)),
            KeyBinding::new("alt-f9", slide_view::ToggleGuides, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::is_mac() { "cmd-alt-h" } else { "ctrl-alt-h" }, slide_view::ToggleHighContrast, Some(slide_view::CONTEXT)),
        ]);
        FormatState::Presentation(State::default())
    }

    /// A slide is centred with a margin around it, and opens fitted, as in
    /// PowerPoint.
    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        let margin = editor.slide_margin();
        let page = editor.presentation().size_of(editor.slide());
        editor.page_content(workspace, page, point(margin, margin), point(margin, margin), false)
    }

    /// The slide list beside the slide, or the slide sorter grid view.
    fn render_body(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        let popup = editor.pptx().and_then(|s| s.slide_popup.as_ref()).map(|p| {
            deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2)
        });
        let infobar = editor.render_pptx_infobar(cx);
        let content = if editor.is_grid_view() {
            div()
                .size_full()
                .child(editor.render_slide_grid_view(window, cx))
                .children(popup)
                .into_any_element()
        } else if editor.is_notes_page_view() {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(editor.render_thumbnails(cx))
                .child(editor.render_notes_page_workspace(window, cx))
                .children(popup)
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(editor.render_thumbnails(cx))
                .child(editor.render_pane_divider(cx))
                .child(
                    gpui_kit::component::v_flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(editor.render_slide_workspace(window, cx))
                        .children(editor.render_notes_divider(cx))
                        .children(editor.render_notes_pane(window, cx)),
                )
                .children(popup)
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| this.drag_pane(event, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_pane_drag(cx)))
                .into_any_element()
        };
        let banner = editor.render_recovery_banner(cx);
        gpui_kit::component::v_flex()
            .size_full()
            .children(infobar)
            .children(banner)
            .child(content)
            .into_any_element()
    }

    fn before_render(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        editor.sync_selection();
    }

    /// The tool's hint (the mock's status bar).
    fn status_items(editor: &SlideEditor) -> Vec<SharedString> {
        if editor.text_editing().is_some() {
            return vec![crate::shell::i18n::text("status.hint.esc")];
        }
        editor.pptx_tool().hint().map(crate::shell::i18n::text).into_iter().collect()
    }

    // The text input (typing, the input method) of the text being edited.

    fn wants_text_input(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        editor.text_editing().is_some() || editor.is_outline_editing()
    }

    fn text_for_range(editor: &mut SlideEditor, range: std::ops::Range<usize>, adjusted: &mut Option<std::ops::Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<String> {
        if editor.is_outline_editing() {
            return editor.outline_text_for_range(range, adjusted);
        }
        editor.input_text_for_range(range, adjusted)
    }

    fn selected_text_range(editor: &mut SlideEditor, _ignore_disabled_input: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        if editor.is_outline_editing() {
            return editor.outline_selected_range();
        }
        editor.input_selected_range()
    }

    fn marked_text_range(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<std::ops::Range<usize>> {
        if editor.is_outline_editing() {
            return editor.outline_marked_range();
        }
        editor.input_marked_range()
    }

    fn unmark_text(editor: &mut SlideEditor, _window: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.is_outline_editing() {
            return editor.outline_input_unmark(cx);
        }
        editor.input_unmark(cx);
    }

    fn replace_text_in_range(editor: &mut SlideEditor, range: Option<std::ops::Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
        // A tab typed in a table cell goes to the next cell (Shift+Tab is
        // the action `PreviousTableCell`).
        if text == "\t" && editor.text_editing().is_some_and(|e| e.cell.is_some()) {
            return editor.next_table_cell(true, window, cx);
        }
        if editor.is_outline_editing() {
            return editor.outline_input_replace(range, text, cx);
        }
        editor.input_replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(editor: &mut SlideEditor, _range: Option<std::ops::Range<usize>>, text: &str, _selected: Option<std::ops::Range<usize>>, _window: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.is_outline_editing() {
            return editor.outline_input_mark(text, cx);
        }
        editor.input_mark(text, cx);
    }

    fn bounds_for_range(editor: &mut SlideEditor, _range: std::ops::Range<usize>, _element: Bounds<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        editor.caret_bounds()
    }

    fn character_index_for_point(editor: &mut SlideEditor, point: Point<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<usize> {
        editor.input_index_for_point(point)
    }

    fn menus(editor: &SlideEditor, menus: &mut Menus) {
        chrome::menus(editor, menus);
        threed_and_nav::append_menus(editor, menus);
    }

    fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
        chrome::toolbar_left(editor)
    }

    /// The title bar's コメントを追加: a comment on the selected shape or the slide.
    fn add_comment(_: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        Some(std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.open_slide_comments(window, cx)))
    }

    /// The title bar's コメントを表示: the comments' panel shown or hidden.
    fn show_comments(editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        let shown = editor.sidebar_open && editor.pptx().is_some_and(|s| s.comments.is_some());
        Some((
            shown,
            std::rc::Rc::new(move |this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| {
                if shown {
                    this.close_slide_comments(cx);
                } else {
                    this.open_slide_comments(window, cx);
                }
            }),
        ))
    }

    fn toolbar_right(editor: &SlideEditor) -> Option<Vec<ToolItem>> {
        Some(chrome::toolbar_right(editor))
    }

    /// Opened with no panel chosen, the pane shows the slide's or the shape's size and position (chrome::sidebar).
    fn sidebar_on_request() -> bool {
        true
    }

    fn sidebar(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        editor.threed_model_panel(cx).or_else(|| editor.record_preview_panel(cx)).or_else(|| editor.pptx_screen_reader_panel(cx)).or_else(|| editor.pptx_ribbon_qat_panel(cx)).or_else(|| editor.pptx_trust_center_panel(cx).map(|panel| panel.modal_named("pptx_trust_center_panel"))).or_else(|| editor.audio_record_panel(cx).map(|panel| panel.modal_named("audio_record_panel"))).or_else(|| editor.compare_panel(cx).map(|panel| panel.modal_named("compare_panel"))).or_else(|| editor.smartart_text_pane(cx)).or_else(|| editor.photo_album_panel(cx).map(|panel| panel.modal_named("photo_album_panel"))).or_else(|| editor.pptx_ink_panel(cx)).or_else(|| editor.pptx_autocorrect_panel(cx).map(|panel| panel.modal_named("pptx_autocorrect_panel"))).or_else(|| editor.pptx_math_panel(cx).map(|panel| panel.modal_named("pptx_math_panel"))).or_else(|| editor.master_view_panel(cx)).or_else(|| editor.notes_format_panel(cx)).or_else(|| editor.autorecover_settings_panel(cx).map(|panel| panel.modal_named("autorecover_settings_panel"))).or_else(|| editor.recover_unsaved_panel(cx).map(|panel| panel.modal_named("recover_unsaved_panel"))).or_else(|| editor.animation_options_panel(cx).map(|panel| panel.modal_named("animation_options_panel"))).or_else(|| editor.pptx_compatibility_panel(cx).map(|panel| panel.modal_named("pptx_compatibility_panel"))).or_else(|| editor.pptx_accessibility_panel(cx)).or_else(|| editor.presentation_inspect_panel(cx).map(|panel| panel.modal_named("presentation_inspect_panel"))).or_else(|| editor.presentation_properties_panel(cx).map(|panel| panel.modal_named("presentation_properties_panel"))).or_else(|| editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel"))).or_else(|| editor.notes_marks_panel(cx).map(|panel| panel.modal_named("notes_marks_panel"))).or_else(|| editor.spelling_panel(cx)).or_else(|| editor.pptx_thesaurus_panel(cx)).or_else(|| editor.compress_panel(cx).map(|panel| panel.modal_named("compress_panel"))).or_else(|| editor.print_panel(cx).map(|panel| panel.modal_named("print_panel"))).or_else(|| editor.layout_name_panel(cx).map(|panel| panel.modal_named("layout_name_panel"))).or_else(|| editor.pptx_section_panel(cx).map(|panel| panel.modal_named("pptx_section_panel"))).or_else(|| editor.pptx_gradient_panel(cx).map(|panel| panel.modal_named("pptx_gradient_panel"))).or_else(|| editor.shadow_panel(cx).map(|panel| panel.modal_named("shadow_panel"))).or_else(|| editor.glow_panel(cx).map(|panel| panel.modal_named("glow_panel"))).or_else(|| editor.pptx_paragraph_panel(cx).map(|panel| panel.modal_named("pptx_paragraph_panel"))).or_else(|| editor.timing_panel(cx).map(|panel| panel.modal_named("timing_panel"))).or_else(|| editor.show_panel(cx).map(|panel| panel.modal_named("show_panel"))).or_else(|| editor.crop_panel(cx).map(|panel| panel.modal_named("crop_panel"))).or_else(|| editor.frame_panel(cx).map(|panel| panel.modal_named("frame_panel"))).or_else(|| editor.cell_size_panel(cx).map(|panel| panel.modal_named("cell_size_panel"))).or_else(|| editor.alt_text_panel(cx).map(|panel| panel.modal_named("alt_text_panel"))).or_else(|| editor.chart_panel(cx).map(|panel| panel.modal_named("chart_panel"))).or_else(|| editor.header_footer_panel(cx).map(|panel| panel.modal_named("header_footer_panel"))).or_else(|| editor.animation_pane(cx)).or_else(|| editor.reading_order_pane(cx)).or_else(|| editor.selection_pane(cx)).or_else(|| editor.slide_link_panel(cx).map(|panel| panel.modal_named("slide_link_panel"))).or_else(|| editor.theme_panel(cx).map(|panel| panel.modal_named("theme_panel"))).or_else(|| editor.text_insets_panel(cx).map(|panel| panel.modal_named("text_insets_panel"))).or_else(|| editor.slide_size_panel(cx).map(|panel| panel.modal_named("slide_size_panel"))).or_else(|| editor.pptx_find_panel(cx)).or_else(|| editor.replace_fonts_panel(cx).map(|panel| panel.modal_named("replace_fonts_panel"))).or_else(|| editor.slide_comments_panel(cx)).or_else(|| chrome::sidebar(editor, window, cx))
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }
}
