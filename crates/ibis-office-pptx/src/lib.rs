// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "scenarios"), allow(dead_code))]
//! The presentation screen (PPTX): the slide list, the slide show, and the presentation's editing, on top of the shell.
//!
//! This is the crate `ibis-office-pptx` (tasks/doc/crate-split-plan.md, 14). It names the shell (`ibis-office-shell`) and the
//! crates below it, never another format. The methods it writes for the editor are extension traits (`ext.rs`); the app
//! registers the format's driver (`Presentation`) in `format_drivers.rs`. See `shell/format.rs` for how the shell hands over to a
//! format.

// The crates below, under the names this crate's code uses (as the shell's lib.rs does).
pub(crate) use ibis_office_base::clock;
pub(crate) use ibis_office_render as render;
pub(crate) use ibis_office_render::media_player;
// The shell is its own crate; the code names it `crate::shell`.
pub(crate) use ibis_office_shell as shell;

pub(crate) mod sidebar_settings;
pub mod chrome;
pub(crate) mod ibis_text;
pub mod editing;
pub(crate) use crate::shell::find;
pub mod animation_ui;
pub(crate) use crate::shell::chart_panel;
pub(crate) mod reading_order_pane;
pub mod links;
pub(crate) mod clips;
pub(crate) mod clip_bar;
pub(crate) mod show_window;
pub(crate) mod symbols;
pub mod comments;
pub(crate) mod cell_size_panel;
pub(crate) use crate::shell::header_footer;
pub(crate) mod timing_panel;
pub(crate) mod show_panel;
pub(crate) use crate::shell::gradient_panel;
pub(crate) use crate::shell::paragraph_panel;
pub(crate) use crate::shell::interaction;
pub(crate) mod notes;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/scenarios.rs"]
pub mod scenarios;
pub mod slide_view;
pub(crate) mod section_panel;
pub(crate) mod layout_panel;
pub(crate) mod print_panel;
pub(crate) mod master_view;
pub(crate) mod guides;
pub(crate) mod notes_format;
pub(crate) mod morph;

/// The presentation's own lines of the menus, its button of the toolbar's left part and its sidebar of sizes.
pub(crate) mod menu_items;
/// The screen's rows of the sidebar table.
pub(crate) mod sidebars;
/// The extension traits of this folder's `impl SlideEditor` blocks, written by `scripts/gen-editor-ext.py`.
pub mod ext;
pub(crate) mod edit_points;
pub mod outline_edit;
pub mod ink;
pub(crate) mod autocorrect_panel;
pub(crate) mod math_panel;
pub(crate) mod path_draw;
pub(crate) mod shape_menu;
pub(crate) use crate::shell::smartart;
pub(crate) mod smartart_pane;
pub(crate) use smartart::{SmartArtColorStyle, SmartArtKind};
pub(crate) mod notes_marks_panel;
pub(crate) mod shape_merge;
pub(crate) mod properties_panel;
pub(crate) mod inspect_panel;
pub(crate) mod accessibility_panel;
pub(crate) mod compatibility_panel;
pub(crate) mod glyph_prewarm;
pub mod slideshow;
pub(crate) mod turn3d;
pub(crate) mod turn3d_tracks;
pub(crate) mod grid_view;
pub(crate) mod thumbnails;
pub(crate) mod slide_zoom;
pub(crate) mod zoom_cards;
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
use crate::shell::format::{FormatEditor, FormatState, impl_format_state};
use crate::ext::*;

