// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation editor window: state, editing operations and layout.
//!
//! Rendering is split by area: `chrome.rs`, `status_bar.rs`, `workspace.rs` and
//! `thumbnails.rs` add `impl SlideEditor` blocks for their part of the window.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::{SliderEvent, SliderScale, SliderState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_doc_model::{
    Command, Document, EmuPoint, EmuRect, Presentation, ResizeHandle, Shape, ShapeId, ShapeKind, Sheet,
    Slide, SlideId,
};

use crate::render::shape_view::Resources;
use crate::format::{FormatState, state_for_model, with_format};
use crate::actions::{CancelTool, CloseFile, DeleteSelection, NextSlide, OpenFile, PreviousSlide, Redo, Undo};

/// Key context of the editor; see the bindings in `ibis_office_app::init`.
pub const CONTEXT: &str = "SlideEditor";
/// Key context of the workspace with the slide (the editor's focus).
pub const WORKSPACE_CONTEXT: &str = "SlideWorkspace";
/// Key context of the slide list.
pub const LIST_CONTEXT: &str = "SlideList";
/// Key context of the workspace when it shows a worksheet: the arrow keys
/// move the selected cell instead of turning pages.
pub const SHEET_CONTEXT: &str = "SheetWorkspace";
/// Key context of the workspace when it shows a flowing document: the keys
/// scroll through its pages, as in Word.
pub const DOCUMENT_CONTEXT: &str = "DocumentWorkspace";

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
pub const PX_PER_INCH_AT_100: f32 = 72.0;
/// A worksheet's: Excel for Mac draws a point as one pixel at 100%, 0.75 of
/// the size a slide has there (the user's measure: ibisOffice's 70% looked
/// like Excel's 100%).
pub const SHEET_PX_PER_INCH_AT_100: f32 = 72.0;
/// The zoom range of the status bar, 1.0 being 100%.
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 4.0;
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
pub enum Tool {
    Select,
    TextBox,
}

/// A pointer drag in progress. Frames are previewed while dragging and
/// committed as one command on release.
#[derive(Clone, Copy, Debug)]
pub enum Drag {
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
pub struct TextEdit {
    pub shape: ShapeId,
    pub edit: crate::text_edit::TextEditing,
    pub positions: crate::render::rich_text::TextPositions,
    pub(crate) goal_x: Option<(usize, f32)>,
    pub(crate) pending_click: Option<Point<Pixels>>,
    pub(crate) drag_anchor: Option<usize>,
}

/// Mapping between slide EMU and window pixels for the current layout.
#[derive(Clone, Copy, Debug)]
pub struct SlideLayout {
    /// Top-left of the slide, in window coordinates.
    pub origin: Point<Pixels>,
    /// Pixels per EMU.
    pub scale: f32,
}

impl SlideLayout {
    /// The zoom this scale shows, 1.0 being 100%.
    pub(crate) fn zoom(&self, px_per_inch_at_100: f32) -> f32 {
        self.scale * ibis_doc_model::EMU_PER_INCH as f32 / px_per_inch_at_100
    }

    pub fn to_px(&self, emu: i64) -> Pixels {
        px(emu as f32 * self.scale)
    }

    pub fn to_emu_len(&self, len: Pixels) -> i64 {
        (f32::from(len) / self.scale).round() as i64
    }

    pub fn to_emu(&self, p: Point<Pixels>) -> EmuPoint {
        EmuPoint::new(self.to_emu_len(p.x - self.origin.x), self.to_emu_len(p.y - self.origin.y))
    }
}

/// What the workspace scrolls: the slide with its margin, and at least the
/// workspace itself so a smaller slide stays centred.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceContent {
    pub size: Size<Pixels>,
    /// Top-left of the slide in the content.
    pub slide_origin: Point<Pixels>,
    /// Pixels per EMU.
    pub scale: f32,
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
    pub fn emu_per_unit(self) -> f32 {
        match self {
            Self::Centimeters => 360_000.0,
            Self::Millimeters => 36_000.0,
            Self::Inches => 914_400.0,
            Self::Points => 12_700.0,
            Self::Picas => 152_400.0,
        }
    }

