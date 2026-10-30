// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation editor window: state, editing operations and layout.
//!
//! Rendering is split by area: `chrome.rs`, `status_bar.rs`, `workspace.rs` and
//! `thumbnails.rs` add `impl SlideEditor` blocks for their part of the window.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::{SliderEvent, SliderScale, SliderState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_doc_model::{
    CellRange, Command, Document, EmuPoint, EmuRect, Presentation, ResizeHandle, Shape, ShapeId, ShapeKind, Sheet,
    Slide, SlideId, TextStyle, step_font_size,
};

use crate::render::shape_view::Resources;
use crate::shell::format::{FormatState, with_format};
use crate::{CancelTool, CloseFile, DeleteSelection, NextSlide, OpenFile, PreviousSlide, Redo, Undo};

/// Key context of the editor; see the bindings in [`crate::init`].
pub(crate) const CONTEXT: &str = "SlideEditor";
/// Key context of the workspace with the slide (the editor's focus).
pub(crate) const WORKSPACE_CONTEXT: &str = "SlideWorkspace";
/// Key context of the slide list.
pub(crate) const LIST_CONTEXT: &str = "SlideList";
/// Key context of the workspace when it shows a worksheet: the arrow keys
/// move the selected cell instead of turning pages.
pub(crate) const SHEET_CONTEXT: &str = "SheetWorkspace";
/// Key context of the workspace when it shows a flowing document: the keys
/// scroll through its pages, as in Word.
pub(crate) const DOCUMENT_CONTEXT: &str = "DocumentWorkspace";

/// Scrolling of a trackpad that turns one page, in pixels.
const WHEEL_PAGE: f32 = 80.0;
/// In the browser, a pixel scroll that follows this long a pause starts a new
/// scroll and turns a page at once, in milliseconds. See `on_workspace_scroll`.
#[cfg(target_family = "wasm")]
const WHEEL_PAUSE_MS: f64 = 100.0;

/// The lines Windows scrolls for a notch of the wheel (the user's setting,
/// which GPUI multiplies a notch by), at least one.
#[cfg(target_os = "windows")]
fn wheel_scroll_lines() -> f32 {
    const SPI_GETWHEELSCROLLLINES: u32 = 0x0068;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut std::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut lines: u32 = 3;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one u32 to `value`.
    let ok = unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, (&raw mut lines).cast(), 0) } != 0;
    if ok { lines.max(1) as f32 } else { 3.0 }
}

/// How often to look for newly decoded pictures, in milliseconds: a frame.
const PICTURE_POLL_MS: u64 = 16;

/// Space kept around the slide inside the workspace, in pixels.
const WORKSPACE_MARGIN: f32 = 40.0;
/// Space kept around a slide fitted to the workspace, in pixels: room for
/// its shadow and little more, as in PowerPoint.
const FIT_MARGIN: f32 = 8.0;

/// Pixels per inch at 100% zoom: a point is one pixel, as in PowerPoint and Word for Mac (#41; before, a point was 4/3 px on a 96 dpi
/// screen). The slide, a page and a worksheet now share it.
const PX_PER_INCH_AT_100: f32 = 72.0;
/// A worksheet's: Excel for Mac draws a point as one pixel at 100%, 0.75 of
/// the size a slide has there (the user's measure: ibisOffice's 70% looked
/// like Excel's 100%).
const SHEET_PX_PER_INCH_AT_100: f32 = 72.0;
/// The zoom range of the status bar, 1.0 being 100%.
pub(crate) const MIN_ZOOM: f32 = 0.1;
pub(crate) const MAX_ZOOM: f32 = 4.0;
/// Ratio of one − / + step, 2^(1/4): four steps double or halve the zoom.
const ZOOM_STEP: f32 = 1.189_207_1;
/// Ratios between the zooms of the status bar's list: over 100% each is
/// 1.2 times the one under it, under 100% 0.8 times the one over it.
const ZOOM_LIST_UP: f32 = 1.2;
const ZOOM_LIST_DOWN: f32 = 0.8;
/// Zooming with the wheel (the zoom modifier held): a notch of a wheel
/// zooms by this ratio, and a trackpad doubles the zoom every this many pixels.
const WHEEL_ZOOM_STEP: f32 = 1.1;
const WHEEL_ZOOM_DOUBLING: f32 = 150.0;
/// A press that moves less than this is a click, not a drag, in pixels.
const DRAG_THRESHOLD: f32 = 3.0;
/// The smallest side a text box can be resized to, in EMU (0.1 in).
const MIN_SHAPE_SIDE: i64 = 91_440;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    TextBox,
}

/// A pointer drag in progress. Frames are previewed while dragging and
/// committed as one command on release.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Drag {
    Move { shape: ShapeId, start: Point<Pixels>, current: Point<Pixels>, origin: EmuRect },
    Resize {
        shape: ShapeId,
        handle: ResizeHandle,
        start: Point<Pixels>,
        current: Point<Pixels>,
        origin: EmuRect,
        keep_aspect_on_corners: bool,
    },
    Create { start: Point<Pixels>, current: Point<Pixels> },
}

/// Text editing in place, with the caret and selection painted over the shape.
pub(crate) struct TextEdit {
    pub(crate) shape: ShapeId,
    pub(crate) edit: crate::pptx::text_edit::TextEditing,
    pub(crate) positions: crate::render::rich_text::TextPositions,
    pub(crate) goal_x: Option<(usize, f32)>,
    pub(crate) pending_click: Option<Point<Pixels>>,
    pub(crate) drag_anchor: Option<usize>,
}

/// Mapping between slide EMU and window pixels for the current layout.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SlideLayout {
    /// Top-left of the slide, in window coordinates.
    pub(crate) origin: Point<Pixels>,
    /// Pixels per EMU.
    pub(crate) scale: f32,
}

impl SlideLayout {
    /// The zoom this scale shows, 1.0 being 100%.
    pub(crate) fn zoom(&self, px_per_inch_at_100: f32) -> f32 {
        self.scale * ibis_doc_model::EMU_PER_INCH as f32 / px_per_inch_at_100
    }

    pub(crate) fn to_px(&self, emu: i64) -> Pixels {
        px(emu as f32 * self.scale)
    }

    pub(crate) fn to_emu_len(&self, len: Pixels) -> i64 {
        (f32::from(len) / self.scale).round() as i64
    }

    pub(crate) fn to_emu(&self, p: Point<Pixels>) -> EmuPoint {
        EmuPoint::new(self.to_emu_len(p.x - self.origin.x), self.to_emu_len(p.y - self.origin.y))
    }
}

/// What the workspace scrolls: the slide with its margin, and at least the
/// workspace itself so a smaller slide stays centred.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkspaceContent {
    pub(crate) size: Size<Pixels>,
    /// Top-left of the slide in the content.
    pub(crate) slide_origin: Point<Pixels>,
    /// Pixels per EMU.
    pub(crate) scale: f32,
}

/// 編集記号の表示設定 (DOCX-OPTIONS-008: Word Options > 表示 > 常に画面に表示する編集記号).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct FormattingMarksOptions {
    /// 常に表示する: タブ (→)
    pub show_tabs: bool,
    /// 常に表示する: スペース (·, □)
    pub show_spaces: bool,
    /// 常に表示する: 段落記号 (¶)
    pub show_paragraph_marks: bool,
    /// 常に表示する: 任意指定の改行 / 手動改行 (↵)
    pub show_line_breaks: bool,
    /// 常に表示する: 隠し文字 (下線点線)
    pub show_hidden_text: bool,
}

impl FormattingMarksOptions {
    pub fn is_tabs_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs
    }
    pub fn is_spaces_visible(&self, all_open: bool) -> bool {
        all_open || self.show_spaces
    }
    pub fn is_paragraph_marks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_paragraph_marks
    }
    pub fn is_line_breaks_visible(&self, all_open: bool) -> bool {
        all_open || self.show_line_breaks
    }
    pub fn is_hidden_text_visible(&self, all_open: bool) -> bool {
        all_open || self.show_hidden_text
    }
    pub fn any_visible(&self, all_open: bool) -> bool {
        all_open || self.show_tabs || self.show_spaces || self.show_paragraph_marks || self.show_line_breaks || self.show_hidden_text
    }
}

/// Word Options > 詳細設定 > 切り取り、コピー、貼り付け: 貼付の既定動作 (DOCX-OPTIONS-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DefaultPasteMode {
    /// 元の書式を保持 (Keep Source Formatting, Word既定値).
    #[default]
    KeepSourceFormatting,
    /// 書式を統合 (Merge Formatting / 貼り付け先の書式に合わせる).
    MergeFormatting,
    /// テキストのみ保持 (Keep Text Only).
    KeepTextOnly,
}

/// Word Options > 詳細設定 > 表示: ルーラーの測定単位 (DOCX-VIEW-013).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RulerUnit {
    /// センチメートル (Centimeters, Word 日本語既定値).
    #[default]
    Centimeters,
    /// ミリメートル (Millimeters).
    Millimeters,
    /// インチ (Inches).
    Inches,
    /// ポイント (Points).
    Points,
    /// ピカ (Picas, 1 pica = 12 pt).
    Picas,
}

impl RulerUnit {
    /// 1 単位あたりの EMU 値。
    pub(crate) fn emu_per_unit(self) -> f32 {
        match self {
            Self::Centimeters => 360_000.0,
            Self::Millimeters => 36_000.0,
            Self::Inches => 914_400.0,
            Self::Points => 12_700.0,
            Self::Picas => 152_400.0,
        }
    }

    /// 単位の略称（ルーラーのコーナー表示等）。
    pub(crate) fn abbreviation(self) -> &'static str {
        match self {
            Self::Centimeters => "cm",
            Self::Millimeters => "mm",
            Self::Inches => "in",
            Self::Points => "pt",
            Self::Picas => "pi",
        }
    }

    /// ルーラーの基本刻み間隔（EMU）。
    pub(crate) fn tick_step_emu(self) -> f32 {
        match self {
            Self::Centimeters => 180_000.0, // 0.5 cm
            Self::Millimeters => 36_000.0,  // 1 mm
            Self::Inches => 114_300.0,      // 1/8 in
            Self::Points => 228_600.0,      // 18 pt
            Self::Picas => 76_200.0,        // 0.5 pica
        }
    }

    /// ラベル数値の間隔候補（単位数）。
    pub(crate) fn label_candidates(self) -> &'static [i32] {
        match self {
            Self::Centimeters => &[1, 2, 5, 10, 20],
            Self::Millimeters => &[5, 10, 20, 50, 100],
            Self::Inches => &[1, 2, 5, 10],
            Self::Points => &[36, 72, 144, 288, 720],
            Self::Picas => &[1, 2, 5, 10, 20],
        }
    }

    /// 刻みインデックスから、大目盛り（数値表示候補）・中目盛り・表示数値を判定。
    /// 返り値: (is_major, is_medium, unit_label_number)
    pub(crate) fn tick_info(self, index: i32) -> (bool, bool, i32) {
        match self {
            Self::Centimeters => {
                let is_major = index % 2 == 0;
                (is_major, false, index.abs() / 2)
            }
            Self::Millimeters => {
                let is_major = index % 10 == 0;
                let is_medium = index % 5 == 0 && !is_major;
                (is_major, is_medium, index.abs())
            }
            Self::Inches => {
                let is_major = index % 8 == 0;
                let is_medium = (index % 4 == 0 || index % 2 == 0) && !is_major;
                (is_major, is_medium, index.abs() / 8)
            }
            Self::Points => {
                let pt = index.abs() * 18;
                let is_major = pt % 72 == 0;
                let is_medium = pt % 36 == 0 && !is_major;
                (is_major, is_medium, pt)
            }
            Self::Picas => {
                let is_major = index % 2 == 0;
                (is_major, false, index.abs() / 2)
            }
        }
    }

    /// 次の測定単位を返す（トグル送り用）。
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Centimeters => Self::Millimeters,
            Self::Millimeters => Self::Inches,
            Self::Inches => Self::Points,
            Self::Points => Self::Picas,
            Self::Picas => Self::Centimeters,
        }
    }
}