/// What only an open presentation has: the selection (several shapes),
/// the tool, the drag in progress and the shapes copied.
pub struct State {
    /// The slide show, while the presentation is shown full screen.
    pub(crate) slideshow: Option<slideshow::Slideshow>,
    /// スライド マスター表示, while it is open.
    pub(crate) master_view: Option<master_view::MasterView>,
    /// ノートの書式編集, while it is open.
    pub(crate) notes_format: Option<notes_format::NotesFormat>,
    pub(crate) section_panel: Option<section_panel::SectionPanel>,
    pub(crate) layout_panel: Option<layout_panel::LayoutNamePanel>,
    /// 印刷の設定 panel, while open, and the settings the file prints with.
    pub print_panel: Option<print_panel::PrintPanel>,
    /// 図の圧縮 panel, while open.
    #[cfg(feature = "export")]
    /// 頂点の編集: the shape whose vertices are shown.
    pub(crate) edit_points: Option<edit_points::EditPoints>,
    /// カスタム軌跡: the path being drawn.
    pub(crate) path_draw: Option<path_draw::PathDraw>,
    /// ノートのヘッダーとフッター panel, while open.
    pub notes_marks_panel: Option<notes_marks_panel::NotesMarksPanel>,
    pub(crate) print_settings: print_panel::PrintSettings,
    pub(crate) folded_sections: std::collections::HashSet<String>,
    pub(crate) section_selection: Option<(String,ibis_doc_model::SlideId)>,
    pub(crate) timing_panel: Option<timing_panel::TimingPanel>,
    pub(crate) show_panel: Option<show_panel::ShowPanel>,
    pub(crate) cell_size_panel: Option<cell_size_panel::CellSizePanel>,
    /// The comments' panel (コメント).
    pub(crate) comments: Option<comments::CommentsPanel>,
    /// アニメーション ウィンドウ shown.
    pub animation_pane: bool,
    /// The text boxes whose per-paragraph effects the animation pane lists open (a parent row of them, expanded).
    pub(crate) animation_expanded: std::collections::HashSet<animation_ui::BoxKey>,
    /// The text boxes that were open when a text box's row was pressed to be dragged (all of them fold while it is dragged);
    /// the pane opens them again when the drag ends.
    pub(crate) animation_drag_folded: Vec<animation_ui::BoxKey>,
    /// The animation pane's table (scroll position, column widths) and its row drag and drop.
    pub(crate) animation_table: crate::shell::chrome::virtual_table::TableState,
    pub(crate) animation_dnd: crate::shell::chrome::row_dnd::RowDnd,
    /// SmartArt テキスト ウィンドウ shown.
    pub(crate) sidebar_settings: Option<sidebar_settings::SidebarSettings>,
    pub smartart_text_pane: bool,
    pub(crate) smartart_reorder: crate::shell::chrome::data_table::ListState,
    /// アニメーション効果のオプション詳細設定パネル (Effect Options Panel), while open.
    pub animation_options_panel: Option<animation_ui::AnimationOptionsPanel>,
    /// 読み上げ順序ペイン, while open.
    pub(crate) reading_order_pane: Option<reading_order_pane::ReadingOrderPane>,
    /// ハイパーリンク (⌘K), while open.
    pub link_panel: Option<links::LinkPanel>,
    /// The video and audio clips playing.
    pub(crate) clips: Vec<clips::Playing>,
    /// The clip the pointer is over (or whose control bar it is over): its control bar shows (clip_bar.rs).
    pub(crate) clip_hover: Option<ibis_doc_model::ShapeId>,
    /// The clip whose seek bar is being dragged.
    pub(crate) clip_seek: Option<(ibis_doc_model::ShapeId, bool)>,
    /// The bar's volume of each clip (a playback aid of the editor, not the document).
    pub(crate) clip_volume: std::collections::HashMap<ibis_doc_model::ShapeId, clip_bar::BarVolume>,
    /// アニメーションの書式設定（Animation Painter）でコピーされたアニメーション群.
    pub(crate) animation_copy: Option<Vec<ibis_doc_model::Animation>>,
    /// フォントの置換, while open.
    /// フォトアルバムの設定・作成, while open.
    pub(crate) photo_album: Option<photo_album::PhotoAlbumState>,
    /// The speaker notes field, and whether the notes pane is hidden.
    pub(crate) notes_field: Option<notes::NotesField>,
    pub(crate) notes_hidden: bool,
    /// What the user did to the notes pane since the file was opened (written to `normalViewPr` by the saving): its border moved (the notes
    /// pane taller than the room allows: to the top), or it hidden or shown.
    pub(crate) notes_touched: notes::NotesTouched,
    /// The width of the slide list (None: the view's own) and the height of the notes pane, as dragged by the dividers (#39); `pane_drag` while one is held.
    pub(crate) pane_width: Option<f32>,
    pub(crate) notes_height: f32,
    /// The share of the slide and notes panes the notes pane opens with (the file's `normalViewPr`), until the first frame has the room to size it.
    pub(crate) notes_open_percent: Option<f32>,
    pub(crate) pane_drag: Option<notes::PaneDrag>,
    /// スライド一覧（グリッド表示 / Slide Sorter View）.
    pub(crate) grid_view: bool,
    /// アウトライン表示 (Outline View).
    pub(crate) outline_view: bool,
    /// The line of the outline being edited (アウトライン表示).
    pub(crate) outline_edit: Option<outline_edit::OutlineEdit>,
    /// The Draw panel (挿入 > 描画…), while it is open.
    pub ink: Option<ink::InkPanel>,
    /// 挿入 > 数式… (the panel to type a formula in).
    pub math_panel: Option<math_panel::MathPanel>,
    /// ノート表示 (Notes Page View).
    pub(crate) notes_page_view: bool,
    /// The slides selected in the list when there are several (Shift or ⌘ and a click); empty for the one shown.
    pub(crate) slide_selection: Vec<ibis_doc_model::SlideId>,
    /// ドキュメント プロパティ パネル (Document Properties Panel), while open.
    pub(crate) properties_panel: Option<properties_panel::PresentationDocPropertiesPanel>,
    /// ドキュメント検査パネル (Document Inspector Panel), while open.
    pub(crate) inspect_panel: Option<inspect_panel::PresentationInspectPanel>,
    /// アクセシビリティ検査パネル (Accessibility Panel), while open.
    pub(crate) accessibility_panel: Option<accessibility_panel::AccessibilityPanel>,
    /// 互換性検査パネル (Compatibility Panel), while open.
    pub(crate) compatibility_panel: Option<compatibility_panel::CompatibilityPanel>,
    /// スライド ズーム挿入ダイアログ (Slide Zoom Dialog), while open.
    pub slide_zoom_dialog: Option<slide_zoom::SlideZoomDialog>,
    /// セクション ズーム挿入ダイアログ (Section Zoom Dialog), while open.
    pub section_zoom_dialog: Option<slide_zoom::SectionZoomDialog>,
    /// サマリー ズーム挿入ダイアログ (Summary Zoom Dialog), while open.
    pub summary_zoom_dialog: Option<slide_zoom::SummaryZoomDialog>,
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

impl Default for State {
    fn default() -> Self {
        Self { slideshow: None, master_view: None, notes_format: None, animation_copy: None, photo_album: None, section_panel: None, layout_panel: None, print_panel: None, #[cfg(feature = "export")] edit_points: None, path_draw: None, notes_marks_panel: None, print_settings: Default::default(), folded_sections: Default::default(), section_selection: None, cell_size_panel: None, comments: None, animation_pane: false, animation_expanded: Default::default(), animation_drag_folded: Vec::new(), animation_table: Default::default(), animation_dnd: Default::default(), sidebar_settings: None, smartart_text_pane: false, smartart_reorder: Default::default(), animation_options_panel: None, reading_order_pane: None, link_panel: None, clips: Vec::new(), clip_hover: None, clip_seek: None, clip_volume: Default::default(), timing_panel: None, show_panel: None, notes_field: None, notes_hidden: false, notes_touched: Default::default(), pane_width: None, notes_height: notes::PANE_HEIGHT, notes_open_percent: None, pane_drag: None, grid_view: false, outline_view: false, outline_edit: None, ink: None, math_panel: None, notes_page_view: false, slide_selection: Vec::new(), properties_panel: None, inspect_panel: None, accessibility_panel: None, compatibility_panel: None, slide_zoom_dialog: None, section_zoom_dialog: None, summary_zoom_dialog: None, compare_panel: None, audio_record: None, threed_panel: None, record_preview: None, screen_reader: None, ribbon_qat: None, trust_center: None }
    }
}

/// A presentation's editor.
pub struct Presentation;

mod dialog_hooks;
pub(crate) mod view_state;
mod view_menu;
mod title_buttons;
mod event_hooks;
mod infobar_hooks;
impl_format_state!(State, ibis_pptx::Kind::Presentation);

impl State {
    /// The presentation's state out of `format`, the editor's `format` field. `SlideEditor::pptx` borrows the whole editor; this
    /// borrows that one field, so the editor's other fields (the document, the slide shown) can be used beside what is borrowed.
    pub(crate) fn of(format: &dyn FormatState) -> Option<&State> {
        format.as_any().downcast_ref()
    }