    /// 単位の略称（ルーラーのコーナー表示等）。
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::Centimeters => "cm",
            Self::Millimeters => "mm",
            Self::Inches => "in",
            Self::Points => "pt",
            Self::Picas => "pi",
        }
    }

    /// ルーラーの基本刻み間隔（EMU）。
    pub fn tick_step_emu(self) -> f32 {
        match self {
            Self::Centimeters => 180_000.0, // 0.5 cm
            Self::Millimeters => 36_000.0,  // 1 mm
            Self::Inches => 114_300.0,      // 1/8 in
            Self::Points => 228_600.0,      // 18 pt
            Self::Picas => 76_200.0,        // 0.5 pica
        }
    }

    /// ラベル数値の間隔候補（単位数）。
    pub fn label_candidates(self) -> &'static [i32] {
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
    pub fn tick_info(self, index: i32) -> (bool, bool, i32) {
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

// The ruler's unit is one setting for the three screens (each screen's menus call these).
impl SlideEditor {
    /// ルーラーの測定単位を取得する (DOCX-VIEW-013).
    pub fn ruler_unit(&self) -> RulerUnit {
        self.ruler_unit
    }

    /// ルーラーの測定単位を設定する (DOCX-VIEW-013).
    pub fn set_ruler_unit(&mut self, unit: RulerUnit, cx: &mut Context<Self>) {
        self.ruler_unit = unit;
        cx.notify();
    }

    /// ルーラーの測定単位を順送りで切り替える (DOCX-VIEW-013).
    pub fn cycle_ruler_unit(&mut self, cx: &mut Context<Self>) {
        self.ruler_unit = self.ruler_unit.next();
        cx.notify();
    }
}

pub struct SlideEditor {
    /// Focus of the workspace; key presses reach the editor through it.
    pub focus_handle: FocusHandle,
    /// Focus of the slide list.
    pub list_focus: FocusHandle,
    pub list_scroll: ScrollHandle,
    /// Trackpad scrolling not yet turned into a page, in pixels.
    wheel: f32,
    /// The menu of the menu bar that is open (shell/menu_bar.rs).
    pub open_menu: Option<crate::menu_bar::OpenMenu>,
    /// Where the pointer was when a menu closed, so that the press that closed it does not open it again.
    pub(crate) menu_dismissed_at: Option<Point<Pixels>>,
    /// A note the menu bar shows for a few seconds (a context menu closed because its target went away).
    pub menu_notice: Option<SharedString>,
    /// The context menus the bar showed at the last frame (so that one that goes away while it is open is noticed).
    pub(crate) bar_contexts: Vec<crate::menu_context::ContextMenu>,
    /// The documents that are open in the app's windows, most recently used first, as of the last frame (the Window
    /// menu's list reads this; the truth is `OpenDocuments`).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) open_documents: Vec<crate::open_documents::OpenDocument>,
    /// Whether this window was the active one at the last frame (a change to active makes it the most recently used).
    #[cfg(not(target_family = "wasm"))]
    window_was_active: bool,
    /// The list of windows (Ctrl+Tab), when it is open.
    #[cfg(not(target_family = "wasm"))]
    pub window_switcher: Option<crate::window_switcher::WindowSwitcher>,
    /// Time of the last pixel scroll, in milliseconds (browser only).
    #[cfg(target_family = "wasm")]
    wheel_at: f64,
    /// Parts of a wheel notch not yet turned into a page (Windows only).
    #[cfg(target_os = "windows")]
    wheel_notches: f32,
    pub document: Document,
    pub document_memory: super::document_memory::DocumentMemoryStore,
    pub current_slide: SlideId,
    pub selection: Option<ShapeId>,
    pub tool: Tool,
    /// The state of editing shapes and their text, the same for every format (`shell/shape_state.rs`).
    pub shape_edit: crate::shape_state::ShapeEditState,
    pub drag: Option<Drag>,
    /// トリミング on the slide (`shell/crop_mode.rs`).
    pub(crate) crop_mode: Option<crate::crop_mode::CropMode>,
    pub editing: Option<TextEdit>,
    /// Bounds of the workspace around the slide, from the last prepaint.
    pub workspace_bounds: Option<Bounds<Pixels>>,
    /// Scrolling of the workspace when the zoomed slide does not fit.
    pub workspace_scroll: ScrollHandle,
    /// Zoom chosen in the status bar, 1.0 being 100%; `None` fits the slide
    /// to the workspace.
    pub zoom: Option<f32>,
    /// The zoom slider of the status bar, in percent.
    pub(crate) zoom_slider: Entity<SliderState>,
    /// Where the status bar was drawn last, in window coordinates: its left
    /// and right edges and the zoom slider's left edge (the sheet tabs'
    /// scrollbar lies between the tabs' left edge and the slider).
    pub status_layout: std::rc::Rc<std::cell::Cell<[f32; 3]>>,
    /// Where the zoom percentage button of the status bar was painted (for the scenarios to press it).
    pub(crate) zoom_controls_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The menu bar's logo (it opens the start window), where it was last drawn.
    pub logo_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Where the title row's Draw button was drawn (the menu hangs from it: `dropdown_list::below_button`).
    /// The keys of the menu bar (Alt, F10): Windows and Linux.
    pub bar_keys: crate::bar_keys::BarKeys,
    pub(crate) draw_button_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub zoom_trigger_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    _zoom_subscription: Subscription,
    /// The status bar's list of zooms is open.
    pub zoom_menu_open: bool,
    /// 「指定」 of the list of zooms: a zoom typed in percent.
    pub(crate) zoom_input: Entity<InputState>,
    _zoom_input_subscription: Subscription,
    /// Whether the zoom dialog is open.
    pub zoom_dialog_open: bool,
    /// The input state inside the zoom dialog.
    pub zoom_dialog_input: Entity<InputState>,
    _zoom_dialog_subscription: Subscription,
    /// EntityId of this SlideEditor, used to identify this window in the Window menu (#111).
    pub entity_id: gpui::EntityId,
    /// Whether the toolbar font size pulldown is open (#120).
    pub font_size_menu_open: bool,
    /// The input state inside the toolbar font size pulldown (#120).
    pub font_size_edit_input:Entity<InputState>,
    pub(crate) font_size_edit_dirty:bool,
    _font_size_edit_subscription:Subscription,
    pub font_size_input: Entity<InputState>,
    _font_size_input_subscription: Subscription,
    /// Scroll handle for the toolbar font size pulldown candidate list (#127, #149).
    pub(crate) font_size_scroll: ScrollHandle,
    /// Currently keyboard-highlighted candidate index in the toolbar font size pulldown (#127).
    pub font_size_selected: Option<usize>,
    /// Whether the user is actively filtering the toolbar font size candidate list (#127, #149).
    pub font_size_filtering: bool,
    /// Search input states for searchable dropdown menus (#98, #103), keyed by button id.
    pub toolbar_search_inputs: std::collections::HashMap<&'static str, Entity<InputState>>,
    /// The hint each search field shows (the text of the language shown when it was set), to set it again when the language changes.
    toolbar_search_hints: std::collections::HashMap<&'static str, String>,
    /// Scroll handles for searchable dropdown menus (#125, #127), keyed by button id.
    pub(crate) toolbar_search_scrolls: std::collections::HashMap<&'static str, ScrollHandle>,
    /// Keyboard-highlighted row index for searchable dropdown menus (#127), keyed by button id.
    pub(crate) toolbar_search_selected: std::collections::HashMap<&'static str, Option<usize>>,
    /// Which searchable dropdown menu is currently open (#98, #103).
    pub toolbar_search_open: Option<&'static str>,
    /// How the searchable list `toolbar_search_open` was asked to be shown by `open_searchable_list` (always a search field, a most rows).
    pub(crate) toolbar_search_spec: Option<(&'static str, crate::chrome::dropdown::ListSpec)>,
    /// Which button (dropdown id, instance index) opened `toolbar_search_open`, when the same dropdown is drawn more than once.
    pub(crate) toolbar_dropdown_instance: Option<(&'static str, usize)>,
    pub toolbar_search_popup_items: Option<(&'static str, Vec<crate::chrome::MenuItem>)>,
    /// Scroll handle for the right-pane sidebar body (#151).
    pub sidebar_scroll: ScrollHandle,
    /// Decoded pictures and fonts for the open presentation.
    pub resources: Resources,
    /// Redraws the editor as pictures decode (`watch_pictures`).
    pub(crate) picture_watch: Option<Task<()>>,
    /// Name of the open file; `None` for a new presentation.
    pub file_name: Option<SharedString>,
    /// Progress or error of the last file operation, for the status bar.
    pub status: Option<SharedString>,
    /// What a save left out, to say once the save is done (an old .xls leaves things out).
    pub save_note: Option<SharedString>,
    /// A chart's workbook (データの編集): the ranges its series take their
    /// data from, framed on the sheet.
    pub chart_ranges: Option<Vec<ibis_pptx::SeriesRefs>>,
    /// The workbook's grid only (no title row, menus, toolbar, side pane or status bar): a workbook editor
    /// embedded in a panel of another editor (the chart's データの編集 in the presentation's pane).
    pub grid_only: bool,
    /// フォントの置換 (the panel shared by the document, spreadsheet and presentation screens: `shell/replace_fonts.rs`).
    pub replace_fonts: Option<crate::replace_fonts::ReplaceFontsState>,
    /// ファイル名を変更 (the dialog of the menu at the file name in the title row: `shell/rename_file.rs`).
    #[cfg(not(target_family = "wasm"))]
    pub rename_file: Option<crate::rename_file::RenameFileState>,
    /// ツール > 設定 (the modal dialog of the app's settings: `shell/settings_dialog.rs`).
    pub settings: Option<crate::settings_dialog::SettingsDialog>,
    /// Recently opened files, most recent first, for ファイル > 履歴.
    #[cfg(not(target_family = "wasm"))]
    pub recent_files: Vec<std::path::PathBuf>,
    /// The password the open file is protected with (パスワードを使用して暗号化):
    /// what is saved is protected again with it.
    #[cfg(not(target_family = "wasm"))]
    pub protection_password: Option<String>,
    /// The colours picked lately from a palette, the last first (for the palette's row of them).
    pub recent_colors: Vec<ibis_doc_model::Rgb>,
    /// The colour last picked from each toolbar palette button (by its id), for the split button's left half.
    pub(crate) last_colors: std::collections::HashMap<&'static str, ibis_doc_model::Rgb>,
    /// The menu item last chosen from each split button of the toolbar (not a colour), by the button's id: what its
    /// left half does at once (`ToolButton::last_used`).
    pub(crate) last_choices: std::collections::HashMap<&'static str, String>,
    /// Scrolling of the recent files list of the start view.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_scroll: ScrollHandle,
    /// Whether each file of the recent list exists, checked once on a background thread (a network share
    /// may be slow), not at every paint; a path with no entry yet is drawn as existing. Cleared when the
    /// start view shows again (`close_file`).
    #[cfg(not(target_family = "wasm"))]
    pub recent_exists: std::collections::HashMap<std::path::PathBuf, bool>,
    /// The check of `recent_exists` is running.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) recent_checking: std::cell::Cell<bool>,
    /// A file is open; until one is, the window shows the start view.
    pub has_document: bool,
    /// How many times a file was closed (`close_file`): a file still being
    /// read when the count changes is not opened.
    pub(crate) closed: u32,
    /// An empty document stands in for the file being read (`begin_loading`).
    pub loading: bool,
    /// What only the open document's format has (see `shell/format.rs`).
    pub format: Box<dyn FormatState>,
    /// What each format keeps for this window over the files loaded into it (`FormatEditor::new_prefs`): one of every
    /// format, never looked into here.
    pub(crate) format_prefs: Vec<Box<dyn std::any::Any>>,
    /// The sidebar is asked for (the title row's toggle, a panel opened). Whether it is on screen is `right_pane`.
    pub sidebar_open: bool,
    /// The last text typed into the find panel (kept after the panel closes: ⌘G / Shift+⌘G find with it).
    pub last_find_query: String,
    pub(crate) sidebar_layout: std::rc::Rc<std::cell::Cell<super::chrome::panel::PanelMeasurements>>,
    /// The row drag and drop of the scrolling tables on screen (Escape gives it up).
    pub active_row_dnd: Option<super::chrome::row_dnd::RowDnd>,
    pub link_hover: bool,
    pub link_text_positions: std::cell::RefCell<std::collections::HashMap<(ibis_doc_model::ShapeId,Option<(usize,usize)>),crate::render::rich_text::TextPositions>>,
    pub link_popup: Option<super::links::LinkPopup>,
    pub theme_colors_ui: super::theme_colors::ThemeColorsUi,
    pub dialog: super::dialog::DialogState,
    _dialog_keystrokes: Subscription,
    pub ruler_settings_open: bool,
    pub ruler_settings_ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    /// Whether the right pane is on screen and whether its toggle can show it, as `render_sidebar` found them this
    /// frame (it runs before the title row and the menus): the one place their buttons read, so a button never says
    /// "shown" for a pane that is not there.
    pub right_pane: crate::chrome::RightPane,
    /// The right pane's width (every panel's), changed by dragging its left edge.
    pub sidebar_width: f32,
    /// The pointer's x and the pane's width when the drag of its left edge began.
    pub(crate) sidebar_drag: Option<(f32, f32)>,
    /// 記号と特殊文字 (挿入 > 特殊文字…), while open in the sidebar.
    pub symbol: Option<crate::symbols::SymbolState>,
    /// A document's outline (概要) is shown.
    pub outline_open: bool,
    /// A document's ruler measurement unit (ルーラーの測定単位, DOCX-VIEW-013).
    pub ruler_unit: RulerUnit,
    /// A document's gridlines (グリッド線) are shown.
    pub gridlines_open: bool,
    /// The title row and the menu bar are hidden (the toolbar's ^).
    pub chrome_compact: bool,
    /// Where the toolbar's bar was drawn last (for the scenarios: its left edge is clear of the Mac window's buttons).
    pub toolbar_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Where the open file came from, for saving it (`shell/save.rs`);
    /// `None` for a new document or one that cannot be saved.
    pub source: Option<std::sync::Arc<ibis_pptx::Source>>,
    /// The file's path, for ⌘S (none in the browser, or for a new document).
    #[cfg(not(target_family = "wasm"))]
    pub path: Option<std::path::PathBuf>,
    /// A unique identifier of the file read into the window, shared by the windows that show the same file
    /// (`FormatInfo::several_windows`). 0: none.
    pub file_id: u64,
    /// Sequence number when multiple windows view the same file (e.g. 1 for Book1:1 / Doc:1, 2 for Book1:2 / Doc:2).
    pub(crate) window_number: Option<usize>,
    /// Whether the save confirmation dialog is open (shown when closing the last window with unsaved edits).
    pub save_confirm_dialog_open: bool,
    /// The open message box's buttons and the one the keyboard is on (shell/message_box.rs).
    pub(crate) msgbox: crate::message_box::MessageBoxKeys,
    pub panel_message: Option<SharedString>,
    /// A message box with OK only (title, message), over the dimmed window: an error the user must see, such as a save
    /// dialog that could not open (shell/dialog.rs `show_alert`).
    pub alert: Option<(SharedString, SharedString)>,
    /// A question with OK and キャンセル (title, message, what OK does), over the window (shell/dialog.rs `ask`).
    pub ask: Option<crate::dialog::PendingAsk>,
    /// The About dialog inside the window, while it is open (shell/about.rs `render_modal`, on every platform).
    pub(crate) about_open: bool,
    /// A comment's delete waiting for the answer (shell/comments.rs).
    pub comment_delete: Option<crate::comments::PendingDelete>,
    /// The CMYK mode was turned on from the colour picker: the picker shows the colours as printed at its next draw.
    pub(crate) cmyk_picker_refresh: bool,
    /// The information bar about the CMYK profile (shell/cmyk_profile.rs), while it is showing.
    pub cmyk_notice: Option<crate::cmyk_profile::CmykNotice>,
    /// Whether the save confirmation was asked by closing the window (its close button, or quitting the app) rather
    /// than by ファイル > 閉じる: once answered, the window goes instead of showing the start view (GitHub issue #27).
    pub close_window_after_confirm: bool,
    /// Whether the confirmation dialog asking to specify the Office executable path is open (#26).
    pub(crate) office_path_confirm_dialog_open: bool,
    /// The watching of the file on disk for changes by other apps, and the question it asks (shell/file_watch.rs).
    pub watch: crate::file_watch::ExternalWatch,
    /// Whether side-by-side comparison mode (並べて比較 / XLSX-VIEW-013) is active.
    pub side_by_side: bool,
    /// Whether synchronous scrolling (同期スクロール / XLSX-VIEW-014) is active.
    pub sync_scrolling: bool,
    /// Paired window and editor entity for side-by-side mode.
    pub(crate) side_by_side_peer: Option<(AnyWindowHandle, Entity<SlideEditor>)>,
    /// Guard against recursive scroll syncing between paired windows.
    pub(crate) is_syncing_scroll: std::cell::Cell<bool>,
    /// Last scroll offset synced with peer window.
    pub(crate) last_synced_scroll: std::cell::Cell<Point<Pixels>>,
    /// Whether this document was opened from an AutoRecover snapshot (XLSX-FILE-015).
    pub is_recovered: bool,
    /// Path of the autorecover backup file associated with this open document (if any).
    pub active_autorecover_file: Option<std::path::PathBuf>,
    /// 自動回復's panels (the settings, the recovery list), while one is the one shown.
    pub autorecover_settings: Option<crate::autorecover::AutoRecoverSettingsPanel>,
    pub recover_unsaved: Option<crate::autorecover::RecoverUnsavedPanel>,
    /// 自動回復 is off for this file.
    pub autorecover_off: bool,
    /// When (ms) its last snapshot was written (or the timer started).
    pub last_autorecover_ms: u64,
    pub(crate) autorecover_timer_started: bool,
    /// Whether this document was opened after repairing corruption (XLSX-FILE-020).
    pub is_repaired: bool,
    /// Summary text of repairs applied (XLSX-FILE-020).
    pub repair_summary: Option<String>,
    /// Detailed report of repairs applied (XLSX-FILE-020).
    pub repair_report: Option<ibis_pptx::repair::RepairReport>,
    /// The open file was read from an old binary format (.xls, .ppt, .doc) and not yet saved in a current one: the compatibility bar is shown.
    pub opened_from_legacy: bool,
    /// Whether this document is opened in read-only / viewing mode (--read-only or ?view=1).
    pub read_only: bool,
    /// The password to modify the file (書き込みパスワード) was given, or set here; until then a file that has one is read-only.
    pub(crate) modify_unlocked: bool,
    /// Whether the menu bar is shown (false by default when ?view=1 unless ?menuBar=1).
    pub show_menu_bar: bool,
    /// Whether the toolbar is shown (false by default when ?view=1 unless ?toolBar=1).
    pub show_toolbar: bool,
    /// Whether the editor is currently streaming CSV rows in the background.
    pub(crate) is_csv_streaming: bool,
}

static NEXT_FILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SlideEditor {
    pub fn new(mut presentation: Presentation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::app::track_start_window(cx.entity_id(),Some(window.window_handle()),true,cx);
        let owner = window.window_handle();
        let editor = cx.weak_entity();
        let dialog_keystrokes = cx.intercept_keystrokes(move |event, window, app| {
            if window.window_handle() == owner {
                editor.update(app, |this, cx| {
                    if this.panel_message.is_some() && matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                        this.dismiss_panel_message(cx);
                        cx.stop_propagation(); return;
                    }
                    // Every message box by the keyboard (K0012, shell/message_box.rs): Tab and Shift+Tab move the ring over its
                    // buttons, Enter and Space press the ringed one, Esc presses the cancel.
                    if matches!(this.interaction_layer(), crate::dialog::InteractionLayer::Save | crate::dialog::InteractionLayer::FileChanged
                        | crate::dialog::InteractionLayer::CommentDelete | crate::dialog::InteractionLayer::Alert | crate::dialog::InteractionLayer::Ask)
                        && !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control && !event.keystroke.modifiers.alt {
                        use crate::message_box::KeyResult;
                        match this.msgbox.key(event.keystroke.key.as_str(), event.keystroke.modifiers.shift) {
                            KeyResult::Ignored => {}
                            KeyResult::Moved => { cx.notify(); cx.stop_propagation(); return; }
                            KeyResult::Nothing => { cx.stop_propagation(); return; }
                            KeyResult::Press(handler) => {
                                window.defer(cx, move |w, cx| handler(&ClickEvent::default(), w, cx));
                                cx.stop_propagation(); return;
                            }
                        }
                    }
                    if this.interaction_layer().is_confirmation() { return; }
                    // The menu bar has the focus (F10): its arrows, Enter and Esc come before the actions bound to them.
                    if this.bar_keys.focus.is_some() && this.bar_key_down(&event.keystroke, window, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    // Reorder capture outlives a format's focus/action context. Cancel it before
                    // DOCX's CancelFormatBrush or a focused input can consume Escape.
                    if event.keystroke.key == "escape" && this.cancel_table_reorder(window, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    if event.keystroke.key=="escape" && this.dismiss_toolbar_dropdown(window,cx) {cx.stop_propagation();return;}
                    if this.crop_mode_key(&event.keystroke, cx) { with_format!(this, crop_fields_synced(this, window, cx)); cx.stop_propagation(); return; }
                    if with_format!(this, format_key(this, &event.keystroke, window, cx)) { cx.stop_propagation(); return; }
                    if with_format!(this, range_pick_key(this, &event.keystroke, window, cx)) { return; }
                    if event.keystroke.key=="escape" && matches!(this.dialog.key,Some("sort_level_editor"|"conditional_rule_editor")) && this.toolbar_search_open.is_none(){this.close_dialog(window,cx);cx.stop_propagation();return;}
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
        for (_, driver) in crate::format::drivers() {
            driver.prepare_model(&mut presentation, &resources, &window.text_system().clone(), None, false);
        }
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
        let format = state_for_model(&presentation, window, cx);
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
                    let candidates = crate::chrome::filtered_font_size_candidates(&query, this.font_size_filtering);
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
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                }
                _ => {}
            }
        });
        let entity_id = cx.entity_id();
        Self {
            focus_handle: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            list_scroll: ScrollHandle::new(),
            wheel: 0.0,
            open_menu: None,
            menu_dismissed_at: None,
            menu_notice: None,
            bar_contexts: Vec::new(),
            #[cfg(not(target_family = "wasm"))]
            open_documents: Vec::new(),
            #[cfg(not(target_family = "wasm"))]
            window_was_active: false,
            #[cfg(not(target_family = "wasm"))]
            window_switcher: None,
            #[cfg(target_family = "wasm")]
            wheel_at: f64::NEG_INFINITY,
            #[cfg(target_os = "windows")]
            wheel_notches: 0.0,
            document: Document::new(presentation),
            document_memory: Default::default(),
            current_slide,
            selection: None,
            shape_edit: Default::default(),
            tool: Tool::Select,
            drag: None,
            crop_mode: None,
            editing: None,
            workspace_bounds: None,
            workspace_scroll: ScrollHandle::new(),
            zoom: None,
            zoom_slider,
            status_layout: Default::default(),
            zoom_trigger_bounds: Default::default(),
            zoom_controls_bounds: Default::default(),
            logo_bounds: Default::default(),
            draw_button_bounds: Default::default(),
            bar_keys: Default::default(),
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
            toolbar_search_hints: Default::default(),
            toolbar_search_scrolls: Default::default(),
            toolbar_search_selected: Default::default(),
            toolbar_search_open: None,
            toolbar_search_spec: None,
            toolbar_dropdown_instance: None,
            toolbar_search_popup_items: None,
            sidebar_scroll: ScrollHandle::new(),
            resources,
            picture_watch: None,
            file_name: None,
            status: None,
            save_note: None,
            chart_ranges: None,
            grid_only: false,
            replace_fonts: None,
            #[cfg(not(target_family = "wasm"))]
            rename_file: None,
            settings: None,
            #[cfg(not(target_family = "wasm"))]
            recent_files: crate::recent_files::load(),
            #[cfg(not(target_family = "wasm"))]
            protection_password: None,
            recent_colors: Vec::new(),
            last_colors: Default::default(),
            last_choices: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            recent_scroll: ScrollHandle::new(),
            #[cfg(not(target_family = "wasm"))]
            recent_exists: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            recent_checking: Default::default(),
            has_document: false,
            closed: 0,
            loading: false,
            format,
            format_prefs: crate::format::drivers().iter().map(|(_, driver)| driver.new_prefs()).collect(),
            sidebar_open: true,
            last_find_query: String::new(),
            sidebar_layout:Default::default(),
            active_row_dnd:None,
            link_hover: false,
            link_text_positions: Default::default(),
            link_popup: None,
            theme_colors_ui: Default::default(),
            dialog: Default::default(),
            _dialog_keystrokes: dialog_keystrokes,
            ruler_settings_open: false,
            ruler_settings_ui: Default::default(),
            right_pane: Default::default(),
            sidebar_width: crate::chrome::SIDEBAR_WIDTH,
            sidebar_drag: None,
            symbol: None,
            outline_open: false,
            ruler_unit: RulerUnit::default(),
            gridlines_open: false,
            chrome_compact: false,
            toolbar_bounds: Default::default(),
            source: None,
            opened_from_legacy: false,
            #[cfg(not(target_family = "wasm"))]
            path: None,
            file_id: NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            window_number: None,
            save_confirm_dialog_open: false,
            msgbox: Default::default(),
            panel_message: None,
            alert: None,
            ask: None,
            about_open: false,
            comment_delete: None,
            cmyk_picker_refresh: false,
            cmyk_notice: None,
            close_window_after_confirm: false,
            office_path_confirm_dialog_open: false,
            watch: Default::default(),
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
            read_only: crate::embed::is_view_only(),
            #[cfg(not(target_arch = "wasm32"))]
            read_only: false,
            modify_unlocked: false,
            #[cfg(target_arch = "wasm32")]
            show_menu_bar: crate::embed::show_menu_bar(),
            #[cfg(not(target_arch = "wasm32"))]
            show_menu_bar: true,
            #[cfg(target_arch = "wasm32")]
            show_toolbar: crate::embed::show_tool_bar(),
            #[cfg(not(target_arch = "wasm32"))]
            show_toolbar: true,
            is_csv_streaming: false,
        }
    }

    /// Make `source` (of the file just given to `load`) the one saving writes from. `load` changes the model as it opens it
    /// (fields, tables of contents, pages, pivots, row heights, dates), which is not an edit: the model as it is now is
    /// the baseline, so saving without an edit writes the original file (`ibis_pptx::Source::rebased`).
    ///
    /// The exception is a table of contents the file marks for making again (`w:dirty`, `w:updateFields`): Word makes it as it
    /// opens the file and saves it made (`docx::toc017_unupdated_toc_is_made_on_open`), so that stays a change to save.
    pub fn set_loaded_source(&mut self, source: std::sync::Arc<ibis_pptx::Source>) {
        let marked_toc = source.baseline().flow.as_ref().is_some_and(|doc| {
            doc.body.iter().any(|b| {
                matches!(b, ibis_doc_model::Block::Paragraph(p) if (doc.update_fields || p.field_dirty) && p.field_start.as_ref().is_some_and(|f| f.trim_start().to_ascii_uppercase().starts_with("TOC")))
            })
        });
        self.source = Some(if marked_toc { source } else { source.rebased(self.presentation()) });
    }

    /// Whether this editor is currently in read-only / viewing mode (either via `--read-only` or during CSV streaming).
    pub fn is_read_only(&self) -> bool {
        self.read_only || self.is_csv_streaming || self.is_xlsx_mark_as_final() || self.is_modify_locked()
    }

    /// Replace the open presentation, dropping undo history and selection.
    pub fn load(&mut self, mut presentation: Presentation, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        with_format!(self, show_end(self, window, cx));
        self.opened_from_legacy = presentation.opened_from_legacy;
        self.modify_unlocked = false;
        self.zoom_menu_open = false;
        self.editing = None;
        self.drag = None;
        self.selection = None;
        self.tool = Tool::Select;
        self.symbol = None;
        let timing = crate::clock::debug_timing::start();
        self.resources = Resources::new(&presentation, cx);
        // What the formats' settings of the window say goes on with the next file (a document's display of hidden text).
        for (_, driver) in crate::format::drivers() {
            driver.resources_renewed(self);
        }
        self.watch_pictures(window, cx);
        crate::clock::debug_timing::report("load: resources", timing);
        let timing = crate::clock::debug_timing::start();
        for (_, driver) in crate::format::drivers() {
            driver.prepare_model(&mut presentation, &self.resources, &window.text_system().clone(), Some(&name), true);
        }
        crate::clock::debug_timing::report("load: prepare_model (update_fields_on_open, expand_pages, pivots, row heights)", timing);
        // A presentation's date fields show today, as PowerPoint's do.
        if presentation.flow.is_none() && presentation.workbook.is_none() {
            presentation.refresh_dates(&crate::clock::local_now(), &ibis_pptx::script_pieces);
        }
        self.current_slide = presentation.slides[0].id;
        self.format = state_for_model(&presentation, window, cx);
        // What was selected, copied or being typed in the file before is not the new file's (the format's own state is new too).
        self.shape_edit = Default::default();
        // A workbook's and a presentation's right pane is there when asked
        // for (a workbook's tabs have no panel to show until one is chosen;
        // either would take the room of the sheet or the slide, which is
        // fitted to the width without it): `FormatEditor::sidebar_on_request`.
        self.sidebar_open = !self.is_read_only() && !with_format!(self, sidebar_on_request());
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
        // The document's split, reading mode, focus mode and the way its pages move are in its own state, which is
        // new (`docx::State`).
        window.set_window_title(&format!("{name} - ibisOffice"));
        self.file_name = Some(name);
        self.has_document = true;
        crate::app::track_start_window(cx.entity_id(),None,false,cx);
        self.loading = false;
        self.file_id = NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.window_number = None;
        self.save_confirm_dialog_open = false;
        self.office_path_confirm_dialog_open = false;
        // The file read before is no longer this window's (`open_read` records the new one).
        self.watch.dialog = false;
        self.watch.asked = None;
        self.watch.known = None;
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
            .map(|_| crate::i18n::text("_shell.macros-in-this-file-will-not-run-they-are-preser"));
        // A presentation opens with the keys on the slide list (Home, End,
        // the arrows move between slides without a click first).
        if self.pages_are_slides() {
            self.list_focus.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        self.check_cmyk_profile_on_open(cx);
        // A workbook's sheets open where the file left them (selection, scroll); a document saved in the outline view opens
        // with its navigation pane.
        with_format!(self, document_loaded(self));
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
                prompt: Some(crate::i18n::text("_shell.open").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.open_path_or_raise(path, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let (name, data) = match crate::web::pick_file().await {
                Ok(Some(file)) => file,
                Ok(None) => return,
                Err(err) => {
                    this.update(cx, |this, cx| this.set_status(crate::i18n::format("file.pick-failed", &[("error", &err)]), cx)).ok();
                    return;
                }
            };
            let name: SharedString = name.into();
            let closed = this.update(cx, |this, _| this.closed).unwrap_or_default();
            let kind = name.rsplit_once('.').and_then(|(_, e)| ibis_pptx::Kind::from_extension(e));
            this.update_in(cx, |this, window, cx| match kind {
                Some(kind) if !this.has_document => this.begin_loading(kind, name.clone(), window, cx),
                _ => this.set_status(crate::i18n::format("file.opening", &[("name", &name)]), cx),
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
                        crate::file_actions::new_file_name(ibis_pptx::Kind::of(&opened.presentation))
                    } else {
                        name
                    };
                    this.load(opened.presentation, doc_name, window, cx);
                    this.set_loaded_source(std::sync::Arc::new(opened.source));
                    // (A presentation opens without the speaker notes, as a new one does.)
                    with_format!(this, file_opened(this));
                    if is_ppsx {
                        with_format!(this, show_start(this, window, cx));
                    }
                }
                Err(err) => this.fail_loading(crate::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]), cx),
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
    pub fn open_path(&mut self, path: std::path::PathBuf, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file::read_file(&read_path), slide, window, cx);
    }

    /// Open `path` as `open_path` does, with `read` reading it (on a
    /// background thread; it may wait for a read started earlier).
    #[cfg(not(target_family = "wasm"))]
    pub fn open_read(
        &mut self,
        path: std::path::PathBuf,
        read: impl FnOnce() -> Result<crate::read_file::Loaded, String> + Send + 'static,
        slide: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_edit(window, cx);
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        let kind = path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension);
        match kind {
            Some(kind) if !self.has_document => self.begin_loading(kind, name.clone(), window, cx),
            _ => self.set_status(crate::i18n::format("file.opening", &[("name", &name)]), cx),
        }
        // What the file is as the read starts: a change by another app after that is found by the watching.
        let stamp_before_read = crate::file_watch::DiskStamp::stat(&path);
        // This path is taken: opening it again (Finder, the Open dialog) raises this window instead.
        self.watch.opening_path = Some(path.clone());
        let closed = self.closed;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { read() }).await;
            this.update(cx, |this, _| this.watch.opening_path = None).ok();
            this.update_in(cx, |this, window, cx| {
            let restore = this.watch.restore_view.take().filter(|(p, _)| *p == path).map(|(_, view)| view);
            match result {
                // Closed (ファイル > 閉じる) while it was read.
                _ if this.closed != closed => {}
                Ok(loaded) => {
                    let is_tmpl = path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
                    if is_tmpl {
                        let tmpl_name = crate::file_actions::new_file_name(ibis_pptx::Kind::of(&loaded.presentation));
                        this.load(loaded.presentation, tmpl_name, window, cx);
                        this.set_loaded_source(loaded.source);
                        this.path = None;
                        this.document.set_file(None);
                    } else if path.extension().is_some_and(|e| ["xls", "ods", "ppt", "pps", "pot", "odp", "otp", "odt", "ott", "doc", "dot", "rtf", "txt", "text", "html", "htm", "pdf"].iter().any(|x| e.eq_ignore_ascii_case(x))) {
                        // An old workbook or presentation (or an OpenDocument workbook) is read as a new .xlsx or .pptx: saving asks where, and leaves the old file as it is.
                        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let extension = crate::format::info(ibis_pptx::Kind::of(&loaded.presentation)).extension;
                        this.load(loaded.presentation, format!("{stem}.{extension}").into(), window, cx);
                        this.set_loaded_source(loaded.source);
                        this.path = None;
                        this.document.set_file(None);
                    } else {
                        this.load(loaded.presentation, name, window, cx);
                        // A sheet read in chunks is saved as changes from the empty sheet it starts as.
                        if loaded.csv_stream.is_some() {
                            this.source = Some(loaded.source);
                        } else {
                            this.set_loaded_source(loaded.source);
                        }
                        this.set_file_path(Some(path.clone()));
                        this.watch.known = stamp_before_read;
                        this.protection_password = loaded.password;
                    }
                    // (A presentation opens with its notes pane as the file's normal view left it.)
                    with_format!(this, file_opened(this));
                    if let Some(rx) = loaded.csv_stream {
                        this.is_csv_streaming = true;
                        this.set_status(crate::i18n::text("editor.csv.loading-first").to_string(), cx);
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
                                                this.sync_peers(cx);
                                            }
                                        }
                                    }
                                    let formatted = format_commas(current_rows);
                                    if is_last {
                                        this.is_csv_streaming = false;
                                        this.set_status(crate::i18n::format("editor.csv.loaded", &[("rows", &formatted)]), cx);
                                    } else {
                                        this.set_status(crate::i18n::format("editor.csv.loading", &[("rows", &formatted)]), cx);
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
                    if let Some(view) = restore {
                        this.restore_view_state(&view, window, cx);
                    }
                    this.recent_files = crate::recent_files::add(&path);
                    crate::timing::document_loaded(window);
                    if path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow) {
                        with_format!(this, show_start(this, window, cx));
                    }
                    // A file with a password to modify: asked for, or read as it is.
                    if this.is_modify_locked() {
                        this.ask_modify_password_to_unlock(window, cx);
                    }
                }
                // A protected file: its password is asked for.
                Err(err) if err == crate::read_file::NEEDS_PASSWORD || err == crate::read_file::WRONG_PASSWORD => {
                    this.fail_loading(String::new(), cx);
                    crate::password_window::ask_to_open(cx.entity().downgrade(), path.clone(), slide, err == crate::read_file::WRONG_PASSWORD, window, cx);
                }
                Err(err) => {
                    let message = crate::i18n::format("file.open-failed", &[("name", &name), ("error", &err)]);
                    // A file that cannot be opened (gone, no permission, broken) is told in a message box, not only in the status.
                    this.show_alert(crate::i18n::text("shell.open-failed.title"), message.clone().into(), cx);
                    this.fail_loading(message, cx)
                }
            }})
            .ok();
        })
        .detach();
    }

    /// Open protected file `path` with `password` (`open_path` for a file that asked for it).
    #[cfg(not(target_family = "wasm"))]
    pub fn open_path_with_password(&mut self, path: std::path::PathBuf, password: String, slide: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        self.open_read(path, move || crate::read_file::read_file_with(&read_path, Some(&password)), slide, window, cx);
    }

    /// Show an empty document of `kind`, titled `name`, with 「読み込み中...」
    /// over it until the file is loaded (`load`) or cannot be
    /// (`fail_loading`).
    pub fn begin_loading(&mut self, kind: ibis_pptx::Kind, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
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
        crate::app::track_start_window(cx.entity_id(),None,true,cx);
        self.document_memory = Default::default();
            self.file_name = None;
        }
        self.set_status(status, cx);
    }

    /// ファイル > 閉じる: close the open file and show the start view
    /// (スタート画面) in the window, as Office does.
    pub fn close_file(&mut self, _: &CloseFile, window: &mut Window, cx: &mut Context<Self>) {
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
        if self.format_info().several_windows.is_some() && self.file_id != 0 {
            let my_id = self.file_id;
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
                self.file_id = 0;
                self.window_number = None;
                window.remove_window();
                Self::renumber_windows(my_id, cx);
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
    pub fn ask_to_save(&mut self, close_window: bool, cx: &mut Context<Self>) {
        self.close_window_after_confirm = close_window;
        self.save_confirm_dialog_open = true;
        self.msgbox.clear();
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
        let shared = self.format_info().several_windows.is_some() && !Self::find_other_matching_windows(self.file_id, me, cx).is_empty();
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

    pub fn is_dirty(&self) -> bool {
        self.has_document
            && (self.document.is_dirty()
                || with_format!(self, has_pending_edit(self)))
    }

    pub fn force_close_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        with_format!(self, show_end(self, window, cx));
        self.editing = None;
        // The file is gone from the window: its AutoRecover snapshot goes, and it is no longer a recovered or a repaired one. For
        // every format (a document kept both until 2026-11-11: the branch here named the workbook and the presentation, and was
        // not widened when the document got AutoRecover).
        self.cleanup_active_autorecover_file();
        self.is_recovered = false;
        self.is_repaired = false;
        self.repair_summary = None;
        self.repair_report = None;
        // An empty presentation, as a new window has, with the state of the
        // file reset (`load`) and then the start view.
        self.load(Presentation::new(), SharedString::default(), window, cx);
        self.list_scroll.set_offset(Point::default());
        self.has_document = false;
        crate::app::track_start_window(cx.entity_id(),None,true,cx);
        self.document_memory = Default::default();
        self.closed += 1;
        self.file_name = None;
        self.file_id = 0;
        self.window_number = None;
        window.set_window_title(&crate::start_view::start_title());
        // The file just closed is first, as other windows may have opened
        // files since this one's list was read.
        #[cfg(not(target_family = "wasm"))]
        {
            self.recent_files = crate::recent_files::load();
            self.recent_scroll.set_offset(Point::default());
            self.recent_exists.clear();
        }
    }

    /// 「読み込み中...」in the middle of the workspace while the file loads.
    pub fn render_loading(&self, cx: &Context<Self>) -> Option<Div> {
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
                    .child(crate::i18n::text("_shell.loading"))
                    .when(progress > 0.0, |this| {
                        this.child(div().absolute().left_0().bottom_0().w_full().h(px(3.)).bg(theme.border))
                            .child(div().absolute().left_0().bottom_0().w(relative(progress.min(1.0))).h(px(3.)).bg(theme.primary))
                    }),
            ),
        )
    }

    pub fn set_status(&mut self, status: String, cx: &mut Context<Self>) {
        self.status = Some(status.into());
        cx.notify();
    }

    // ----- Reading state -------------------------------------------------

    pub fn presentation(&self) -> &Presentation {
        self.document.presentation()
    }

    pub fn slide(&self) -> &Slide {
        self.presentation()
            .slide(self.current_slide)
            .unwrap_or(&self.presentation().slides[0])
    }

    pub fn selected_shape(&self) -> Option<&Shape> {
        self.selection.and_then(|id| self.slide().shape(id))
    }

    /// The worksheet of the current page and the shape that holds it.
    pub fn current_sheet(&self) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
        sheet_of(self.slide())
    }

    /// The worksheet of the current slide, mutable.
    #[allow(dead_code)]
    pub(crate) fn current_sheet_mut(&mut self) -> Option<&mut Sheet> {
        let slide = self.current_slide;
        ibis_doc_model::sheet_command::sheet_mut(self.document.presentation_mut(), slide).ok()
    }

    /// The scrolled content of the workspace at the current zoom, as the
    /// open document's format lays it out (`FormatEditor::workspace_content`).
    pub fn workspace_content(&self) -> Option<WorkspaceContent> {
        let ws = self.workspace_bounds?;
        Some(with_format!(self, workspace_content(self, ws.size)))
    }

    /// A page `page` big in a workspace `view` big: fitted into it (less the
    /// space `before` and `after` it on each axis, in pixels), or at the
    /// chosen zoom; in the top-left corner after `before` (`top_left`), or
    /// centred while it fits.
    pub fn page_content(&self, view: Size<Pixels>, page: ibis_doc_model::EmuSize, before: Point<f32>, after: Point<f32>, top_left: bool) -> WorkspaceContent {
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
    pub fn slide_margin(&self) -> f32 {
        if self.zoom.is_none() { FIT_MARGIN } else { WORKSPACE_MARGIN }
    }

    /// Where the slide sits in the window, as the workspace is scrolled.
    pub fn slide_layout(&self) -> Option<SlideLayout> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let origin = ws.origin + self.workspace_scroll.offset() + content.slide_origin;
        Some(SlideLayout { origin, scale: content.scale })
    }

    /// Pixels per inch at 100%: a slide's, or a worksheet's (the screen's size
    /// only: a PDF and a print keep the real size).
    pub fn px_per_inch_at_100(&self) -> f32 {
        self.format_info().px_per_inch_at_100
    }

    /// The zoom shown, 1.0 being 100%.
    pub fn current_zoom(&self) -> f32 {
        self.zoom.or_else(|| self.slide_layout().map(|l| l.zoom(self.px_per_inch_at_100()))).unwrap_or(1.0)
    }

    /// Zoom to `zoom` (clamped to the range), or fit the slide with `None`.
    /// The point of the slide in the middle of the workspace stays there.
    pub fn set_zoom(&mut self, zoom: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
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
        with_format!(self, zoom_set(self, zoom));
        cx.notify();
    }

    /// The wheel over the workspace with the zoom modifier held (see
    /// `wheel_zooms`) zooms about the pointer: in with the wheel turned up
    /// (away from you), a notch by `WHEEL_ZOOM_STEP`, a trackpad smoothly.
    /// Whether the wheel was taken for zooming.
    pub(crate) fn on_wheel_zoom(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.dialog.active && self.dialog.bounds.is_some_and(|b| b.contains(&event.position)) { return false; }
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
    pub fn open_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn apply_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn fit_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

        super::dialog::confirmation_surface("zoom-dialog-backdrop", false)
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
                            .child(div().font_weight(FontWeight::BOLD).text_base().child(crate::i18n::text("zoom.dialog.title")))
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
                                    .child(crate::i18n::text("zoom.dialog.presets")),
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
                                            .child(crate::i18n::text("zoom.dialog.fit"))
                                            .on_click(cx.listener(|this, _, window, cx| this.fit_zoom_dialog(window, cx))),
                                    ),
                            ),
                    )
                    // 3. 倍率入力
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_sm().child(crate::i18n::text("zoom.dialog.percent")))
                            .child(Input::new(&input).w(px(80.)))
                            .child(div().text_sm().child("%")),
                    )
                    // 4. ボタン群
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                crate::chrome::action_button("zoom-dialog-cancel")
                                    .label(crate::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_zoom_dialog(cx))),
                            )
                            .child(
                                crate::chrome::primary_button("zoom-dialog-ok")
                                    .label(crate::i18n::text("OK"))
                                    .on_click(cx.listener(|this, _, window, cx| this.apply_zoom_dialog(window, cx))),
                            ),
                    ),
            )
    }

    /// Open a new window duplicating current presentation or open path.
    pub fn new_window(&mut self, cx: &mut App) {
        let document_memory=self.document_memory.clone();
        if self.format_info().several_windows.is_some() {
            // The same file in another window: the two share the document and the file's id.
            let file_id = if self.file_id == 0 {
                let id = NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.file_id = id;
                id
            } else {
                self.file_id
            };
            let pres = self.presentation().clone();
            let doc = self.document.clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::i18n::text(self.format_info().untitled_key).into());
            let path = self.file_path();
            let source = self.source.clone();
            let current = self.current_slide;
            let current_zoom = self.zoom;
            let existing_count = Self::count_matching_windows(file_id, cx) + 1;
            if self.window_number.is_none() {
                self.window_number = Some(1);
            }
            let new_num = existing_count + 1;
            crate::app::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name.clone(), window, cx);
                editor.document = doc;
                editor.document_memory=document_memory.clone();
                editor.set_file_path(path);
                editor.source = source;
                editor.file_id = file_id;
                editor.window_number = Some(new_num);
                editor.current_slide = current;
                editor.zoom = current_zoom;
                with_format!(editor, relayout_if_changed(editor));
                let title = format!("{name}:{new_num}");
                window.set_window_title(&format!("{title} - ibisOffice"));
            });
        } else if let Some(path) = self.file_path() {
            let slide = self.presentation().slide_index(self.current_slide).map(|i| i + 1);
            crate::app::open_window_with(cx, move |editor, window, cx| {
                #[cfg(not(target_family = "wasm"))]
                editor.open_path(path, slide, window, cx);
                #[cfg(target_family = "wasm")]
                let _ = (editor, path, slide, window, cx);
            });
        } else {
            let pres = self.presentation().clone();
            let name = self.file_name.clone().unwrap_or_else(|| crate::i18n::text("file.new.presentation").into());
            let current = self.current_slide;
            crate::app::open_window_with(cx, move |editor, window, cx| {
                editor.load(pres, name, window, cx);
                editor.current_slide = current;
            });
        }
    }

    /// The file's path (always `None` in the browser).
    pub fn file_path(&self) -> Option<std::path::PathBuf> {
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
    pub fn set_file_path(&mut self, path: Option<std::path::PathBuf>) {
        #[cfg(not(target_family = "wasm"))]
        {
            self.document.set_file(path.as_ref().map(|p| p.to_string_lossy().into_owned()));
            // Another app changing the file is watched for (shell/file_watch.rs); what it is now is what this window has.
            if let Some(path) = &path {
                self.watch.known = crate::file_watch::DiskStamp::stat(path);
                crate::file_watch::watch(path);
            }
            self.path = path;
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = path;
        }
    }

    /// The other windows that show the file `file_id`.
    pub(crate) fn find_other_matching_windows(
        file_id: u64,
        my_entity_id: gpui::EntityId,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if file_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id && entity.read(app).file_id == file_id {
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

    /// Every window that shows the file `file_id`.
    pub fn find_matching_windows(file_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        if file_id == 0 {
            return Vec::new();
        }
        let mut matching = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.read(app).file_id == file_id {
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

    pub(crate) fn count_matching_windows(file_id: u64, cx: &mut App) -> usize {
        Self::find_matching_windows(file_id, cx).len()
    }

    /// The numbers of the windows on one file (`Book1:1`, `Book1:2`; none when one window is left).
    pub(crate) fn renumber_windows(file_id: u64, cx: &mut App) {
        if file_id == 0 {
            return;
        }
        let matching = Self::find_matching_windows(file_id, cx);
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

    pub fn display_title_formatted(&self) -> String {
        let base = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| crate::i18n::text(self.format_info().untitled_key).to_string());
        // Only a window that shares its file with another has a number.
        if let Some(num) = self.window_number {
            return format!("{base}:{num}");
        }
        base
    }

    /// The model changed in this window: the other windows on the same file take it (`FormatEditor::model_from_peer`).
    pub fn sync_peers(&self, cx: &mut Context<Self>) {
        if self.format_info().several_windows.is_none() || self.file_id == 0 {
            return;
        }
        let my_id = self.file_id;
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
                                if peer.file_id == my_id {
                                    peer.document = doc.clone();
                                    with_format!(peer, model_from_peer(peer));
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

    /// The other windows of the format `kind`, for 並べて比較: those on the file `file_id` if there are any, else those on other files.
    pub(crate) fn find_other_windows_of(
        my_entity_id: gpui::EntityId,
        kind: ibis_pptx::Kind,
        file_id: u64,
        cx: &mut App,
    ) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
        let mut same = Vec::new();
        let mut different = Vec::new();
        for &w in &cx.windows() {
            if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                if let Ok(Some((entity, is_same))) = root.update(cx, |root, _, app| {
                    if let Ok(entity) = root.view().clone().downcast::<SlideEditor>() {
                        if entity.entity_id() != my_entity_id && entity.read(app).kind() == kind {
                            let is_same = file_id != 0 && entity.read(app).file_id == file_id;
                            return Some((entity, is_same));
                        }
                    }
                    None
                }) {
                    if is_same {
                        same.push((w, entity));
                    } else {
                        different.push((w, entity));
                    }
                }
            }
        }
        if !same.is_empty() {
            same
        } else {
            different
        }
    }

    pub fn toggle_view_side_by_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let other = Self::find_other_windows_of(my_id, self.kind(), self.file_id, cx);
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

        Self::arrange_side_by_side_windows(self.compares_left_right(), window, peer_window_handle, cx);
        cx.notify();
    }

    /// 並べて比較 puts the two windows left and right (`FormatInfo::several_windows`), not one over the other.
    fn compares_left_right(&self) -> bool {
        self.format_info().several_windows.as_ref().is_some_and(|w| w.compare_left_right)
    }

    pub(crate) fn arrange_side_by_side_windows(
        left_right: bool,
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
        let (tile_w, tile_h) = if left_right {
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

    pub fn reset_side_by_side_positions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            Self::arrange_side_by_side_windows(self.compares_left_right(), window, peer_handle, cx);
            cx.notify();
        }
    }

    pub fn toggle_sync_scrolling(&mut self, cx: &mut Context<Self>) {
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

    pub fn scroll_workspace_by(&mut self, delta: Point<Pixels>, cx: &mut Context<Self>) {
        let cur = self.workspace_scroll.offset();
        self.workspace_scroll.set_offset(point(cur.x + delta.x, cur.y + delta.y));
        self.clamp_workspace_scroll();
        // Only two windows put side by side scroll together (`toggle_view_side_by_side`).
        if self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        cx.notify();
    }

    /// The message box button the keyboard is on (0 is the first button).
    pub fn message_box_focus(&self) -> usize { self.msgbox.focus() }
    pub fn close_save_confirm_dialog(&mut self, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        self.close_window_after_confirm = false;
        cx.notify();
    }

    /// The file is on the disk (「Office で開く」, which only macOS and Windows have: the title row's button and the Window
    /// menu's line).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
        self.format_info().office.name
    }

    pub(crate) fn office_setting_key(&self) -> &'static str {
        self.format_info().office.setting_key
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn detect_office_app_path(&self) -> Option<std::path::PathBuf> {
        let key = self.office_setting_key();
        if let Some(saved) = crate::settings::get(key) {
            let p = std::path::PathBuf::from(&saved);
            if p.exists() {
                return Some(p);
            }
        }
        #[cfg(target_os = "macos")]
        {
            let bundle_name = self.format_info().office.bundle;
            let mut candidates = vec![std::path::PathBuf::from("/Applications").join(bundle_name)];
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(std::path::PathBuf::from(home).join("Applications").join(bundle_name));
            }
            for cand in candidates {
                if cand.exists() {
                    if let Some(s) = cand.to_str() {
                        crate::settings::set(key, s);
                    }
                    return Some(cand);
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            let exe_name = self.format_info().office.exe;
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
                            crate::settings::set(key, s);
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
        // Through the gate: a scenario that presses Word で開く opens no Word (external.rs).
        if !crate::external::may_launch(|| format!("{} {}", app_path.display(), self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default())) {
            return;
        }
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
        #[cfg(target_family = "wasm")]
        let _ = window;
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
                prompt: Some(crate::i18n::text("_shell.open").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, _, _| {
                    if let Some(s) = path.to_str() {
                        crate::settings::set(key, s);
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
        let title = crate::i18n::text("window.open-in-office.confirm-title");
        let message = crate::i18n::format("window.open-in-office.specify-path", &[("app", &self.office_app_name())]);

        super::dialog::confirmation_surface("office-path-confirm-dialog-backdrop", false)
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
                                crate::chrome::action_button("office-path-confirm-cancel")
                                    .label(crate::i18n::text("_shell.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_office_path_confirm_dialog(cx))),
                            )
                            .child(
                                crate::chrome::primary_button("office-path-confirm-ok")
                                    .label("OK")
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm_office_path_dialog(window, cx))),
                            ),
                    ),
            )
    }

    pub fn confirm_save_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_confirm_dialog_open = false;
        // Closed only once the file is written: a new file asks where first, and a cancelled or failed save keeps it open.
        self.save_then(window, cx, |this, window, cx| this.finish_close_after_confirm(window, cx));
    }

    pub fn confirm_discard_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preserve_or_cleanup_autorecover_on_discard();
        self.finish_close_after_confirm(window, cx);
    }

    pub(crate) fn render_save_confirm_dialog(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.file_name.as_ref().map(|s| s.to_string()).unwrap_or_else(|| crate::i18n::text(self.format_info().untitled_key).to_string());
        let message = crate::i18n::text("fix.editor.do-you-want-save-changes");

        use crate::chrome::{ActionButtonKind, ActionDefinition, action_bar};
        // The shared message box and action bar: the buttons get their Alt access keys there (shell/access_key.rs).
        // The order is 保存, キャンセル, 保存しない (user decision 2026-11-12): 保存 is the primary button (Enter presses it,
        // shell/dialog.rs `dialog_key`), and 保存しない, which loses the changes, is the destructive (red) one.
        let actions = action_bar()
            .action(ActionDefinition::new("save-confirm-save", crate::i18n::text("_shell.save"), ActionButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.confirm_save_and_close(window, cx))))
            .action(ActionDefinition::new("save-confirm-cancel", crate::i18n::text("_shell.cancel"), ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, _, cx| this.close_save_confirm_dialog(cx))))
            .action(ActionDefinition::new("save-confirm-discard", crate::i18n::text("fix.editor.don-t-save"), ActionButtonKind::Destructive)
                .on_click(cx.listener(|this, _, window, cx| this.confirm_discard_and_close(window, cx))));
        super::message_box::message_box("save-confirm-dialog-backdrop", name.into(), message, actions, &self.msgbox, true, cx)
    }

    /// Arrange all open windows across the screen evenly.
    pub fn arrange_all_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(crate::arrange_kind::ArrangeKind::Tiled, current, None, cx);
    }

    /// Cascade all open windows.
    pub(crate) fn cascade_windows(window: Option<&mut Window>, cx: &mut App) {
        let current = window.map(|w| {
            let h = w.window_handle();
            (w, h)
        });
        Self::arrange_windows_impl(crate::arrange_kind::ArrangeKind::Cascade, current, None, cx);
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
    pub fn clamp_workspace_scroll(&self) {
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
    pub fn display_frame(&self, shape: &Shape) -> EmuRect {
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

    pub fn execute(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        let is_admin = commands.iter().all(|c| matches!(c, Command::SetMarkAsFinal { .. } | Command::SetReadOnlyRecommended { .. } | Command::SetModifyVerifier { .. }));
        if !is_admin && self.pages_are_slides() {
            if with_format!(self, edit_blocked_by_file(self)) {
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
        self.sync_peers(cx);
        cx.notify();
    }

    /// THE way to change the page shown (the sheet, the slide). Assigning `current_slide` directly is forbidden outside this and
    /// loading (`crates/ibis-office-xlsx/src/tests/active_sheet_rule.rs` fails otherwise): the format's `page_changed` saves what the page
    /// left keeps (a worksheet's scroll, by its stable `SlideId`) and restores the new page's, whatever caused the change (a tab, the
    /// keys, a deleted or moved or inserted sheet, undo or redo, a jump from a panel, a search).
    pub fn set_current_slide(&mut self, to: SlideId) {
        let from = self.current_slide;
        self.current_slide = to;
        if from != to {
            with_format!(self, page_changed(self, from, to));
        }
    }

    /// Drop references to shapes or slides that an undo or redo removed.
    pub fn fix_selection(&mut self) {
        if self.presentation().slide(self.current_slide).is_none() {
            let preferred = with_format!(self, page_when_current_gone(self))
                .filter(|&id| self.presentation().slide(id).is_some());
            let target = preferred
                .or_else(|| {
                    self.presentation()
                        .workbook
                        .as_ref()
                        .and_then(|w| w.sheets.iter().find_map(|s| s.slide))
                })
                .unwrap_or(self.presentation().slides[0].id);
            self.set_current_slide(target);
        }
        if let Some(id) = self.selection {
            if self.slide().shape(id).is_none() {
                self.selection = None;
            }
        }
    }

    /// 表示 > カラー/白黒 (ColorMode: Color, Grayscale, HighContrast)
    pub fn color_mode(&self) -> crate::render::ColorMode {
        self.resources.color_mode
    }

    pub fn set_color_mode(&mut self, mode: crate::render::ColorMode, cx: &mut Context<Self>) {
        if self.resources.color_mode != mode {
            self.resources.color_mode = mode;
            cx.notify();
        }
    }

    pub fn toggle_high_contrast(&mut self, cx: &mut Context<Self>) {
        let next = if self.resources.color_mode == crate::render::ColorMode::HighContrast {
            crate::render::ColorMode::Color
        } else {
            crate::render::ColorMode::HighContrast
        };
        self.set_color_mode(next, cx);
    }

    pub fn is_high_contrast(&self) -> bool {
        self.resources.color_mode == crate::render::ColorMode::HighContrast
    }

    pub fn is_grayscale(&self) -> bool {
        self.resources.color_mode == crate::render::ColorMode::Grayscale
    }

    pub fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_running() {
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
                    self.set_current_slide(id);
                }
            }
        }
        with_format!(self, history_applied_early(self, window, cx));
        with_format!(self, view_from_model(self));
        self.fix_selection();
        with_format!(self, history_applied(self, cx));
        self.sync_peers(cx);
        cx.notify();
    }

    pub fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_running() {
            return;
        }
        self.commit_edit(window, cx);
        self.document.redo();
        with_format!(self, history_applied_early(self, window, cx));
        with_format!(self, view_from_model(self));
        self.fix_selection();
        with_format!(self, history_applied(self, cx));
        self.sync_peers(cx);
        cx.notify();
    }

    pub fn delete_selection(
        &mut self,
        _: &DeleteSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_running() {
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

    pub fn cancel_tool(&mut self, _: &CancelTool, window: &mut Window, cx: &mut Context<Self>) {
        if crate::format::with_format!(self, range_pick_active(self)){crate::format::with_format!(self, finish_range_pick(self, true, window, cx));return;}
        if self.cancel_table_reorder(window,cx){return;}
        if with_format!(self, cancel_tool_early(self, window, cx)) { return; }
        if self.zoom_dialog_open {
            self.close_zoom_dialog(cx);
            return;
        }
        if with_format!(self, cancel_tool_late(self, window, cx)) { return; }
        if self.text_editing().is_some() {
            self.end_text_edit(window, cx);
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selection = None;
        }
        cx.notify();
    }

    /// Esc while the pointer drags (a shape, or a selection in the edited text): the drag is dropped, and the text's caret goes
    /// back to where the drag began. False when nothing was dragged. Each format's `cancel_tool_late` calls it at its place.
    pub fn cancel_pointer_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if self.drag.take().is_some() {
            cx.notify();
            return true;
        }
        if self.editing.as_mut().and_then(|e| e.drag_anchor.take()).is_some() {
            if let Some(edit) = self.editing.as_mut() {
                edit.edit.caret = edit.edit.anchor;
            }
            cx.notify();
            return true;
        }
        false
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

    pub fn add_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pages_are_slides() {
            return;
        }
        if with_format!(self, add_page_blocked(self)) {
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
        self.set_current_slide(id);
        self.selection = None;
        self.set_selection(Vec::new());
    }

    /// A new slide `id` to put at `index`, following `layout` (else the
    /// current slide's): with the layout's placeholders empty, as the file
    /// reads them, when there is a file; else without them.
    pub fn new_slide(&mut self, index: usize, id: ibis_doc_model::SlideId, layout: Option<ibis_doc_model::LayoutId>) -> ibis_doc_model::Slide {
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

    /// The pages selected in the list, in order: those the format's list holds (a presentation's slides chosen with
    /// Shift or ⌘), else the one shown.
    pub fn selected_slide_ids(&self) -> Vec<SlideId> {
        with_format!(self, selected_pages(self))
    }

    pub fn delete_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            let target = slides[first.min(slides.len() - 1)].id;
            self.set_current_slide(target);
            self.selection = None;
            with_format!(self, clear_page_selection(self));
            return;
        }
        self.execute(vec![Command::RemoveSlide { slide: self.current_slide }], cx);
        let slides = &self.presentation().slides;
        let target = slides[index.min(slides.len() - 1)].id;
        self.set_current_slide(target);
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
    pub fn show_slide_number(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let id = slides[number.clamp(1, slides.len()) - 1].id;
        self.select_slide(id, window, cx);
    }

    pub fn next_slide(&mut self, _: &NextSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_running() {
            return with_format!(self, show_step(self, true, window, cx));
        }
        self.step_slide(true, window, cx);
    }

    pub fn previous_slide(&mut self, _: &PreviousSlide, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_running() {
            return with_format!(self, show_step(self, false, window, cx));
        }
        self.step_slide(false, window, cx);
    }

    /// The last slide (`last`) or the first, in the editor or the slide show.
    fn end_slide(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_running() {
            return with_format!(self, show_to_end(self, last, cx));
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

    pub fn select_slide(&mut self, slide: SlideId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.presentation().slide_index(slide) {
            // Its pictures before those still waiting.
            self.resources.preload.prioritize(&self.presentation().slides[index]);
        }
        with_format!(self, clear_page_selection(self));
        self.shape_edit.popup = None;
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
            let row = with_format!(self, page_list_row(self, index));
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
    pub fn on_workspace_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_wheel_zoom(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // The format's own use of the wheel (a presentation: over the Draw panel's ruler the wheel turns the ruler; a worksheet:
        // each split pane scrolls like Excel's, its pages change with the tabs). A document's pages scroll by, as in Word.
        if with_format!(self, workspace_wheel(self, event, window, cx)) {
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
    pub fn wheel_pages(&mut self, event: &ScrollWheelEvent) -> i32 {
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

    // ----- In-place text editing ------------------------------------------

    pub fn begin_edit(
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
            edit: crate::text_edit::TextEditing {
                shape,
                anchor: len,
                caret: len,
                marked: None,
                marked_clauses: Vec::new(),
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
    pub fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn on_workspace_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_is_modal() { cx.stop_propagation(); return; }
        self.finish_crop_mode(true, cx);
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
        if self.dialog_is_modal() { cx.stop_propagation(); return; }
        self.finish_crop_mode(true, cx);
        if self.tool == Tool::TextBox {
            // Drawing a text box may start on top of another shape.
            return self.on_workspace_down(event, window, cx);
        }
        if with_format!(self, shape_press_early(self, event, cx)) {
            cx.stop_propagation();
            return;
        }
        cx.stop_propagation();
        if self.editing.as_ref().is_some_and(|e| e.shape == shape) {
            return; // the text field handles its own clicks
        }
        if with_format!(self, shape_press(self, shape, event, window, cx)) { return; }
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

    pub fn on_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if with_format!(self, pointer_move_early(self, event, window, cx)) { return; }
        if self.crop_pointer_move(event.position, event.pressed_button == Some(MouseButton::Left), cx) { return; }
        if with_format!(self, range_pick_move(self, event, window, cx)) { return; }
        self.update_link_hover(event,cx);
        if with_format!(self, pointer_move(self, event, window, cx)) { return; }
        let Some(drag) = self.drag.as_mut() else { return };
        match drag {
            Drag::Move { current, .. } | Drag::Resize { current, .. } | Drag::Create { current, .. } => {
                *current = event.position;
            }
        }
        cx.notify();
    }

    pub fn on_pointer_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        with_format!(self, pointer_up_early(self));
        if self.crop_pointer_up(cx) { return; }
        if with_format!(self, range_pick_up(self, window, cx)) { return; }
        if with_format!(self, pointer_up(self, event, window, cx)) { return; }
        let Some(drag) = self.drag.take() else { return };
        let Some(layout) = self.slide_layout() else { return };
        let slide = self.current_slide;
        match drag {
            Drag::Move { shape, start, current, .. } | Drag::Resize { shape, start, current, .. } => {
                if !moved(start, current) {
                    if matches!(drag,Drag::Move{..}) {
                        self.set_selection(vec![shape]);
                        let at=layout.to_emu(start);
                        let border=self.slide().shape(shape).is_some_and(|s|super::selection_handles::on_border((s.frame.x as f32,s.frame.y as f32,s.frame.width as f32,s.frame.height as f32),s.rotation,(at.x as f32,at.y as f32),super::selection_handles::BORDER_SLOP_PX/layout.scale));
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
pub fn zoom_scale(zoom: f32) -> f32 {
    zoom * PX_PER_INCH_AT_100 / ibis_doc_model::EMU_PER_INCH as f32
}

/// The worksheet of `slide` and the shape that holds it.
pub fn sheet_of(slide: &Slide) -> Option<(&Shape, &std::sync::Arc<Sheet>)> {
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
        crate::cmyk::set_display_gamut(self.has_document && self.cmyk_on());
        self.sync_zoom_slider(window, cx);
        if self.zoom_menu_open && !self.has_document {
            self.zoom_menu_open = false;
        }
        with_format!(self, before_render(self, window, cx));
        if self.interaction_layer().is_confirmation() {
            if self.dialog.drag.take().is_some() {window.release_pointer();}
            self.cancel_table_reorder(window,cx);
            with_format!(self, range_pick_suspend_drag(self, window));
        }
        crate::access_key::begin_frame(window);
        let theme = cx.theme();
        v_flex()
            .id("slide-editor").relative()
            .children(with_format!(self, root_event_surface(self, cx)))
            .child(gpui_kit::component::popup_guard::begin())
            .child(gpui_kit::component::popup_guard::catcher())
            .key_context(CONTEXT)
            // A press that closes a popup only closes it (`popup_guard`): the root asks first, `popup_guard::catcher` decides.
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                // The press that closed a menu is swallowed now, so a stale mark must not turn the next press on the bar into a close.
                this.menu_dismissed_at=None;
                if with_format!(this, range_pick_down(this, event, window, cx)) {cx.stop_propagation();return;}
                if !this.dialog_allows_pointer(event.position, window) { cx.stop_propagation();return; }
                if this.interaction_layer().is_confirmation() {return;}
                if with_format!(this, link_pressed(this, event, window, cx)) {cx.stop_propagation();}
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, window, cx| {
                if with_format!(this, range_pick_up(this, window, cx)) {cx.stop_propagation();return;}
                if this.dialog.drag.take().is_some() { cx.notify(); }
                if !this.dialog_allows_pointer(event.position, window) { cx.stop_propagation(); }
            }))
            .capture_key_down(cx.listener(Self::dialog_key))
            .capture_key_down(cx.listener(|e,event:&KeyDownEvent,w,cx|{if with_format!(e, formula_edit_key(e, event, w, cx)){w.prevent_default();cx.stop_propagation();}}))
            .map(|d| self.guard_panel_actions(d, cx))
            .on_modifiers_changed(cx.listener(|this, e: &ModifiersChangedEvent, _, cx| with_format!(this, modifiers_changed(this, e, cx))))
            // Alt held: the access letters of the bar's menus are underlined (Windows and Linux).
            .on_modifiers_changed(cx.listener(|this, e: &ModifiersChangedEvent, _, cx| {
                if crate::access_key::supported() && this.bar_keys.alt_held != e.modifiers.alt {
                    this.bar_keys.alt_held = e.modifiers.alt;
                    cx.notify();
                }
            }))
            // Documents dropped from the Finder on the window are opened (pictures dropped on a slide are put on it).
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                #[cfg(not(target_family = "wasm"))]
                for path in paths.paths() {
                    if path.extension().and_then(|e| e.to_str()).and_then(ibis_pptx::Kind::from_extension).is_some() {
                        // The file this window has (it cannot be looked up from the app while it is updating) is raised, not opened again.
                        if this.shows_path(path) {
                            window.activate_window();
                            continue;
                        }
                        crate::app::open_path_in_app(path.clone(), cx);
                    }
                }
                #[cfg(target_family = "wasm")]
                let _ = (this, paths, window, cx);
            }))
            .on_action(cx.listener(|e,_:&crate::actions::AddPresentationSlide,w,cx|{if e.pages_are_slides(){e.add_slide(w,cx);}}))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::cancel_tool))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(|_, _: &crate::actions::OpenStartWindow, _, cx| crate::app::open_editor(None, None, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::NewDocument, window, cx| this.new_file(this.kind(), window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::NewFromTemplate, window, cx| this.new_from_template(window, cx)))
            // Also on the start view (⌘W closes its window; the menu's ファイル > 閉じる was greyed there, #75).
            .on_action(cx.listener(Self::close_file))
            .when(self.has_document, |d| d.on_action(cx.listener(Self::save)).on_action(cx.listener(Self::save_as)).on_action(cx.listener(|this, _: &crate::actions::PrintFile, window, cx| this.print(window, cx))))
            .when(self.has_document, |d| {
                d.on_action(cx.listener(|this, _: &crate::actions::ToggleMenus, _, cx| {
                    if !this.show_menu_bar {
                        this.show_menu_bar = true;
                        this.chrome_compact = false;
                    } else {
                        this.chrome_compact = !this.chrome_compact;
                    }
                    cx.notify();
                }))
            })
            .on_action(cx.listener(|this, _: &crate::actions::ZoomInView, window, cx| this.step_zoom(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::ZoomOutView, window, cx| this.step_zoom(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::ZoomActualView, window, cx| this.set_zoom(Some(1.0), window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::ZoomFitView, window, cx| this.set_zoom(None, window, cx)))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::previous_slide))
            .on_action(cx.listener(|this, _: &crate::actions::FirstSlide, window, cx| this.end_slide(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::LastSlide, window, cx| this.end_slide(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::InsertHyperlink, window, cx| {
                with_format!(this, open_link_panel_for_page(this, window, cx));
            }))
            .on_action(cx.listener(|this, _: &crate::actions::InsertComment, window, cx| {
                if let Some(add) = crate::format::with_format!(this, add_comment(this)) {
                    add(this, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::actions::StartSlideshowFromBeginning, window, cx| {
                if !this.show_running() && this.pages_are_slides() && this.has_document && !this.loading {
                    this.show_slide_number(1, window, cx);
                    with_format!(this, show_start(this, window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &crate::actions::OpenZoomDialog, window, cx| this.open_zoom_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::NewEditorWindow, _, cx| this.new_window(cx)))
            .map(|d| {
                #[cfg(not(target_family = "wasm"))]
                let d = d
                    .on_action(cx.listener(|this, _: &crate::actions::WindowList, window, cx| this.open_window_switcher(false, window, cx)))
                    .on_action(cx.listener(|this, _: &crate::actions::WindowListBack, window, cx| this.open_window_switcher(true, window, cx)))
                    .on_action(cx.listener(|this, _: &crate::actions::NextWindow, window, cx| this.step_window(true, window, cx)))
                    .on_action(cx.listener(|this, _: &crate::actions::PreviousWindow, window, cx| this.step_window(false, window, cx)));
                d
            })
            .on_action(cx.listener(|_, _: &crate::actions::ArrangeAllWindows, window, cx| SlideEditor::arrange_all_windows(Some(window), cx)))
            .on_action(cx.listener(|_, _: &crate::actions::CascadeWindows, window, cx| SlideEditor::cascade_windows(Some(window), cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let ks = &event.keystroke;
                if this.has_document
                    && !this.show_running()
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
                if !this.move_dialog(event, window, cx) { Self::on_pointer_move(this, event, window, cx); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &crate::actions::NextSheet, window, cx| this.step_sheet(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::PreviousSheet, window, cx| this.step_sheet(false, window, cx)))
            // What only a format answers to (a presentation's slide show, a workbook's cells, a document's scrolling).
            .map(|d| crate::format::with_format!(self, register_actions(d, cx)))
            // The slide show takes the whole window, in place of the rest.
            .when_some(with_format!(self, show_element(self, window, cx)), |d, show| d.child(show))
            .when(!with_format!(self, show_in_main(self)), |d| self.render_window(d, window, cx))
            .map(|d| self.render_dialog_stack(d, window, cx))
            .map(|d| {
                #[cfg(not(target_family = "wasm"))]
                let d = d.when_some(self.render_window_switcher(window, cx), |d, list| d.child(list));
                d
            })
    }
}

impl SlideEditor {
    /// The editor window's parts, into `d`: the title bar, and the start view
    /// or the chrome, the document and the status bar.
    fn render_window(&mut self, d: Stateful<Div>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        #[cfg(not(target_family = "wasm"))]
        self.sync_open_documents(window, cx);
        self.sync_context_menus(cx);
        #[cfg(not(target_family = "wasm"))]
        self.advance_window_switcher(window, cx);
        // The mock's layout (shell/chrome.rs): the title row, the menu bar
        // and the toolbar (the first two hidden by the toolbar's ^), the
        // document with the sidebar on its right, and the status bar.
        if !self.has_document {
            window.set_window_title(&crate::start_view::start_title());
            // The settings dialog (modal) over the start view: its panel is built, and drawn, by the sidebar's code as in a document.
            // The セットアップ dialog (K0001) is shown the same way: the first window at launch is often this one.
            if self.settings.is_some() || self.font_setup_is_open() {
                let _ = self.render_sidebar(window, cx);
            }
            return d.child(self.render_title_row(window, cx)).child(self.render_start_view(cx));
        }
        let compact = self.chrome_compact;
        let focus = with_format!(self, chrome_hidden(self));
        let show_menu_bar = self.show_menu_bar;
        let show_toolbar = self.show_toolbar;
        let show_title_row = !self.is_read_only() || show_menu_bar || {
            #[cfg(target_arch = "wasm32")]
            {
                crate::embed::show_close() || crate::embed::show_download()
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                true
            }
        };
        let sidebar = if focus || self.grid_only {
            self.right_pane = Default::default();
            None
        } else {
            self.render_sidebar(window, cx)
        };
        let body = with_format!(self, render_body(self, window, cx));
        if self.grid_only {
            return d.child(div().relative().flex().flex_col().flex_1().min_w_0().min_h_0().child(body).child(crate::input::input_region(cx)));
        }
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
                    .child(div().relative().flex().flex_col().flex_1().min_w_0().min_h_0().child(body).child(crate::input::input_region(cx)))
                    .children(sidebar),
            )
            .when(!focus, |d| d.child(self.render_status_bar(cx)))
    }

    /// Apply a font size (in points) chosen or typed in the toolbar font size pulldown (#120).
    pub(crate) fn commit_toolbar_font_size(&mut self,window:&mut Window,cx:&mut Context<Self>){
        if self.font_size_edit_dirty && let Ok(value)=self.font_size_edit_input.read(cx).value().trim().parse::<f32>() && value.is_finite() && (1.0..=1638.).contains(&value) {self.font_size_edit_dirty=false;self.set_toolbar_font_size(value,window,cx);}
        self.focus_handle.focus(window,cx);cx.notify();
    }
    pub fn set_toolbar_font_size(&mut self, pt: f32, window: &mut Window, cx: &mut Context<Self>) {
        with_format!(self, set_font_size(self, pt, window, cx));
    }

    /// Open the toolbar's "メニューを検索" (`menu-search`) popup and focus its search input (#147).
    pub fn open_toolbar_menu_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        crate::menu_search_history::invalidate();
        let input = self.ensure_toolbar_search_input("menu-search", window, cx);
        let scroll = self.ensure_toolbar_search_scroll("menu-search");
        scroll.set_offset(Point::default());
        self.toolbar_search_open = Some("menu-search");
        self.toolbar_search_selected.insert("menu-search", None);
        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    /// Open the searchable list `id` over `items` without a button of its own to click (the settings dialog's add menu): the search
    /// field is shown whatever the number of items and has the focus, nothing is highlighted until a key is typed. The list is drawn by
    /// the dropdown whose popup has the id `id` (`Dropdown::with_button`), and Enter, the arrows, Escape and the pointer work as in every
    /// searchable list.
    pub(crate) fn open_searchable_list(&mut self, id: &'static str, items: Vec<crate::chrome::MenuItem>, spec: crate::chrome::dropdown::ListSpec, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let input = self.ensure_toolbar_search_input(id, window, cx);
        let scroll = self.ensure_toolbar_search_scroll(id);
        scroll.set_offset(Point::default());
        self.font_size_menu_open = false;
        self.toolbar_search_spec = Some((id, spec));
        self.toolbar_search_open = Some(id);
        self.toolbar_search_popup_items = Some((id, items));
        self.toolbar_search_selected.insert(id, None);
        input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    /// How the searchable list `id` was asked to be shown (the default for a list opened by its own button).
    pub(crate) fn searchable_list_spec(&self, id: &str) -> crate::chrome::dropdown::ListSpec {
        self.toolbar_search_spec.filter(|(spec_id, _)| *spec_id == id).map(|(_, spec)| spec).unwrap_or_default()
    }

    /// Get or create a `ScrollHandle` for a searchable dropdown menu (#125, #127).
    pub(crate) fn ensure_toolbar_search_scroll(&mut self, id: &'static str) -> ScrollHandle {
        self.toolbar_search_scrolls
            .entry(id)
            .or_insert_with(ScrollHandle::new)
            .clone()
    }

    /// Get or create a cleanable search `InputState` for a searchable dropdown menu (#98, #103, #127).
    pub fn ensure_toolbar_search_input(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let placeholder = crate::i18n::text("fix.menu.search-placeholder");
        if let Some(existing) = self.toolbar_search_inputs.get(id) {
            // The field is kept between openings: its hint follows the language shown now (it was the language of the first opening).
            let existing = existing.clone();
            if self.toolbar_search_hints.get(id).is_none_or(|shown| shown.as_str() != &*placeholder) {
                self.toolbar_search_hints.insert(id, placeholder.to_string());
                existing.update(cx, |state, cx| state.set_placeholder(placeholder, window, cx));
            }
            return existing;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.clone()));
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
        self.toolbar_search_hints.insert(id, placeholder.to_string());
        self.toolbar_search_inputs.insert(id, input.clone());
        input
    }

    /// Dismiss the top dropdown before any parent-dialog action, independent of input focus.
    pub(crate) fn dismiss_toolbar_dropdown(&mut self,window:&mut Window,cx:&mut Context<Self>)->bool {
        let Some(id)=self.toolbar_search_open.take() else{return false;};
        self.toolbar_search_popup_items=None;
        self.toolbar_search_selected.insert(id,None);
        if self.dialog.active {if let Some(focus)=self.dialog.focus.as_ref(){focus.focus(window,cx);}}
        else {self.focus_handle.focus(window,cx);}
        cx.notify();true
    }

    /// Own list navigation before input bindings, including lists without a search field.
    fn toolbar_dropdown_keystroke(&mut self,id:&'static str,key:&Keystroke,window:&mut Window,cx:&mut Context<Self>)->bool {
        let items=self.toolbar_search_popup_items.as_ref().filter(|(popup,_)|*popup==id).map(|(_,items)|items.clone()).unwrap_or_else(||self.searchable_menu_items_for(id));
        let spec=self.searchable_list_spec(id);
        let search=items.len()>20||spec.always_search;
        match key.key.as_str() {
            "enter"=>{self.activate_top_toolbar_search_match(id,window,cx);true}
            "escape"=>self.dismiss_toolbar_dropdown(window,cx),
            "up"|"down"|"pageup"|"pagedown"=>{
                let query=if search {self.toolbar_search_inputs.get(id).map(|i|i.read(cx).value().to_string()).unwrap_or_default()}else{String::new()};
                let matched=self.search_rows(id,&items,&query);
                if id=="menu-search" && key.key=="down" && matched.iter().any(|item|item.id!=crate::chrome::MENU_SEARCH_HISTORY_ITEM && item.enabled()) {self.record_menu_search(&query);}
                let row_h=crate::chrome::dropdown_row_height(id);let max_rows=spec.rows.unwrap_or_else(||(((f32::from(window.viewport_size().height)-214.).max(224.)/row_h).floor() as usize).max(8));
                let visible=items.len().clamp(1,max_rows);let page=visible.saturating_sub(1).max(1) as isize;
                let delta=match key.key.as_str(){"up"=>-1,"down"=>1,"pageup"=>-page,_=>page};
                if let Some(index)=crate::chrome::step_enabled_menu_index(&matched,self.toolbar_search_selected.get(id).copied().flatten(),delta){
                    self.toolbar_search_selected.insert(id,Some(index));
                    if let Some(scroll)=self.toolbar_search_scrolls.get(id){crate::chrome::scroll_row_into_view(scroll,index,row_h,visible as f32*row_h);}
                    cx.notify();
                }true
            }
            _=>!search && key.key_char.is_some(),
        }
    }

    /// When Enter is pressed in a searchable dropdown menu, execute the keyboard-selected item or (if a query is typed) the top matched item (#98, #103, #127).
    pub fn activate_top_toolbar_search_match(
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
        let matched = self.search_rows(id, &items, &query);
        let chosen = if let Some(idx) = selected_idx {
            matched.get(idx).filter(|item| !item.disabled && item.handler.is_some()).cloned()
        } else {
            matched.into_iter().find(|item| !item.disabled && item.handler.is_some())
        };
        if let Some(item) = chosen
            && let Some(handler) = item.handler
        {
            if item.id == crate::chrome::MENU_SEARCH_HISTORY_ITEM {
                // A lately searched string goes into the field; the list stays open.
                handler(self, window, cx);
                return;
            }
            if id == "menu-search" {
                self.record_menu_search(&query);
            }
            input.update(cx, |state, cx| state.set_value("", window, cx));
            self.toolbar_search_open = None;
            self.toolbar_search_selected.insert(id, None);
            let editor = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                editor.update(cx, |this, cx| handler(this, window, cx)).ok();
            });
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// The rows the searchable list `id` shows for `query`: the menu search also lists its history and headings
    /// while nothing is typed (`chrome::menu_search_rows`); the other lists filter their items.
    fn search_rows(&self, id: &str, items: &[crate::chrome::MenuItem], query: &str) -> Vec<crate::chrome::MenuItem> {
        if id == "menu-search" {
            crate::chrome::menu_search_rows(items, query, &crate::menu_search_history::load(self.kind()))
        } else {
            crate::chrome::filter_menu_items(items, query)
        }
    }

    /// Note `text` as searched in the menu search of this kind of screen (written to the settings at once).
    pub(crate) fn record_menu_search(&mut self, text: &str) {
        crate::menu_search_history::record(self.kind(), text);
    }

    /// Put a lately searched string into the menu search field (a "履歴" row was chosen).
    pub(crate) fn set_menu_search_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.ensure_toolbar_search_input("menu-search", window, cx);
        let text = text.to_string();
        input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.focus(window, cx);
        });
        self.toolbar_search_selected.insert("menu-search", None);
        cx.notify();
    }

    /// Return the menu items for a searchable dropdown button `id` (#98, #103, #127).
    pub fn searchable_menu_items_for(&self, id: &str) -> Vec<crate::chrome::MenuItem> {
        if let Some(entries) = with_format!(self, searchable_menu_entries(self, id)) {
            return crate::chrome::flatten_menu_entries(&entries);
        }
        if id == "menu-search" {
            return self.command_search_items();
        }
        for tool in self.toolbar_items() {
            match tool {
                crate::chrome::ToolItem::Button(btn) if btn.id == id => {
                    if let Some(entries) = btn.menu {
                        return crate::chrome::flatten_menu_entries(&entries);
                    }
                }
                crate::chrome::ToolItem::Select { id: item_id, menu, .. } if item_id == id => {
                    return crate::chrome::flatten_menu_entries(&menu);
                }
                _ => {}
            }
        }
        Vec::new()
    }

    /// Display title for this editor window in the Window menu (#111).
    pub fn window_menu_label(&self) -> SharedString {
        let base = self
            .file_name
            .clone()
            .unwrap_or_else(|| crate::file_actions::new_file_name(self.format.kind()));
        match self.window_number {
            Some(n) => SharedString::from(format!("{base}:{n}")),
            None => base,
        }
    }

    /// Tell the app's table of open documents about this window, and read the table back (once a frame).
    #[cfg(not(target_family = "wasm"))]
    fn sync_open_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::open_documents::OpenDocuments;
        let active = window.is_window_active();
        let activated = active && !self.window_was_active;
        self.window_was_active = active;
        if self.has_document {
            let (handle, me, weak) = (window.window_handle(), self.entity_id, cx.entity().downgrade());
            OpenDocuments::sync(cx, handle, me, weak, self.window_menu_label(), self.kind(), self.is_dirty(), activated);
        } else {
            OpenDocuments::unregister(cx, self.entity_id);
        }
        self.open_documents = OpenDocuments::mru(cx);
    }

    /// Menu entries for the open document windows at the bottom of the Window menu (#111; the redesign's list): in the
    /// order they were opened, at most five (this window always one of them), the current one checked, each with the
    /// screen's icon and a ● when it has unsaved changes. More than five end with 「その他のウィンドウ（N）…」, which
    /// opens the list of windows.
    #[cfg(not(target_family = "wasm"))]
    pub fn open_window_menu_items(&self) -> Vec<crate::chrome::MenuEntry> {
        use crate::chrome::{MenuEntry, MenuItem};
        use crate::menu_context::format_color;
        use crate::window_switcher::document_label;
        const LIMIT: usize = 5;
        const IDS: [&str; LIMIT] = ["window.open.0", "window.open.1", "window.open.2", "window.open.3", "window.open.4"];
        let mut docs = self.open_documents.clone();
        docs.sort_by_key(|d| d.opened_seq);
        let total = docs.len();
        if docs.len() > LIMIT {
            let mine = docs.iter().position(|d| d.id == self.entity_id);
            docs.truncate(LIMIT);
            if let Some(at) = mine.filter(|at| *at >= LIMIT) {
                let _ = at;
                // This window is not among the first five: it takes the last place (the others keep their order).
                let me = self.open_documents.iter().find(|d| d.id == self.entity_id).cloned();
                if let Some(me) = me {
                    docs.pop();
                    docs.push(me);
                }
            }
        }
        let mut entries: Vec<MenuEntry> = Vec::new();
        for (ix, doc) in docs.iter().enumerate() {
            let id = doc.id;
            let is_self = id == self.entity_id;
            entries.push(
                MenuItem::new(IDS[ix], document_label(doc))
                    .checked(is_self)
                    .format_mark(format_color(doc.kind), crate::format::info(doc.kind).icon)
                    .on(move |this, window, cx| this.activate_open_document(id, window, cx))
                    .into(),
            );
        }
        if !docs.iter().any(|d| d.id == self.entity_id) {
            // Not in the table yet (before its first frame): this window alone.
            entries.insert(
                0,
                MenuItem::new("window.open.0", self.window_menu_label())
                    .checked(true)
                    .format_mark(format_color(self.kind()), crate::format::info(self.kind()).icon)
                    .on(|_, window, _| window.activate_window())
                    .into(),
            );
        }
        if total > LIMIT {
            let more = crate::i18n::format("window.more", &[("n", &(total - LIMIT))]);
            entries.push(MenuItem::new("window.open.more", more).on(|this, window, cx| this.open_window_switcher(false, window, cx)).into());
        }
        entries
    }

    #[cfg(target_family = "wasm")]
    pub(crate) fn open_window_menu_items(&self) -> Vec<crate::chrome::MenuEntry> {
        Vec::new()
    }
}

/// An element filling its (positioned) parent that calls `f` with the
/// parent's bounds in window coordinates at every prepaint. (gpui-kit's
/// `on_prepaint` adds an absolute child without an inset, which lands after
/// the parent's other children rather than at its origin.)
pub fn bounds_observer(f: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> impl IntoElement {
    canvas(move |bounds, window, cx| f(bounds, window, cx), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

/// An element filling the (positioned) workspace that hands wheel and pinch
/// gestures over it to `SlideEditor::on_wheel_zoom` and `on_pinch_zoom`
/// before anything in the workspace scrolls, and stops the event there when
/// it zooms.
pub fn wheel_zoom_listener(cx: &Context<SlideEditor>) -> impl IntoElement {
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

impl SlideEditor {
    /// The name a window shows for its file: the file's own, else the name a new file is given (Book1: it is kept for a new
    /// file of any format; it was in `xlsx/window_hide.rs`).
    pub fn workbook_display_name(&self) -> String {
        self.file_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| crate::i18n::text("Book1").to_string())
    }
}