pub struct SlideEditor {
    /// Focus of the workspace; key presses reach the editor through it.
    pub(crate) focus_handle: FocusHandle,
    /// Focus of the slide list.
    pub(crate) list_focus: FocusHandle,
    pub(crate) list_scroll: ScrollHandle,
    /// Trackpad scrolling not yet turned into a page, in pixels.
    wheel: f32,
    /// The menu of the menu bar that is open (shell/menu_bar.rs).
    pub(crate) open_menu: Option<crate::shell::menu_bar::OpenMenu>,
    /// Where the pointer was when a menu closed, so that the press that closed it does not open it again.
    pub(crate) menu_dismissed_at: Option<Point<Pixels>>,
    /// Time of the last pixel scroll, in milliseconds (browser only).
    #[cfg(target_family = "wasm")]
    wheel_at: f64,
    /// Parts of a wheel notch not yet turned into a page (Windows only).
    #[cfg(target_os = "windows")]
    wheel_notches: f32,
    pub(crate) document: Document,
    pub(crate) document_memory: super::document_memory::DocumentMemoryStore,
    pub(crate) current_slide: SlideId,
    pub(crate) selection: Option<ShapeId>,
    pub(crate) tool: Tool,
    pub(crate) drag: Option<Drag>,
    pub(crate) editing: Option<TextEdit>,
    /// Bounds of the workspace around the slide, from the last prepaint.
    pub(crate) workspace_bounds: Option<Bounds<Pixels>>,
    /// Scrolling of the workspace when the zoomed slide does not fit.
    pub(crate) workspace_scroll: ScrollHandle,
    /// Zoom chosen in the status bar, 1.0 being 100%; `None` fits the slide
    /// to the workspace.
    pub(crate) zoom: Option<f32>,
    /// The zoom slider of the status bar, in percent.
    pub(crate) zoom_slider: Entity<SliderState>,
    /// Where the status bar was drawn last, in window coordinates: its left
    /// and right edges and the zoom slider's left edge (the sheet tabs'
    /// scrollbar lies between the tabs' left edge and the slider).
    pub(crate) status_layout: std::rc::Rc<std::cell::Cell<[f32; 3]>>,
    /// Where the zoom percentage button of the status bar was painted (for the scenarios to press it).
    pub(crate) zoom_controls_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The menu bar's logo (it opens the start window), where it was last drawn.
    pub(crate) logo_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub(crate) zoom_trigger_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    _zoom_subscription: Subscription,
    /// The status bar's list of zooms is open.
    pub(crate) zoom_menu_open: bool,
    /// 「指定」 of the list of zooms: a zoom typed in percent.
    pub(crate) zoom_input: Entity<InputState>,
    _zoom_input_subscription: Subscription,
    /// Whether the zoom dialog is open.
    pub(crate) zoom_dialog_open: bool,
    /// The input state inside the zoom dialog.
    pub(crate) zoom_dialog_input: Entity<InputState>,
    _zoom_dialog_subscription: Subscription,
    /// EntityId of this SlideEditor, used to identify this window in the Window menu (#111).
    pub(crate) entity_id: gpui::EntityId,
    /// Whether the toolbar font size pulldown is open (#120).
    pub(crate) font_size_menu_open: bool,
    /// The input state inside the toolbar font size pulldown (#120).
    pub(crate) font_size_edit_input:Entity<InputState>,
    pub(crate) font_size_edit_dirty:bool,
    _font_size_edit_subscription:Subscription,
    pub(crate) font_size_input: Entity<InputState>,
    _font_size_input_subscription: Subscription,
    /// Scroll handle for the toolbar font size pulldown candidate list (#127, #149).
    pub(crate) font_size_scroll: ScrollHandle,
    /// Currently keyboard-highlighted candidate index in the toolbar font size pulldown (#127).
    pub(crate) font_size_selected: Option<usize>,
    /// Whether the user is actively filtering the toolbar font size candidate list (#127, #149).
    pub(crate) font_size_filtering: bool,
    /// Search input states for searchable dropdown menus (#98, #103), keyed by button id.
    pub(crate) toolbar_search_inputs: std::collections::HashMap<&'static str, Entity<InputState>>,
    /// Scroll handles for searchable dropdown menus (#125, #127), keyed by button id.
    pub(crate) toolbar_search_scrolls: std::collections::HashMap<&'static str, ScrollHandle>,
    /// Keyboard-highlighted row index for searchable dropdown menus (#127), keyed by button id.
    pub(crate) toolbar_search_selected: std::collections::HashMap<&'static str, Option<usize>>,
    /// Which searchable dropdown menu is currently open (#98, #103).
    pub(crate) toolbar_search_open: Option<&'static str>,
    pub(crate) toolbar_search_popup_items: Option<(&'static str, Vec<crate::shell::chrome::MenuItem>)>,
    /// Scroll handle for the right-pane sidebar body (#151).
    pub(crate) sidebar_scroll: ScrollHandle,
    /// The slide show, while the presentation is shown full screen.
    pub(crate) slideshow: Option<crate::pptx::slideshow::Slideshow>,
    /// Decoded pictures and fonts for the open presentation.
    pub(crate) resources: Resources,
    /// Redraws the editor as pictures decode (`watch_pictures`).
    pub(crate) picture_watch: Option<Task<()>>,
    /// Name of the open file; `None` for a new presentation.
    pub(crate) file_name: Option<SharedString>,
    /// Progress or error of the last file operation, for the status bar.
    pub(crate) status: Option<SharedString>,
    /// What a save left out, to say once the save is done (an old .xls leaves things out).
    pub(crate) save_note: Option<SharedString>,
    /// A chart's workbook (データの編集): the ranges its series take their
    /// data from, framed on the sheet.
    pub(crate) chart_ranges: Option<Vec<ibis_pptx::SeriesRefs>>,
    /// Recently opened files, most recent first, for ファイル > 履歴.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_files: Vec<std::path::PathBuf>,
    /// The password the open file is protected with (パスワードを使用して暗号化):
    /// what is saved is protected again with it.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) protection_password: Option<String>,
    /// The colours picked lately from a palette, the last first (for the palette's row of them).
    pub(crate) recent_colors: Vec<ibis_doc_model::Rgb>,
    /// The colour last picked from each toolbar palette button (by its id), for the split button's left half.
    pub(crate) last_colors: std::collections::HashMap<&'static str, ibis_doc_model::Rgb>,
    /// The menu item last chosen from each split button of the toolbar (not a colour), by the button's id: what its
    /// left half does at once (`ToolButton::last_used`).
    pub(crate) last_choices: std::collections::HashMap<&'static str, String>,
    /// Scrolling of the recent files list of the start view.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_scroll: ScrollHandle,
    /// A file is open; until one is, the window shows the start view.
    pub(crate) has_document: bool,
    /// How many times a file was closed (`close_file`): a file still being
    /// read when the count changes is not opened.
    pub(crate) closed: u32,
    /// An empty document stands in for the file being read (`begin_loading`).
    pub(crate) loading: bool,
    /// What only the open document's format has (see `shell/format.rs`).
    pub(crate) format: FormatState,
    /// The sidebar is asked for (the title row's toggle, a panel opened). Whether it is on screen is `right_pane`.
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_layout: std::rc::Rc<std::cell::Cell<super::chrome::panel::PanelMeasurements>>,
    pub(crate) active_table_reorder: Option<std::rc::Rc<std::cell::RefCell<super::chrome::data_table::RowReorder>>>,
    pub(crate) link_hover: bool,
    pub(crate) link_text_positions: std::cell::RefCell<std::collections::HashMap<(ibis_doc_model::ShapeId,Option<(usize,usize)>),crate::render::rich_text::TextPositions>>,
    pub(crate) link_popup: Option<super::links::LinkPopup>,
    pub(crate) theme_colors_ui: super::theme_colors::ThemeColorsUi,
    pub(crate) panel_dialog: super::panel_dialog::PanelDialogState,
    _panel_dialog_keystrokes: Subscription,
    pub(crate) ruler_settings_open: bool,
    pub(crate) ruler_settings_ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    /// Whether the right pane is on screen and whether its toggle can show it, as `render_sidebar` found them this
    /// frame (it runs before the title row and the menus): the one place their buttons read, so a button never says
    /// "shown" for a pane that is not there.
    pub(crate) right_pane: crate::shell::chrome::RightPane,
    /// 記号と特殊文字 (挿入 > 特殊文字…), while open in the sidebar.
    pub(crate) symbol: Option<crate::shell::symbols::SymbolState>,
    /// A document's outline (概要) is shown.
    pub(crate) outline_open: bool,
    /// A document's ruler (ルーラー) is shown.
    pub(crate) ruler_open: bool,
    /// A document's vertical ruler (垂直ルーラー) is shown (DOCX-VIEW-012).
    pub(crate) vertical_ruler_open: bool,
    /// A document's ruler measurement unit (ルーラーの測定単位, DOCX-VIEW-013).
    pub(crate) ruler_unit: RulerUnit,
    /// A document's gridlines (グリッド線) are shown.
    pub(crate) gridlines_open: bool,
    /// A document's formatting marks (編集記号) are shown.
    pub(crate) formatting_marks_open: bool,
    /// ページ間の余白の表示 (DOCX-VIEW-017). True displays normal page gaps and margins; false collapses page gaps into a thin dividing line.
    pub(crate) show_white_space: bool,
    /// ウィンドウの分割比率 (DOCX-VIEW-019). None = 分割なし, Some(ratio) = 上下分割 (例: 0.5).
    pub(crate) doc_split: Option<f32>,
    /// 分割時の上部ペインの独立スクロールハンドル (DOCX-VIEW-019).
    pub(crate) doc_split_scroll: ScrollHandle,
    /// 分割境界バーのドラッグ中フラグ (DOCX-VIEW-019).
    pub(crate) doc_split_dragging: bool,
    /// 分割時に直近でアクティブになったペイン (DOCX-VIEW-019: true = 上部, false = 下部).
    pub(crate) doc_split_active: bool,
    /// ページの左右移動モード (DOCX-VIEW-008). false = 縦方向スクロール (既定), true = 左右ページ移動 (Side to Side).
    pub(crate) page_movement_side_to_side: bool,
    /// 閲覧モード (DOCX-VIEW-005).
    pub(crate) doc_read_mode: bool,
    /// フォーカス表示モード (DOCX-VIEW-006).
    pub(crate) doc_focus_mode: bool,
    /// 常に画面に表示する編集記号オプション (DOCX-OPTIONS-008).
    pub(crate) formatting_marks_options: FormattingMarksOptions,
    /// 貼付の既定動作 (DOCX-OPTIONS-006).
    pub(crate) default_paste_mode: DefaultPasteMode,
    /// ユーザー名設定 (DOCX-OPTIONS-003).
    pub(crate) custom_user_name: Option<String>,
    /// ユーザー頭文字設定 (DOCX-OPTIONS-003).
    pub(crate) custom_user_initials: Option<String>,
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
    /// 変更履歴の特定の人物フィルタ (DOCX-REVIEW-005). None = すべての校閲者.
    pub(crate) reviewer_filter: Option<std::collections::HashSet<String>>,
    /// The title row and the menu bar are hidden (the toolbar's ^).
    pub(crate) chrome_compact: bool,
    /// Where the toolbar's bar was drawn last (for the scenarios: its left edge is clear of the Mac window's buttons).
    pub(crate) toolbar_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Where the open file came from, for saving it (`shell/save.rs`);
    /// `None` for a new document or one that cannot be saved.
    pub(crate) source: Option<std::sync::Arc<ibis_pptx::Source>>,
    /// スライド マスター表示, while it is open.
    pub(crate) master_view: Option<crate::pptx::master_view::MasterView>,
    /// ノートの書式編集, while it is open.
    pub(crate) notes_format: Option<crate::pptx::notes_format::NotesFormat>,
    /// The file's path, for ⌘S (none in the browser, or for a new document).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) path: Option<std::path::PathBuf>,
    /// A unique identifier for the workbook session, shared across windows viewing the same workbook.
    pub(crate) workbook_id: u64,
    /// A unique identifier for the document session, shared across windows viewing the same document.
    pub(crate) document_id: u64,
    /// Sequence number when multiple windows view the same file (e.g. 1 for Book1:1 / Doc:1, 2 for Book1:2 / Doc:2).
    pub(crate) window_number: Option<usize>,
    /// Whether the save confirmation dialog is open (shown when closing the last window with unsaved edits).
    pub(crate) save_confirm_dialog_open: bool,
    pub(crate) panel_message: Option<SharedString>,
    /// Whether the save confirmation was asked by closing the window (its close button, or quitting the app) rather
    /// than by ファイル > 閉じる: once answered, the window goes instead of showing the start view (GitHub issue #27).
    pub(crate) close_window_after_confirm: bool,
    /// Whether the confirmation dialog asking to specify the Office executable path is open (#26).
    pub(crate) office_path_confirm_dialog_open: bool,
    /// Whether side-by-side comparison mode (並べて比較 / XLSX-VIEW-013) is active.
    pub(crate) side_by_side: bool,
    /// Whether synchronous scrolling (同期スクロール / XLSX-VIEW-014) is active.
    pub(crate) sync_scrolling: bool,
    /// Paired window and editor entity for side-by-side mode.
    pub(crate) side_by_side_peer: Option<(AnyWindowHandle, Entity<SlideEditor>)>,
    /// Guard against recursive scroll syncing between paired windows.
    pub(crate) is_syncing_scroll: std::cell::Cell<bool>,
    /// Last scroll offset synced with peer window.
    pub(crate) last_synced_scroll: std::cell::Cell<Point<Pixels>>,
    /// Whether this document was opened from an AutoRecover snapshot (XLSX-FILE-015).
    pub(crate) is_recovered: bool,
    /// Path of the autorecover backup file associated with this open document (if any).
    pub(crate) active_autorecover_file: Option<std::path::PathBuf>,
    /// 自動回復's panels (the settings, the recovery list), while one is the one shown.
    pub(crate) autorecover_settings: Option<crate::shell::autorecover::AutoRecoverSettingsPanel>,
    pub(crate) recover_unsaved: Option<crate::shell::autorecover::RecoverUnsavedPanel>,
    /// 自動回復 is off for this file.
    pub(crate) autorecover_off: bool,
    /// When (ms) its last snapshot was written (or the timer started).
    pub(crate) last_autorecover_ms: u64,
    pub(crate) autorecover_timer_started: bool,
    /// Whether this document was opened after repairing corruption (XLSX-FILE-020).
    pub(crate) is_repaired: bool,
    /// Summary text of repairs applied (XLSX-FILE-020).
    pub(crate) repair_summary: Option<String>,
    /// Detailed report of repairs applied (XLSX-FILE-020).
    pub(crate) repair_report: Option<ibis_pptx::repair::RepairReport>,
    /// Whether this document is opened in read-only / viewing mode (--read-only or ?view=1).
    pub(crate) read_only: bool,
    /// Whether the menu bar is shown (false by default when ?view=1 unless ?menuBar=1).
    pub(crate) show_menu_bar: bool,
    /// Whether the toolbar is shown (false by default when ?view=1 unless ?toolBar=1).
    pub(crate) show_toolbar: bool,
    /// Whether the editor is currently streaming CSV rows in the background.
    pub(crate) is_csv_streaming: bool,
}

static NEXT_WORKBOOK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static NEXT_DOCUMENT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SlideEditor {
    pub fn new(mut presentation: Presentation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::track_start_window(cx.entity_id(),Some(window.window_handle()),true,cx);
        let owner = window.window_handle();
        let editor = cx.weak_entity();
        let panel_dialog_keystrokes = cx.intercept_keystrokes(move |event, window, app| {
            if window.window_handle() == owner {
                editor.update(app, |this, cx| {
                    if this.panel_message.is_some() && matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                        this.dismiss_panel_message(cx);
                        cx.stop_propagation(); return;
                    }
                    if this.interaction_layer().is_confirmation() { return; }
                    // Reorder capture outlives a format's focus/action context. Cancel it before
                    // DOCX's CancelFormatBrush or a focused input can consume Escape.
                    if event.keystroke.key == "escape" && this.cancel_table_reorder(window, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    if event.keystroke.key=="escape" && this.dismiss_toolbar_dropdown(window,cx) {cx.stop_propagation();return;}
                    if this.validation_list_key(&event.keystroke, cx) { cx.stop_propagation(); return; }
                    if this.sheet_range_pick_key(&event.keystroke, window, cx) { return; }
                    if event.keystroke.key=="escape" && this.panel_dialog.key==Some("sort_level_editor") && this.toolbar_search_open.is_none(){this.close_panel_dialog(window,cx);cx.stop_propagation();return;}
                    if let Some(id)=this.toolbar_search_open.filter(|id|this.toolbar_search_inputs.get(id).is_some_and(|input|input.read(cx).focus_handle(cx).is_focused(window))) {
                        if this.toolbar_dropdown_keystroke(id,&event.keystroke,window,cx) {cx.stop_propagation();return;}
                    }
                    this.modal_viewport_keystroke(&event.keystroke, window, cx);
                }).ok();
            }
        });
        // NOW, TODAY and a date typed without its year read the local time.
        ibis_calc::eval::convert::set_clock(crate::clock::calc_clock);
        let resources = Resources::new(&presentation, cx);
        crate::docx::flow_layout::expand_pages(&mut presentation, &resources, None);
        crate::xlsx::pivot::fit_built_pivots(&mut presentation, &resources, &window.text_system().clone());
        crate::xlsx::row_fit::fit_measured_rows(&mut presentation, &resources);
        let current_slide = presentation.slides[0].id;
        // Logarithmic, so equal distances on the slider are equal zoom ratios.
        let zoom_slider = cx.new(|_| {
            SliderState::new()
                .min(MIN_ZOOM * 100.0)
                .max(MAX_ZOOM * 100.0)
                .scale(SliderScale::Logarithmic)
                .default_value(100.0)
        });
        let zoom_subscription = cx.subscribe_in(&zoom_slider, window, |this, _, event: &SliderEvent, window, cx| {
            if let SliderEvent::Change(value) = event {
                this.set_zoom(Some(value.end() / 100.0), window, cx);
            }
        });
        let format = FormatState::for_model(&presentation, window, cx);
        let zoom_input = cx.new(|cx| InputState::new(window, cx));
        let zoom_input_subscription = cx.subscribe_in(&zoom_input, window, |this, input, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                // A number that cannot be read leaves the zoom as it is.
                if let Some(percent) = parse_percent(&input.read(cx).value()) {
                    this.set_zoom(Some(percent / 100.0), window, cx);
                }
                this.zoom_menu_open = false;
                this.focus_handle.focus(window, cx);
                cx.notify();
            }
        });
        let zoom_dialog_input = cx.new(|cx| InputState::new(window, cx));
        let zoom_dialog_subscription = cx.subscribe_in(&zoom_dialog_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.apply_zoom_dialog(window, cx);
            }
        });
        let font_size_edit_input=cx.new(|cx|InputState::new(window,cx));
        let font_size_edit_subscription=cx.subscribe_in(&font_size_edit_input,window,|e,input,event:&InputEvent,w,cx|match event {
            InputEvent::Change if input.read(cx).focus_handle(cx).is_focused(w)=>{e.font_size_edit_dirty=true;},
            InputEvent::PressEnter{..}=>e.commit_toolbar_font_size(w,cx),
            InputEvent::Blur=>{e.font_size_edit_dirty=false;cx.notify();},
            _=>{}
        });
        let font_size_input = cx.new(|cx| InputState::new(window, cx));
        let font_size_input_subscription = cx.subscribe_in(&font_size_input, window, |this, input, event: &InputEvent, window, cx| {
            match event {
                InputEvent::Change => {
                    this.font_size_filtering = !input.read(cx).value().trim().is_empty();
                    this.font_size_selected = None;
                    this.font_size_scroll.set_offset(Point::default());
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    let query = input.read(cx).value().to_string();
                    let candidates = crate::shell::chrome::filtered_font_size_candidates(&query, this.font_size_filtering);
                    if let Some(idx) = this.font_size_selected
                        && let Some(&pt) = candidates.get(idx)
                    {
                        this.set_toolbar_font_size(pt, window, cx);
                    } else if let Ok(pt) = query.trim().parse::<f32>()
                        && (1.0..=1638.0).contains(&pt)
                    {
                        this.set_toolbar_font_size(pt, window, cx);
                    } else if this.font_size_filtering
                        && let Some(&pt) = candidates.first()
                    {
                        this.set_toolbar_font_size(pt, window, cx);
                    }
                    this.font_size_menu_open = false;
                    this.font_size_filtering = false;
                    this.font_size_selected = None;
                    crate::shell::chrome::set_current_toolbar_open(false, this.toolbar_search_open);
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                }
                _ => {}
            }
        });
        let entity_id = cx.entity_id();
        register_open_editor_window(entity_id, window.window_handle(), cx.entity().downgrade());
        Self {
            focus_handle: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            list_scroll: ScrollHandle::new(),
            wheel: 0.0,
            open_menu: None,
            menu_dismissed_at: None,
            #[cfg(target_family = "wasm")]
            wheel_at: f64::NEG_INFINITY,
            #[cfg(target_os = "windows")]
            wheel_notches: 0.0,
            document: Document::new(presentation),
            document_memory: Default::default(),
            current_slide,
            selection: None,
            tool: Tool::Select,
            drag: None,
            editing: None,
            workspace_bounds: None,
            workspace_scroll: ScrollHandle::new(),
            zoom: None,
            zoom_slider,
            status_layout: Default::default(),
            zoom_trigger_bounds: Default::default(),
            zoom_controls_bounds: Default::default(),
            logo_bounds: Default::default(),
            _zoom_subscription: zoom_subscription,
            zoom_menu_open: false,
            zoom_input,
            _zoom_input_subscription: zoom_input_subscription,
            zoom_dialog_open: false,
            zoom_dialog_input,
            _zoom_dialog_subscription: zoom_dialog_subscription,
            entity_id,
            font_size_menu_open: false,
            font_size_edit_input,font_size_edit_dirty:false,_font_size_edit_subscription:font_size_edit_subscription,
            font_size_input,
            _font_size_input_subscription: font_size_input_subscription,
            font_size_scroll: ScrollHandle::new(),
            font_size_selected: None,
            font_size_filtering: false,
            toolbar_search_inputs: Default::default(),
            toolbar_search_scrolls: Default::default(),
            toolbar_search_selected: Default::default(),
            toolbar_search_open: None,
            toolbar_search_popup_items: None,
            sidebar_scroll: ScrollHandle::new(),
            slideshow: None,
            resources,
            picture_watch: None,
            file_name: None,
            status: None,
            save_note: None,
            chart_ranges: None,
            #[cfg(not(target_family = "wasm"))]
            recent_files: crate::shell::recent_files::load(),
            #[cfg(not(target_family = "wasm"))]
            protection_password: None,
            recent_colors: Vec::new(),
            last_colors: Default::default(),
            last_choices: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            recent_scroll: ScrollHandle::new(),
            has_document: false,
            closed: 0,
            loading: false,
            format,
            sidebar_open: true,
            sidebar_layout:Default::default(),
            active_table_reorder:None,
            link_hover: false,
            link_text_positions: Default::default(),
            link_popup: None,
            theme_colors_ui: Default::default(),
            panel_dialog: Default::default(),
            _panel_dialog_keystrokes: panel_dialog_keystrokes,
            ruler_settings_open: false,
            ruler_settings_ui: Default::default(),
            right_pane: Default::default(),
            symbol: None,
            outline_open: false,
            ruler_open: true,
            vertical_ruler_open: true,
            ruler_unit: RulerUnit::default(),
            gridlines_open: false,
            formatting_marks_open: false,
            show_white_space: true,
            doc_split: None,
            doc_split_scroll: ScrollHandle::new(),
            doc_split_dragging: false,
            doc_split_active: false,
            page_movement_side_to_side: false,
            doc_read_mode: false,
            doc_focus_mode: false,
            formatting_marks_options: FormattingMarksOptions::default(),
            default_paste_mode: DefaultPasteMode::default(),
            custom_user_name: None,
            custom_user_initials: None,
            update_links_before_printing: false,
            print_backgrounds: false,
            print_hidden_text: false,
            print_markup: true,
            show_document_text: true,
            reviewer_filter: None,
            chrome_compact: false,
            toolbar_bounds: Default::default(),
            source: None,
            master_view: None,
            notes_format: None,
            #[cfg(not(target_family = "wasm"))]
            path: None,
            workbook_id: NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            document_id: NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            window_number: None,
            save_confirm_dialog_open: false,
            panel_message: None,
            close_window_after_confirm: false,
            office_path_confirm_dialog_open: false,
            side_by_side: false,
            sync_scrolling: false,
            side_by_side_peer: None,
            is_syncing_scroll: std::cell::Cell::new(false),
            last_synced_scroll: std::cell::Cell::new(Point::default()),
            is_recovered: false,
            autorecover_settings: None,
            recover_unsaved: None,
            autorecover_off: false,
            last_autorecover_ms: 0,
            autorecover_timer_started: false,
            active_autorecover_file: None,
            is_repaired: false,
            repair_summary: None,
            repair_report: None,
            #[cfg(target_arch = "wasm32")]
            read_only: crate::shell::embed::is_view_only(),
            #[cfg(not(target_arch = "wasm32"))]
            read_only: false,
            #[cfg(target_arch = "wasm32")]
            show_menu_bar: crate::shell::embed::show_menu_bar(),
            #[cfg(not(target_arch = "wasm32"))]
            show_menu_bar: true,
            #[cfg(target_arch = "wasm32")]
            show_toolbar: crate::shell::embed::show_tool_bar(),
            #[cfg(not(target_arch = "wasm32"))]
            show_toolbar: true,
            is_csv_streaming: false,
        }
    }

    /// Whether this editor is currently in read-only / viewing mode (either via `--read-only` or during CSV streaming).
    pub(crate) fn is_read_only(&self) -> bool {
        self.read_only || self.is_csv_streaming
    }

    /// Replace the open presentation, dropping undo history and selection.
    pub(crate) fn load(&mut self, mut presentation: Presentation, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.end_slideshow(window, cx);
        self.zoom_menu_open = false;
        self.editing = None;
        self.drag = None;
        self.selection = None;
        self.tool = Tool::Select;
        self.symbol = None;
        let timing = crate::clock::debug_timing::start();
        self.resources = Resources::new(&presentation, cx);
        self.watch_pictures(window, cx);
        crate::clock::debug_timing::report("load: resources", timing);
        let timing = crate::clock::debug_timing::start();
        crate::docx::flow_layout::update_fields_on_open(&mut presentation, &self.resources);
        crate::clock::debug_timing::report("load: update_fields_on_open", timing);
        let timing = crate::clock::debug_timing::start();
        crate::docx::flow_layout::expand_pages(&mut presentation, &self.resources, Some(&name));
        crate::clock::debug_timing::report("load: expand_pages", timing);
        crate::xlsx::pivot::fit_built_pivots(&mut presentation, &self.resources, &window.text_system().clone());
        crate::xlsx::row_fit::fit_measured_rows(&mut presentation, &self.resources);
        // A presentation's date fields show today, as PowerPoint's do.
        if presentation.flow.is_none() && presentation.workbook.is_none() {
            presentation.refresh_dates(&crate::clock::local_now(), &ibis_pptx::script_pieces);
        }
        self.current_slide = presentation.slides[0].id;
        self.format = FormatState::for_model(&presentation, window, cx);
        // A workbook's and a presentation's right pane is there when asked
        // for (a workbook's tabs have no panel to show until one is chosen;
        // either would take the room of the sheet or the slide, which is
        // fitted to the width without it).
        self.sidebar_open = !self.is_read_only() && self.format.kind() == ibis_pptx::Kind::Document;
        let is_reading = presentation.flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Reading);
        let is_outline = presentation.flow.as_ref().is_some_and(|f| f.view_mode == ibis_doc_model::DocumentViewMode::Outline);
        self.document = Document::new(presentation);
        self.document_memory = Default::default();
        self.source = None;
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = None;
            self.protection_password = None;
        }
        self.zoom = with_format!(self, initial_zoom(self));
        self.workspace_scroll.set_offset(Point::default());
        self.sidebar_scroll.set_offset(Point::default());
        self.doc_split = None;
        self.doc_split_scroll.set_offset(Point::default());
        self.doc_split_dragging = false;
        self.doc_split_active = false;
        self.page_movement_side_to_side = false;
        self.doc_read_mode = is_reading;
        if is_outline {
            self.outline_open = true;
        }
        self.doc_focus_mode = false;
        window.set_window_title(&format!("{name} - ibisOffice"));
        self.file_name = Some(name);
        self.has_document = true;
        crate::track_start_window(cx.entity_id(),None,false,cx);
        self.loading = false;
        self.workbook_id = NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.document_id = NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.window_number = None;
        self.save_confirm_dialog_open = false;
        self.office_path_confirm_dialog_open = false;
        self.autorecover_off = false;
        #[cfg(not(target_family = "wasm"))]
        self.start_autorecover_timer(window, cx);
        // A file with macros (.docm, .xlsm, .pptm) opens and saves with
        // them as they are; they never run.
        self.status = self
            .file_name
            .as_ref()
            .and_then(|n| n.rsplit_once('.'))
            .filter(|(_, ext)| ibis_pptx::Kind::has_macros(ext))
            .map(|_| crate::shell::i18n::text("_shell.macros-in-this-file-will-not-run-they-are-preser"));
        // A presentation opens with the keys on the slide list (Home, End,
        // the arrows move between slides without a click first).
        if self.format.kind() == ibis_pptx::Kind::Presentation {
            self.list_focus.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Redraw as the pictures decode, until all are.
    fn watch_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preload = self.resources.preload.clone();
        self.picture_watch = Some(cx.spawn_in(window, async move |this, cx| {
            let mut seen = preload.generation();
            let mut slide_ready = false;
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(PICTURE_POLL_MS)).await;
                let finished = preload.finished();
                let generation = preload.generation();
                let changed = generation != seen;
                seen = generation;
                let alive = this.update(cx, |this, cx| {
                    // The empty document shown while a file loads is not timed.
                    if !slide_ready && !this.loading && this.resources.preload.slide_ready(this.slide()) {
                        slide_ready = true;
                        #[cfg(not(target_family = "wasm"))]
                        crate::timing::mark("slide-pictures");
                    }
                    if changed {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
                if finished {
                    #[cfg(not(target_family = "wasm"))]
                    if slide_ready {
                        crate::timing::mark("all-pictures");
                    }
                    break;
                }
            }
        }));
    }

    /// Ask for a .pptx, .xlsx or .docx file and open it.
    pub(crate) fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: ibis_pptx::Kind::OPENABLE_EXTENSIONS.iter().map(|e| e.to_string()).collect(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.open").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.open_path(path, None, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let (name, data) = match crate::web::pick_file().await {
                Ok(Some(file)) => file,
                Ok(None) => return,
                Err(err) => {
                    this.update(cx, |this, cx| this.set_status(crate::shell::i18n::format("file.pick-failed", &[("error", &err)]), cx)).ok();
                    return;
                }
            };
            let name: SharedString = name.into();
            let closed = this.update(cx, |this, _| this.closed).unwrap_or_default();
            let kind = name.rsplit_once('.').and_then(|(_, e)| ibis_pptx::Kind::from_extension(e));
            this.update_in(cx, |this, window, cx| match kind {
                Some(kind) if !this.has_document => this.begin_loading(kind, name.clone(), window, cx),
                _ => this.set_status(crate::shell::i18n::format("file.opening", &[("name", &name)]), cx),
            })
            .ok();
            // Let the empty document show before reading the file, which
            // holds the page while it runs.
            cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
            let result = ibis_pptx::open(data).map_err(|e| e.to_string());
            this.update_in(cx, |this, window, cx| match result {
                // Closed (ファイル > 閉じる) while it was read.
                _ if this.closed != closed => {}
                Ok(opened) => {
                    let is_ppsx = name.rsplit_once('.').is_some_and(|(_, e)| ibis_pptx::is_slideshow(e));
                    let is_tmpl = name.rsplit_once('.').is_some_and(|(_, e)| ibis_pptx::is_template(e));
                    let doc_name = if is_tmpl {
                        crate::shell::file_actions::new_file_name(ibis_pptx::Kind::of(&opened.presentation))
                    } else {
                        name
                    };
                    this.load(opened.presentation, doc_name, window, cx);
                    this.source = Some(std::sync::Arc::new(opened.source));
                    // A presentation opens without the speaker notes, as a new one does (the title row's notes button shows them).
                    if let Some(state) = this.pptx_mut() {
                        state.notes_hidden = true;
                    }
                    if is_ppsx {
                        this.start_slideshow(window, cx);
                    }
                }
                Err(err) => this.fail_loading(crate::shell::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Open the .pptx, .xlsx or .docx at `path` and put it first in the
    /// recent files, then show slide `slide` (1-based) if given. A window
    /// with no document shows an empty one of the file's kind while it
    /// loads (`begin_loading`).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_path(&mut self, path: std::path::PathBuf, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file(&read_path), slide, window, cx);
    }

    /// Open `path` as `open_path` does, with `read` reading it (on a
    /// background thread; it may wait for a read started earlier).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_read(
        &mut self,
        path: std::path::PathBuf,
        read: impl FnOnce() -> Result<crate::Loaded, String> + Send + 'static,
        slide: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_edit(window, cx);
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        let kind = path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension);
        match kind {
            Some(kind) if !self.has_document => self.begin_loading(kind, name.clone(), window, cx),
            _ => self.set_status(crate::shell::i18n::format("file.opening", &[("name", &name)]), cx),
        }
        let closed = self.closed;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { read() }).await;
            this.update_in(cx, |this, window, cx| match result {
                // Closed (ファイル > 閉じる) while it was read.
                _ if this.closed != closed => {}
                Ok(loaded) => {
                    let is_tmpl = path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
                    if is_tmpl {
                        let tmpl_name = crate::shell::file_actions::new_file_name(ibis_pptx::Kind::of(&loaded.presentation));
                        this.load(loaded.presentation, tmpl_name, window, cx);
                        this.source = Some(loaded.source);
                        this.path = None;
                        this.document.set_file(None);
                    } else if path.extension().is_some_and(|e| ["xls", "ods", "ppt", "pps", "pot", "odp", "otp", "odt", "ott", "doc", "rtf", "txt", "text", "html", "htm", "pdf"].iter().any(|x| e.eq_ignore_ascii_case(x))) {
                        // An old workbook or presentation (or an OpenDocument workbook) is read as a new .xlsx or .pptx: saving asks where, and leaves the old file as it is.
                        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let extension = match ibis_pptx::Kind::of(&loaded.presentation) {
                            ibis_pptx::Kind::Presentation => "pptx",
                            ibis_pptx::Kind::Document => "docx",
                            ibis_pptx::Kind::Workbook => "xlsx",
                        };
                        this.load(loaded.presentation, format!("{stem}.{extension}").into(), window, cx);
                        this.source = Some(loaded.source);
                        this.path = None;
                        this.document.set_file(None);
                    } else {
                        this.load(loaded.presentation, name, window, cx);
                        this.source = Some(loaded.source);
                        this.set_file_path(Some(path.clone()));
                        this.protection_password = loaded.password;
                    }
                    if let Some(rx) = loaded.csv_stream {
                        this.is_csv_streaming = true;
                        this.set_status(crate::shell::i18n::text("editor.csv.loading-first").to_string(), cx);
                        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
                        let format_commas = |n: usize| -> String {
                            let s = n.to_string();
                            let mut out = String::with_capacity(s.len() + s.len() / 3);
                            let bytes = s.as_bytes();
                            let len = bytes.len();
                            for (i, &b) in bytes.iter().enumerate() {
                                if i > 0 && (len - i) % 3 == 0 {
                                    out.push(',');
                                }
                                out.push(b as char);
                            }
                            out
                        };
                        cx.spawn_in(window, async move |this, cx| {
                            loop {
                                let rx_clone = rx.clone();
                                let chunk_res = cx.background_executor().spawn(async move {
                                    rx_clone.lock().unwrap().recv()
                                }).await;
                                let Ok(chunk) = chunk_res else { break; };
                                let is_last = chunk.is_last;
                                let current_rows = chunk.current_rows;
                                let updated = this.update_in(cx, |this, window, cx| {
                                    let slide_id = this.current_slide;
                                    if let Some(slide) = this.document.presentation_mut().slide_mut(slide_id) {
                                        if let Some(shape) = slide.shapes.iter_mut().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Sheet(_))) {
                                            let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { unreachable!() };
                                            let sheet = std::sync::Arc::make_mut(sheet);
                                            if !chunk.cells.is_empty() {
                                                // The rows that came first go in front of the rest, in the rest's own list (it keeps
                                                // room for them): no second list of 2 GB beside it.
                                                let first = std::mem::take(&mut sheet.cells).into_vec();
                                                let mut cells = chunk.cells;
                                                cells.splice(0..0, first);
                                                sheet.cells = cells.into();
                                                sheet.rows.extend(chunk.heights);
                                                let total_h: ibis_doc_model::Emu = sheet.rows.iter().sum();
                                                shape.frame.height = shape.frame.height.max(total_h);
                                                this.sync_workbook_peers(cx);
                                            }
                                        }
                                    }
                                    let formatted = format_commas(current_rows);
                                    if is_last {
                                        this.is_csv_streaming = false;
                                        this.set_status(crate::shell::i18n::format("editor.csv.loaded", &[("rows", &formatted)]), cx);
                                    } else {
                                        this.set_status(crate::shell::i18n::format("editor.csv.loading", &[("rows", &formatted)]), cx);
                                    }
                                    this.clamp_workspace_scroll();
                                    cx.notify();
                                    window.refresh();
                                });
                                if updated.is_err() || is_last {
                                    break;
                                }
                            }
                        })
                        .detach();
                    }
                    if let Some(number) = slide {
                        this.show_slide_number(number, window, cx);
                    }
                    this.recent_files = crate::shell::recent_files::add(&path);
                    crate::timing::document_loaded(window);
                    if path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow) {
                        this.start_slideshow(window, cx);
                    }
                    // A presentation with a password to modify: asked for, or read as it is.
                    if this.is_pptx_modify_locked() {
                        this.ask_modify_password_to_unlock(window, cx);
                    }
                }
                // A protected file: its password is asked for.
                Err(err) if err == crate::NEEDS_PASSWORD || err == crate::WRONG_PASSWORD => {
                    this.fail_loading(String::new(), cx);
                    crate::shell::password_window::ask_to_open(cx.entity().downgrade(), path.clone(), slide, err == crate::WRONG_PASSWORD, window, cx);
                }
                Err(err) => this.fail_loading(crate::shell::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Open protected file `path` with `password` (`open_path` for a file that asked for it).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn open_path_with_password(&mut self, path: std::path::PathBuf, password: String, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file_with(&read_path, Some(&password)), slide, window, cx);
    }

    /// Show an empty document of `kind`, titled `name`, with 「読み込み中...」
    /// over it until the file is loaded (`load`) or cannot be
    /// (`fail_loading`).
    pub(crate) fn begin_loading(&mut self, kind: ibis_pptx::Kind, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.load(ibis_pptx::blank(kind), name, window, cx);
        self.loading = true;
        // The bar of the loading box moves while the file is read.
        cx.spawn_in(window, async move |this, cx| {
            while this.update(cx, |this, cx| { cx.notify(); this.loading }).unwrap_or(false) {
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            }
        })
        .detach();
    }

    /// The file could not be opened: back to the start view if an empty
    /// document stood in for it, with `status` saying why.
    pub(crate) fn fail_loading(&mut self, status: String, cx: &mut Context<Self>) {
        if self.loading {
            self.loading = false;
            self.has_document = false;
        crate::track_start_window(cx.entity_id(),None,true,cx);
        self.document_memory = Default::default();
            self.file_name = None;
        }
        self.set_status(status, cx);
    }

    /// ファイル > 閉じる: close the open file and show the start view
    /// (スタート画面) in the window, as Office does.
    pub(crate) fn close_file(&mut self, _: &CloseFile, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document {
            // The start window closes too (⌘W / Ctrl+W), so the windows can be closed one after
            // another; the app ends with its last window, as with the window's red button. Not in
            // the browser, where the tab is the browser's.
            #[cfg(not(target_family = "wasm"))]
            window.remove_window();
            #[cfg(target_family = "wasm")]
            let _ = window;
            return;
        }
        if self.is_workbook() && self.workbook_id != 0 {
            let my_id = self.workbook_id;
            let my_entity_id = cx.entity_id();
            let other_matching = Self::find_other_matching_windows(my_id, my_entity_id, cx);
            if !other_matching.is_empty() {
                if self.side_by_side {
                    if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                        let _ = peer_entity.update(cx, |peer, cx| {
                            peer.side_by_side = false;
                            peer.sync_scrolling = false;
                            peer.side_by_side_peer = None;
                            cx.notify();
                        });
                    }
                    self.side_by_side = false;
                    self.sync_scrolling = false;
                }
                self.workbook_id = 0;
                self.window_number = None;
                window.remove_window();
                Self::renumber_workbook_windows(my_id, cx);
                return;
            }
        } else if self.is_document() && self.document_id != 0 {
            let my_id = self.document_id;
            let my_entity_id = cx.entity_id();
            let other_matching = Self::find_other_matching_document_windows(my_id, my_entity_id, cx);
            if !other_matching.is_empty() {
                if self.side_by_side {
                    if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                        let _ = peer_entity.update(cx, |peer, cx| {
                            peer.side_by_side = false;
                            peer.sync_scrolling = false;
                            peer.side_by_side_peer = None;
                            cx.notify();
                        });
                    }
                    self.side_by_side = false;
                    self.sync_scrolling = false;
                }
                self.document_id = 0;
                self.window_number = None;
                window.remove_window();
                Self::renumber_document_windows(my_id, cx);
                return;
            }
        }
        // The last window on the file, with edits not saved (any format, a new file too): ask first.
        if self.is_dirty() {
            self.ask_to_save(true, cx);
            return;
        }
        // The window closes, in every format (not the start view in it); the app ends with its last window.
        self.force_close_file(window, cx);
        window.remove_window();
    }

    /// Show 「変更を保存しますか?」; `close_window`: the window goes once it is answered (its close button, quitting),
    /// rather than showing the start view (ファイル > 閉じる).
    pub(crate) fn ask_to_save(&mut self, close_window: bool, cx: &mut Context<Self>) {
        self.close_window_after_confirm = close_window;
        self.save_confirm_dialog_open = true;
        cx.notify();
    }

    /// The window's close button (the red button on a Mac, × elsewhere): whether the window may close now. With edits
    /// not saved in the last window on its file, it stays and asks whether to save them (GitHub issue #27).
    pub(crate) fn window_should_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.save_confirm_dialog_open {
            // Already asking: the answer closes it.
            return false;
        }
        if !self.is_dirty() {
            return true;
        }
        let me = cx.entity_id();
        let shared = (self.is_workbook() && !Self::find_other_matching_windows(self.workbook_id, me, cx).is_empty())
            || (self.is_document() && !Self::find_other_matching_document_windows(self.document_id, me, cx).is_empty());
        if shared {
            // Another window shows the same file, with the edits: only this view goes.
            return true;
        }
        self.ask_to_save(true, cx);
        false
    }

    /// The question answered with 保存 (once saved) or 保存しない: close the file, or the window when the question came
    /// from closing it.
    pub(crate) fn finish_close_after_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        if std::mem::take(&mut self.close_window_after_confirm) {
            self.force_close_file(window, cx);
            window.remove_window();
        } else {
            self.force_close_file(window, cx);
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.has_document
            && (self.document.is_dirty()
                || self.cell_edit().is_some_and(|e| e.mode == crate::xlsx::cell_edit::EditMode::Enter || e.pending.is_some()))
    }

    pub(crate) fn force_close_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.side_by_side {
            if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                let _ = peer_entity.update(cx, |peer, cx| {
                    peer.side_by_side = false;
                    peer.sync_scrolling = false;
                    peer.side_by_side_peer = None;
                    cx.notify();
                });
            }
            self.side_by_side = false;
            self.sync_scrolling = false;
        }
        self.save_confirm_dialog_open = false;
        self.office_path_confirm_dialog_open = false;
        self.end_slideshow(window, cx);
        self.editing = None;
        if self.is_workbook() || self.is_presentation() {
            self.cleanup_active_autorecover_file();
            self.is_recovered = false;
            self.is_repaired = false;
            self.repair_summary = None;
            self.repair_report = None;
        }
        // An empty presentation, as a new window has, with the state of the
        // file reset (`load`) and then the start view.
        self.load(Presentation::new(), SharedString::default(), window, cx);
        self.list_scroll.set_offset(Point::default());
        self.has_document = false;
        crate::track_start_window(cx.entity_id(),None,true,cx);
        self.document_memory = Default::default();
        self.closed += 1;
        self.file_name = None;
        self.workbook_id = 0;
        self.document_id = 0;
        self.window_number = None;
        window.set_window_title(&crate::shell::start_view::start_title());
        // The file just closed is first, as other windows may have opened
        // files since this one's list was read.
        #[cfg(not(target_family = "wasm"))]
        {
            self.recent_files = crate::shell::recent_files::load();
            self.recent_scroll.set_offset(Point::default());
        }
    }

    /// 「読み込み中...」in the middle of the workspace while the file loads.
    pub(crate) fn render_loading(&self, cx: &Context<Self>) -> Option<Div> {
        if !self.loading {
            return None;
        }
        let theme = cx.theme();
        // How far a large file has got (only the reading that reports it, such as a big workbook's): a bar along the bottom edge of the box.
        let progress = ibis_pptx::progress::current();
        Some(
            div().absolute().inset_0().flex().items_center().justify_center().child(
                div()
                    .relative()
                    .overflow_hidden()
                    .min_w(px(220.))
                    .px_4()
                    .py_2()
                    .rounded_md()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_md()
                    .text_color(theme.muted_foreground)
                    .child(crate::shell::i18n::text("_shell.loading"))
                    .when(progress > 0.0, |this| {
                        this.child(div().absolute().left_0().bottom_0().w_full().h(px(3.)).bg(theme.border))
                            .child(div().absolute().left_0().bottom_0().w(relative(progress.min(1.0))).h(px(3.)).bg(theme.primary))
                    }),
            ),
        )
    }

    pub(crate) fn set_status(&mut self, status: String, cx: &mut Context<Self>) {
        self.status = Some(status.into());
        cx.notify();
    }

    // ----- Reading state -------------------------------------------------

    pub(crate) fn presentation(&self) -> &Presentation {
        self.document.presentation()
    }

    pub(crate) fn slide(&self) -> &Slide {
        self.presentation()
            .slide(self.current_slide)
            .unwrap_or(&self.presentation().slides[0])
    }

    pub(crate) fn selected_shape(&self) -> Option<&Shape> {
        self.selection.and_then(|id| self.slide().shape(id))
    }

    /// The worksheet of the current page and the shape that holds it.
    pub(crate) fn current_sheet(&self) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
        sheet_of(self.slide())
    }

    /// The worksheet of the current slide, mutable.
    #[allow(dead_code)]
    pub(crate) fn current_sheet_mut(&mut self) -> Option<&mut Sheet> {
        let slide = self.current_slide;
        ibis_doc_model::sheet_command::sheet_mut(self.document.presentation_mut(), slide).ok()
    }

    /// The selected cell of the current worksheet: (column, row) of the cell
    /// or of the top-left cell of its merged range.
    pub(crate) fn current_cell(&self) -> Option<(u32, u32)> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.sheet_state()?.cells.get(&self.current_slide).copied().unwrap_or((0, 0));
        Some(sheet.merge_at(row, column).map_or((column, row), |m| (m.first_column, m.first_row)))
    }

    /// The cells the selection covers: from the selected cell to the other
    /// corner of the range, grown to take in whole merged ranges.
    pub(crate) fn selected_range(&self) -> Option<CellRange> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        let (end_column, end_row) = self.sheet_state()?.cell_ends.get(&self.current_slide).copied().unwrap_or((column, row));
        let mut range = CellRange {
            first_row: row.min(end_row),
            first_column: column.min(end_column),
            last_row: row.max(end_row),
            last_column: column.max(end_column),
        };
        // Merged ranges it cuts into are taken in whole, until none is left.
        loop {
            let grown = sheet.merges.iter().fold(range, |r, m| {
                let meets = m.first_row <= r.last_row && m.last_row >= r.first_row && m.first_column <= r.last_column && m.last_column >= r.first_column;
                if !meets {
                    return r;
                }
                CellRange {
                    first_row: r.first_row.min(m.first_row),
                    first_column: r.first_column.min(m.first_column),
                    last_row: r.last_row.max(m.last_row),
                    last_column: r.last_column.max(m.last_column),
                }
            });
            if grown == range {
                return Some(range);
            }
            range = grown;
        }
    }

    /// The scrolled content of the workspace at the current zoom, as the
    /// open document's format lays it out (`FormatEditor::workspace_content`).
    pub(crate) fn workspace_content(&self) -> Option<WorkspaceContent> {
        let ws = self.workspace_bounds?;
        Some(with_format!(self, workspace_content(self, ws.size)))
    }

    /// A page `page` big in a workspace `view` big: fitted into it (less the
    /// space `before` and `after` it on each axis, in pixels), or at the
    /// chosen zoom; in the top-left corner after `before` (`top_left`), or
    /// centred while it fits.
    pub(crate) fn page_content(&self, view: Size<Pixels>, page: ibis_doc_model::EmuSize, before: Point<f32>, after: Point<f32>, top_left: bool) -> WorkspaceContent {
        let (view_w, view_h) = (f32::from(view.width), f32::from(view.height));
        let scale = match self.zoom {
            Some(zoom) => zoom * self.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32,
            None => {
                let avail_w = (view_w - before.x - after.x).max(40.0);
                let avail_h = (view_h - before.y - after.y).max(40.0);
                (avail_w / page.width.max(1) as f32).min(avail_h / page.height.max(1) as f32)
            }
        };
        let slide_w = page.width as f32 * scale;
        let slide_h = page.height as f32 * scale;
        let content_w = view_w.max(slide_w + before.x + after.x);
        let content_h = view_h.max(slide_h + before.y + after.y);
        let slide_origin = if top_left {
            point(px(before.x), px(before.y))
        } else {
            point(px((content_w - slide_w) / 2.0), px((content_h - slide_h) / 2.0))
        };
        WorkspaceContent { size: size(px(content_w), px(content_h)), slide_origin, scale }
    }

    /// The margin kept around a slide: little when it is fitted.
    pub(crate) fn slide_margin(&self) -> f32 {
        if self.zoom.is_none() { FIT_MARGIN } else { WORKSPACE_MARGIN }
    }

    /// Where the slide sits in the window, as the workspace is scrolled.
    pub(crate) fn slide_layout(&self) -> Option<SlideLayout> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let origin = ws.origin + self.workspace_scroll.offset() + content.slide_origin;
        Some(SlideLayout { origin, scale: content.scale })
    }

    /// Pixels per inch at 100%: a slide's, or a worksheet's (the screen's size
    /// only: a PDF and a print keep the real size).
    pub(crate) fn px_per_inch_at_100(&self) -> f32 {
        if self.is_workbook() { SHEET_PX_PER_INCH_AT_100 } else { PX_PER_INCH_AT_100 }
    }

    /// The zoom shown, 1.0 being 100%.
    pub(crate) fn current_zoom(&self) -> f32 {
        self.zoom.or_else(|| self.slide_layout().map(|l| l.zoom(self.px_per_inch_at_100()))).unwrap_or(1.0)
    }

    /// Zoom to `zoom` (clamped to the range), or fit the slide with `None`.
    /// The point of the slide in the middle of the workspace stays there.
    pub(crate) fn set_zoom(&mut self, zoom: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom_at(zoom, None, window, cx);
    }

    /// Zoom as `set_zoom` does, keeping the point of the slide at `anchor`
    /// (from the workspace's top-left corner) where it is, or the one in the
    /// middle of the workspace with `None`.
    pub(crate) fn set_zoom_at(&mut self, zoom: Option<f32>, anchor: Option<Point<Pixels>>, _: &mut Window, cx: &mut Context<Self>) {
        let zoom = zoom.map(|z| z.clamp(MIN_ZOOM, MAX_ZOOM));
        let before = self.workspace_content();
        self.zoom = zoom;
        let offset = match (zoom, before, self.workspace_content(), self.workspace_bounds) {
            (Some(_), Some(before), Some(after), Some(ws)) => {
                let old = self.workspace_scroll.offset();
                let anchor = anchor.unwrap_or(point(ws.size.width / 2.0, ws.size.height / 2.0));
                let ratio = after.scale / before.scale;
                let axis = |anchor: Pixels, view: Pixels, old: Pixels, before_origin: Pixels, after_origin: Pixels, content: Pixels| {
                    let f = f32::from;
                    px(zoom_axis_offset(f(anchor), f(view), f(old), f(before_origin), f(after_origin), ratio, f(content)))
                };
                let y = with_format!(self, zoom_scroll_y(self, &before, &after, old.y, ws.size.height, anchor.y))
                    .unwrap_or_else(|| axis(anchor.y, ws.size.height, old.y, before.slide_origin.y, after.slide_origin.y, after.size.height));
                point(axis(anchor.x, ws.size.width, old.x, before.slide_origin.x, after.slide_origin.x, after.size.width), y)
            }
            // Fitting a document keeps its place: only the sideways
            // scrolling goes, as the pages then fit across.
            (None, Some(before), Some(after), Some(ws)) => {
                let old = self.workspace_scroll.offset().y;
                match with_format!(self, zoom_scroll_y(self, &before, &after, old, ws.size.height, ws.size.height / 2.0)) {
                    Some(y) => point(px(0.0), y),
                    None => Point::default(),
                }
            }
            _ => Point::default(),
        };
        self.workspace_scroll.set_offset(offset);
        if self.sheet_state().is_some() {
            if let Some((_, sheet)) = self.current_sheet() {
                let percent = zoom.map(|z| (z * 100.0).round() as u16).filter(|&z| z != 100);
                if sheet.zoom != percent {
                    let slide = self.current_slide;
                    let _ = self.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetZoom { slide, zoom: percent })]);
                }
            }
        }
        cx.notify();
    }

    /// Excel: Zoom to selection (表示 > 選択範囲に合わせて拡大/縮小).
    /// Fits the currently selected cell range into the workspace.
    pub(crate) fn zoom_to_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some((_, sheet)), Some(range)) = (self.current_sheet(), self.selected_range()) else { return };
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();
        let first_col = range.first_column as usize;
        let last_col = (range.last_column + 1) as usize;
        let first_row = range.first_row as usize;
        let last_row = (range.last_row + 1) as usize;
        let width_emu = if last_col < xs.len() && first_col < xs.len() {
            xs[last_col] - xs[first_col]
        } else if first_col < xs.len() {
            *xs.last().unwrap_or(&0) - xs[first_col]
        } else {
            0
        };
        let height_emu = if last_row < ys.len() && first_row < ys.len() {
            ys[last_row] - ys[first_row]
        } else if first_row < ys.len() {
            *ys.last().unwrap_or(&0) - ys[first_row]
        } else {
            0
        };
        if width_emu <= 0 || height_emu <= 0 {
            return;
        }

        // Available area inside workspace (excluding row/column headers and margins)
        let avail_w = self.workspace_bounds.map_or(800.0, |ws| f32::from(ws.size.width) - 60.0).max(100.0);
        let avail_h = self.workspace_bounds.map_or(600.0, |ws| f32::from(ws.size.height) - 50.0).max(100.0);
        let scale_100 = self.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32;
        let px_at_100_w = width_emu as f32 * scale_100;
        let px_at_100_h = height_emu as f32 * scale_100;
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / px_at_100_h;
        let zoom = zoom_w.min(zoom_h).clamp(MIN_ZOOM, MAX_ZOOM);

        self.set_zoom(Some(zoom), window, cx);
        self.scroll_range_into_view(true);
        cx.notify();
    }

    /// The wheel over the workspace with the zoom modifier held (see
    /// `wheel_zooms`) zooms about the pointer: in with the wheel turned up
    /// (away from you), a notch by `WHEEL_ZOOM_STEP`, a trackpad smoothly.
    /// Whether the wheel was taken for zooming.
    pub(crate) fn on_wheel_zoom(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.panel_dialog.active && self.panel_dialog.bounds.is_some_and(|b| b.contains(&event.position)) { return false; }
        if !wheel_zooms(&event.modifiers, zoom_modifier_is_mac()) || !self.has_document {
            return false;
        }
        let Some(ws) = self.workspace_bounds else { return false };
        if !ws.contains(&event.position) { return false; }
        let factor = match event.delta {
            // A notch at a time: some systems send several lines for one.
            ScrollDelta::Lines(delta) if delta.y > 0.0 => WHEEL_ZOOM_STEP,
            ScrollDelta::Lines(delta) if delta.y < 0.0 => 1.0 / WHEEL_ZOOM_STEP,
            ScrollDelta::Lines(_) => 1.0,
            ScrollDelta::Pixels(delta) => 2f32.powf(f32::from(delta.y) / WHEEL_ZOOM_DOUBLING),
        };
        if factor != 1.0 {
            let anchor = event.position - ws.origin;
            self.set_zoom_at(Some(self.current_zoom() * factor), Some(anchor), window, cx);
        }
        true
    }

    /// A pinch gesture on a trackpad over the workspace zooms about the pointer:
    /// in (pinch out, fingers moving apart) increases zoom, out (pinch in) decreases zoom.
    /// Returns whether the pinch event was handled.
    pub(crate) fn on_pinch_zoom(&mut self, event: &PinchEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.has_document {
            return false;
        }
        let Some(ws) = self.workspace_bounds else { return false };
        if !ws.contains(&event.position) { return false; }
        let factor = 1.0 + event.delta;
        if factor > 0.0 && factor != 1.0 {
            let anchor = event.position - ws.origin;
            self.set_zoom_at(Some(self.current_zoom() * factor), Some(anchor), window, cx);
        }
        true
    }


    /// Zoom in (`grow`) or out by one step of the status bar's − / + buttons.
    pub(crate) fn step_zoom(&mut self, grow: bool, window: &mut Window, cx: &mut Context<Self>) {
        let zoom = self.current_zoom();
        let zoom = if grow { zoom * ZOOM_STEP } else { zoom / ZOOM_STEP };
        self.set_zoom(Some(zoom), window, cx);
    }

    /// Open the Zoom dialog.
    pub(crate) fn open_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = (self.current_zoom() * 100.0).round() as u32;
        self.zoom_dialog_input.update(cx, |input, cx| input.set_value(percent.to_string(), window, cx));
        self.zoom_dialog_open = true;
        self.zoom_dialog_input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Close the Zoom dialog.
    pub(crate) fn close_zoom_dialog(&mut self, cx: &mut Context<Self>) {
        self.zoom_dialog_open = false;
        cx.notify();
    }

    /// Apply the zoom specified in the zoom dialog.
    pub(crate) fn apply_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.zoom_dialog_input.read(cx).value().to_string();
        if let Some(percent) = parse_percent(&text) {
            let clamped = (percent / 100.0).clamp(MIN_ZOOM, MAX_ZOOM);
            self.set_zoom(Some(clamped), window, cx);
        }
        self.zoom_dialog_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Set zoom to fit to window from the zoom dialog.
    pub(crate) fn fit_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(None, window, cx);
        self.zoom_dialog_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Render the Zoom modal dialog.
    pub(crate) fn render_zoom_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let input = self.zoom_dialog_input.clone();
        let current_val = self.zoom_dialog_input.read(cx).value().to_string();
        let is_fit = self.zoom.is_none();

        let presets = [400, 200, 100, 66, 50, 33];

        let preset_buttons = presets.into_iter().map(|pct| {
            let selected = !is_fit && current_val.trim() == pct.to_string();
            div()
                .id(("zoom-preset", pct as usize))
                .px_3()
                .py_1p5()
                .rounded_sm()
                .border_1()
                .border_color(if selected { theme.primary } else { theme.border })
                .bg(if selected { theme.accent } else { theme.background })
                .cursor_pointer()
                .text_sm()
                .text_color(if selected { theme.primary } else { theme.foreground })
                .child(format!("{pct}%"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.zoom_dialog_input.update(cx, |inp, cx| inp.set_value(pct.to_string(), window, cx));
                    cx.notify();
                }))
        });

        super::panel_dialog::confirmation_surface("zoom-dialog-backdrop")
            .child(
                v_flex()
                    .id("zoom-dialog")
                    .w(px(380.))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .shadow_xl()
                    .p_5()
                    .gap_4()
                    // 1. ヘッダー
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().font_weight(FontWeight::BOLD).text_base().child(crate::shell::i18n::text("zoom.dialog.title")))
                            .child(
                                div()
                                    .id("zoom-dialog-close")
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .hover(|s| s.text_color(theme.foreground))
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| this.close_zoom_dialog(cx))),
                            ),
                    )
                    // 2. プリセット
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child(crate::shell::i18n::text("zoom.dialog.presets")),
                            )
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .children(preset_buttons)
                                    .child(
                                        div()
                                            .id("zoom-preset-fit")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(if is_fit { theme.primary } else { theme.border })
                                            .bg(if is_fit { theme.accent } else { theme.background })
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(if is_fit { theme.primary } else { theme.foreground })
                                            .child(crate::shell::i18n::text("zoom.dialog.fit"))
                                            .on_click(cx.listener(|this, _, window, cx| this.fit_zoom_dialog(window, cx))),
                                    ),
                            ),
                    )
                    // 3. 倍率入力
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_sm().child(crate::shell::i18n::text("zoom.dialog.percent")))
                            .child(Input::new(&input).w(px(80.)))
                            .child(div().text_sm().child("%")),
                    )
                    // 4. ボタン群
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                crate::shell::chrome::action_button("zoom-dialog-cancel")
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_zoom_dialog(cx))),
                            )
                            .child(
                                crate::shell::chrome::primary_button("zoom-dialog-ok")
                                    .label(crate::shell::i18n::text("OK"))
                                    .on_click(cx.listener(|this, _, window, cx| this.apply_zoom_dialog(window, cx))),
                            ),
                    ),
            )
    }

    /// Open a new window duplicating current presentation or open path.
    pub(crate) fn new_window(&mut self, cx: &mut App) {
        let document_memory=self.document_memory.clone();
        if self.is_workbook() {
            let wb_id = if self.workbook_id == 0 {
                let id = NEXT_WORKBOOK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.workbook_id = id;
                id
            } else {
                self.workbook_id
            };
            let pres = self.presentation().clone();
            let doc = self.document.clone();
            let name = self.file_name.clone().unwrap_or_else(|| self.workbook_display_name().into());
            let path = self.file_path();
            let source = self.source.clone();
            let current = self.current_slide;
            let current_zoom = self.zoom;
            let existing_count = Self::count_matching_windows(wb_id, cx) + 1;
            if self.window_number.is_none() {
                self.window_number = Some(1);
            }
            let new_num = existing_count + 1;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name.clone(), window, cx);
                editor.document = doc;
                editor.document_memory=document_memory.clone();
                editor.set_file_path(path);
                editor.source = source;
                editor.workbook_id = wb_id;
                editor.window_number = Some(new_num);
                editor.current_slide = current;
                editor.zoom = current_zoom;
                let title = format!("{name}:{new_num}");
                window.set_window_title(&format!("{title} - ibisOffice"));
            });
        } else if self.is_document() {
            let doc_id = if self.document_id == 0 {
                let id = NEXT_DOCUMENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.document_id = id;
                id
            } else {
                self.document_id
            };
            let pres = self.presentation().clone();
            let doc = self.document.clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::shell::i18n::text("_shell.document-1-docx").into());
            let path = self.file_path();
            let source = self.source.clone();
            let current = self.current_slide;
            let current_zoom = self.zoom;
            let existing_count = Self::count_matching_document_windows(doc_id, cx) + 1;
            if self.window_number.is_none() {
                self.window_number = Some(1);
            }
            let new_num = existing_count + 1;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name.clone(), window, cx);
                editor.document = doc;
                editor.document_memory=document_memory.clone();
                editor.set_file_path(path);
                editor.source = source;
                editor.document_id = doc_id;
                editor.window_number = Some(new_num);
                editor.current_slide = current;
                editor.zoom = current_zoom;
                editor.relayout_if_changed();
                let title = format!("{name}:{new_num}");
                window.set_window_title(&format!("{title} - ibisOffice"));
            });
        } else if let Some(path) = self.file_path() {
            let slide = self.presentation().slide_index(self.current_slide).map(|i| i + 1);
            crate::open_window_with(cx, move |editor, window, cx| {
                #[cfg(not(target_family = "wasm"))]
                editor.open_path(path, slide, window, cx);
                #[cfg(target_family = "wasm")]
                let _ = (editor, path, slide, window, cx);
            });
        } else {
            let pres = self.presentation().clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::shell::i18n::text("file.new.presentation").into());
            let current = self.current_slide;
            crate::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name, window, cx);
                editor.current_slide = current;
            });
        }
    }

    /// The file's path (always `None` in the browser).
    pub(crate) fn file_path(&self) -> Option<std::path::PathBuf> {
        #[cfg(not(target_family = "wasm"))]
        {
            self.path.clone()
        }
        #[cfg(target_family = "wasm")]
        {
            None
        }
    }

    /// Remember the file's path (nothing in the browser).
    pub(crate) fn set_file_path(&mut self, path: Option<std::path::PathBuf>) {
        #[cfg(not(target_family = "wasm"))]
        {
            self.document.set_file(path.as_ref().map(|p| p.to_string_lossy().into_owned()));
            self.path = path;
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = path;
        }
    }

    pub(crate) fn find_other_matching_windows(
        workbook_id: u64,
        my_entity_id: gpui::EntityId,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if workbook_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id
                            && entity.read(app).is_workbook()
                            && entity.read(app).workbook_id == workbook_id
                        {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_other_matching_document_windows(
        document_id: u64,
        my_entity_id: gpui::EntityId,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if document_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id
                            && entity.read(app).is_document()
                            && entity.read(app).document_id == document_id
                        {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_matching_windows(workbook_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if workbook_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.read(app).is_workbook() && entity.read(app).workbook_id == workbook_id {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn find_matching_document_windows(document_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if document_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.read(app).is_document() && entity.read(app).document_id == document_id {
                            return Some(entity);
                        }
                    }
                    None
                }) {
                    matching.push((w, entity));
                }
            }
        }
        matching
    }

    pub(crate) fn count_matching_windows(workbook_id: u64, cx: &mut App) -> usize {
        Self::find_matching_windows(workbook_id, cx).len()
    }

    pub(crate) fn count_matching_document_windows(document_id: u64, cx: &mut App) -> usize {
        Self::find_matching_document_windows(document_id, cx).len()
    }

    pub(crate) fn renumber_workbook_windows(workbook_id: u64, cx: &mut App) {
        if workbook_id == 0 {
            return;
        }
        let matching = Self::find_matching_windows(workbook_id, cx);
        let count = matching.len();
        for (i, (w, editor_entity)) in matching.into_iter().enumerate() {
            let num = if count > 1 { Some(i + 1) } else { None };
            let _ = w.update(cx, |_, window, app| {
                editor_entity.update(app, |editor, cx| {
                    editor.window_number = num;
                    let title = editor.display_title_formatted();
                    window.set_window_title(&format!("{title} - ibisOffice"));
                    cx.notify();
                    window.refresh();
                });
            });
        }
    }

    pub(crate) fn renumber_document_windows(document_id: u64, cx: &mut App) {
        if document_id == 0 {
            return;
        }
        let matching = Self::find_matching_document_windows(document_id, cx);
        let count = matching.len();
        for (i, (w, editor_entity)) in matching.into_iter().enumerate() {
            let num = if count > 1 { Some(i + 1) } else { None };
            let _ = w.update(cx, |_, window, app| {
                editor_entity.update(app, |editor, cx| {
                    editor.window_number = num;
                    let title = editor.display_title_formatted();
                    window.set_window_title(&format!("{title} - ibisOffice"));
                    cx.notify();
                    window.refresh();
                });
            });
        }
    }

    pub(crate) fn display_title_formatted(&self) -> String {
        let base = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| {
            if self.is_workbook() {
                self.workbook_display_name()
            } else if self.is_document() {
                crate::shell::i18n::text("_shell.document-1-docx").to_string()
            } else {
                crate::shell::i18n::text("_shell.presentation").to_string()
            }
        });
        if self.is_workbook() || self.is_document() {
            if let Some(num) = self.window_number {
                return format!("{base}:{num}");
            }
        }
        base
    }

    pub(crate) fn sync_workbook_peers(&self, cx: &mut Context<Self>) {
        if !self.is_workbook() || self.workbook_id == 0 {
            return;
        }
        let my_id = self.workbook_id;
        let my_entity_id = cx.entity_id();
        let doc = self.document.clone();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(root_ref) = root.read(cx) {
                    if let Ok(peer_entity) = root_ref.view().clone().downcast::<SlideEditor>() {
                        if peer_entity.entity_id() == my_entity_id {
                            continue;
                        }
                        let _ = w.update(cx, |_, window, app| {
                            peer_entity.update(app, |peer, cx| {
                                if peer.is_workbook() && peer.workbook_id == my_id {
                                    peer.document = doc.clone();
                                    peer.fix_selection();
                                    peer.sync_sheet_zoom();
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn sync_document_peers(&self, cx: &mut Context<Self>) {
        if !self.is_document() || self.document_id == 0 {
            return;
        }
        let my_id = self.document_id;
        let my_entity_id = cx.entity_id();
        let doc = self.document.clone();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(root_ref) = root.read(cx) {
                    if let Ok(peer_entity) = root_ref.view().clone().downcast::<SlideEditor>() {
                        if peer_entity.entity_id() == my_entity_id {
                            continue;
                        }
                        let _ = w.update(cx, |_, window, app| {
                            peer_entity.update(app, |peer, cx| {
                                if peer.is_document() && peer.document_id == my_id {
                                    peer.document = doc.clone();
                                    peer.sync_document_view_mode();
                                    peer.relayout_if_changed();
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn find_other_document_windows(
        my_entity_id: gpui::EntityId,
        document_id: u64,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        let mut same_doc = Vec::new();
        let mut diff_doc = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some((entity, is_same))) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id && entity.read(app).is_document() {
                            let is_same = document_id != 0 && entity.read(app).document_id == document_id;
                            return Some((entity, is_same));
                        }
                    }
                    None
                }) {
                    if is_same {
                        same_doc.push((w, entity));
                    } else {
                        diff_doc.push((w, entity));
                    }
                }
            }
        }
        if !same_doc.is_empty() {
            same_doc
        } else {
            diff_doc
        }
    }

    pub(crate) fn find_other_workbook_windows(
        my_entity_id: gpui::EntityId,
        workbook_id: u64,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        let mut same_wb = Vec::new();
        let mut diff_wb = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some((entity, is_same))) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id && entity.read(app).is_workbook() {
                            let is_same = workbook_id != 0 && entity.read(app).workbook_id == workbook_id;
                            return Some((entity, is_same));
                        }
                    }
                    None
                }) {
                    if is_same {
                        same_wb.push((w, entity));
                    } else {
                        diff_wb.push((w, entity));
                    }
                }
            }
        }
        if !same_wb.is_empty() {
            same_wb
        } else {
            diff_wb
        }
    }

    pub(crate) fn toggle_view_side_by_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.side_by_side {
            self.side_by_side = false;
            self.sync_scrolling = false;
            if let Some((_, peer_entity)) = self.side_by_side_peer.take() {
                let _ = peer_entity.update(cx, |peer, cx| {
                    peer.side_by_side = false;
                    peer.sync_scrolling = false;
                    peer.side_by_side_peer = None;
                    cx.notify();
                });
            }
            cx.notify();
            return;
        }

        let my_id = cx.entity_id();
        let other = if self.is_document() {
            Self::find_other_document_windows(my_id, self.document_id, cx)
        } else {
            Self::find_other_workbook_windows(my_id, self.workbook_id, cx)
        };
        let Some((peer_window_handle, peer_entity)) = other.first().cloned() else {
            return;
        };

        let my_handle = window.window_handle();
        let my_entity = cx.entity();
        self.side_by_side = true;
        self.sync_scrolling = true;
        self.side_by_side_peer = Some((peer_window_handle, peer_entity.clone()));
        self.last_synced_scroll.set(self.workspace_scroll.offset());

        let _ = peer_entity.update(cx, |peer, cx| {
            peer.side_by_side = true;
            peer.sync_scrolling = true;
            peer.side_by_side_peer = Some((my_handle, my_entity));
            peer.last_synced_scroll.set(peer.workspace_scroll.offset());
            cx.notify();
        });

        Self::arrange_side_by_side_windows(self.is_document(), window, peer_window_handle, cx);
        cx.notify();
    }

    pub(crate) fn arrange_side_by_side_windows(
        is_document: bool,
        window: &mut Window,
        peer_handle: AnyWindowHandle,
        cx: &mut App,
    ) {
        let (screen_w, screen_h) = cx
            .primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            })
            .unwrap_or((1280.0, 800.0));
        let (tile_w, tile_h) = if is_document {
            ((screen_w / 2.0).max(400.0), screen_h)
        } else {
            (screen_w, (screen_h / 2.0).max(200.0))
        };
        window.resize(size(px(tile_w), px(tile_h)));
        window.bounds_changed(cx);
        let _ = peer_handle.update(cx, |_, w, cx| {
            w.resize(size(px(tile_w), px(tile_h)));
            w.bounds_changed(cx);
        });
    }

    pub(crate) fn reset_side_by_side_positions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.side_by_side {
            return;
        }
        let peer_opt = self.side_by_side_peer.clone();
        if let Some((peer_handle, _)) = peer_opt {
            if !cx.windows().contains(&peer_handle) {
                self.side_by_side = false;
                self.sync_scrolling = false;
                self.side_by_side_peer = None;
                return;
            }
            Self::arrange_side_by_side_windows(self.is_document(), window, peer_handle, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_sync_scrolling(&mut self, cx: &mut Context<Self>) {
        if !self.side_by_side {
            return;
        }
        self.sync_scrolling = !self.sync_scrolling;
        let new_val = self.sync_scrolling;
        self.last_synced_scroll.set(self.workspace_scroll.offset());
        let peer_opt = self.side_by_side_peer.clone();
        if let Some((peer_handle, peer_entity)) = peer_opt {
            if !cx.windows().contains(&peer_handle) {
                self.side_by_side = false;
                self.sync_scrolling = false;
                self.side_by_side_peer = None;
                return;
            }
            let _ = peer_entity.update(cx, |peer, cx| {
                peer.sync_scrolling = new_val;
                peer.last_synced_scroll.set(peer.workspace_scroll.offset());
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(crate) fn sync_scroll_to_peer(&mut self, cx: &mut Context<Self>) {
        if !self.sync_scrolling || self.is_syncing_scroll.get() {
            return;
        }
        let peer_opt = self.side_by_side_peer.clone();
        let Some((peer_handle, peer_entity)) = peer_opt else {
            self.side_by_side = false;
            self.sync_scrolling = false;
            return;
        };
        if !cx.windows().contains(&peer_handle) {
            self.side_by_side = false;
            self.sync_scrolling = false;
            self.side_by_side_peer = None;
            return;
        }
        let cur_offset = self.workspace_scroll.offset();
        let last_offset = self.last_synced_scroll.get();
        let delta = point(cur_offset.x - last_offset.x, cur_offset.y - last_offset.y);
        if delta.x == px(0.0) && delta.y == px(0.0) {
            return;
        }
        self.last_synced_scroll.set(cur_offset);
        let _ = peer_entity.update(cx, |peer, cx| {
            if peer.is_syncing_scroll.get() {
                return;
            }
            peer.is_syncing_scroll.set(true);
            let peer_cur = peer.workspace_scroll.offset();
            let peer_new = point(peer_cur.x + delta.x, peer_cur.y + delta.y);
            peer.workspace_scroll.set_offset(peer_new);
            peer.clamp_workspace_scroll();
            peer.last_synced_scroll.set(peer.workspace_scroll.offset());
            peer.is_syncing_scroll.set(false);
            cx.notify();
        });
    }

    pub(crate) fn scroll_workspace_by(&mut self, delta: Point<Pixels>, cx: &mut Context<Self>) {
        let cur = self.workspace_scroll.offset();
        self.workspace_scroll.set_offset(point(cur.x + delta.x, cur.y + delta.y));
        self.clamp_workspace_scroll();
        if (self.is_workbook() || self.is_document()) && self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_save_confirm_dialog(&mut self, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        self.close_window_after_confirm = false;
        cx.notify();
    }

    pub(crate) fn has_file_on_disk(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.path.as_ref().is_some_and(|p| p.exists())
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    pub(crate) fn office_app_name(&self) -> &'static str {
        match self.kind() {
            ibis_pptx::Kind::Document => "Word",
            ibis_pptx::Kind::Workbook => "Excel",
            ibis_pptx::Kind::Presentation => "PowerPoint",
        }
    }

    pub(crate) fn office_setting_key(&self) -> &'static str {
        match self.kind() {
            ibis_pptx::Kind::Document => "office_path.word",
            ibis_pptx::Kind::Workbook => "office_path.excel",
            ibis_pptx::Kind::Presentation => "office_path.powerpoint",
        }
    }

    pub(crate) fn detect_office_app_path(&self) -> Option<std::path::PathBuf> {
        let key = self.office_setting_key();
        if let Some(saved) = crate::shell::settings::get(key) {
            let p = std::path::PathBuf::from(&saved);
            if p.exists() {
                return Some(p);
            }
        }
        #[cfg(target_os = "macos")]
        {
            let bundle_name = match self.kind() {
                ibis_pptx::Kind::Document => "Microsoft Word.app",
                ibis_pptx::Kind::Workbook => "Microsoft Excel.app",
                ibis_pptx::Kind::Presentation => "Microsoft PowerPoint.app",
            };
            let mut candidates = vec![std::path::PathBuf::from("/Applications").join(bundle_name)];
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(std::path::PathBuf::from(home).join("Applications").join(bundle_name));
            }
            for cand in candidates {
                if cand.exists() {
                    if let Some(s) = cand.to_str() {
                        crate::shell::settings::set(key, s);
                    }
                    return Some(cand);
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            let exe_name = match self.kind() {
                ibis_pptx::Kind::Document => "WINWORD.EXE",
                ibis_pptx::Kind::Workbook => "EXCEL.EXE",
                ibis_pptx::Kind::Presentation => "POWERPNT.EXE",
            };
            let mut roots = vec![
                std::path::PathBuf::from(r"C:\Program Files"),
                std::path::PathBuf::from(r"C:\Program Files (x86)"),
            ];
            for env_var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                if let Some(val) = std::env::var_os(env_var) {
                    let p = std::path::PathBuf::from(val);
                    if !roots.contains(&p) {
                        roots.push(p);
                    }
                }
            }
            for root in roots {
                for sub in [r"Microsoft Office\root\Office16", r"Microsoft Office\Office16", r"Microsoft Office\Office15"] {
                    let cand = root.join(sub).join(exe_name);
                    if cand.exists() {
                        if let Some(s) = cand.to_str() {
                            crate::shell::settings::set(key, s);
                        }
                        return Some(cand);
                    }
                }
            }
        }
        None
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn launch_office_app(&self, app_path: &std::path::Path) {
        #[cfg(target_os = "macos")]
        {
            let is_bundle = app_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"));
            let mut cmd = if is_bundle {
                let mut c = std::process::Command::new("open");
                c.arg("-a").arg(app_path);
                c
            } else {
                std::process::Command::new(app_path)
            };
            if let Some(doc_path) = &self.path {
                cmd.arg(doc_path);
            }
            let _ = cmd.spawn();
        }
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new(app_path);
            if let Some(doc_path) = &self.path {
                cmd.arg(doc_path);
            }
            let _ = cmd.spawn();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = app_path;
        }
    }

    pub(crate) fn open_in_office_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_file_on_disk() {
            return;
        }
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            if let Some(app_path) = self.detect_office_app_path() {
                self.launch_office_app(&app_path);
            } else {
                self.office_path_confirm_dialog_open = true;
                cx.notify();
            }
        }
    }

    pub(crate) fn close_office_path_confirm_dialog(&mut self, cx: &mut Context<Self>) {
        self.office_path_confirm_dialog_open = false;
        cx.notify();
    }

    pub(crate) fn confirm_office_path_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.office_path_confirm_dialog_open = false;
        cx.notify();
        #[cfg(not(target_family = "wasm"))]
        {
            let key = self.office_setting_key();
            #[cfg(target_os = "macos")]
            let extensions = vec!["app".to_string()];
            #[cfg(target_os = "windows")]
            let extensions = vec!["exe".to_string()];
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let extensions: Vec<String> = Vec::new();
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions,
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.open").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, _, _| {
                    if let Some(s) = path.to_str() {
                        crate::shell::settings::set(key, s);
                    }
                    this.launch_office_app(&path);
                })
                .ok();
            })
            .detach();
        }
    }

    pub(crate) fn render_office_path_confirm_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let title = crate::shell::i18n::text("window.open-in-office.confirm-title");
        let message = crate::shell::i18n::format("window.open-in-office.specify-path", &[("app", &self.office_app_name())]);

        super::panel_dialog::confirmation_surface("office-path-confirm-dialog-backdrop")
            .child(
                v_flex()
                    .id("office-path-confirm-dialog")
                    .w(px(380.))
                    .p_6()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .gap_4()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(message),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                crate::shell::chrome::action_button("office-path-confirm-cancel")
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_office_path_confirm_dialog(cx))),
                            )
                            .child(
                                crate::shell::chrome::primary_button("office-path-confirm-ok")
                                    .label("OK")
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_office_path_dialog(window, cx))),
                            ),
                    ),
            )
    }

    pub(crate) fn confirm_save_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        // Closed only once the file is written: a new file asks where first, and a cancelled or failed save keeps it open.
        self.save_then(window, cx, |this, window, cx| this.finish_close_after_confirm(window, cx));
    }

    pub(crate) fn confirm_discard_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preserve_or_cleanup_autorecover_on_discard();
        self.finish_close_after_confirm(window, cx);
    }

    pub(crate) fn render_save_confirm_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let name = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| {
            if self.is_workbook() {
                self.workbook_display_name()
            } else if self.is_document() {
                crate::shell::i18n::text("_shell.document-1-docx").to_string()
            } else {
                crate::shell::i18n::text("_shell.presentation").to_string()
            }
        });
        let message = crate::shell::i18n::text("fix.editor.do-you-want-save-changes");

        super::panel_dialog::confirmation_surface("save-confirm-dialog-backdrop")
            .child(
                v_flex()
                    .id("save-confirm-dialog")
                    .w(px(380.))
                    .p_6()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .gap_4()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(message),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                crate::shell::chrome::action_button("save-confirm-cancel")
                                    .label(crate::shell::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_save_confirm_dialog(cx))),
                            )
                            .child(
                                crate::shell::chrome::action_button("save-confirm-discard")
                                    .label(crate::shell::i18n::text("fix.editor.don-t-save"))
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_discard_and_close(window, cx))),
                            )
                            .child(
                                crate::shell::chrome::primary_button("save-confirm-save")
                                    .label(crate::shell::i18n::text("_shell.save"))
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_save_and_close(window, cx))),
                            ),
                    ),
            )
    }

    /// Arrange all open windows across the screen evenly.
    pub(crate) fn arrange_all_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(
            crate::xlsx::arrange_windows::ArrangeKind::Tiled,
            false,
            current,
            None,
            String::new(),
            cx,
        );
    }

    /// Cascade all open windows.
    pub(crate) fn cascade_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(
            crate::xlsx::arrange_windows::ArrangeKind::Cascade,
            false,
            current,
            None,
            String::new(),
            cx,
        );
    }

    /// Word: ウィンドウの分割状態 (DOCX-VIEW-019).
    pub(crate) fn is_document_split(&self) -> bool {
        self.doc_split.is_some()
    }

    /// Word: ウィンドウの分割を切り替える (表示 > ウィンドウ > 分割 / 分割の解除, ⌥⌘S, DOCX-VIEW-019).
    pub(crate) fn toggle_document_split(&mut self, cx: &mut Context<Self>) {
        if self.doc_split.is_some() {
            self.doc_split = None;
            self.doc_split_dragging = false;
            self.doc_split_active = false;
        } else {
            self.doc_split = Some(0.5);
            self.doc_split_scroll.set_offset(self.workspace_scroll.offset());
            self.doc_split_active = false;
        }
        cx.notify();
    }

    /// Word: ウィンドウの分割比率を設定する (DOCX-VIEW-019).
    pub(crate) fn set_document_split(&mut self, split: Option<f32>, cx: &mut Context<Self>) {
        self.doc_split = split;
        if split.is_some() {
            self.doc_split_scroll.set_offset(self.workspace_scroll.offset());
        } else {
            self.doc_split_dragging = false;
            self.doc_split_active = false;
        }
        cx.notify();
    }

    /// Word: ページの左右移動モードかどうか (DOCX-VIEW-008).
    pub(crate) fn is_page_movement_side_to_side(&self) -> bool {
        self.page_movement_side_to_side
    }

    /// Word: ページの移動方向（縦／左右）を設定する (DOCX-VIEW-008).
    pub(crate) fn set_page_movement_side_to_side(&mut self, side_to_side: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.page_movement_side_to_side == side_to_side {
            return;
        }
        let visible = self.visible_page();
        self.page_movement_side_to_side = side_to_side;
        self.workspace_scroll.set_offset(Point::default());
        self.doc_split_scroll.set_offset(Point::default());
        self.scroll_to_page(visible, cx);
        cx.notify();
        window.refresh();
    }

    /// Word: ページの移動方向（縦／左右）を切り替える (DOCX-VIEW-008).
    #[allow(dead_code)]
    pub(crate) fn toggle_page_movement_side_to_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.page_movement_side_to_side;
        self.set_page_movement_side_to_side(next, window, cx);
    }

    /// Word: 文書のビューモードを取得する (DOCX-VIEW-001..005).
    pub(crate) fn document_view_mode(&self) -> ibis_doc_model::DocumentViewMode {
        if let Some(flow) = self.presentation().flow.as_ref() {
            flow.view_mode
        } else if self.doc_read_mode {
            ibis_doc_model::DocumentViewMode::Reading
        } else {
            ibis_doc_model::DocumentViewMode::Print
        }
    }

    /// Word: 文書のビューモードを設定する (DOCX-VIEW-001..005).
    pub(crate) fn set_document_view_mode(&mut self, mode: ibis_doc_model::DocumentViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.doc_read_mode = mode == ibis_doc_model::DocumentViewMode::Reading;
        if mode == ibis_doc_model::DocumentViewMode::Outline {
            self.outline_open = true;
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetViewMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-005).
    pub(crate) fn is_read_mode(&self) -> bool {
        self.doc_read_mode
    }

    /// Word: 閲覧モードを設定する (DOCX-VIEW-005).
    pub(crate) fn set_read_mode(&mut self, read: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if read { ibis_doc_model::DocumentViewMode::Reading } else { ibis_doc_model::DocumentViewMode::Print };
        self.set_document_view_mode(mode, window, cx);
        if !read && self.flow().is_some_and(|d| d.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing) {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Editing)], cx);
        }
    }

    /// Word: 閲覧モードを切り替える (DOCX-VIEW-005).
    #[allow(dead_code)]
    pub(crate) fn toggle_read_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.doc_read_mode;
        self.set_read_mode(next, window, cx);
    }

    /// Word: 閲覧・レビュー・編集モードを取得する (DOCX-VIEW-021).
    pub(crate) fn interaction_mode(&self) -> ibis_doc_model::DocumentInteractionMode {
        if let Some(doc) = self.flow() {
            if self.is_read_only() || self.is_read_mode() || doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing || self.is_protection_read_only() {
                ibis_doc_model::DocumentInteractionMode::Viewing
            } else if doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Reviewing || doc.track_changes {
                ibis_doc_model::DocumentInteractionMode::Reviewing
            } else {
                ibis_doc_model::DocumentInteractionMode::Editing
            }
        } else {
            ibis_doc_model::DocumentInteractionMode::Editing
        }
    }

    /// Word: 編集モードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_editing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Editing
    }

    /// Word: レビューモードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_reviewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Reviewing
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_viewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Viewing
    }

    /// Word: 閲覧・レビュー・編集モードを設定する (DOCX-VIEW-021).
    pub(crate) fn set_interaction_mode(&mut self, mode: ibis_doc_model::DocumentInteractionMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.interaction_mode() == mode {
            return;
        }
        if mode == ibis_doc_model::DocumentInteractionMode::Editing && self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("docx.interaction.locked-warning").into());
            cx.notify();
            return;
        }
        if mode != ibis_doc_model::DocumentInteractionMode::Viewing && self.is_read_mode() {
            self.doc_read_mode = false;
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 印刷レイアウト表示かどうか (DOCX-VIEW-001).
    pub(crate) fn is_print_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Print
    }

    /// Word: Web レイアウト表示かどうか (DOCX-VIEW-002).
    pub(crate) fn is_web_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Web
    }

    /// Word: 下書き表示かどうか (DOCX-VIEW-003).
    pub(crate) fn is_draft_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Draft
    }

    /// Word: アウトライン表示かどうか (DOCX-VIEW-004).
    pub(crate) fn is_outline_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Outline
    }

    /// Word: フォーカス表示モードかどうか (DOCX-VIEW-006).
    pub(crate) fn is_focus_mode(&self) -> bool {
        self.doc_focus_mode
    }

    /// Word: フォーカス表示モードを設定する (DOCX-VIEW-006).
    pub(crate) fn set_focus_mode(&mut self, focus: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_focus_mode == focus {
            return;
        }
        self.doc_focus_mode = focus;
        cx.notify();
    }

    /// Word: フォーカス表示モードを切り替える (DOCX-VIEW-006).
    pub(crate) fn toggle_focus_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.doc_focus_mode;
        self.set_focus_mode(next, window, cx);
    }

    /// Keep the status bar's slider on the zoom shown, which changes with
    /// the window size while the slide is fitted.
    fn sync_zoom_slider(&self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = (self.current_zoom() * 100.0).clamp(MIN_ZOOM * 100.0, MAX_ZOOM * 100.0);
        // Setting the value notifies; only a real change may, or render would repeat.
        if (self.zoom_slider.read(cx).value().end() - percent).abs() > 0.01 {
            self.zoom_slider.update(cx, |slider, cx| slider.set_value(percent, window, cx));
        }
    }

    /// Whether the slide is taller than the workspace, so it scrolls vertically.
    fn slide_overflows_vertically(&self) -> bool {
        match (self.workspace_content(), self.workspace_bounds) {
            (Some(content), Some(ws)) => content.size.height > ws.size.height + px(0.5),
            _ => false,
        }
    }

    /// Keep the workspace's scrolling within the current content.
    pub(crate) fn clamp_workspace_scroll(&self) {
        let (Some(content), Some(ws)) = (self.workspace_content(), self.workspace_bounds) else { return };
        let offset = self.workspace_scroll.offset();
        let clamp = |v: Pixels, view: Pixels, content: Pixels| {
            px(f32::from(v).clamp(f32::from(view - content).min(0.0), 0.0))
        };
        self.workspace_scroll.set_offset(point(
            clamp(offset.x, ws.size.width, content.size.width),
            clamp(offset.y, ws.size.height, content.size.height),
        ));
    }

    /// The frame to draw for `shape`, including an in-progress drag.
    pub(crate) fn display_frame(&self, shape: &Shape) -> EmuRect {
        match (self.drag, self.slide_layout()) {
            (Some(drag @ (Drag::Move { shape: id, .. } | Drag::Resize { shape: id, .. })), Some(layout))
                if id == shape.id =>
            {
                self.display_frame_for(shape, drag, layout)
            }
            (Some(Drag::Move { shape: id, start, current, origin }), Some(layout))
                if self.selected_ids().len() > 1
                    && self.selected_ids().contains(&id)
                    && self.selected_ids().contains(&shape.id) =>
            {
                let moved_primary = self.display_frame_for(
                    &Shape { id, frame: origin, ..shape.clone() },
                    Drag::Move { shape: id, start, current, origin },
                    layout,
                );
                let (dx, dy) = (moved_primary.x - origin.x, moved_primary.y - origin.y);
                EmuRect::new(
                    (shape.frame.x + dx).max(0),
                    (shape.frame.y + dy).max(0),
                    shape.frame.width,
                    shape.frame.height,
                )
            }
            _ => shape.frame,
        }
    }

    /// The rubber band of a text box being drawn.
    pub(crate) fn creation_rect(&self) -> Option<EmuRect> {
        let Some(Drag::Create { start, current }) = self.drag else { return None };
        let layout = self.slide_layout()?;
        Some(EmuRect::from_corners(layout.to_emu(start), layout.to_emu(current)))
    }

    // ----- Document edits ------------------------------------------------

    pub(crate) fn execute(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        let is_admin = commands.iter().all(|c| matches!(c, Command::SetMarkAsFinal { .. } | Command::SetReadOnlyRecommended { .. } | Command::SetModifyVerifier { .. }));
        if !is_admin && self.pages_are_slides() {
            if self.is_pptx_mark_as_final() {
                log::warn!("edit rejected: presentation is marked as final");
                return;
            }
            if self.is_pptx_read_only_blocked() {
                log::warn!("edit rejected: presentation is read-only (recommended, or locked by a password)");
                return;
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("edit rejected: {err}");
        }
        self.fix_selection();
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    /// Drop references to shapes or slides that an undo or redo removed.
    fn fix_selection(&mut self) {
        if self.presentation().slide(self.current_slide).is_none() {
            let from_analysis = self
                .sheet_state()
                .and_then(|s| s.panels.analysis.as_ref())
                .map(|p| p.slide)
                .filter(|&id| self.presentation().slide(id).is_some());
            self.current_slide = from_analysis
                .or_else(|| {
                    self.presentation()
                        .workbook
                        .as_ref()
                        .and_then(|w| w.sheets.iter().find_map(|s| s.slide))
                })
                .unwrap_or(self.presentation().slides[0].id);
        }
        if let Some(id) = self.selection {
            if self.slide().shape(id).is_none() {
                self.selection = None;
            }
        }
    }

    /// 表示 > カラー/白黒 (ColorMode: Color, Grayscale, HighContrast)
    pub(crate) fn color_mode(&self) -> crate::render::ColorMode {
        self.resources.color_mode
    }

    pub(crate) fn set_color_mode(&mut self, mode: crate::render::ColorMode, cx: &mut Context<Self>) {
        if self.resources.color_mode != mode {
            self.resources.color_mode = mode;
            cx.notify();
        }
    }

    pub(crate) fn toggle_high_contrast(&mut self, cx: &mut Context<Self>) {
        let next = if self.resources.color_mode == crate::render::ColorMode::HighContrast {
            crate::render::ColorMode::Color
        } else {
            crate::render::ColorMode::HighContrast
        };
        self.set_color_mode(next, cx);
    }

    pub(crate) fn is_high_contrast(&self) -> bool {
        self.resources.color_mode == crate::render::ColorMode::HighContrast
    }

    pub(crate) fn is_grayscale(&self) -> bool {
        self.resources.color_mode == crate::render::ColorMode::Grayscale
    }

    pub(crate) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return;
        }
        self.commit_edit(window, cx);
        let (order, was) = (self.presentation().slides.iter().map(|s| s.id).collect::<Vec<_>>(), self.current_slide);
        self.document.undo();
        // Undoing the creation of the sheet being shown leaves the sheet beside it (the one before, else after), as Excel does.
        if self.presentation().slide(was).is_none() {
            if let Some(at) = order.iter().position(|&id| id == was) {
                let near = order[..at].iter().rev().chain(order[at + 1..].iter()).find(|&&id| self.presentation().slide(id).is_some());
                if let Some(&id) = near {
                    self.current_slide = id;
                }
            }
        }
        self.refresh_chart_source_field_after_history(window,cx);
        self.sync_sheet_zoom();
        self.sync_document_view_mode();
        self.fix_selection();
        self.sheet_spelling_refresh(cx);
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    pub(crate) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return;
        }
        self.commit_edit(window, cx);
        self.document.redo();
        self.refresh_chart_source_field_after_history(window,cx);
        self.sync_sheet_zoom();
        self.sync_document_view_mode();
        self.fix_selection();
        self.sheet_spelling_refresh(cx);
        self.sync_workbook_peers(cx);
        self.sync_document_peers(cx);
        cx.notify();
    }

    fn sync_document_view_mode(&mut self) {
        if self.is_document() {
            if let Some(view_mode) = self.flow().map(|f| f.view_mode) {
                self.doc_read_mode = view_mode == ibis_doc_model::DocumentViewMode::Reading;
                if view_mode == ibis_doc_model::DocumentViewMode::Outline && !self.outline_open {
                    self.outline_open = true;
                }
            }
        }
    }

    fn sync_sheet_zoom(&mut self) {
        if self.sheet_state().is_some() {
            if let Some((_, sheet)) = self.current_sheet() {
                let sheet_zoom_val = sheet.zoom.map(|z| z as f32 / 100.0).unwrap_or(1.0);
                self.zoom = Some(sheet_zoom_val);
            }
        }
    }

    pub(crate) fn delete_selection(
        &mut self,
        _: &DeleteSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.slideshow.is_some() {
            return;
        }
        if self.text_editing().is_some() {
            self.delete_text(false, cx);
            return;
        }
        if let Some(shape) = self.selection.take() {
            // A worksheet's grid or a document's page is the page itself,
            // not a shape to remove.
            if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Sheet(_) | ShapeKind::Page { .. })) {
                return;
            }
            self.execute(vec![Command::RemoveShape { slide: self.current_slide, shape }], cx);
        }
    }

    pub(crate) fn cancel_tool(&mut self, _: &CancelTool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_range_pick_active(){self.finish_sheet_range_pick(true,window,cx);return;}
        if self.cancel_table_reorder(window,cx){return;}
        if let Some(show) = self.slideshow.as_mut() {
            if let Some(rehearsal) = show.rehearsal.as_mut() {
                if rehearsal.dialog_open {
                    return self.discard_rehearsal_timings(window, cx);
                } else {
                    let elapsed_ms = (rehearsal.slide_accumulated + rehearsal.slide_started.elapsed()).as_millis().max(1000) as u32;
                    rehearsal.recorded_timings.insert(self.current_slide, elapsed_ms);
                    rehearsal.dialog_open = true;
                    let total = rehearsal.total_accumulated + rehearsal.total_started.elapsed();
                    rehearsal.final_total = Some(total);
                    cx.notify();
                    return;
                }
            }
            if show.grid_open {
                show.grid_open = false;
                cx.notify();
                return;
            }
            if show.zoom > 1.001 || show.pan != (0.0, 0.0) {
                self.reset_slideshow_zoom(cx);
                return;
            }
            if show.has_inks() {
                if show.ink_dialog_open {
                    return self.end_slideshow_with_ink_choice(false, window, cx);
                } else {
                    show.ink_dialog_open = true;
                    cx.notify();
                    return;
                }
            }
            return self.end_slideshow(window, cx);
        }
        if self.zoom_dialog_open {
            self.close_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.slide_zoom_dialog.is_some()) {
            self.close_slide_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.section_zoom_dialog.is_some()) {
            self.close_section_zoom_dialog(cx);
            return;
        }
        if self.pptx().is_some_and(|p| p.summary_zoom_dialog.is_some()) {
            self.close_summary_zoom_dialog(cx);
            return;
        }
        if self.cancel_cell_jump(window, cx) {
            return;
        }
        if self.is_grid_view() {
            self.close_grid_view(cx);
            return;
        }
        if self.is_notes_page_view() {
            self.switch_to_normal_view(window, cx);
            return;
        }
        if self.text_editing().is_some() {
            self.end_text_edit(window, cx);
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selection = None;
        }
        cx.notify();
    }

    pub(crate) fn set_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        // A document's pages are laid out from its text; no text box is put on them.
        let tool = if self.allows_shapes() { tool } else { Tool::Select };
        self.tool = tool;
        if tool == Tool::TextBox {
            self.selection = None;
        }
        cx.notify();
    }

    pub(crate) fn add_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() {
            return;
        }
        if self.is_pptx_mark_as_final() || self.is_pptx_read_only_blocked() {
            log::warn!("add_slide rejected: presentation is read-only");
            return;
        }
        self.commit_edit(window, cx);
        self.end_text_edit(window, cx);
        let index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        let id = self.document.ids().new_slide_id();
        let layout = self.slide().layout;
        let slide = self.new_slide(index, id, layout);
        self.execute(vec![Command::InsertSlide { index, slide }], cx);
        self.current_slide = id;
        self.selection = None;
        self.set_selection(Vec::new());
    }

    /// A new slide `id` to put at `index`, following `layout` (else the
    /// current slide's): with the layout's placeholders empty, as the file
    /// reads them, when there is a file; else without them.
    pub(crate) fn new_slide(&mut self, index: usize, id: ibis_doc_model::SlideId, layout: Option<ibis_doc_model::LayoutId>) -> ibis_doc_model::Slide {
        if let (Some(source), Some(layout)) = (self.source.clone(), layout) {
            let current = self.presentation().clone();
            match source.new_slide(&current, index, id, layout, || self.document.ids().new_shape_id()) {
                Ok(slide) => return slide,
                Err(err) => log::warn!("Cannot make the placeholder of the new slide: {err}"),
            }
        }
        match layout.and_then(|l| self.presentation().layout(l)) {
            Some(layout) => ibis_doc_model::Slide::from_layout(id, layout),
            None => self.presentation().new_slide_like(id, self.current_slide),
        }
    }

    pub(crate) fn delete_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() {
            return;
        }
        self.commit_edit(window, cx);
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        // Several slides selected in the list (not all of them): all taken away, as one edit.
        let chosen = self.selected_slide_ids();
        if chosen.len() > 1 && chosen.len() < self.presentation().slides.len() {
            let first = chosen.iter().filter_map(|id| self.presentation().slide_index(*id)).min().unwrap_or(index);
            self.execute(vec![Command::Group(chosen.into_iter().map(|slide| Command::RemoveSlide { slide }).collect())], cx);
            let slides = &self.presentation().slides;
            self.current_slide = slides[first.min(slides.len() - 1)].id;
            self.selection = None;
            if let Some(s) = self.pptx_mut() {
                s.slide_selection.clear();
            }
            return;
        }
        self.execute(vec![Command::RemoveSlide { slide: self.current_slide }], cx);
        let slides = &self.presentation().slides;
        self.current_slide = slides[index.min(slides.len() - 1)].id;
        self.selection = None;
    }

    fn step_slide(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        // PageDown / PageUp in a document scroll to the next or previous page,
        // in a worksheet by a screen (Control+PageDown changes sheets).
        if with_format!(self, step_page(self, forward, window, cx)) {
            return;
        }
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let next = if forward { (index + 1).min(slides.len() - 1) } else { index.saturating_sub(1) };
        // Past the first or last slide nothing changes, and nothing is drawn
        // again (keys held down there would keep the window busy).
        if next == index {
            return;
        }
        let id = slides[next].id;
        self.select_slide(id, window, cx);
    }

    /// Show the next or previous worksheet.
    pub(crate) fn step_sheet(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let next = if forward { (index + 1).min(slides.len() - 1) } else { index.saturating_sub(1) };
        // Past the first or last slide nothing changes, and nothing is drawn
        // again (keys held down there would keep the window busy).
        if next == index {
            return;
        }
        let id = slides[next].id;
        self.select_slide(id, window, cx);
    }

    /// Show slide `number` (1-based), clamped to the presentation.
    pub(crate) fn show_slide_number(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let id = slides[number.clamp(1, slides.len()) - 1].id;
        self.select_slide(id, window, cx);
    }

    pub(crate) fn next_slide(&mut self, _: &NextSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.step_slideshow(true, window, cx);
        }
        self.step_slide(true, window, cx);
    }

    pub(crate) fn previous_slide(&mut self, _: &PreviousSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.step_slideshow(false, window, cx);
        }
        self.step_slide(false, window, cx);
    }

    /// The last slide (`last`) or the first, in the editor or the slide show.
    fn end_slide(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() {
            return self.slideshow_to_end(last, cx);
        }
        if self.editing.is_some() || !self.pages_are_slides() {
            return cx.propagate();
        }
        let count = self.presentation().slides.len();
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let target = if last { count - 1 } else { 0 };
        if target != index {
            self.show_slide_number(target + 1, window, cx);
        }
    }

    pub(crate) fn select_slide(&mut self, slide: SlideId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.presentation().slide_index(slide) {
            // Its pictures before those still waiting.
            self.resources.preload.prioritize(&self.presentation().slides[index]);
        }
        if let Some(s) = self.pptx_mut() {
            s.slide_selection.clear();
            s.slide_popup = None;
        }
        let list_focused = self.list_focus.is_focused(window);
        self.commit_edit(window, cx);
        if list_focused {
            // Committing an edit focuses the workspace; the list keeps the keys.
            self.list_focus.focus(window, cx);
        }
        if with_format!(self, show_page(self, slide, window, cx)) {
            return cx.notify();
        }
        if self.current_slide != slide {
            // Slides may differ in size: the scrolling is kept only as far
            // as it goes.
            let offset = self.workspace_scroll.offset();
            self.current_slide = slide;
            self.workspace_scroll.set_offset(offset);
            self.clamp_workspace_scroll();
        }
        self.selection = None;
        if let Some(index) = self.presentation().slide_index(slide) {
            let row = self.row_of_slide(index);
            self.list_scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// The mouse wheel over the workspace turns pages: a notch of a wheel
    /// one page, a trackpad every `WHEEL_PAGE` pixels. When the zoomed slide
    /// is taller than the workspace, the wheel scrolls it instead (the
    /// workspace's own scrolling) and turns no pages.
    ///
    /// Browsers report a notch in pixels, like a trackpad, with no gesture
    /// phase, and some send only a few pixels for it. There the first pixel
    /// scroll after a pause turns a page at once; the rest of a quick scroll
    /// (a spun wheel, a trackpad swipe and its momentum) counts pixels.
    pub(crate) fn on_workspace_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_wheel_zoom(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // Over the Draw panel's ruler, the wheel turns the ruler.
        if self.pptx_ink_wheel(event, cx) {
            cx.stop_propagation();
            return;
        }
        // A worksheet scrolls like Excel's; its pages change with the tabs.
        // A document's pages scroll by, as in Word.
        if self.split_panes().is_some() && !self.page_layout_shown() && self.scroll_split_pane(event, false, false, cx) {
            cx.stop_propagation();
            return;
        }
        if !self.pages_are_slides() || self.slide_overflows_vertically() {
            self.wheel = 0.0;
            return;
        }
        let steps = self.wheel_pages(event);
        if steps != 0 {
            for _ in 0..steps.unsigned_abs() {
                self.step_slide(steps > 0, window, cx);
            }
            cx.stop_propagation();
        }
    }

    /// The pages the wheel turns with `event`, forward if positive: scrolling
    /// down (content moving up) shows the next page. See `on_workspace_scroll`.
    pub(crate) fn wheel_pages(&mut self, event: &ScrollWheelEvent) -> i32 {
        match event.delta {
            ScrollDelta::Lines(delta) => {
                self.wheel = 0.0;
                self.wheel_line_pages(-delta.y)
            }
            ScrollDelta::Pixels(delta) => {
                let y = f32::from(delta.y);
                #[cfg(target_family = "wasm")]
                {
                    let now = js_sys::Date::now();
                    let paused = now - self.wheel_at > WHEEL_PAUSE_MS;
                    self.wheel_at = now;
                    // Mostly sideways scrolling does not start a page turn.
                    if paused && y.abs() > f32::from(delta.x).abs() {
                        self.wheel = 0.0;
                        return if y < 0.0 { 1 } else { -1 };
                    }
                }
                // A change of direction starts over.
                if y * self.wheel < 0.0 {
                    self.wheel = 0.0;
                }
                self.wheel += y;
                let pages = (self.wheel / WHEEL_PAGE).trunc();
                self.wheel -= pages * WHEEL_PAGE;
                -(pages as i32)
            }
        }
    }

    /// The pages `lines` of a wheel turn (down if positive): a line one page,
    /// with nothing held over.
    #[cfg(not(target_os = "windows"))]
    fn wheel_line_pages(&mut self, lines: f32) -> i32 {
        let pages = lines.round() as i32;
        if pages == 0 && lines != 0.0 { if lines > 0.0 { 1 } else { -1 } } else { pages }
    }

    /// The pages `lines` of a wheel turn (down if positive): a notch one page.
    /// Windows reports a notch as the lines the system scrolls for it (3 by
    /// default), and a high-resolution wheel parts of a notch, which add up.
    #[cfg(target_os = "windows")]
    fn wheel_line_pages(&mut self, lines: f32) -> i32 {
        let notches = lines / wheel_scroll_lines();
        // A change of direction starts over.
        if notches * self.wheel_notches < 0.0 {
            self.wheel_notches = 0.0;
        }
        self.wheel_notches += notches;
        let pages = self.wheel_notches.trunc();
        self.wheel_notches -= pages;
        pages as i32
    }

    /// Change the text style of the selected text box.
    pub(crate) fn update_style(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut TextStyle),
    ) {
        self.commit_edit(window, cx);
        let Some(shape) = self.selected_shape() else { return };
        let Some(body) = shape.text_body() else { return };
        let mut style = body.style.clone();
        update(&mut style);
        if style != body.style {
            let command = Command::SetTextStyle { slide: self.current_slide, shape: shape.id, style };
            self.execute(vec![command], cx);
        }
    }

    pub(crate) fn step_font_size(&mut self, grow: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_style(window, cx, |s| s.font_size = step_font_size(s.font_size, grow));
    }

    // ----- In-place text editing ------------------------------------------

    pub(crate) fn begin_edit(
        &mut self,
        shape: ShapeId,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            return;
        }
        self.commit_edit(window, cx);
        let len = self
            .slide()
            .shape(shape)
            .and_then(|s| s.text_body())
            .map(|b| b.text().chars().count())
            .unwrap_or(0);
        self.selection = Some(shape);
        self.editing = Some(TextEdit {
            shape,
            edit: crate::pptx::text_edit::TextEditing {
                shape,
                anchor: len,
                caret: len,
                marked: None,
                is_new,
                typing: false,
                cell: None,
                cell_anchor: None,
            added_body: None,
            },
            positions: Default::default(),
            goal_x: None,
            pending_click: None,
            drag_anchor: None,
        });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn place_xlsx_edit_caret_at_pending_click(&mut self, cx: &mut Context<Self>) {
        let Some(click_pos) = self.editing.as_mut().and_then(|e| e.pending_click.take()) else { return };
        if let Some(char_idx) = self.text_index_at(click_pos) {
            self.shape_move_caret(char_idx, false, cx);
        }
    }

    /// Finish in-place editing and write the text back to the document.
    pub(crate) fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.as_ref().is_some_and(|e| e.edit.marked.is_some()) {
            self.commit_marked(cx);
        }
        let Some(edit) = self.editing.take() else { return };
        let empty = self
            .slide()
            .shape(edit.shape)
            .and_then(|s| s.text_body())
            .is_none_or(|b| b.text().trim().is_empty());

        if edit.edit.is_new && empty {
            // PowerPoint drops a new text box that was left empty.
            self.document.retract_last();
            self.selection = None;
        }
        self.fix_selection();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    // ----- Pointer interaction --------------------------------------------

    /// Press on the workspace or empty slide area.
    pub(crate) fn on_workspace_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panel_dialog_is_modal() { cx.stop_propagation(); return; }
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        match self.tool {
            Tool::TextBox if self.allows_shapes() => {
                self.drag = Some(Drag::Create { start: event.position, current: event.position });
            }
            _ => self.selection = None,
        }
        cx.notify();
    }

    /// Press on a shape body.
    pub(crate) fn on_shape_down(
        &mut self,
        shape: ShapeId,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panel_dialog_is_modal() { cx.stop_propagation(); return; }
        if self.tool == Tool::TextBox {
            // Drawing a text box may start on top of another shape.
            return self.on_workspace_down(event, window, cx);
        }
        if self.ink_press(event.position, cx) {
            cx.stop_propagation();
            return;
        }
        cx.stop_propagation();
        if self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            return; // the text field handles its own clicks
        }
        if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Control(_))) {
            self.commit_edit(window, cx);
            self.focus_handle.focus(window, cx);
            if self.control_press(shape, event, window, cx) {
                return cx.notify();
            }
        }
        if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Timeline(_))) {
            self.commit_edit(window, cx);
            self.focus_handle.focus(window, cx);
            if self.timeline_press(shape, event, cx) {
                self.set_selection(vec![shape]);
                return cx.notify();
            }
        }
        if self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::Slicer(_))) {
            self.commit_edit(window, cx);
            self.focus_handle.focus(window, cx);
            if self.slicer_press(shape, event, cx) {
                self.set_selection(vec![shape]);
                return cx.notify();
            }
        }
        if event.click_count >= 2 {
            self.begin_edit(shape, false, window, cx);
            if let Some(edit) = self.editing.as_mut() {
                edit.pending_click = Some(event.position);
            }
            return;
        }
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        if super::selection_handles::additive(event.modifiers) {
            let mut ids = self.selected_ids();
            if ids.contains(&shape) {
                ids.retain(|&id| id != shape);
            } else {
                ids.push(shape);
            }
            self.set_selection(ids);
            return cx.notify();
        } else if !self.selected_ids().contains(&shape) {
            self.set_selection(vec![shape]);
        }
        if let Some(origin) = self.slide().shape(shape).map(|s| s.frame) {
            self.drag = Some(Drag::Move {
                shape,
                start: event.position,
                current: event.position,
                origin,
            });
        }
        cx.notify();
    }

    /// Press on a selection handle.
    pub(crate) fn on_handle_down(
        &mut self,
        shape: ShapeId,
        handle: ResizeHandle,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            self.commit_edit(window, cx);
        }
        if let Some((origin, keep_aspect_on_corners)) = self
            .slide()
            .shape(shape)
            .map(|s| (s.frame, matches!(s.kind, ShapeKind::Picture(_))))
        {
            self.drag = Some(Drag::Resize {
                shape,
                handle,
                start: event.position,
                current: event.position,
                origin,
                keep_aspect_on_corners,
            });
        }
        cx.notify();
    }

    pub(crate) fn on_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.move_formula_reference_drag(event,cx){return;}
        if self.sheet_range_pick_move(event,window,cx) { return; }
        self.update_link_hover(event,cx);
        if self.ink_move(event.position, event.pressed_button == Some(MouseButton::Left), cx) {
            return;
        }
        if self.cell_edit().is_some_and(|e| e.dragging.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.drag_in_cell_text(event.position, window, cx);
            } else if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut()) {
                edit.dragging = None;
            }
            return;
        }
        if self.sheet_state().is_some_and(|s| s.split_drag.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.on_split_move(event.position, cx);
            } else {
                self.on_split_up(event.position, cx);
            }
            return;
        }
        if self.sheet_state().is_some_and(|s| s.timeline_drag.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.on_timeline_drag_move(event.position, cx);
            } else {
                self.on_timeline_drag_up(cx);
            }
            return;
        }
        if self.sheet_state().is_some_and(|s| s.control_drag.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.on_control_drag_move(event.position, cx);
            } else {
                self.on_control_drag_up(cx);
            }
            return;
        }
        if let Some(state) = self.sheet_state_mut()
            && state.fill_copy != event.modifiers.alt
        {
            state.fill_copy = event.modifiers.alt;
            cx.notify();
        }
        if self.sheet_state().is_some_and(|s| s.cell_move.is_some()) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.update_cell_move(event.position, event.modifiers.alt, cx);
            } else {
                self.finish_cell_move(cx);
            }
            return;
        }
        if self.sheet_state().is_some_and(|s| s.cell_drag) {
            if event.pressed_button == Some(MouseButton::Left) {
                self.extend_cells_to(event.position);
                cx.notify();
            } else if let Some(state) = self.sheet_state_mut() {
                state.cell_drag = false;
                state.deselect_drag = None;
            }
            return;
        }
        if self.drag_ruler_margin(event.position, event.pressed_button == Some(MouseButton::Left), cx) {
            return;
        }
        let Some(drag) = self.drag.as_mut() else { return };
        match drag {
            Drag::Move { current, .. } | Drag::Resize { current, .. } | Drag::Create { current, .. } => {
                *current = event.position;
            }
        }
        cx.notify();
    }

    pub(crate) fn on_pointer_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit)=self.cell_edit_mut(){edit.reference_drag=false;}
        if self.sheet_range_pick_up(window,cx) { return; }
        if self.ink_release(cx) {
            return;
        }
        if self.end_ruler_margin_drag(cx) {
            return;
        }
        if self.on_timeline_drag_up(cx) {
            return;
        }
        if self.on_control_drag_up(cx) {
            return;
        }
        if self.finish_cell_move(cx) {
            return;
        }
        if self.sheet_state().is_some_and(|s| s.split_drag.is_some()) {
            self.on_split_up(event.position, cx);
            return;
        }
        if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
            && edit.dragging.take().is_some()
        {
            return cx.notify();
        }
        if let Some(state) = self.sheet_state_mut()
            && state.cell_drag
        {
            state.cell_drag = false;
            state.deselect_drag = None;
            return cx.notify();
        }
        let Some(drag) = self.drag.take() else { return };
        let Some(layout) = self.slide_layout() else { return };
        let slide = self.current_slide;
        match drag {
            Drag::Move { shape, start, current, .. } | Drag::Resize { shape, start, current, .. } => {
                if !moved(start, current) {
                    if matches!(drag,Drag::Move{..}) {
                        self.set_selection(vec![shape]);
                        let at=layout.to_emu(start);
                        let border=self.slide().shape(shape).is_some_and(|s|super::selection_handles::on_border((s.frame.x as f32,s.frame.y as f32,s.frame.width as f32,s.frame.height as f32),s.rotation,(at.x as f32,at.y as f32),3./layout.scale));
                        if !border && self.slide().shape(shape).is_some_and(|s|matches!(s.kind,ShapeKind::TextBox(_)|ShapeKind::Auto(_))){
                            self.begin_edit(shape,false,window,cx);
                            if let Some(edit)=self.editing.as_mut(){edit.pending_click=Some(current);}
                        }
                    }
                    return cx.notify();
                }
                let Some(current_shape) = self.slide().shape(shape) else { return };
                let frame = self.display_frame_for(current_shape, drag, layout);
                if frame != current_shape.frame {
                    if matches!(drag, Drag::Move { .. })
                        && self.selected_ids().len() > 1
                        && self.selected_ids().contains(&shape)
                    {
                        let (dx, dy) = (frame.x - current_shape.frame.x, frame.y - current_shape.frame.y);
                        let cmds: Vec<Command> = self
                            .selected_ids()
                            .into_iter()
                            .filter_map(|id| {
                                let s = self.slide().shape(id)?;
                                let next = EmuRect::new(
                                    (s.frame.x + dx).max(0),
                                    (s.frame.y + dy).max(0),
                                    s.frame.width,
                                    s.frame.height,
                                );
                                Some(Command::SetFrame { slide, shape: id, frame: next })
                            })
                            .collect();
                        self.execute(cmds, cx);
                    } else {
                        self.execute(vec![Command::SetFrame { slide, shape, frame }], cx);
                    }
                    if let Some(edit) = self.editing.as_mut() {
                        edit.edit.is_new = false;
                    }
                }
            }
            Drag::Create { start, current } => {
                let frame = if moved(start, current) {
                    let r = EmuRect::from_corners(layout.to_emu(start), layout.to_emu(current));
                    EmuRect { width: r.width.max(MIN_SHAPE_SIDE), height: r.height.max(MIN_SHAPE_SIDE), ..r }
                } else {
                    self.presentation().default_text_box_frame(layout.to_emu(start))
                };
                let id = self.document.ids().new_shape_id();
                let index = self.slide().shapes.len();
                self.execute(
                    vec![Command::InsertShape { slide, index, shape: Shape::text_box(id, frame, "") }],
                    cx,
                );
                self.tool = Tool::Select;
                self.begin_edit(id, true, window, cx);
            }
        }
    }

    fn display_frame_for(&self, shape: &Shape, drag: Drag, layout: SlideLayout) -> EmuRect {
        match drag {
            Drag::Move { start, current, origin, .. } => origin.translated(
                layout.to_emu_len(current.x - start.x),
                layout.to_emu_len(current.y - start.y),
            ),
            Drag::Resize {
                handle,
                start,
                current,
                origin,
                keep_aspect_on_corners,
                ..
            } => origin.resized_with_aspect(
                handle,
                layout.to_emu_len(current.x - start.x),
                layout.to_emu_len(current.y - start.y),
                MIN_SHAPE_SIDE,
                keep_aspect_on_corners,
            ),
            Drag::Create { .. } => shape.frame,
        }
    }
}