    /// See `of`: the other fields can be written too (a slide show steps while it reads the document and moves the slide shown).
    pub(crate) fn of_mut(format: &mut dyn FormatState) -> Option<&mut State> {
        format.as_any_mut().downcast_mut()
    }
}

/// The context menu of a presentation's mode, the master view (`FormatInfo::outer_contexts`).
fn outer_contexts(state: &crate::shell::menu_context::ContextState) -> Vec<crate::shell::menu_context::ContextMenu> {
    if state.master_view { vec![crate::shell::menu_context::ContextMenu::SlideMaster] } else { Vec::new() }
}

impl FormatEditor for Presentation {
    const INFO: &'static crate::shell::format::FormatInfo = &crate::shell::format::FormatInfo {
        order: 2,
        extension: "pptx",
        new_file_name: || crate::shell::i18n::text("_shell.presentation-1-pptx"),
        untitled_key: "_shell.presentation",
        office: crate::shell::format::OfficeApp {
            name: "PowerPoint",
            setting_key: "office_path.powerpoint",
            #[cfg(target_os = "macos")]
            bundle: "Microsoft PowerPoint.app",
            #[cfg(target_os = "windows")]
            exe: "POWERPNT.EXE",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            open_key: "window.open-in-powerpoint",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            logo_svg: include_bytes!("../logo/open_in_powerpoint.svg"),
        },
        new_command: "file.new.presentation",
        command_menus: {
            use crate::shell::chrome::MenuId as M;
            &[M::File, M::Edit, M::View, M::Insert, M::Format, M::Slide, M::Arrange, M::Tools, M::Window, M::Help]
        },
        icon: "slideshow",
        color: 0xe8a100,
        context_tones: [
            crate::shell::menu_context::ContextTone { fg: 0xa86a00, bg: 0xe8a100, bg_alpha: 0.16 },
            crate::shell::menu_context::ContextTone { fg: 0xffc94d, bg: 0xffc94d, bg_alpha: 0.18 },
        ],
        menus: &crate::shell::menu_layout::SLIDE_MENUS,
        fold_order: {
            use crate::shell::chrome::MenuId;
            &[MenuId::Design, MenuId::Transitions, MenuId::Animations, MenuId::SlideShow, MenuId::Review]
        },
        sidebars: sidebars::SIDEBARS,
        mask: crate::shell::commands::KindMask::PPTX,
        several_windows: None,
        shapes: crate::shell::format::ShapeRules { in_shell: true, several: true, try_first: true, cmyk: true, break_is_typed: false },
        outer_contexts,
        font_substitutes: true,
        drawing_canvas: false,
        new_badge_color: 0xe8710a,
        search_history_key: "menu-search.history.presentation",
        px_per_inch_at_100: crate::shell::editor::PX_PER_INCH_AT_100,
        #[cfg(not(target_family = "wasm"))]
        save_formats: &[("file.save-format.pptx", "pptx"), ("file.save-format.ppsx", "ppsx"), ("file.save-format.potx", "potx"), ("file.save-format.odp", "odp")],
    };