/// The zooms of the status bar's list, in percent, largest first: 100%,
/// and from it up by `ZOOM_LIST_UP` and down by `ZOOM_LIST_DOWN` as far as
/// the zoom range goes, each rounded to a whole percent.
pub(crate) fn zoom_levels() -> Vec<u32> {
    let mut levels = Vec::new();
    let mut zoom = ZOOM_LIST_UP;
    while zoom <= MAX_ZOOM + 1e-4 {
        levels.push((zoom * 100.0).round() as u32);
        zoom *= ZOOM_LIST_UP;
    }
    levels.reverse();
    let mut zoom = 1.0;
    while zoom >= MIN_ZOOM - 1e-4 {
        levels.push((zoom * 100.0).round() as u32);
        zoom *= ZOOM_LIST_DOWN;
    }
    levels
}

/// A zoom typed in percent, such as `150`, `150%` or ` 72.5 `.
pub(crate) fn parse_percent(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text.strip_suffix('%').or_else(|| text.strip_suffix('％')).unwrap_or(text).trim();
    text.parse::<f32>().ok().filter(|p| p.is_finite() && *p > 0.0)
}

/// The scrolling on one axis that keeps the content point at `anchor` (from
/// the start of a view `view` long) there when the zoom changes by `ratio`:
/// the page starts at `before_origin` in the content scrolled by `old`, and
/// at `after_origin` in the new content, `content` long. Kept within the
/// content, so a point near its edge moves.
fn zoom_axis_offset(anchor: f32, view: f32, old: f32, before_origin: f32, after_origin: f32, ratio: f32, content: f32) -> f32 {
    let at = after_origin + (anchor - old - before_origin) * ratio;
    (anchor - at).clamp((view - content).min(0.0), 0.0)
}

/// Whether the wheel zooms with `modifiers` held: Command or Control on a
/// Mac (`mac`), Control (or Super/Cmd) elsewhere.
fn wheel_zooms(modifiers: &Modifiers, mac: bool) -> bool {
    if mac { modifiers.platform || modifiers.control } else { modifiers.control || modifiers.platform }
}

/// Whether the wheel's zoom modifier is a Mac's. In the browser, the
/// browser's platform says (see `secondary_modifier` in `lib.rs`).
fn zoom_modifier_is_mac() -> bool {
    #[cfg(target_family = "wasm")]
    return crate::web::is_mac();
    #[cfg(not(target_family = "wasm"))]
    cfg!(target_os = "macos")
}

/// Pixels per EMU at `zoom`, 1.0 being 100%.
pub(crate) fn zoom_scale(zoom: f32) -> f32 {
    zoom * PX_PER_INCH_AT_100 / ibis_doc_model::EMU_PER_INCH as f32
}

/// The worksheet of `slide` and the shape that holds it.
pub(crate) fn sheet_of(slide: &Slide) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
    slide.shapes.iter().find_map(|shape| match &shape.kind {
        ShapeKind::Sheet(sheet) => Some((shape, sheet)),
        _ => None,
    })
}

fn moved(a: Point<Pixels>, b: Point<Pixels>) -> bool {
    let dx = f32::from(b.x - a.x);
    let dy = f32::from(b.y - a.y);
    dx.abs() >= DRAG_THRESHOLD || dy.abs() >= DRAG_THRESHOLD
}