    /// A window's view over a reload: the shown slide and the selected shapes (`view_state.rs`).
    fn view_kept(editor: &SlideEditor) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(editor.keep_slide_view()))
    }

    fn view_put_back(editor: &mut SlideEditor, kept: &dyn std::any::Any, window: &mut Window, cx: &mut Context<SlideEditor>) {
        if let Some(view) = kept.downcast_ref::<view_state::KeptView>() {
            editor.put_slide_view_back(view, window, cx);
        }
    }

    fn open_print_panel(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        // What is printed is the file, as what is saved is: the master view and the notes formatting are closed first.
        if editor.refuse_as(crate::shell::save::RefusedWay::Print, cx) {
            return true;
        }
        editor.commit_edit(window, cx);
        editor.open_print_panel(window, cx);
        true
    }

    /// A presentation as .odp (OpenDocument), as a template, as a slide show (.ppsx) and as .pptx (whatever it was opened as).
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(editor: &SlideEditor, path: &std::path::Path, source: &ibis_pptx::Source, copy: bool) -> Option<Result<crate::shell::format::FileBytes, String>> {
        use crate::shell::format::FileBytes;
        // Every kind of file made here (.odp, templates, slide shows, packages) is refused while the master view or the notes formatting is open.
        if let Some(why) = Self::save_refused(editor) {
            return Some(Err(why));
        }
        let extension = path.extension()?;
        if extension.eq_ignore_ascii_case("odp") {
            // Saved, the file's password encrypts it; a copy is left as it is (as the shell did before the formats were cut apart).
            return Some(ibis_pptx::write_odp(editor.presentation()).map(|bytes| FileBytes { bytes, protect: !copy, note: None }).map_err(|e| e.to_string()));
        }
        fn package<E: ToString>(bytes: Result<Vec<u8>, E>) -> Option<Result<FileBytes, String>> {
            Some(bytes.map(|bytes| FileBytes { bytes, protect: true, note: None }).map_err(|e| e.to_string()))
        }
        if extension.to_str().is_some_and(ibis_pptx::is_template) {
            return package(source.save_template(&Self::model_for_save(editor), ibis_pptx::SaveOptions::default()));
        }
        if extension.to_str().is_some_and(ibis_pptx::is_slideshow) {
            return package(source.save_slideshow(&Self::model_for_save(editor), ibis_pptx::SaveOptions::default()));
        }
        if extension.eq_ignore_ascii_case("pptx") || extension.eq_ignore_ascii_case("pptm") {
            return package(source.save_presentation(&Self::model_for_save(editor), ibis_pptx::SaveOptions::default()));
        }
        None
    }

    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(editor: &SlideEditor, printing: bool) -> Result<crate::shell::format::PdfJob, String> {
        let bytes = editor.current_bytes()?;
        // A presentation's print: the settings' slides, and pictures when the colour, a frame or handouts ask for them.
        if !printing {
            return Ok(crate::shell::format::PdfJob { bytes, slides: None, format: "pdf", args: None });
        }
        let settings = editor.print_settings();
        let current = editor.presentation().slide_index(editor.current_slide).map_or(1, |i| i + 1);
        Ok(crate::shell::format::PdfJob {
            bytes,
            slides: None,
            format: if settings.needs_pictures() { "pdf-image" } else { "pdf" },
            args: Some(settings.arguments(current, &editor.presentation().notes_marks)),
        })
    }

    /// The title bar's button of the notes pane (below the slide), filled while the pane is shown.
    fn title_buttons(editor: &SlideEditor) -> Vec<crate::shell::format::TitleButton> {
        let open = editor.notes_shown();
        vec![crate::shell::format::TitleButton {
            id: "notes-pane",
            icon: "dock_to_bottom",
            on: open,
            tooltip: if open { "_shell.hide-notes-pane" } else { "_shell.show-notes-pane" },
            handler: std::rc::Rc::new(|this: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>| this.toggle_notes(window, cx)),
        }]
    }

    fn title_mode_button(editor: &SlideEditor, weak: &WeakEntity<SlideEditor>, icon_only: bool) -> Option<AnyElement> {
        Some(editor.render_slideshow_or_close_button(weak, icon_only))
    }

    fn dialog_background_panel(editor: &mut SlideEditor, child_key: Option<&'static str>, cx: &mut Context<SlideEditor>) -> Option<crate::shell::chrome::SidebarPanel> {
        let retain_animation = child_key == Some("animation_options_panel") && editor.pptx().is_some_and(|s| s.animation_pane);
        if retain_animation { editor.animation_pane(cx) } else { None }
    }

    fn sidebar_blocked_by_dialog(editor: &SlideEditor) -> bool {
        editor.dialog.active && editor.dialog.key == Some("animation_options_panel")
    }

    fn show_start(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.start_slideshow(window, cx);
    }

    fn show_end(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.end_slideshow(window, cx);
    }

    fn show_running(editor: &SlideEditor) -> bool {
        editor.slideshow().is_some()
    }

    fn show_step(editor: &mut SlideEditor, forward: bool, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.step_slideshow(forward, window, cx);
    }

    fn show_to_end(editor: &mut SlideEditor, last: bool, cx: &mut Context<SlideEditor>) {
        editor.slideshow_to_end(last, cx);
    }

    fn show_element(editor: &SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        editor.render_slideshow(window, cx)
    }

    fn show_in_main(editor: &SlideEditor) -> bool {
        editor.show_in_main()
    }

    fn open_link_panel_for_page(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.slideshow().is_none() {
            editor.slide_open_link_panel(window, cx);
        }
    }

    fn panel_opening(editor: &mut SlideEditor, panel: crate::shell::shape_state::ShapePanel) {
        use crate::shell::shape_state::ShapePanel;
        let Some(s) = editor.pptx_mut() else { return };
        match panel {
            ShapePanel::Compress | ShapePanel::Spelling => {
                s.section_panel = None;
                s.layout_panel = None;
                s.print_panel = None;
            }
            // The section panel stays under the gradient panel until that closes (`panel_closed`).
            ShapePanel::Gradient => {
                s.timing_panel = None;
                s.comments = None;
            }
            _ => {
                s.timing_panel = None;
                s.section_panel = None;
                s.comments = None;
                match panel {
                    ShapePanel::AltText => s.accessibility_panel = None,
                    ShapePanel::HeaderFooter => s.notes_marks_panel = None,
                    ShapePanel::Symbols => s.link_panel = None,
                    _ => {}
                }
            }
        }
    }

    fn panel_closed(editor: &mut SlideEditor, panel: crate::shell::shape_state::ShapePanel) {
        if panel == crate::shell::shape_state::ShapePanel::Gradient && let Some(s) = editor.pptx_mut() {
            s.section_panel = None;
        }
    }

    fn edit_blocked_by_file(editor: &SlideEditor) -> bool {
        editor.is_pptx_mark_as_final()
    }

    fn add_page_blocked(editor: &SlideEditor) -> bool {
        editor.is_pptx_mark_as_final() || editor.is_pptx_read_only_blocked()
    }

    // The master view and the notes formatting are closed first: what is saved is the file, with what was made in it.
    fn save_refused(editor: &SlideEditor) -> Option<String> {
        (editor.is_master_view() || editor.is_page_view()).then(|| crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef").to_string())
    }

    // The notes pane as the user left it is put in the copy the file is written from (the border moved, hidden or shown): the file keeps its own
    // `normalViewPr` when the pane was not touched.
    fn model_for_save(editor: &SlideEditor) -> std::borrow::Cow<'_, ibis_doc_model::Presentation> {
        let Some(state) = editor.pptx() else { return std::borrow::Cow::Borrowed(editor.presentation()) };
        let share = state.notes_slide_share_thousandths(editor.workspace_bounds.map(|b| f32::from(b.size.height)));
        match state.notes_touched.to_save(state.notes_hidden, share) {
            Some(save) if editor.presentation().notes_pane_save != Some(save) => {
                let mut copy = editor.presentation().clone();
                copy.notes_pane_save = Some(save);
                std::borrow::Cow::Owned(copy)
            }
            _ => std::borrow::Cow::Borrowed(editor.presentation()),
        }
    }

    // The notes pane opens as the file's normal view left it (`viewProps.xml`, measured in PowerPoint): folded (no viewProps, or
    // the slide pane 95% or more) is the thin strip, which here is no pane (the title row's notes button shows it); open with
    // the share `100 - restoredTop`; `maximized` has none.
    fn file_opened(editor: &mut SlideEditor) {
        let opening = editor.presentation().notes_pane;
        if let Some(state) = editor.pptx_mut() {
            match opening {
                ibis_doc_model::NotesPaneOpening::Open(percent) => {
                    state.notes_hidden = false;
                    state.notes_open_percent = Some(f32::from(percent));
                }
                ibis_doc_model::NotesPaneOpening::Folded | ibis_doc_model::NotesPaneOpening::Hidden => {
                    state.notes_hidden = true;
                    state.notes_open_percent = None;
                }
            }
        }
    }

    fn new_file_shown(editor: &mut SlideEditor, _: &mut Context<SlideEditor>) {
        if let Some(state) = editor.pptx_mut() {
            state.notes_hidden = true;
        }
    }

    // The section headings are rows of the list too, and a folded section's slides have none.
    fn page_list_row(editor: &SlideEditor, index: usize) -> usize {
        editor.row_of_slide(index)
    }

    // Those chosen with Shift or ⌘ in the list, else the one shown.
    fn selected_pages(editor: &SlideEditor) -> Vec<ibis_doc_model::SlideId> {
        let chosen = editor.pptx().map(|s| s.slide_selection.clone()).unwrap_or_default();
        if chosen.len() > 1 && chosen.contains(&editor.current_slide) {
            editor.presentation().slides.iter().map(|s| s.id).filter(|id| chosen.contains(id)).collect()
        } else {
            vec![editor.current_slide]
        }
    }

    fn clear_page_selection(editor: &mut SlideEditor) {
        if let Some(s) = editor.pptx_mut() {
            s.slide_selection.clear();
        }
    }

    // (The slide's text takes the keys and the typing from the outline's line.)
    fn text_edit_starting(editor: &mut SlideEditor) {
        if let Some(s) = editor.pptx_mut() {
            s.outline_edit = None;
        }
    }

    fn text_key_early(editor: &mut SlideEditor, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.pptx_text_key_early(event, window, cx)
    }

    fn own_text_copy(editor: &mut SlideEditor, cut: bool, cx: &mut Context<SlideEditor>) -> Option<bool> {
        editor.is_outline_editing().then(|| editor.outline_copy(cut, cx))
    }

    fn own_text_paste(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Option<bool> {
        editor.is_outline_editing().then(|| editor.outline_paste(cx))
    }

    fn link_under_pointer(editor: &SlideEditor, position: Point<Pixels>) -> bool {
        editor.slide_layout().is_some_and(|l| editor.slide_link_at(l.to_emu(position)).is_some())
    }

    /// The context menus' state: the master view, and the shapes selected on the slide (a picture that is a clip is a
    /// video, the shapes of a SmartArt selected are a SmartArt).
    fn selection_context(editor: &SlideEditor, state: &mut crate::shell::menu_context::ContextState) {
        use crate::shell::menu_context::ObjectKind;
        state.master_view = editor.is_master_view();
        let smartart = editor.selected_smartart_group().is_some();
        state.objects = editor
            .selected_shapes()
            .into_iter()
            .map(|shape| match ObjectKind::of(shape) {
                ObjectKind::Picture if SlideEditor::clip_of(shape).is_some() => ObjectKind::Video,
                ObjectKind::Shape if smartart => ObjectKind::SmartArt,
                kind => kind,
            })
            .collect();
        state.ink_selected = editor.ink_is_selected();
    }

    fn panel_menu_extras(_: &SlideEditor, panel: crate::shell::shape_state::ShapePanel) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::MenuItem;
        use crate::shell::shape_state::ShapePanel;
        match panel {
            ShapePanel::AltText => vec![MenuItem::tr("pptx.check-accessibility").icon("accessibility").on(|this, window, cx| this.open_pptx_accessibility_panel(window, cx)).into()],
            ShapePanel::HeaderFooter => vec![
                MenuItem::new("pptx.notes-header-footer", crate::shell::i18n::text("pptx.notes-marks-panel.notes-header-footer")).on(|this, w, cx| this.open_notes_marks(w, cx)).into(),
            ],
            _ => Vec::new(),
        }
    }

    fn set_font_size(editor: &mut SlideEditor, pt: f32, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.pptx_set_font_size(pt, window, cx);
    }

    fn cancel_tool_early(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.pptx_cancel_tool_early(window, cx)
    }

    fn cancel_tool_late(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.pptx_cancel_tool_late(window, cx)
    }

    fn format_key(editor: &mut SlideEditor, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.master_grid_key(keystroke, window, cx)
    }

    fn workspace_wheel(editor: &mut SlideEditor, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        editor.pptx_ink_wheel(event, cx)
    }

    fn register_actions(root: Stateful<Div>, cx: &mut Context<SlideEditor>) -> Stateful<Div> {
        root
            .on_action(cx.listener(|e,_:&crate::shell::actions::DuplicatePresentationSelection,w,cx|{if e.kind()==ibis_pptx::Kind::Presentation{if e.selected_shapes().is_empty(){e.duplicate_current_slide(w,cx);}else{e.duplicate_selected(cx);}}}))
            .on_action(cx.listener(|this, _: &crate::shell::actions::StartSlideshow, window, cx| this.start_slideshow(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::StartPresenterView, window, cx| this.start_presenter_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::BlackScreen, _, cx| this.blank_slideshow(0x000000, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::WhiteScreen, _, cx| this.blank_slideshow(0xffffff, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::LaserPointer, _, cx| this.set_slideshow_pointer_mode(crate::shell::pointer_mode::PointerMode::Laser, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::PenPointer, _, cx| this.set_slideshow_pointer_mode(crate::shell::pointer_mode::PointerMode::Pen, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::HighlighterPointer, _, cx| this.set_slideshow_pointer_mode(crate::shell::pointer_mode::PointerMode::Highlighter, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ArrowPointer, _, cx| this.set_slideshow_pointer_mode(crate::shell::pointer_mode::PointerMode::Arrow, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::EraserPointer, _, cx| this.set_slideshow_pointer_mode(crate::shell::pointer_mode::PointerMode::Eraser, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ClearSlideInks, _, cx| this.clear_current_slide_inks(cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ToggleSlideshowGrid, _, cx| this.toggle_slideshow_grid(cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ZoomInSlideshow, _, cx| this.zoom_slideshow(1.25, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ZoomOutSlideshow, _, cx| this.zoom_slideshow(0.8, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ResetSlideshowZoom, _, cx| this.reset_slideshow_zoom(cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::RehearseTimings, window, cx| this.start_rehearsal(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ToggleSlideGridView, window, cx| this.toggle_grid_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ToggleReadingView, window, cx| {
                if this.is_reading_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.start_reading_view(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::shell::actions::SwitchToNormalView, window, cx| this.switch_to_normal_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ToggleOutlineView, window, cx| this.toggle_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::SwitchToOutlineView, window, cx| this.switch_to_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::ToggleNotesPageView, window, cx| this.toggle_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::SwitchToNotesPageView, window, cx| this.switch_to_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::CopyAnimation, _, cx| this.copy_shape_animations(cx)))
            .on_action(cx.listener(|this, _: &crate::shell::actions::PasteAnimation, _, cx| this.paste_shape_animations(cx)))
    }

    fn infobar_kind(editor: &SlideEditor) -> Option<crate::shell::infobar::InfoBarKind> {
        editor.pptx_infobar_kind()
    }

    fn infobar_action(editor: &mut SlideEditor, kind: crate::shell::infobar::InfoBarKind, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.pptx_infobar_action(kind, window, cx);
    }

    /// The presentation's own words for an old format, 最終版 and the advice to open read-only.
    fn infobar_spec(kind: crate::shell::infobar::InfoBarKind) -> Option<crate::shell::infobar::InfoBarSpec> {
        use crate::shell::infobar::InfoBarKind;
        match kind {
            InfoBarKind::LegacyFormat => Some(("pptx-infobar-legacy", "infobar-btn-convert-legacy", "pptx.infobar.legacy.title", "pptx.infobar.legacy.message", "pptx.infobar.legacy.convert")),
            InfoBarKind::MarkAsFinal => Some(("pptx-infobar-final", "infobar-btn-edit-anyway", "pptx.infobar.final.title", "pptx.infobar.final.message", "pptx.infobar.final.edit-anyway")),
            InfoBarKind::ReadOnlyRecommended => Some(("pptx-infobar-readonly", "infobar-btn-enable-editing", "pptx.infobar.readonly.title", "pptx.infobar.readonly.message", "pptx.infobar.readonly.enable-editing")),
            _ => None,
        }
    }

    fn discard_dialog(editor: &mut SlideEditor, key: &'static str) {
        editor.discard_pptx_dialog(key);
    }

    fn autocorrect_dialog_parent_open(editor: &SlideEditor) -> bool {
        editor.shape_edit.autocorrect.as_ref().is_some_and(|p| !p.entry_open)
    }

    fn menu_buttons(editor: &SlideEditor) -> Vec<crate::shell::chrome::ToolItem> {
        chrome::menu_buttons(editor)
    }

    const PAGE_NOUN: &'static str = "menu.slide";
    const FIT_TOOLTIP: &'static str = "pptx.mod.fit-slide-window";
    const SLIDES: bool = true;

    fn new_state(_: &Model, _: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState> {
        for context in [slide_view::CONTEXT,crate::shell::editor::WORKSPACE_CONTEXT,crate::shell::editor::LIST_CONTEXT] {
            cx.bind_keys([
                KeyBinding::new("ctrl-m",crate::shell::actions::AddPresentationSlide,Some(context)),
                KeyBinding::new("cmd-shift-n",crate::shell::actions::AddPresentationSlide,Some(context)),
                KeyBinding::new("cmd-d",crate::shell::actions::DuplicatePresentationSelection,Some(context)),
            ]);
        }
        // ⌃D is forward-delete while text is edited on a Mac, so it duplicates only off the Mac.
        if !crate::shell::app::is_mac() {
            for context in [slide_view::CONTEXT,crate::shell::editor::WORKSPACE_CONTEXT,crate::shell::editor::LIST_CONTEXT] {
                cx.bind_keys([KeyBinding::new("ctrl-d",crate::shell::actions::DuplicatePresentationSelection,Some(context))]);
            }
        }
        cx.bind_keys([
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-h" } else { "ctrl-h" }, slide_view::OpenFind, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-f" } else { "ctrl-f" }, slide_view::OpenFind, Some(slide_view::CONTEXT)),
            // ⌘G / ⇧⌘G: Mac only (PowerPoint for Mac). Windows and Linux have no such key here.
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-f" } else { "ctrl-f" }, slide_view::OpenFind, Some(find::FIND_PANEL_CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-h" } else { "ctrl-h" }, slide_view::OpenFind, Some(find::FIND_PANEL_CONTEXT)),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-g", slide_view::FindNext, Some(find::FIND_PANEL_CONTEXT)),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-g", slide_view::FindPrevious, Some(find::FIND_PANEL_CONTEXT)),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-g", slide_view::FindNext, Some(slide_view::CONTEXT)),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-g", slide_view::FindPrevious, Some(slide_view::CONTEXT)),
            KeyBinding::new("f3", slide_view::FindNext, Some(find::FIND_PANEL_CONTEXT)),
            KeyBinding::new("shift-f3", slide_view::FindPrevious, Some(find::FIND_PANEL_CONTEXT)),
            KeyBinding::new("f3", slide_view::FindNext, Some(slide_view::CONTEXT)),
            KeyBinding::new("shift-f3", slide_view::FindPrevious, Some(slide_view::CONTEXT)),
            KeyBinding::new("delete", slide_view::DeleteForward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-\\" } else { "ctrl-\\" }, slide_view::ClearFormatting, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-alt-g" } else { "ctrl-alt-g" }, slide_view::GroupShapes, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-alt-g" } else { "ctrl-shift-alt-g" }, slide_view::UngroupShapes, Some(slide_view::CONTEXT)),
            // 上付き ⇧⌘+・下付き ⌘=（文字を編集しているか図形を選んでいるとき。ほかは表示の拡大）.
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-+" } else { "ctrl-+" }, slide_view::ToggleSuperscript, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-=" } else { "ctrl-shift-=" }, slide_view::ToggleSuperscript, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-=" } else { "ctrl-=" }, slide_view::ToggleSubscript, Some(slide_view::CONTEXT)),
            // 順序（メニューの表示どおり）: 最前面 ⌥⇧⌘↑・前面へ ⌥⌘↑・背面へ ⌥⌘↓・最背面 ⌥⇧⌘↓.
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-alt-up" } else { "ctrl-shift-alt-up" }, slide_view::BringToFront, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-alt-up" } else { "ctrl-alt-up" }, slide_view::BringForward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-alt-down" } else { "ctrl-alt-down" }, slide_view::SendBackward, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-shift-alt-down" } else { "ctrl-shift-alt-down" }, slide_view::SendToBack, Some(slide_view::CONTEXT)),
            KeyBinding::new("shift-f9", slide_view::ToggleGridlines, Some(slide_view::CONTEXT)),
            KeyBinding::new("alt-f9", slide_view::ToggleGuides, Some(slide_view::CONTEXT)),
            KeyBinding::new(if crate::shell::app::is_mac() { "cmd-alt-h" } else { "ctrl-alt-h" }, slide_view::ToggleHighContrast, Some(slide_view::CONTEXT)),
        ]);
        Box::new(State::default())
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
        // The pictures of the master view's list of pages.
        editor.fill_master_thumbs(cx);
        let popup = editor.shape_edit.popup.as_ref().map(|p| {
            deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2)
        });
        let infobar = editor.render_infobar(cx);
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

    fn insert_symbol(editor: &mut SlideEditor, c: char, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.insert_symbol_in_slide(c, window, cx);
    }

    fn set_ime_clauses(editor: &mut SlideEditor, clauses: Vec<ImeClause>, _window: &mut Window, cx: &mut Context<SlideEditor>) {
        if editor.is_outline_editing() {
            editor.outline_set_clauses(clauses, cx);
        } else {
            editor.input_set_clauses(clauses, cx);
        }
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
        view_menu::connect(editor, menus);
        chrome::menus(editor, menus);
        threed_and_nav::append_menus(editor, menus);
    }

    fn menu_items(editor: &SlideEditor, part: crate::shell::chrome::MenuPart) -> Vec<crate::shell::chrome::MenuEntry> {
        menu_items::menu_items(editor, part)
    }

    fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
        menu_items::toolbar_left(editor)
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

    fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
        chrome::toolbar_right(editor)
    }

    /// Opened with no panel chosen, the pane shows the slide's or the shape's size and position (chrome::sidebar).
    fn sidebar_on_request() -> bool {
        true
    }

    // A presentation's pane, asked for by this button, is the アニメーション ウィンドウ (another panel that is open comes first).
    fn ink_panel_open(editor: &SlideEditor) -> bool {
        editor.sidebar_open && editor.pptx().is_some_and(|s| s.ink.is_some())
    }

    fn close_ink_panel(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
        editor.close_pptx_ink_panel(cx);
    }

    fn sidebar_toggled(editor: &mut SlideEditor) {
        if editor.sidebar_open && let Some(s) = editor.pptx_mut() {
            s.animation_pane = true;
        }
    }

    // The sections or the slides' order changed: the summary zooms follow. The notes field's next typing is a new undo step.
    fn commands_ran(editor: &mut SlideEditor, structural: bool, cx: &mut Context<SlideEditor>) {
        if structural {
            editor.refresh_summary_zooms(cx);
        }
        editor.end_notes_typing();
    }

    // 頂点の編集: Delete takes the vertex away, not the shape.
    fn delete_own_selection(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> bool {
        if editor.is_editing_points() {
            editor.delete_selected_vertex(cx);
            return true;
        }
        false
    }

    fn sidebar(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        editor.threed_model_panel(cx).or_else(|| editor.record_preview_panel(cx)).or_else(|| editor.pptx_screen_reader_panel(cx)).or_else(|| editor.pptx_ribbon_qat_panel(cx)).or_else(|| editor.pptx_trust_center_panel(cx).map(|panel| panel.modal_named("pptx_trust_center_panel"))).or_else(|| editor.audio_record_panel(cx).map(|panel| panel.modal_named("audio_record_panel"))).or_else(|| editor.compare_panel(cx).map(|panel| panel.modal_named("compare_panel"))).or_else(|| editor.smartart_text_pane(cx)).or_else(|| editor.photo_album_panel(cx).map(|panel| panel.modal_named("photo_album_panel"))).or_else(|| editor.pptx_ink_panel(cx)).or_else(|| editor.pptx_autocorrect_panel(cx).map(|panel| panel.modal_named("pptx_autocorrect_panel"))).or_else(|| editor.pptx_math_panel(cx).map(|panel| panel.modal_named("pptx_math_panel"))).or_else(|| editor.master_view_panel(cx)).or_else(|| editor.notes_format_panel(cx)).or_else(|| editor.autorecover_settings_panel(cx).map(|panel| panel.modal_named("autorecover_settings_panel"))).or_else(|| editor.recover_unsaved_panel(cx).map(|panel| panel.modal_named("recover_unsaved_panel"))).or_else(|| editor.animation_options_panel(cx).map(|panel| panel.modal_named("animation_options_panel"))).or_else(|| editor.pptx_compatibility_panel(cx).map(|panel| panel.modal_named("pptx_compatibility_panel"))).or_else(|| editor.pptx_accessibility_panel(cx)).or_else(|| editor.presentation_inspect_panel(cx).map(|panel| panel.modal_named("presentation_inspect_panel"))).or_else(|| editor.presentation_properties_panel(cx).map(|panel| panel.modal_named("presentation_properties_panel"))).or_else(|| editor.symbol_panel(cx).map(|panel| panel.modal_named("symbol_panel"))).or_else(|| editor.notes_marks_panel(cx).map(|panel| panel.modal_named("notes_marks_panel"))).or_else(|| editor.spelling_panel(cx)).or_else(|| editor.pptx_thesaurus_panel(cx)).or_else(|| editor.compress_panel(cx).map(|panel| panel.modal_named("compress_panel"))).or_else(|| editor.print_panel(cx).map(|panel| panel.modal_named("print_panel"))).or_else(|| editor.layout_name_panel(cx).map(|panel| panel.modal_named("layout_name_panel"))).or_else(|| editor.pptx_section_panel(cx).map(|panel| panel.modal_named("pptx_section_panel"))).or_else(|| editor.pptx_gradient_panel(cx).map(|panel| panel.modal_named("pptx_gradient_panel"))).or_else(|| editor.shadow_panel(cx).map(|panel| panel.modal_named("shadow_panel"))).or_else(|| editor.glow_panel(cx).map(|panel| panel.modal_named("glow_panel"))).or_else(|| editor.pptx_paragraph_panel(cx).map(|panel| panel.modal_named("pptx_paragraph_panel"))).or_else(|| editor.timing_panel(cx).map(|panel| panel.modal_named("timing_panel"))).or_else(|| editor.show_panel(cx).map(|panel| panel.modal_named("show_panel"))).or_else(|| editor.crop_panel(cx).map(|panel| panel.modal_named("crop_panel"))).or_else(|| editor.frame_panel(cx).map(|panel| panel.modal_named("frame_panel"))).or_else(|| editor.cell_size_panel(cx).map(|panel| panel.modal_named("cell_size_panel"))).or_else(|| editor.alt_text_panel(cx).map(|panel| panel.modal_named("alt_text_panel"))).or_else(|| editor.chart_panel(cx).map(|panel| panel.modal_named("chart_panel"))).or_else(|| editor.header_footer_panel(cx).map(|panel| panel.modal_named("header_footer_panel"))).or_else(|| editor.text_insets_panel(cx).map(|panel| panel.modal_named("text_insets_panel"))).or_else(|| editor.slide_link_panel(cx).map(|panel| panel.modal_named("slide_link_panel"))).or_else(|| editor.theme_panel(cx).map(|panel| panel.modal_named("theme_panel"))).or_else(|| editor.slide_size_panel(cx).map(|panel| panel.modal_named("slide_size_panel"))).or_else(|| editor.animation_pane(cx)).or_else(|| editor.reading_order_pane(cx)).or_else(|| editor.selection_pane(cx)).or_else(|| editor.pptx_find_panel(cx)).or_else(|| editor.slide_comments_panel(cx)).or_else(|| chrome::sidebar(editor, window, cx)).or_else(|| menu_items::size_sidebar(editor))
    }

    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        scenarios::all()
    }
}