impl Focusable for SlideEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SlideEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_zoom_slider(window, cx);
        if self.zoom_menu_open && !self.has_document {
            self.zoom_menu_open = false;
        }
        with_format!(self, before_render(self, window, cx));
        if self.interaction_layer().is_confirmation() {
            if self.panel_dialog.drag.take().is_some() {window.release_pointer();}
            self.cancel_table_reorder(window,cx);
            self.suspend_sheet_range_drag(window);
        }
        let theme = cx.theme();
        v_flex()
            .id("slide-editor").relative()
            .child(crate::xlsx::range_field::event_surface(cx))
            .key_context(CONTEXT)
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if this.sheet_range_pick_down(event,window,cx) {cx.stop_propagation();return;}
                if !this.panel_dialog_allows_pointer(event.position, window) { cx.stop_propagation();return; }
                if this.interaction_layer().is_confirmation() {return;}
                if this.handle_document_link_pointer(event,window,cx){cx.stop_propagation();}
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, window, cx| {
                if this.sheet_range_pick_up(window,cx) {cx.stop_propagation();return;}
                if this.panel_dialog.drag.take().is_some() { cx.notify(); }
                if !this.panel_dialog_allows_pointer(event.position, window) { cx.stop_propagation(); }
            }))
            .capture_key_down(cx.listener(Self::panel_dialog_key))
            .capture_key_down(cx.listener(|e,event:&KeyDownEvent,w,cx|{if e.sheet_state().is_some()&&e.formula_edit_key(event,w,cx){w.prevent_default();cx.stop_propagation();}}))
            .map(|d| self.guard_panel_actions(d, cx))
            .on_modifiers_changed(cx.listener(|this, e: &ModifiersChangedEvent, _, cx| {
                if let Some(state) = this.sheet_state_mut() {
                    let mut changed = false;
                    if let Some(m) = &mut state.cell_move {
                        if m.copy != e.modifiers.alt {
                            m.copy = e.modifiers.alt;
                            changed = true;
                        }
                    }
                    if state.fill_copy != e.modifiers.alt {
                        state.fill_copy = e.modifiers.alt;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                }
            }))
            // Documents dropped from the Finder on the window are opened (pictures dropped on a slide are put on it).
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                #[cfg(not(target_family = "wasm"))]
                for path in paths.paths() {
                    if path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension).is_some() {
                        crate::open_path_in_app(path.clone(), cx);
                    }
                }
                #[cfg(target_family = "wasm")]
                let _ = (paths, cx);
            }))
            .on_action(cx.listener(|e,_:&crate::AddPresentationSlide,w,cx|{if e.kind()==ibis_pptx::Kind::Presentation{e.add_slide(w,cx);}}))
            .on_action(cx.listener(|e,_:&crate::DuplicatePresentationSelection,w,cx|{if e.kind()==ibis_pptx::Kind::Presentation{if e.selected_shapes().is_empty(){e.duplicate_current_slide(w,cx);}else{e.duplicate_selected(cx);}}}))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::cancel_tool))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(|_, _: &crate::OpenStartWindow, _, cx| crate::open_editor(None, None, cx)))
            .on_action(cx.listener(|this, _: &crate::NewDocument, window, cx| this.new_file(this.kind(), window, cx)))
            .on_action(cx.listener(|this, _: &crate::NewFromTemplate, window, cx| this.new_from_template(window, cx)))
            // Also on the start view (⌘W closes its window; the menu's ファイル > 閉じる was greyed there, #75).
            .on_action(cx.listener(Self::close_file))
            .when(self.has_document, |d| d.on_action(cx.listener(Self::save)).on_action(cx.listener(Self::save_as)).on_action(cx.listener(|this, _: &crate::PrintFile, window, cx| this.print(window, cx))))
            .when(self.has_document, |d| {
                d.on_action(cx.listener(|this, _: &crate::ToggleMenus, _, cx| {
                    if !this.show_menu_bar {
                        this.show_menu_bar = true;
                        this.chrome_compact = false;
                    } else {
                        this.chrome_compact = !this.chrome_compact;
                    }
                    cx.notify();
                }))
            })
            .on_action(cx.listener(|this, _: &crate::ZoomInView, window, cx| this.step_zoom(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomOutView, window, cx| this.step_zoom(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomActualView, window, cx| this.set_zoom(Some(1.0), window, cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomFitView, window, cx| this.set_zoom(None, window, cx)))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::previous_slide))
            .on_action(cx.listener(|this, _: &crate::FirstSlide, window, cx| this.end_slide(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::LastSlide, window, cx| this.end_slide(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::StartSlideshow, window, cx| this.start_slideshow(window, cx)))
            .on_action(cx.listener(|this, _: &crate::StartPresenterView, window, cx| this.start_presenter_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::InsertHyperlink, window, cx| {
                if this.pptx().is_some() && this.slideshow.is_none() {
                    this.slide_open_link_panel(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::InsertComment, window, cx| {
                if let Some(add) = crate::shell::format::with_format!(this, add_comment(this)) {
                    add(this, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::StartSlideshowFromBeginning, window, cx| {
                if this.slideshow.is_none() && this.pages_are_slides() && this.has_document && !this.loading {
                    this.show_slide_number(1, window, cx);
                    this.start_slideshow(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::BlackScreen, _, cx| this.blank_slideshow(0x000000, cx)))
            .on_action(cx.listener(|this, _: &crate::WhiteScreen, _, cx| this.blank_slideshow(0xffffff, cx)))
            .on_action(cx.listener(|this, _: &crate::LaserPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Laser, cx)))
            .on_action(cx.listener(|this, _: &crate::PenPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Pen, cx)))
            .on_action(cx.listener(|this, _: &crate::HighlighterPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Highlighter, cx)))
            .on_action(cx.listener(|this, _: &crate::ArrowPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Arrow, cx)))
            .on_action(cx.listener(|this, _: &crate::EraserPointer, _, cx| this.set_slideshow_pointer_mode(crate::pptx::slideshow::PointerMode::Eraser, cx)))
            .on_action(cx.listener(|this, _: &crate::ClearSlideInks, _, cx| this.clear_current_slide_inks(cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleSlideshowGrid, _, cx| this.toggle_slideshow_grid(cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomInSlideshow, _, cx| this.zoom_slideshow(1.25, cx)))
            .on_action(cx.listener(|this, _: &crate::ZoomOutSlideshow, _, cx| this.zoom_slideshow(0.8, cx)))
            .on_action(cx.listener(|this, _: &crate::ResetSlideshowZoom, _, cx| this.reset_slideshow_zoom(cx)))
            .on_action(cx.listener(|this, _: &crate::RehearseTimings, window, cx| this.start_rehearsal(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleSlideGridView, window, cx| this.toggle_grid_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleReadingView, window, cx| {
                if this.is_reading_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.start_reading_view(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::SwitchToNormalView, window, cx| this.switch_to_normal_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleOutlineView, window, cx| this.toggle_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::SwitchToOutlineView, window, cx| this.switch_to_outline_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::ToggleNotesPageView, window, cx| this.toggle_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::SwitchToNotesPageView, window, cx| this.switch_to_notes_page_view(window, cx)))
            .on_action(cx.listener(|this, _: &crate::OpenZoomDialog, window, cx| this.open_zoom_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &crate::NewEditorWindow, _, cx| this.new_window(cx)))
            .on_action(cx.listener(|this, _: &crate::docx::editing::ToggleSplit, _, cx| {
                if this.is_workbook() {
                    this.toggle_split(cx);
                } else if this.is_document() {
                    this.toggle_document_split(cx);
                }
            }))
            .on_action(cx.listener(|_, _: &crate::ArrangeAllWindows, window, cx| SlideEditor::arrange_all_windows(Some(window), cx)))
            .on_action(cx.listener(|_, _: &crate::CascadeWindows, window, cx| SlideEditor::cascade_windows(Some(window), cx)))
            .on_action(cx.listener(|this, _: &crate::CopyAnimation, _, cx| this.copy_shape_animations(cx)))
            .on_action(cx.listener(|this, _: &crate::PasteAnimation, _, cx| this.paste_shape_animations(cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let ks = &event.keystroke;
                if this.has_document
                    && this.slideshow.is_none()
                    && ks.modifiers.alt
                    && !ks.modifiers.platform
                    && !ks.modifiers.control
                    && (ks.key == "/" || ks.key == "?" || ks.key_char.as_deref() == Some("/") || ks.key_char.as_deref() == Some("÷"))
                {
                    this.open_toolbar_menu_search(window, cx);
                    cx.stop_propagation();
                }
            }))
            // Drags are tracked on the whole window so they continue past the slide.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if !this.move_panel_dialog(event, window, cx) { Self::on_pointer_move(this, event, window, cx); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &crate::SelectCellUp, window, cx| this.move_cell(0, -1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellDown, window, cx| this.move_cell(0, 1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellLeft, window, cx| this.move_cell(-1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::SelectCellRight, window, cx| this.move_cell(1, 0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::NextSheet, window, cx| this.step_sheet(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::PreviousSheet, window, cx| this.step_sheet(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsUp, _, cx| this.extend_cells(0, -1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsDown, _, cx| this.extend_cells(0, 1, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsLeft, _, cx| this.extend_cells(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::ExtendCellsRight, _, cx| this.extend_cells(1, 0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollLineUp, _, cx| this.scroll_lines(-1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollLineDown, _, cx| this.scroll_lines(1.0, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToStart, _, cx| this.scroll_to_end(false, cx)))
            .on_action(cx.listener(|this, _: &crate::ScrollToEnd, _, cx| this.scroll_to_end(true, cx)))
            .map(|d| Self::cell_jump_actions(d, cx))
            // The slide show takes the whole window, in place of the rest.
            .when_some(self.render_slideshow(window, cx), |d, show| d.child(show))
            .when(!self.show_in_main(), |d| self.render_window(d, window, cx))
            .map(|d| self.render_dialog_stack(d, window, cx))
    }
}

impl SlideEditor {
    /// The editor window's parts, into `d`: the title bar, and the start view
    /// or the chrome, the document and the status bar.
    fn render_window(&mut self, d: Stateful<Div>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        register_open_editor_window(self.entity_id, window.window_handle(), cx.entity().downgrade());
        update_open_editor_window_label(self.entity_id, self.window_menu_label());
        // The mock's layout (shell/chrome.rs): the title row, the menu bar
        // and the toolbar (the first two hidden by the toolbar's ^), the
        // document with the sidebar on its right, and the status bar.
        if !self.has_document {
            window.set_window_title(&crate::shell::start_view::start_title());
            return d.child(self.render_title_row(window, cx)).child(self.render_start_view(cx));
        }
        let compact = self.chrome_compact;
        let focus = self.is_focus_mode();
        let show_menu_bar = self.show_menu_bar;
        let show_toolbar = self.show_toolbar;
        let show_title_row = !self.is_read_only() || show_menu_bar || {
            #[cfg(target_arch = "wasm32")]
            {
                crate::shell::embed::show_close() || crate::shell::embed::show_download()
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                true
            }
        };
        let sidebar = if focus {
            self.right_pane = Default::default();
            None
        } else {
            self.render_sidebar(window, cx)
        };
        let body = with_format!(self, render_body(self, window, cx));
        d.when(!compact && !focus && show_title_row, |d| d.child(self.render_title_row(window, cx)))
            .when(!compact && !focus && show_menu_bar, |d| d.child(self.render_menu_bar(window, cx)))
            .when(!focus && show_toolbar, |d| d.child(self.render_toolbar(window, cx)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    // Text input goes to the format (shell/input.rs).
                    .child(div().relative().flex().flex_col().flex_1().min_w_0().min_h_0().child(body).child(crate::shell::input::input_region(cx)))
                    .children(sidebar),
            )
            .when(!focus, |d| d.child(self.render_status_bar(cx)))
    }

    /// Apply a font size (in points) chosen or typed in the toolbar font size pulldown (#120).
    pub(crate) fn commit_toolbar_font_size(&mut self,window:&mut Window,cx:&mut Context<Self>){
        if self.font_size_edit_dirty && let Ok(value)=self.font_size_edit_input.read(cx).value().trim().parse::<f32>() && value.is_finite() && (1.0..=1638.).contains(&value) {self.font_size_edit_dirty=false;self.set_toolbar_font_size(value,window,cx);}
        self.focus_handle.focus(window,cx);cx.notify();
    }
    pub(crate) fn set_toolbar_font_size(&mut self, pt: f32, window: &mut Window, cx: &mut Context<Self>) {
        match self.format.kind() {
            ibis_pptx::Kind::Document => {
                let hundredths = (pt * 100.0).round().clamp(100.0, 400_000.0) as u32;
                if self.apply_box_edit_run_change(&ibis_doc_model::RunChange::Size(hundredths), cx) {
                    return;
                }
                self.apply_text_properties(
                    ibis_doc_model::RunFormat {
                        size: Some(hundredths),
                        ..Default::default()
                    },
                    cx,
                );
            }
            ibis_pptx::Kind::Workbook => {
                let hundredths = (pt * 100.0).round().clamp(100.0, 40_900.0) as u32;
                self.format_cells(ibis_doc_model::StyleChange::FontSize(hundredths), cx);
            }
            ibis_pptx::Kind::Presentation => {
                let hundredths = (pt * 100.0).round().clamp(100.0, 400_000.0) as u32;
                self.format_characters(
                    window,
                    cx,
                    ibis_doc_model::CharacterFormat {
                        font_size: Some(hundredths),
                        ..Default::default()
                    },
                );
            }
        }
    }

    /// Open the toolbar's "メニューを検索" (`menu-search`) popup and focus its search input (#147).
    pub(crate) fn open_toolbar_menu_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let input = self.ensure_toolbar_search_input("menu-search", window, cx);
        let scroll = self.ensure_toolbar_search_scroll("menu-search");
        scroll.set_offset(Point::default());
        self.toolbar_search_open = Some("menu-search");
        self.toolbar_search_selected.insert("menu-search", None);
        crate::shell::chrome::set_current_toolbar_open(self.font_size_menu_open, self.toolbar_search_open);
        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    /// Get or create a `ScrollHandle` for a searchable dropdown menu (#125, #127).
    pub(crate) fn ensure_toolbar_search_scroll(&mut self, id: &'static str) -> ScrollHandle {
        self.toolbar_search_scrolls
            .entry(id)
            .or_insert_with(ScrollHandle::new)
            .clone()
    }

    /// Get or create a cleanable search `InputState` for a searchable dropdown menu (#98, #103, #127).
    pub(crate) fn ensure_toolbar_search_input(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(existing) = self.toolbar_search_inputs.get(id) {
            return existing.clone();
        }
        let placeholder = crate::shell::i18n::text("fix.menu.search-placeholder");
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, window, cx| {
            match event {
                InputEvent::Change => {
                    this.toolbar_search_selected.insert(id, None);
                    if let Some(scroll) = this.toolbar_search_scrolls.get(id) {
                        scroll.set_offset(Point::default());
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.activate_top_toolbar_search_match(id, window, cx);
                }
                _ => {}
            }
        })
        .detach();
        self.toolbar_search_inputs.insert(id, input.clone());
        input
    }

    /// Dismiss the top dropdown before any parent-dialog action, independent of input focus.
    pub(crate) fn dismiss_toolbar_dropdown(&mut self,window:&mut Window,cx:&mut Context<Self>)->bool {
        let Some(id)=self.toolbar_search_open.take() else{return false;};
        self.toolbar_search_popup_items=None;
        self.toolbar_search_selected.insert(id,None);
        crate::shell::chrome::set_current_toolbar_open(self.font_size_menu_open,None);
        if self.panel_dialog.active {if let Some(focus)=self.panel_dialog.focus.as_ref(){focus.focus(window,cx);}}
        else {self.focus_handle.focus(window,cx);}
        cx.notify();true
    }

    /// Own list navigation before input bindings, including lists without a search field.
    fn toolbar_dropdown_keystroke(&mut self,id:&'static str,key:&Keystroke,window:&mut Window,cx:&mut Context<Self>)->bool {
        let items=self.toolbar_search_popup_items.as_ref().filter(|(popup,_)|*popup==id).map(|(_,items)|items.clone()).unwrap_or_else(||self.searchable_menu_items_for(id));
        let search=items.len()>20;
        match key.key.as_str() {
            "enter"=>{self.activate_top_toolbar_search_match(id,window,cx);true}
            "escape"=>self.dismiss_toolbar_dropdown(window,cx),
            "up"|"down"|"pageup"|"pagedown"=>{
                let query=if search {self.toolbar_search_inputs.get(id).map(|i|i.read(cx).value().to_string()).unwrap_or_default()}else{String::new()};
                let matched=crate::shell::chrome::filter_menu_items(&items,&query);
                let row_h=28.;let max_rows=(((f32::from(window.viewport_size().height)-214.).max(224.)/row_h).floor() as usize).max(8);
                let visible=items.len().clamp(1,max_rows);let page=visible.saturating_sub(1).max(1) as isize;
                let delta=match key.key.as_str(){"up"=>-1,"down"=>1,"pageup"=>-page,_=>page};
                if let Some(index)=crate::shell::chrome::step_enabled_menu_index(&matched,self.toolbar_search_selected.get(id).copied().flatten(),delta){
                    self.toolbar_search_selected.insert(id,Some(index));
                    if let Some(scroll)=self.toolbar_search_scrolls.get(id){crate::shell::chrome::scroll_row_into_view(scroll,index,row_h,visible as f32*row_h);}
                    cx.notify();
                }true
            }
            _=>!search && key.key_char.is_some(),
        }
    }

    /// When Enter is pressed in a searchable dropdown menu, execute the keyboard-selected item or (if a query is typed) the top matched item (#98, #103, #127).
    pub(crate) fn activate_top_toolbar_search_match(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.toolbar_search_inputs.get(id).cloned() else {
            return;
        };
        let query = input.read(cx).value().to_string();
        let selected_idx = self.toolbar_search_selected.get(id).copied().flatten();
        if query.trim().is_empty() && selected_idx.is_none() {
            return;
        }
        let items = self.toolbar_search_popup_items.as_ref()
            .filter(|(popup, _)| *popup == id).map(|(_, items)| items.clone())
            .unwrap_or_else(|| self.searchable_menu_items_for(id));
        let matched = crate::shell::chrome::filter_menu_items(&items, &query);
        let chosen = if let Some(idx) = selected_idx {
            matched.get(idx).filter(|item| !item.disabled && item.handler.is_some()).cloned()
        } else {
            matched.into_iter().find(|item| !item.disabled && item.handler.is_some())
        };
        if let Some(item) = chosen
            && let Some(handler) = item.handler
        {
            input.update(cx, |state, cx| state.set_value("", window, cx));
            self.toolbar_search_open = None;
            self.toolbar_search_selected.insert(id, None);
            crate::shell::chrome::set_current_toolbar_open(self.font_size_menu_open, None);
            let editor = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                editor.update(cx, |this, cx| handler(this, window, cx)).ok();
            });
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// Return the menu items for a searchable dropdown button `id` (#98, #103, #127).
    pub(crate) fn searchable_menu_items_for(&self, id: &str) -> Vec<crate::shell::chrome::MenuItem> {
        if let Some(entries) = self.doc_font_menu_entries(id) {
            return crate::shell::chrome::flatten_menu_entries(&entries);
        }
        if id == "menu-search" {
            return crate::shell::chrome::collect_searchable_menu_items(&self.menus());
        }
        for tool in self.toolbar_items() {
            match tool {
                crate::shell::chrome::ToolItem::Button(btn) if btn.id == id => {
                    if let Some(entries) = btn.menu {
                        return crate::shell::chrome::flatten_menu_entries(&entries);
                    }
                }
                crate::shell::chrome::ToolItem::Select { id: item_id, menu, .. } if item_id == id => {
                    return crate::shell::chrome::flatten_menu_entries(&menu);
                }
                _ => {}
            }
        }
        Vec::new()
    }

    /// Display title for this editor window in the Window menu (#111).
    pub(crate) fn window_menu_label(&self) -> SharedString {
        let base = self
            .file_name
            .clone()
            .unwrap_or_else(|| crate::shell::file_actions::new_file_name(self.format.kind()));
        match self.window_number {
            Some(n) => SharedString::from(format!("{base}:{n}")),
            None => base,
        }
    }

    /// Menu entries for the open editor windows at the bottom of the Window menu (#111).
    pub(crate) fn open_window_menu_items(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let mut entries: Vec<MenuEntry> = Vec::new();
        let mut found_self = false;
        OPEN_EDITOR_WINDOWS.with(|cell| {
            let mut list = cell.borrow_mut();
            list.retain(|entry| entry.editor.upgrade().is_some());
            for (idx, entry) in list.iter().enumerate() {
                let is_self = entry.entity_id == self.entity_id;
                if is_self {
                    found_self = true;
                }
                let label = if is_self {
                    self.window_menu_label()
                } else {
                    entry.label.borrow().clone()
                };
                let handle = entry.handle;
                let id_str: &'static str = match idx {
                    0 => "window.open.0",
                    1 => "window.open.1",
                    2 => "window.open.2",
                    3 => "window.open.3",
                    4 => "window.open.4",
                    5 => "window.open.5",
                    6 => "window.open.6",
                    7 => "window.open.7",
                    _ => "window.open.more",
                };
                entries.push(
                    MenuItem::new(id_str, label)
                        .checked(is_self)
                        .on(move |_, _, cx| {
                            let _ = handle.update(cx, |_, window, _| {
                                window.activate_window();
                            });
                        })
                        .into(),
                );
            }
        });
        if !found_self {
            entries.push(
                MenuItem::new("window.open.0", self.window_menu_label())
                    .checked(true)
                    .on(|_, window, _| {
                        window.activate_window();
                    })
                    .into(),
            );
        }
        entries
    }
}

struct OpenEditorWindowEntry {
    entity_id: gpui::EntityId,
    handle: AnyWindowHandle,
    editor: WeakEntity<SlideEditor>,
    label: std::rc::Rc<std::cell::RefCell<SharedString>>,
}

thread_local! {
    static OPEN_EDITOR_WINDOWS: std::cell::RefCell<Vec<OpenEditorWindowEntry>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn register_open_editor_window(
    entity_id: gpui::EntityId,
    handle: AnyWindowHandle,
    editor: WeakEntity<SlideEditor>,
) {
    OPEN_EDITOR_WINDOWS.with(|cell| {
        let mut list = cell.borrow_mut();
        list.retain(|e| e.editor.upgrade().is_some());
        if let Some(existing) = list.iter_mut().find(|e| e.entity_id == entity_id) {
            existing.handle = handle;
            existing.editor = editor;
        } else {
            list.push(OpenEditorWindowEntry {
                entity_id,
                handle,
                editor,
                label: std::rc::Rc::new(std::cell::RefCell::new(SharedString::from("ibisOffice"))),
            });
        }
    });
}

pub(crate) fn update_open_editor_window_label(entity_id: gpui::EntityId, label: SharedString) {
    OPEN_EDITOR_WINDOWS.with(|cell| {
        let list = cell.borrow();
        if let Some(existing) = list.iter().find(|e| e.entity_id == entity_id) {
            *existing.label.borrow_mut() = label;
        }
    });
}

/// An element filling its (positioned) parent that calls `f` with the
/// parent's bounds in window coordinates at every prepaint. (gpui-kit's
/// `on_prepaint` adds an absolute child without an inset, which lands after
/// the parent's other children rather than at its origin.)
pub(crate) fn bounds_observer(f: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> impl IntoElement {
    canvas(move |bounds, window, cx| f(bounds, window, cx), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

/// An element filling the (positioned) workspace that hands wheel and pinch
/// gestures over it to `SlideEditor::on_wheel_zoom` and `on_pinch_zoom`
/// before anything in the workspace scrolls, and stops the event there when
/// it zooms.
pub(crate) fn wheel_zoom_listener(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor_wheel = cx.entity().downgrade();
    let editor_pinch = cx.entity().downgrade();
    let f_wheel = move |event: &ScrollWheelEvent, window: &mut Window, cx: &mut App| {
        editor_wheel.update(cx, |this, cx| this.on_wheel_zoom(event, window, cx)).unwrap_or(false)
    };
    let f_pinch = move |event: &PinchEvent, window: &mut Window, cx: &mut App| {
        editor_pinch.update(cx, |this, cx| this.on_pinch_zoom(event, window, cx)).unwrap_or(false)
    };
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let hitbox_pinch = hitbox.clone();
            let hitbox_wheel = hitbox.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && (hitbox_wheel.is_hovered(window) || hitbox_wheel.bounds.contains(&event.position))
                    && f_wheel(event, window, cx)
                {
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &PinchEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && (hitbox_pinch.is_hovered(window) || hitbox_pinch.bounds.contains(&event.position))
                    && f_pinch(event, window, cx)
                {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::{MAX_ZOOM, MIN_ZOOM, parse_percent, wheel_zooms, zoom_axis_offset, zoom_levels};
    use gpui_kit::Modifiers;

    #[test]
    fn zoom_levels_step_from_100() {
        let levels = zoom_levels();
        assert_eq!(levels, [358, 299, 249, 207, 173, 144, 120, 100, 80, 64, 51, 41, 33, 26, 21, 17, 13, 11]);
        assert!(levels.iter().all(|&p| p as f32 <= MAX_ZOOM * 100.0 && p as f32 >= MIN_ZOOM * 100.0));
    }

    #[test]
    fn percent_is_read_with_or_without_the_sign() {
        assert_eq!(parse_percent("150"), Some(150.0));
        assert_eq!(parse_percent(" 72.5 % "), Some(72.5));
        assert_eq!(parse_percent("80％"), Some(80.0));
        assert_eq!(parse_percent("abc"), None);
        assert_eq!(parse_percent("0"), None);
        assert_eq!(parse_percent(""), None);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        // The page at 40 in the content, scrolled by -300: the pointer at 250
        // in the view is 510 into the page, and 1020 once the zoom doubles,
        // which the new scrolling brings under the pointer.
        let offset = zoom_axis_offset(250.0, 1000.0, -300.0, 40.0, 40.0, 2.0, 5000.0);
        assert_eq!(250.0 - offset - 40.0, 1020.0);
        // The scrolling stays within the content: at its start, and at its end.
        assert_eq!(zoom_axis_offset(500.0, 1000.0, 0.0, 40.0, 40.0, 0.5, 3000.0), 0.0);
        assert_eq!(zoom_axis_offset(990.0, 1000.0, -2000.0, 40.0, 40.0, 1.0, 2500.0), -1500.0);
    }

    #[test]
    fn the_zoom_modifier_follows_the_platform() {
        let ctrl = Modifiers { control: true, ..Default::default() };
        let cmd_ctrl = Modifiers { control: true, platform: true, ..Default::default() };
        let cmd = Modifiers { platform: true, ..Default::default() };
        assert!(wheel_zooms(&cmd_ctrl, true));
        assert!(wheel_zooms(&ctrl, true));
        assert!(wheel_zooms(&cmd, true));
        assert!(wheel_zooms(&ctrl, false));
        assert!(wheel_zooms(&cmd, false));
        assert!(!wheel_zooms(&Modifiers::default(), false));
        assert!(!wheel_zooms(&Modifiers::default(), true));
    }
}

