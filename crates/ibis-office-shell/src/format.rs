// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The hand-over from the editor window (the shell) to each format's editor.
//!
//! The window is one [`SlideEditor`] whatever the file. What differs by
//! format lives in the format's directory:
//!
//! - its state, in [`FormatState`] (`pptx::State`, `xlsx::State`,
//!   `docx::State`): the selected cells of a workbook, the character count
//!   of a document, …;
//! - its behaviour, as an implementation of [`FormatEditor`]
//!   (`pptx::Presentation`, `xlsx::Workbook`, `docx::Document`): the layout
//!   under the toolbar, how the pages scroll and turn, what the status bar
//!   says, …;
//! - its views and editing, as `impl SlideEditor` blocks in its own files
//!   (`xlsx/sheet_ui.rs` and so on), reading its state with
//!   [`SlideEditor::sheet_state`] and the like.
//!
//! A format's editor changes only files in its directory. When it needs a
//! hook the shell does not call yet, the hook is added to [`FormatEditor`]
//! (with a default that keeps the other formats as they are) and called
//! from the shell: a change to the shell, made once, through the core担当.

use std::ops::Range;

use gpui_kit::*;
use ibis_doc_model::{EmuRect, Presentation, SlideId};
use ibis_pptx::Kind;

use crate::chrome::{Menus, SidebarPanel, ToolItem};
use crate::editor::{SlideEditor, WorkspaceContent};

/// A dialog the format asks the sidebar to show over the window (besides the panel of the open command): `footer_from_sidebar` takes
/// the footer the sidebar's own panel left, `resets_panel_mode` puts the sidebar's panel back to a plain sidebar panel afterwards.
pub struct ExtraDialog {
    pub panel: SidebarPanel,
    pub key: &'static str,
    pub mode: crate::dialog::DialogMode,
    pub footer_from_sidebar: bool,
    pub resets_panel_mode: bool,
}

/// What the PDF made for printing or export is made from (`FormatEditor::pdf_job`): the file's bytes, the pages asked for (1-based)
/// when it is not all of them, the export's format (`pdf` or `pdf-image`) and the arguments the print settings give it.
/// Not on the web: the PDF is made by this program's `--export` run (`SlideEditor::write_pdf_with`), which the web has not.
#[cfg(not(target_family = "wasm"))]
pub struct PdfJob {
    pub bytes: Vec<u8>,
    pub slides: Option<Vec<usize>>,
    pub format: &'static str,
    pub args: Option<Vec<String>>,
}

/// A button of the title row's right-hand part that only one format has (`FormatEditor::title_buttons`): the element's id,
/// its icon (drawn filled while `on`), the tooltip's key and what a click does.
pub struct TitleButton {
    pub id: &'static str,
    pub icon: &'static str,
    pub on: bool,
    pub tooltip: &'static str,
    pub handler: crate::chrome::Handler,
}

/// What a worksheet draws over its grid: `over_sheet` each right over the sheet (so the one put in last is lowest), and the page
/// lines and numbers over all of them.
#[derive(Default)]
pub struct SheetOverlays {
    pub over_sheet: Vec<AnyElement>,
    pub page_lines: Option<AnyElement>,
}

/// The state of the open document that only its format has (a workbook's selected cells, a document's character count, ...). The
/// shell holds it as an object and never looks inside: the format reaches its own through `SlideEditor::format_state::<T>()`.
pub trait FormatState: std::any::Any {
    /// The kind of document this is the state of.
    fn kind(&self) -> Kind;
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// `impl FormatState for $state`, for the document of `$kind`.
// Exported for the formats (ibis-office-app); named from here, `crate::format::impl_format_state`, through the `pub use` below.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_format_state {
    ($state:ty, $kind:expr) => {
        impl $crate::format::FormatState for $state {
            fn kind(&self) -> ibis_pptx::Kind {
                $kind
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }
    };
}
pub use crate::impl_format_state;

/// The state for `presentation`, just read: its kind is told by its content (worksheets, a flowing document, or slides).
pub(crate) fn state_for_model(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState> {
    let kind = if presentation.flow.is_some() {
        Kind::Document
    } else if presentation.slides.iter().any(|slide| crate::editor::sheet_of(slide).is_some()) {
        Kind::Workbook
    } else {
        Kind::Presentation
    };
    driver(kind).new_state(presentation, window, cx)
}

/// Call `FormatDriver::$f(args)` of the open document's format (the driver is the one of `$editor.format.kind()`).
// Exported for the formats (ibis-office-app); named from here, `crate::format::with_format`, through the `pub use` below.
#[macro_export]
#[doc(hidden)]
macro_rules! with_format {
    ($editor:expr, $f:ident ( $($arg:expr),* )) => {
        $crate::format::driver($editor.format.kind()).$f($($arg),*)
    };
}
pub use crate::with_format;

/// What the shell shows and stores for a format without asking how it edits: its names, its keys in the settings and its colours.
/// One per format (`FormatEditor::INFO`); `info(kind)` reads it where no window is open (the start window, 新規作成).
pub struct FormatInfo {
    /// Where the format comes in a list of the formats (新規作成): the document, the workbook, the presentation.
    pub order: u8,
    /// The extension of the format's own files ("docx"): an old file read as a new one, the AutoRecover snapshot, 新規作成's badge.
    pub extension: &'static str,
    /// The name a new file shows until it is saved.
    pub new_file_name: fn() -> SharedString,
    /// The string of the name a window without a file name shows in its title and when it asks whether to save.
    pub untitled_key: &'static str,
    /// Microsoft's application for the format (「Word で開く」).
    pub office: OfficeApp,
    /// The command of 新規作成's line for the format (`file.new.document`): its picture is `icon`.
    pub new_command: &'static str,
    /// The menus of the commands' table (`chrome_defaults::menus`), in their order.
    pub command_menus: &'static [crate::chrome::MenuId],
    /// The format's mark in the title bar: the icon and its colour (the format's own colour: the badge in the lists of windows too).
    pub icon: &'static str,
    pub color: u32,
    /// The colours of the context menus' labels in the menu bar, in the light chrome and the dark one (the spec's tokens
    /// `menu.context.{doc,sheet,slide}.{fg,bg}`).
    pub context_tones: [crate::menu_context::ContextTone; 2],
    /// The screen's menus of the layout (`shell/menu_layout.rs`).
    pub menus: &'static crate::menu_layout::ScreenMenus,
    /// The static menus used least, in the order they fold when the bar is narrow (the menu redesign's spec, 4.3).
    pub fold_order: &'static [crate::chrome::MenuId],
    /// The screen's sidebars: its rows of the sidebar table (`shell/sidebars.rs`), in the order of the Window menu's 「サイドバー ▸」.
    pub sidebars: &'static [crate::sidebars::SidebarDef],
    /// The format's bit in the sets of screens of the commands.
    pub mask: crate::commands::KindMask,
    /// The same file can be shown in several windows that share its model (ウィンドウ > 新しいウィンドウ, 並べて比較); `None`: a new
    /// window reads the file again.
    pub several_windows: Option<SeveralWindows>,
    /// How the screen uses the shell's shape editor.
    pub shapes: ShapeRules,
    /// The context menus of the screen's modes (a header being edited, the outline view, the master view) and of what the caret
    /// or the cell is in (a table, an equation, a pivot table), in the bar's order: `menu_context::visible_contexts` puts the
    /// selected objects' after them.
    pub outer_contexts: fn(&crate::menu_context::ContextState) -> Vec<crate::menu_context::ContextMenu>,
    /// The file keeps substitute fonts and embeds fonts: フォントの置換's panel lists and edits them. `false`: it only replaces.
    pub font_substitutes: bool,
    /// 描画キャンバス (描画 menu) is this screen's (still to be made); `false`: its line says it is the document's.
    pub drawing_canvas: bool,
    /// The colour of 新規作成's badge in the start window.
    pub new_badge_color: u32,
    /// The setting that keeps what was typed in the menu search.
    pub search_history_key: &'static str,
    /// Pixels per inch at 100%: a slide's, or a worksheet's (the screen's size only: a PDF and a print keep the real size).
    pub px_per_inch_at_100: f32,
    /// The formats the save dialog offers, in its pop-up's order: the string of the label and the extension. Each is one
    /// `SlideEditor::save_to` writes by its extension (the format's own package, or `FormatEditor::bytes_for_path`).
    #[cfg(not(target_family = "wasm"))]
    pub save_formats: &'static [(&'static str, &'static str)],
}

/// How the windows on one file behave (`FormatInfo::several_windows`).
pub struct SeveralWindows {
    /// 並べて比較 puts the two windows left and right (a document's pages), not one over the other (a workbook's sheets).
    pub compare_left_right: bool,
    /// An edit of a shape or of its text through the shell goes to the other windows at once (a workbook's; a document's
    /// windows follow from its own editing).
    pub follow_shape_edits: bool,
    /// The name in the title bar has the window's number when the file is in several (`Book1.xlsx:2`); `false`: the bare file
    /// name there (the window's title in the system has the number either way).
    pub number_in_title_bar: bool,
}

/// How a screen uses the shell's shape editor (`FormatInfo::shapes`; `shell/shape_editing.rs`, `shell/text_edit.rs` and the panels
/// of a shape).
pub struct ShapeRules {
    /// The shell's own state (`SlideEditor::shape_edit`) is the screen's: the text edited in place, the panels of a shape in the
    /// sidebar, the find bar, the grid and the snapping, what was copied. `false`: the screen edits text with its own state (a
    /// workbook's `editing`, a document's text boxes) and these of the shell's stay shut.
    pub in_shell: bool,
    /// Several shapes can be selected at once (`shape_edit.selection`); `false`: only the shell's one selection.
    pub several: bool,
    /// An edit of several commands is tried first: applied to a copy that is then written as a file, and done only when that
    /// succeeds; `false`: it is done as it is.
    pub try_first: bool,
    /// Colours can be chosen as CMYK (the file says whether the mode is on).
    pub cmyk: bool,
    /// Shift+Enter types a line feed into the text being edited (a document's text box makes the break of it); `false`: the
    /// shell splits the paragraph and marks the new one as a line of the same paragraph.
    pub break_is_typed: bool,
}

/// What is written to a file whose extension is not the format's own package (`FormatEditor::bytes_for_path`): the bytes, whether
/// the file's password may encrypt them (an OOXML package), and, for a kind of file the format has something to say of after a
/// save, what the status bar says in place of 「保存しました」 (`Some(None)`: nothing this time).
#[cfg(not(target_family = "wasm"))]
pub struct FileBytes {
    pub bytes: Vec<u8>,
    pub protect: bool,
    pub note: Option<Option<SharedString>>,
}

/// Microsoft's application that opens a format's files.
pub struct OfficeApp {
    /// Its name in messages ("Word").
    pub name: &'static str,
    /// The setting that keeps where it is installed.
    pub setting_key: &'static str,
    /// Its bundle in /Applications.
    #[cfg(target_os = "macos")]
    pub bundle: &'static str,
    /// Its executable under Program Files.
    #[cfg(target_os = "windows")]
    pub exe: &'static str,
    /// The string of the title bar's button's tooltip and of the Window menu's line (「Word で開く」).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub open_key: &'static str,
    /// The picture on that button (an SVG).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub logo_svg: &'static [u8],
}

/// What a format's editor provides to the shell. The defaults are a
/// presentation's behaviour.
pub trait FormatEditor {
    /// The format's names, keys and colours.
    const INFO: &'static FormatInfo;
    /// What a page is called in the status bar (スライド, シート, ページ).
    const PAGE_NOUN: &'static str;
    /// The tooltip of the status bar's button that fits the page to the window.
    const FIT_TOOLTIP: &'static str;
    /// The pages are slides: turned with the wheel and the keys, added and
    /// removed, shown in a slide show.
    const SLIDES: bool = false;
    /// Shapes (text boxes) may be put on the pages.
    const SHAPES: bool = true;
    /// The status bar's list of zooms offers 「画面にフィット」.
    const CAN_FIT: bool = true;

    /// The format's state for a model just read.
    fn new_state(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState>;

    /// What the format keeps for a window over the files loaded into it (a document's rulers, its formatting marks, how
    /// it pastes): made once with the window's editor, for every format whatever the first file is, and never looked
    /// into by the shell. The format reaches its own through `SlideEditor::format_prefs::<T>()`. The state of the open
    /// file is `new_state`'s, which is new with every file.
    fn new_prefs() -> Box<dyn std::any::Any> {
        Box::new(())
    }

    /// The window's resources are new (a file is being loaded, of whatever kind): the format puts into them what its
    /// settings of the window say (a document's display of hidden text). Called for every format, before
    /// `prepare_model` lays the file out with them.
    fn resources_renewed(_editor: &mut SlideEditor) {}

    /// The zoom the file opens at, 1.0 being 100%; `None` fits the page.
    fn initial_zoom(_editor: &SlideEditor) -> Option<f32> {
        None
    }

    /// What the workspace (`workspace` big) scrolls at the current zoom
    /// (see `SlideEditor::workspace_content`).
    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent;

    /// Where to scroll vertically when the zoom changes, if the format
    /// keeps its own place (a document's page); `None` keeps the point
    /// under the anchor, as a slide does (and nothing special when fitting).
    fn zoom_scroll_y(_editor: &SlideEditor, _before: &WorkspaceContent, _after: &WorkspaceContent, _old: Pixels, _view: Pixels, _anchor: Pixels) -> Option<Pixels> {
        None
    }

    /// PageDown / PageUp: whether the format handled it (else the slide turns).
    fn step_page(_editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// Show page `slide`: whether the format handled it (else it is shown
    /// as a presentation shows a slide).
    fn show_page(_editor: &mut SlideEditor, _slide: SlideId, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The page shown changed from `from` to `to` (`SlideEditor::set_current_slide`, the one place that changes it). The
    /// format saves what is per page for `from` (a worksheet: its scroll) and puts back what `to` kept. `from` may be gone.
    fn page_changed(_editor: &mut SlideEditor, _from: SlideId, _to: SlideId) {}

    /// Before each render.
    fn before_render(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The window's content between the toolbar and the status bar (the sidebar goes on its right).
    fn render_body(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement;

    /// Items the status bar shows after the page number.
    fn status_items(_editor: &SlideEditor) -> Vec<SharedString> {
        Vec::new()
    }

    /// Whether the status bar puts the format's items at the right, next to the zoom, instead of after the page number
    /// (a sheet: the summary of the selection, as Excel's).
    fn status_items_at_right() -> bool {
        false
    }

    /// The screen's own hint for the status bar, in the UI's language: shown while the shell has none of its own (a
    /// tool's, a text edit's, a status message). A sheet: the mode of the cell being typed into.
    fn status_hint(_editor: &SlideEditor) -> Option<SharedString> {
        None
    }

    /// The scroll of the screen's page tabs, when the status bar is to give it a scrollbar (a workbook's sheet tabs).
    fn status_tab_scroll(_editor: &SlideEditor) -> Option<ScrollHandle> {
        None
    }

    // ----- Text input (see `shell/input.rs`) -------------------------------
    // The system's typing and input method, in UTF-16 code units of the
    // text the format shows it (a paragraph, a cell). The meanings are
    // those of GPUI's `EntityInputHandler` methods of the same names.

    /// Whether text input goes to the format now (a caret in text). While
    /// false, the other input hooks are not called.
    fn wants_text_input(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    fn text_for_range(_editor: &mut SlideEditor, _range: Range<usize>, _adjusted: &mut Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<String> {
        None
    }

    fn selected_text_range(_editor: &mut SlideEditor, _ignore_disabled_input: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        None
    }

    fn marked_text_range(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    fn replace_text_in_range(_editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    fn replace_and_mark_text_in_range(_editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _selected: Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// How the input method underlines each clause of the marked text (UTF-16 units from its start; empty: the platform does not say).
    /// Follows `replace_and_mark_text_in_range`. A format that shows composing text draws the clauses as `render::ime_text` does
    /// (the one being converted thick, the part being typed dotted, as Word does; GitHub issue #138).
    fn set_ime_clauses(_editor: &mut SlideEditor, _clauses: Vec<ImeClause>, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// Where the characters `range` are on screen (for the input method's
    /// candidate window); `element` is the document area's bounds.
    fn bounds_for_range(_editor: &mut SlideEditor, _range: Range<usize>, _element: Bounds<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        None
    }

    /// Whether the text at the caret runs top to bottom: the input method
    /// then lays its candidates out vertically, beside the column. Asked
    /// every frame, so it must be cheap.
    fn text_vertical(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    fn character_index_for_point(_editor: &mut SlideEditor, _point: Point<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<usize> {
        None
    }

    /// A character picked in the symbols panel (`shell/symbols.rs`) goes in where the format types: a document's caret, a
    /// workbook's cell, a slide's text.
    fn insert_symbol(_editor: &mut SlideEditor, _c: char, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    // ----- Dialogs (see `shell/dialog.rs`) -----------------------------------

    /// A cell or range is being picked on the page for a dialog's field (a workbook's: the dialog gives way to the page).
    fn range_pick_active(_editor: &SlideEditor) -> bool {
        false
    }

    /// Finish the pick that `range_pick_active` says is going on (`accept` false: cancelled).
    fn finish_range_pick(_editor: &mut SlideEditor, _accept: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The small panel that stands in for the dialog while a range is picked.
    fn range_pick_panel(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// Closing the dialog (× or Escape) when the format has its own way of doing so for the dialog shown (a dialog that goes back
    /// to another one, or that leaves a pick going on): true if it closed or handled it, so that the shell does no more.
    fn close_dialog(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The panel state of the dialog `key` is thrown away (the command's UI draft; the document's edits stay).
    fn discard_dialog(_editor: &mut SlideEditor, _key: &'static str) {}

    /// The sidebar stays open after the dialog `key` closes (a workbook's chart pane, while a chart is being edited).
    fn keeps_sidebar_after_dialog(_editor: &SlideEditor) -> bool {
        false
    }

    /// Whether the autocorrect options dialog of the format is open on its list (not on an entry).
    fn autocorrect_dialog_parent_open(_editor: &SlideEditor) -> bool {
        false
    }

    /// The open オートコレクト panel, where the screen keeps it (by default with the shell's panels of the shapes; a
    /// document and a workbook keep it with their own panels).
    fn autocorrect_panel_mut(editor: &mut SlideEditor) -> Option<&mut crate::autocorrect::AutoCorrectPanel> {
        editor.shape_edit.autocorrect.as_mut()
    }

    /// Up and Down in a dialog that chooses among options by keys (a workbook's insert or delete cells): true if used.
    fn dialog_arrow_step(_editor: &mut SlideEditor, _step: i32, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// A field of the dialog keeps Enter for itself (a workbook's import dialog's file path loads the file).
    fn dialog_field_owns_enter(_editor: &SlideEditor, _window: &Window, _cx: &App) -> bool {
        false
    }

    /// A message over a panel is shown: the format forgets what it was waiting on (a rejected cell entry).
    fn panel_message_shown(_editor: &mut SlideEditor) {}

    /// The message is dismissed: the format carries on with what it was waiting on (a rejected cell entry is cancelled).
    fn panel_message_dismissed(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// Where the message is put when it belongs beside something on the page (a rejected cell); `None`: centred as usual.
    fn panel_message_origin(_editor: &SlideEditor) -> Option<Point<Pixels>> {
        None
    }

    /// The message is anchored to something on the page (it is then not pushed to the bottom beside the sidebar).
    fn panel_message_anchored(_editor: &SlideEditor) -> bool {
        false
    }

    /// The size the anchored message was drawn at.
    fn panel_message_measured(_editor: &mut SlideEditor, _bounds: Bounds<Pixels>, _cx: &mut Context<SlideEditor>) {}

    // ----- Information bar (see `shell/infobar.rs`) ---------------------------

    /// The bar of the open file that is the format's own (protection, marked as final, read-only advice, compatibility, ...):
    /// `None` for none. The shell's own bars (a password to modify, the CMYK notices) are decided before.
    fn infobar_kind(_editor: &SlideEditor) -> Option<crate::infobar::InfoBarKind> {
        None
    }

    /// What the button of one of the format's bars does.
    fn infobar_action(_editor: &mut SlideEditor, _kind: crate::infobar::InfoBarKind, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The ids and the strings of a bar the format words in its own way (`InfoBarKind::spec` has the rest).
    fn infobar_spec(_kind: crate::infobar::InfoBarKind) -> Option<crate::infobar::InfoBarSpec> {
        None
    }

    /// Words added in brackets to the bar's title (a document's compatibility mode's name).
    fn infobar_title_suffix(_editor: &SlideEditor, _kind: crate::infobar::InfoBarKind) -> Option<String> {
        None
    }

    // ----- The workspace's page (see `shell/workspace.rs`) ----------------------

    /// The page is shown as the papers of a page layout (a workbook's 改ページ プレビュー / ページ レイアウト) and not as the grid.
    fn workspace_page_layout_shown(_editor: &SlideEditor) -> bool {
        false
    }

    /// The papers of the page layout view, for the page shown.
    fn workspace_page_layout(_editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        None
    }

    /// Where the main page starts in the scrolled area (a workbook's: after the row and column headers).
    fn workspace_main_pane_lead(_editor: &SlideEditor) -> Point<Pixels> {
        Point::default()
    }

    /// The cell being typed into, drawn over the page.
    fn workspace_cell_editor(_editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        None
    }

    /// The panes held in place over the scrolled page (a worksheet's frozen panes).
    fn workspace_frozen_panes(_editor: &SlideEditor, _layout: crate::editor::SlideLayout, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Vec<AnyElement> {
        Vec::new()
    }

    /// The headers along the page's edges (a worksheet's row and column headers).
    fn workspace_headers(_editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        None
    }

    /// The scrollbars of each pane, when the page is split into panes (and not shown as papers): `None` when it is not.
    fn workspace_split_scrollbars(_editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<Vec<AnyElement>> {
        None
    }

    /// The page break preview is on (the workspace's scrollbar track is then half-transparent white).
    fn workspace_page_break_preview_on(_editor: &SlideEditor) -> bool {
        false
    }

    /// The frame of the page's grid when the page is a worksheet (the page is at least as big as the grid).
    fn workspace_sheet_frame(_editor: &SlideEditor) -> Option<EmuRect> {
        None
    }

    /// Ink is hidden on the page (a worksheet's 描画の非表示).
    fn workspace_hide_ink(_editor: &SlideEditor) -> bool {
        false
    }

    /// What is drawn over the grid of a worksheet and under its charts and pictures.
    fn workspace_sheet_overlays(_editor: &SlideEditor, _scale: f32, _cx: &mut Context<SlideEditor>) -> SheetOverlays {
        SheetOverlays::default()
    }

    /// Form controls are in design mode (a group box then takes the mouse).
    fn workspace_shape_design_mode(_editor: &SlideEditor) -> bool {
        false
    }

    /// A press of the right button on a shape is the format's (a worksheet's shapes have their own context menu).
    fn workspace_shape_has_right_click(_editor: &SlideEditor) -> bool {
        false
    }

    fn workspace_shape_right_down(_editor: &mut SlideEditor, _id: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// A press (the left or the right button, `_event.button`) on the page's grid, the shape that is the page itself
    /// and is never moved (a worksheet's cells: the left one selects, the right one opens the cells' menu).
    fn workspace_grid_down(_editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The key context of the workspace while no text is edited: the keys bound to it are the screen's (a sheet's
    /// arrows move the cell).
    fn workspace_key_context() -> &'static str {
        crate::editor::WORKSPACE_CONTEXT
    }

    /// Whether the page is drawn with a shadow, as a sheet of paper on the workspace (not a worksheet, whose page is
    /// the whole workspace).
    fn workspace_page_shadow() -> bool {
        true
    }

    // ----- Actions (see `shell/editor.rs`) -------------------------------------

    /// Puts on the window's root element the handlers of the actions only this format answers to (a presentation's slide show and
    /// views, a workbook's cell moves, a document's scrolling). The ones every format shares are in the shell.
    fn register_actions(_root: Stateful<Div>, _cx: &mut Context<SlideEditor>) -> Stateful<Div> {
        _root
    }

    // ----- Events (see `shell/editor.rs`) ---------------------------------------
    // Each is asked at the place of the window's handler where the format's part used to be; `true` says the format used the event.

    /// Escape (the cancel command) before the window's own dialogs are looked at (a slide show in progress).
    fn cancel_tool_early(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// Escape after the window's own dialogs and before it ends text editing or drops the selection.
    fn cancel_tool_late(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The pointer moved: the first thing the format does with it (before the window's crop).
    fn pointer_move_early(_editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The pointer moved: the format's drags (a workbook's pen stroke, then its cell, split, timeline, control and ruler drags).
    fn pointer_move(_editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The button went up: the first thing the format does (a cell's reference drag ends).
    fn pointer_up_early(_editor: &mut SlideEditor) {}

    /// The button went up: the format's drags end (a workbook's pen stroke first).
    fn pointer_up(_editor: &mut SlideEditor, _event: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// A picked range follows the pointer.
    fn range_pick_move(_editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    fn range_pick_down(_editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    fn range_pick_up(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    fn range_pick_key(_editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// A confirmation is shown: a drag of the range pick is let go.
    fn range_pick_suspend_drag(_editor: &mut SlideEditor, _window: &mut Window) {}

    /// An element over the whole window that sees the press before the dialog in front does (the range pick's).
    fn root_event_surface(_editor: &SlideEditor, _cx: &Context<SlideEditor>) -> Option<AnyElement> {
        None
    }

    /// A key the format takes before the dialog and menu keys (a presentation's master grid, a workbook's lists).
    fn format_key(_editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The picture crop mode took a key: the format's crop fields are brought up to date.
    fn crop_fields_synced(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The window's zoom was set (`zoom`, 1.0 being 100%, `None` fitting the page), when the file keeps it
    /// (a workbook's sheet has its zoom).
    fn zoom_set(_editor: &mut SlideEditor, _zoom: Option<f32>) {}

    /// The model has changed under the window (an undo, a redo, another window of the same file): what the window
    /// shows that the file keeps follows it (the zoom of a workbook's sheet, a document's view mode).
    fn view_from_model(_editor: &mut SlideEditor) {}

    /// Another window on the same file changed the model and this window has it now (`SlideEditor::sync_peers`).
    fn model_from_peer(editor: &mut SlideEditor) {
        Self::view_from_model(editor);
    }

    /// What only this screen has of the window's view, taken before the file is read again (`shell/view_state.rs`:
    /// 読み込み直す after another app changed it). The value is the screen's own type; the shell only hands it back.
    fn view_kept(_editor: &SlideEditor) -> Option<Box<dyn std::any::Any>> {
        None
    }

    /// Put back what `view_kept` took, on the file that was read again, as far as it is still there. `_kept` is
    /// downcast to the screen's own type; a value of another type (kept by another kind of screen) is left.
    fn view_put_back(_editor: &mut SlideEditor, _kept: &dyn std::any::Any, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The interval of the autorecover snapshots the file itself keeps, in minutes (`shell/autorecover.rs`; a
    /// document's save frequency): `Some(0)` is off for this file, `None` leaves it to the app's setting.
    fn autorecover_minutes(_editor: &SlideEditor) -> Option<u32> {
        None
    }

    /// Keep the interval of the autorecover snapshots in the file (0: off for this file), when the format's files
    /// have a place for it. Nothing by default.
    fn set_autorecover_minutes(_editor: &mut SlideEditor, _minutes: u32, _cx: &mut Context<SlideEditor>) {}

    /// The fonts the open file uses, for フォントの置換 (`shell/replace_fonts.rs`). By default the fonts of the shapes' text;
    /// a screen whose text is elsewhere says its own (a document's text runs, a workbook's cells).
    fn used_fonts(editor: &SlideEditor) -> Vec<String> {
        crate::replace_fonts::presentation_used_fonts(editor.presentation())
    }

    /// Replace the font `target` by `replacement` in the whole file, as one step of the undo history; how many places
    /// changed. By default the shapes' text (`SlideEditor::replace_fonts_in_shapes`).
    fn replace_fonts(editor: &mut SlideEditor, target: &str, replacement: &str, cx: &mut Context<SlideEditor>) -> usize {
        editor.replace_fonts_in_shapes(target, replacement, cx)
    }

    /// The page shown has gone from the model (an edit, an undo or a redo removed it): the page the format would
    /// show instead, when it has one to prefer (the sheet a workbook's analysis panel works on). `None`, or a page
    /// that is not in the model, leaves it to the window (a workbook's first sheet, else the first page).
    fn page_when_current_gone(_editor: &SlideEditor) -> Option<SlideId> {
        None
    }

    /// Undo or redo has changed the document: before the window brings its zoom and selection up to date.
    fn history_applied_early(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// Undo or redo has changed the document: after the window brought its own parts up to date.
    fn history_applied(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// A modifier key went down or up, when a drag of the format's follows it (a workbook's cell move and fill copy while ⌥ is held).
    fn modifiers_changed(_editor: &mut SlideEditor, _event: &ModifiersChangedEvent, _cx: &mut Context<SlideEditor>) {}

    /// The wheel over the workspace, when the format uses it (a presentation's Draw panel ruler, a workbook's split panes).
    fn workspace_wheel(_editor: &mut SlideEditor, _event: &ScrollWheelEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// A press on a shape that the format takes before the window does anything with it (a workbook's pen draws over
    /// the shape, and its eraser rubs a stroke out).
    fn shape_press_early(_editor: &mut SlideEditor, _event: &MouseDownEvent, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// A press on a shape that is the format's (a workbook's form controls, timelines and slicers act on their own).
    fn shape_press(_editor: &mut SlideEditor, _shape: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    // ----- The window's life (see `shell/editor.rs`) ----------------------------

    /// A slide show is started (a presentation's; the others have none).
    fn show_start(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// A slide show in progress is ended (a new file is loaded, the file is closed).
    fn show_end(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// A slide show is on (the window's keys and commands for editing wait until it ends; the show itself is the format's state).
    fn show_running(_editor: &SlideEditor) -> bool {
        false
    }

    /// The next or previous page is asked for while a slide show is on: it steps the show.
    fn show_step(_editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The first or last page is asked for while a slide show is on.
    fn show_to_end(_editor: &mut SlideEditor, _last: bool, _cx: &mut Context<SlideEditor>) {}

    /// The slide show, drawn over the whole window.
    fn show_element(_editor: &SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        None
    }

    /// The slide show fills this window (and the rest of the window is not drawn).
    fn show_in_main(_editor: &SlideEditor) -> bool {
        false
    }

    /// 挿入 > リンク on a page (a presentation's slide), unless a slide show is on.
    fn open_link_panel_for_page(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// A panel of the shell opens (its state is in `ShapeEditState`): the format closes the panels of its own that give way to it.
    fn panel_opening(_editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel) {}

    /// A panel of the shell closed: the format closes the panels of its own that go with it.
    fn panel_closed(_editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel) {}

    /// The file is marked as final, so edits are refused (a presentation's).
    fn edit_blocked_by_file(_editor: &SlideEditor) -> bool {
        false
    }

    /// A page cannot be added because of the file (marked as final, or read-only by its password).
    fn add_page_blocked(_editor: &SlideEditor) -> bool {
        false
    }

    /// A model has been loaded: the format puts the view where the file left it (a workbook's sheets: selection and scroll).
    fn document_loaded(_editor: &mut SlideEditor) {}

    /// A file the user opened is shown (a presentation opens without its speaker notes). Only the browser's way of opening
    /// (the file picked there and read in the page, `open_file` under wasm) calls it, so the other targets do not use it.
    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn file_opened(_editor: &mut SlideEditor) {}

    /// Why the file cannot be saved for now, in the user's words (`SlideEditor::save_to` gives it as its error): a view
    /// that edits the model in a form that is not the file's is open (a presentation's master view, its notes formatting).
    fn save_refused(_editor: &SlideEditor) -> Option<String> {
        None
    }

    /// The model the file is written from (`SlideEditor::presentation_for_save`): the open model, or a copy of it with
    /// what the screen keeps outside the model put in (where each worksheet is scrolled to and what is selected in it).
    /// The open model by default.
    fn model_for_save(editor: &SlideEditor) -> std::borrow::Cow<'_, Presentation> {
        std::borrow::Cow::Borrowed(editor.presentation())
    }

    /// The file at `_path` is about to be written over (`SlideEditor::save_to`): the format keeps a backup copy of what
    /// the file is now, when its option says so (a workbook's and a document's バックアップ コピーを作成する). Nothing by default.
    #[cfg(not(target_family = "wasm"))]
    fn backup_before_write(_editor: &SlideEditor, _path: &std::path::Path) {}

    /// The pages selected in the list of pages, in order: the one shown, unless the format's list holds several (a
    /// presentation's slides chosen with Shift or ⌘).
    fn selected_pages(editor: &SlideEditor) -> Vec<SlideId> {
        vec![editor.current_slide]
    }

    /// The list of pages keeps only the page shown (another page is shown, or the chosen ones were removed).
    fn clear_page_selection(_editor: &mut SlideEditor) {}

    /// The row of the list of pages that shows the page at `index` (the list scrolls to it when the page is shown): the
    /// page's own index, unless the list has rows that are not pages (a presentation's section headings, with the folded
    /// sections' slides hidden).
    fn page_list_row(_editor: &SlideEditor, index: usize) -> usize {
        index
    }

    /// The text of a shape is about to be edited in place (a presentation's outline gives up the keys).
    fn text_edit_starting(_editor: &mut SlideEditor) {}

    /// A key that the shape text's keys would take (`SlideEditor::on_text_key`), before them: true when the keys are the
    /// format's for now (a presentation's path being drawn, its outline's line being edited), and nothing more is done.
    fn text_key_early(_editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// ⌘B ⌘I ⌘U while a shape's text is edited: true when the format changed the characters by its own means (a
    /// document's text box holds the document's runs); false and the shell toggles the shape's characters.
    fn toggle_text_characters(_editor: &mut SlideEditor, _toggle: crate::text_keys::CharacterToggle, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// ⌘C ⌘X while the format edits a text of its own, not a shape's (a presentation's outline): the selected text on the
    /// clipboard (`cut`: and taken away), and whether there was a text. `None`: no such edit, the shell copies the shape's.
    fn own_text_copy(_editor: &mut SlideEditor, _cut: bool, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        None
    }

    /// ⌘V while the format edits a text of its own (see `own_text_copy`).
    fn own_text_paste(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        None
    }

    /// Whether a link is under the pointer (the pointer becomes a hand).
    fn link_under_pointer(_editor: &SlideEditor, _position: Point<Pixels>) -> bool {
        false
    }

    /// A press seen at the window's root before the page's layers get it: true if the format used it on a link under the
    /// pointer (a document follows the link or opens its menu, so that no layer over the text swallows the press).
    fn link_pressed(_editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// What is selected on the screen, as data for the menu bar's context menus and the sidebars (`shell/menu_context.rs`):
    /// the format fills in the fields of its own screen (`state` comes with nothing selected).
    fn selection_context(_editor: &SlideEditor, _state: &mut crate::menu_context::ContextState) {}

    /// The picture トリミング (`shell/crop_mode.rs`) would work on now, the place the whole picture has (EMU of the slide,
    /// the sheet or the page it is on) and its crop; `None` when the selection is not one picture that can be cropped
    /// there. The default is the picture shape selected on a slide or a sheet.
    fn crop_target(editor: &SlideEditor) -> Option<(crate::crop_mode::CropTarget, EmuRect, ibis_doc_model::Crop)> {
        editor.slide_crop_target()
    }

    /// Crop mode ends keeping the crop: `target` (the one `crop_target` gave) takes `crop`, and its frame the kept part of
    /// `full`, as one change to undo.
    fn crop_keep(editor: &mut SlideEditor, target: &crate::crop_mode::CropTarget, full: EmuRect, crop: ibis_doc_model::Crop, cx: &mut Context<SlideEditor>) {
        editor.keep_slide_crop(target, full, crop, cx);
    }

    /// Where the pointer is while `target` is being cropped, in the EMU `crop_target` gave its place in.
    fn crop_pointer(editor: &SlideEditor, _target: &crate::crop_mode::CropTarget, position: Point<Pixels>) -> Option<(f64, f64)> {
        editor.slide_crop_pointer(position)
    }

    /// Open the pane `id` of the sidebar table (`shell/sidebars.rs`, `sidebar.*`) that this screen has no command for
    /// (a document's spelling, thesaurus, screen reader, read aloud, ink and toolbar panes). A pane the screen does
    /// not have is left alone.
    fn open_sidebar(_editor: &mut SlideEditor, _id: &'static str, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// What a command of the Draw menu (`shell/draw_menu.rs`) does on this screen: choosing a pen, converting the ink
    /// selected to a shape. `None`: the screen does not do it from the menu yet, and the row is grey with that reason.
    fn draw_command(_editor: &SlideEditor, _command: crate::draw_menu::DrawCommand) -> Option<crate::chrome::Handler> {
        None
    }

    /// Move the shapes a paste is about to insert (`SlideEditor::paste_office_shapes`, `shell/shape_clipboard.rs`) to where the screen puts a
    /// paste. `stamp` stands for the clipboard's data (`shell::paste_placement::stamp`), so a screen can step a
    /// repeated paste of the same data. Nothing by default: the shapes keep the place the common paste gave them
    /// (a sheet: the selected cell, `xlsx/paste_placement.rs`).
    fn place_pasted_shapes(_editor: &mut SlideEditor, _shapes: &mut [ibis_doc_model::Shape], _stamp: u64) {}

    /// Put the shapes of a paste (`SlideEditor::paste_office_shapes`) into the text of a flowing document, at the
    /// caret; whether they went in. Asked only when the open document has such a text; a screen without one keeps the
    /// default.
    fn paste_shapes_in_text(_editor: &mut SlideEditor, _shapes: Vec<ibis_doc_model::Shape>, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The lines the screen puts after the first group of the right-click menu of its text
    /// (`SlideEditor::doc_body_popup_entries`): commands that are only buttons elsewhere and so have no line in the
    /// layout (a document's フォント… and 段落…).
    fn body_popup_lines(_editor: &SlideEditor) -> Vec<crate::chrome::MenuEntry> {
        Vec::new()
    }

    /// The lines the format adds to the context menu of a panel of the shell (a presentation: アクセシビリティ チェック in
    /// the alt text panel's, ノートのヘッダーとフッター in the header and footer panel's).
    fn panel_menu_extras(_editor: &SlideEditor, _panel: crate::shape_state::ShapePanel) -> Vec<crate::chrome::MenuEntry> {
        Vec::new()
    }

    /// The screen's own editor of text is open (a workbook's cell being typed in, in any mode): Esc gives that up first
    /// (the data grid of a chart's pane, `shell/chart_data.rs`).
    fn own_edit_open(_editor: &SlideEditor) -> bool {
        false
    }

    /// Something typed and not yet entered counts as a change that is not saved (a workbook's cell being edited).
    fn has_pending_edit(_editor: &SlideEditor) -> bool {
        false
    }

    /// What is typed and not yet entered is entered, so that a save that follows writes it too (`SlideEditor::save_then`;
    /// a workbook's cell being edited). Nothing by default: a shape's text being edited is the shell's own to put in.
    fn enter_pending_edit(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// The document's layout is made again if the model it was laid out from has changed (a document's pages).
    fn relayout_if_changed(_editor: &mut SlideEditor) {}

    /// A key for the formula bar and cell editor (a workbook's); `true` if it took it.
    fn formula_edit_key(_editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// The font size (points) chosen in the toolbar, applied to what is selected.
    fn set_font_size(_editor: &mut SlideEditor, _pt: f32, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The items of a searchable dropdown of the toolbar that is the format's (a document's font lists); `None`: not its.
    fn searchable_menu_entries(_editor: &SlideEditor, _id: &str) -> Option<Vec<crate::chrome::MenuEntry>> {
        None
    }

    // ----- The sidebar (see `shell/chrome.rs`) ----------------------------------

    /// Before the sidebar is drawn (a workbook's fields pane follows the current cell into a pivot table and back out).
    fn sidebar_sync(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The panel of the ruler settings (a document's), while it is open: it is shown before the format's sidebar.
    fn ruler_panel(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// The pane that stays behind a dialog opened over it (a workbook's chart, sort, names or pivot pane; a presentation's
    /// animation pane): the dialog's key says which.
    fn dialog_background_panel(_editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// The dialog a dialog was opened from, kept in view behind it (the conditional formatting manager under its rule editor).
    fn dialog_parent_panel(_editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// Dialogs of the format that are shown whatever the open command (a document's merge and tab stop editors, a workbook's
    /// series editor and range picker).
    fn extra_dialogs(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Vec<ExtraDialog> {
        Vec::new()
    }

    /// The dialog in front blocks the pane behind it (it is drawn over, not usable).
    fn sidebar_blocked_by_dialog(_editor: &SlideEditor) -> bool {
        false
    }

    // ----- The title row (see `shell/chrome.rs`) -------------------------------

    /// The title row's button of the format's own mode (a presentation's: the one that closes the master view or the notes
    /// formatting, else the slide show's with its start menu); `icon_only` leaves its label out.
    fn title_mode_button(_editor: &SlideEditor, _weak: &WeakEntity<SlideEditor>, _icon_only: bool) -> Option<AnyElement> {
        None
    }

    // ----- The theme's colours (see `shell/theme_colors.rs`) ---------------------
    // The dialog of the palettes and the editor of a custom palette are shell's, one for every format. The theme itself is
    // the format's (a workbook's is in its workbook part, a document applies it to its flow), and so is the draft's place.
    // The defaults are the ones of a file whose theme is the model's own (a presentation).

    /// The draft that the theme's panel edits (`None` while the panel is closed).
    fn theme_panel(editor: &SlideEditor) -> Option<&crate::theme_panel::ThemePanel> {
        editor.shape_edit.theme_panel.as_ref()
    }

    /// The theme's panel opens with a fresh draft read from the file's theme (an open one is replaced).
    fn theme_panel_open(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.open_theme_panel(window, cx)
    }

    /// The draft is applied to the file's theme. The panel closes; it stays open with its error if a value is not valid.
    fn theme_panel_apply(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_theme_panel(window, cx)
    }

    /// The data grid of the chart being edited (`shell/chart_data.rs`), where the screen keeps it: with the shapes'
    /// panels by default; a document keeps it with its own chart panel.
    fn chart_data_window(editor: &SlideEditor) -> Option<&crate::chart_data::DataWindow> {
        editor.shape_edit.chart_data.as_ref()
    }

    /// The place the data grid of the chart being edited is kept in, to put one in or take it out. None: the screen
    /// keeps none (a worksheet's chart takes its data from the sheet's own cells).
    fn chart_data_slot(editor: &mut SlideEditor) -> Option<&mut Option<crate::chart_data::DataWindow>> {
        Some(&mut editor.shape_edit.chart_data)
    }

    /// The data of chart `shape` as the file has it (its workbook, or one made from its cached values), for the data
    /// grid. By default the chart is a shape of page `slide`; a document's is in its text.
    fn read_chart_data(editor: &SlideEditor, source: &ibis_pptx::Source, slide: SlideId, shape: ibis_doc_model::ShapeId) -> Result<ibis_pptx::ChartData, ibis_pptx::Error> {
        source.chart_data(editor.presentation(), slide, shape)
    }

    /// The data grid of chart `shape` changed: the chart is made anew from `data` (`book`: the grid's workbook model).
    /// By default the chart is a shape of page `slide` (`SlideEditor::apply_shape_chart_data`).
    fn apply_chart_data(editor: &mut SlideEditor, slide: SlideId, shape: ibis_doc_model::ShapeId, book: &Presentation, data: ibis_pptx::ChartData, cx: &mut Context<SlideEditor>) {
        editor.apply_shape_chart_data(slide, shape, book, data, cx)
    }

    /// A field of the chart panel was typed in: what the panel holds is put in the chart. By default the chart is a
    /// shape (`SlideEditor::apply_chart_panel`).
    fn chart_panel_apply(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_chart_panel(window, cx)
    }

    /// The theme's ten colours the colour palettes show in their theme rows (`shell/chrome.rs`), in SpreadsheetML's order
    /// (lt1, dk1, lt2, dk2, accent1–6). Office's own by default, as it was before the hook: only a workbook's theme
    /// reaches the palettes (the file was asked for a workbook's theme part, which a presentation and a document lack).
    fn palette_theme_colors(_editor: &SlideEditor) -> [ibis_doc_model::Rgb; 10] {
        ibis_pptx::workbook_theme_colors(None)
    }

    /// The palettes the format offers: the id, the i18n key of the name, the colours by their scheme names (`dk1`, `accent1`, …).
    fn theme_palettes() -> Vec<(&'static str, &'static str, Vec<(&'static str, ibis_doc_model::Rgb)>)> {
        crate::theme_panel::THEME_PALETTES.iter().map(|p| (p.id, p.label_key, p.colors.to_vec())).collect()
    }

    /// The palette of that id (one of `theme_palettes`) becomes the file's theme colours.
    fn theme_palette_apply(editor: &mut SlideEditor, id: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
        editor.apply_theme_palette(id, window, cx)
    }

    // ----- Files (see `shell/file_actions.rs`) -----------------------------------

    /// A new file is shown (a document gets its default design).
    fn new_file_shown(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// ファイル > 印刷…: the format asks what to print (a presentation's 印刷の設定, a document's and a workbook's print panels);
    /// `true` if it opened its panel, else the file is printed as it is.
    fn open_print_panel(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }

    /// Before the file is printed or exported (a document updates its fields and links if its settings say so).
    fn prepare_print_or_export(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// What the PDF of the file, for printing (`printing`) or for export, is made from.
    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(editor: &SlideEditor, _printing: bool) -> Result<PdfJob, String> {
        Ok(PdfJob { bytes: editor.current_bytes()?, slides: None, format: "pdf", args: None })
    }

    /// What is written to `path` when its extension is not written as the format's own package (a workbook as .csv, a
    /// presentation as .ppsx, a template): None for the format's own (`Source::save` of `model_for_save`, which the file's
    /// password encrypts). `copy`: the file is written beside its own (the AutoRecover snapshot, a backup), not saved.
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(_editor: &SlideEditor, _path: &std::path::Path, _source: &ibis_pptx::Source, _copy: bool) -> Option<Result<FileBytes, String>> {
        None
    }

    /// The folder the save dialog opens in for a new file, when the format has a setting of its own for it (a workbook's
    /// 既定のローカル ファイルの保存場所).
    #[cfg(not(target_family = "wasm"))]
    fn new_file_save_folder(_editor: &SlideEditor) -> Option<std::path::PathBuf> {
        None
    }

    /// More arguments for this program's `--export` run of the open file (`SlideEditor::write_pdf_with`, `write_png`,
    /// `write_gif`): what the format's settings of the window say of the output (a document's print options).
    fn export_args(_editor: &SlideEditor) -> Vec<&'static str> {
        Vec::new()
    }

    /// The window shows only the pages: the title row, the menu bar, the toolbar, the status bar and the right pane are
    /// not drawn (a document's フォーカス, DOCX-VIEW-006). The window's `render` asks.
    fn chrome_hidden(_editor: &SlideEditor) -> bool {
        false
    }

    // ----- Text edited in a box of the page's own kind (a document's floating text box; see `shell/text_edit.rs`) -------

    /// A text box of the document is being edited (the shell's own text editing session is not).
    fn box_edit_active(_editor: &SlideEditor) -> bool {
        false
    }

    /// The editing state of that box: the caret, the selection, the composing text.
    fn box_text_edit(_editor: &SlideEditor) -> Option<&crate::text_edit::TextEditing> {
        None
    }

    fn box_text_edit_mut(_editor: &mut SlideEditor) -> Option<&mut crate::text_edit::TextEditing> {
        None
    }

    /// The text body of that box.
    fn box_edit_body(_editor: &SlideEditor) -> Option<&ibis_doc_model::TextBody> {
        None
    }

    /// Editing the box is finished.
    fn box_edit_commit(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The text of the box changes at `start` (`remove` characters out, `insert` in): `Some(done)` if a box is being edited.
    fn box_edit_splice(_editor: &mut SlideEditor, _start: usize, _remove: usize, _insert: &str, _typing: bool, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        None
    }

    /// The edited box or its composing text changed: what is drawn from it is brought up to date.
    fn box_edit_changed(_editor: &mut SlideEditor) {}

    /// Where the box's characters are (for the caret and the selection).
    fn box_edit_positions(_editor: &SlideEditor) -> Option<crate::render::rich_text::TextPositions> {
        None
    }

    /// The x the caret keeps moving a line up or down.
    fn box_edit_goal_x(_editor: &SlideEditor) -> Option<(usize, f32)> {
        None
    }

    fn box_edit_set_goal_x(_editor: &mut SlideEditor, _goal: (usize, f32)) {}

    // ----- Opening a file (see `shell/read_file.rs`) -----------------------

    /// The file at `path` could not be parsed. Every format hears it, whatever the file's kind (a format that offers to
    /// repair what it failed to open keeps its own record).
    fn note_open_failed(_path: &std::path::Path) {}

    /// The file at `path` was opened. Every format hears it, whatever the file's kind.
    fn note_opened(_path: &std::path::Path) {}

    // ----- Scenario tests (see `shell/scenario.rs`) ------------------------

    /// The format's scenario tests: (name, function), run by
    /// `tests/scenarios.rs` on the main thread. Put them in
    /// `src/<format>/scenarios.rs` (under `#[cfg(feature = "scenarios")]`).
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
        Vec::new()
    }

    /// The format's scenarios of bugs not fixed yet (its corpus reports and the like): a plain `cargo test` skips them,
    /// `--ignored` or `--include-ignored` runs them.
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn ignored_scenarios() -> Vec<(&'static str, fn())> {
        Vec::new()
    }

    // ----- The chrome (see `shell/chrome.rs`) -----------------------------

    /// Add to or connect the menus: the shell has put in the items every
    /// format shares and placeholders (disabled until connected, ids in
    /// `shell/chrome_defaults.rs`); connect them with `menus.connect(id, …)`
    /// or add items with `menus.items_mut(MenuId::…)`.
    fn menus(_editor: &SlideEditor, _menus: &mut Menus) {}

    /// The common left part's 書式のコピー/貼り付け button: whether it is
    /// on (a format picked up, waiting to be put on), and what a click does.
    /// `None` leaves the button disabled.
    fn format_paint(_editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)> {
        None
    }

    /// コメントを追加: what ⌥⌘M does (the title row has no button for it: コメントを表示 is the one). `None`: the key does nothing.
    fn add_comment(_editor: &SlideEditor) -> Option<crate::chrome::Handler> {
        None
    }

    /// The title bar's コメントを表示 button: whether the comments are shown
    /// (the button is drawn filled), and what a click does. `None` leaves
    /// it disabled.
    fn show_comments(_editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)> {
        None
    }

    /// The format's own buttons of the title row, after コメントを表示 (a document's navigation pane, a presentation's notes
    /// pane). The row counts them for its room (`title_buttons_hidden`).
    fn title_buttons(_editor: &SlideEditor) -> Vec<TitleButton> {
        Vec::new()
    }

    /// The screen's own lines of the commands' table at `part` (`chrome_defaults::menus`): what only this screen has in a
    /// shared menu, and the whole of a menu only it has. They are placeholders as the shared ones are (`menus` connects them).
    fn menu_items(_editor: &SlideEditor, _part: crate::chrome::MenuPart) -> Vec<crate::chrome::MenuEntry> {
        Vec::new()
    }

    /// The screen's own buttons of the toolbar's left part, between メニューを検索 and 書式のコピー/貼り付け (新しいスライド,
    /// スペル チェックと文章校正).
    fn toolbar_left(_editor: &SlideEditor) -> Vec<ToolItem> {
        Vec::new()
    }

    /// The toolbar's right half, for what is selected. Empty: the toolbar ends with its left part.
    fn toolbar_right(_editor: &SlideEditor) -> Vec<ToolItem> {
        Vec::new()
    }

    /// After a model is read (a file opened or loaded again): the format lays out or fills in what its pages need before they are
    /// shown (a document's pages and fields, a workbook's pivot tables and row heights). Every format is asked, each does its own;
    /// `reloaded` is false for the first window of a file. `file_name` is the file's name, if it has one.
    fn prepare_model(_presentation: &mut Presentation, _resources: &crate::render::shape_view::Resources, _text_system: &TextSystem, _file_name: Option<&str>, _reloaded: bool) {}

    /// The Home tab's command list gets these buttons after the toolbar's own (a presentation: its title-bar menus).
    fn menu_buttons(_editor: &SlideEditor) -> Vec<ToolItem> {
        Vec::new()
    }

    /// The sidebar's content. `None`: nothing to show (no sidebar).
    fn sidebar(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// The tabs across the sidebar's top, for the panels that stand side by
    /// side (a workbook's 書式設定・グラフ エディター・データの入力規則).
    fn sidebar_tabs() -> Vec<crate::chrome::SidebarTab> {
        Vec::new()
    }

    /// Whether the right pane has something to show when it is opened with no panel chosen (a workbook: what is
    /// selected; a presentation: the slide's or the shape's size and position). A document's shows only a panel opened
    /// for it (書式・コメント・検索…), so its title-row button cannot open the pane by itself. A pane with something to show
    /// starts shut when a file opens (it would take the room of the sheet or the slide) and the Window menu has its toggle; a
    /// document's is open from the start in a file that can be edited, and its menu has no toggle.
    fn sidebar_on_request() -> bool {
        false
    }

    /// The right pane was shown or hidden by its button (`SlideEditor::toggle_sidebar`; `sidebar_open` says which): the
    /// format chooses what the pane is for (a presentation's アニメーション ウィンドウ, a workbook's selection).
    fn sidebar_toggled(_editor: &mut SlideEditor) {}

    /// The screen's Ink panel is open in the sidebar (the title row's Draw button is drawn filled, and a press closes it).
    fn ink_panel_open(_editor: &SlideEditor) -> bool {
        false
    }

    /// Closes the screen's Ink panel (a press on the title row's Draw button while it is open).
    fn close_ink_panel(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {}

    /// Commands ran as an undo step (`SlideEditor::run`); `structural` when the sections or the slides' order changed. The
    /// format brings what it keeps beside the document in step (a presentation's summary zooms, its notes field's typing).
    fn commands_ran(_editor: &mut SlideEditor, _structural: bool, _cx: &mut Context<SlideEditor>) {}

    /// Delete, when the format has a selection of its own inside the selected shape (a vertex while a shape's points are
    /// edited): true when the format took it, and the shapes are left.
    fn delete_own_selection(_editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> bool {
        false
    }
}


/// What the shell asks of the open document's format, as an object: `with_format!` and the driver table (`driver`) call it. A format
/// writes only a [`FormatEditor`]; [`Driver<F>`] turns it into this (so that the shell never names a format's type) and the app
/// lists the three in `DRIVERS` (`format_drivers.rs`). A new format is a crate with a `FormatEditor` and a line there.
pub trait FormatDriver: Sync {
    fn info(&self) -> &'static FormatInfo;
    fn page_noun(&self) -> &'static str;
    fn fit_tooltip(&self) -> &'static str;
    fn slides(&self) -> bool;
    fn shapes(&self) -> bool;
    fn can_fit(&self) -> bool;
    fn new_state(&self, presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState>;
    fn new_prefs(&self) -> Box<dyn std::any::Any>;
    fn resources_renewed(&self, _editor: &mut SlideEditor);
    fn initial_zoom(&self, _editor: &SlideEditor) -> Option<f32>;
    fn workspace_content(&self, editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent;
    fn zoom_scroll_y(&self, _editor: &SlideEditor, _before: &WorkspaceContent, _after: &WorkspaceContent, _old: Pixels, _view: Pixels, _anchor: Pixels) -> Option<Pixels>;
    fn step_page(&self, _editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn show_page(&self, _editor: &mut SlideEditor, _slide: SlideId, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn page_changed(&self, _editor: &mut SlideEditor, _from: SlideId, _to: SlideId);
    fn before_render(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn render_body(&self, editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement;
    fn status_items(&self, _editor: &SlideEditor) -> Vec<SharedString>;
    fn status_items_at_right(&self) -> bool;
    fn status_hint(&self, _editor: &SlideEditor) -> Option<SharedString>;
    fn status_tab_scroll(&self, _editor: &SlideEditor) -> Option<ScrollHandle>;
    fn wants_text_input(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn text_for_range(&self, _editor: &mut SlideEditor, _range: Range<usize>, _adjusted: &mut Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<String>;
    fn selected_text_range(&self, _editor: &mut SlideEditor, _ignore_disabled_input: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<UTF16Selection>;
    fn marked_text_range(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Range<usize>>;
    fn unmark_text(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn replace_text_in_range(&self, _editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn replace_and_mark_text_in_range(&self, _editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _selected: Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn set_ime_clauses(&self, _editor: &mut SlideEditor, _clauses: Vec<ImeClause>, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn bounds_for_range(&self, _editor: &mut SlideEditor, _range: Range<usize>, _element: Bounds<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>>;
    fn text_vertical(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn character_index_for_point(&self, _editor: &mut SlideEditor, _point: Point<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<usize>;
    fn insert_symbol(&self, _editor: &mut SlideEditor, _c: char, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn range_pick_active(&self, _editor: &SlideEditor) -> bool;
    fn finish_range_pick(&self, _editor: &mut SlideEditor, _accept: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn range_pick_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel>;
    fn close_dialog(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn discard_dialog(&self, _editor: &mut SlideEditor, _key: &'static str);
    fn keeps_sidebar_after_dialog(&self, _editor: &SlideEditor) -> bool;
    fn autocorrect_dialog_parent_open(&self, _editor: &SlideEditor) -> bool;
    fn autocorrect_panel_mut<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut crate::autocorrect::AutoCorrectPanel>;
    fn dialog_arrow_step(&self, _editor: &mut SlideEditor, _step: i32, _cx: &mut Context<SlideEditor>) -> bool;
    fn dialog_field_owns_enter(&self, _editor: &SlideEditor, _window: &Window, _cx: &App) -> bool;
    fn panel_message_shown(&self, _editor: &mut SlideEditor);
    fn panel_message_dismissed(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    fn panel_message_origin(&self, _editor: &SlideEditor) -> Option<Point<Pixels>>;
    fn panel_message_anchored(&self, _editor: &SlideEditor) -> bool;
    fn panel_message_measured(&self, _editor: &mut SlideEditor, _bounds: Bounds<Pixels>, _cx: &mut Context<SlideEditor>);
    fn infobar_kind(&self, _editor: &SlideEditor) -> Option<crate::infobar::InfoBarKind>;
    fn infobar_action(&self, _editor: &mut SlideEditor, _kind: crate::infobar::InfoBarKind, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn infobar_spec(&self, _kind: crate::infobar::InfoBarKind) -> Option<crate::infobar::InfoBarSpec>;
    fn infobar_title_suffix(&self, _editor: &SlideEditor, _kind: crate::infobar::InfoBarKind) -> Option<String>;
    fn workspace_page_layout_shown(&self, _editor: &SlideEditor) -> bool;
    fn workspace_page_layout(&self, _editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement>;
    fn workspace_main_pane_lead(&self, _editor: &SlideEditor) -> Point<Pixels>;
    fn workspace_cell_editor(&self, _editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement>;
    fn workspace_frozen_panes(&self, _editor: &SlideEditor, _layout: crate::editor::SlideLayout, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Vec<AnyElement>;
    fn workspace_headers(&self, _editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement>;
    fn workspace_split_scrollbars(&self, _editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<Vec<AnyElement>>;
    fn workspace_page_break_preview_on(&self, _editor: &SlideEditor) -> bool;
    fn workspace_sheet_frame(&self, _editor: &SlideEditor) -> Option<EmuRect>;
    fn workspace_hide_ink(&self, _editor: &SlideEditor) -> bool;
    fn workspace_sheet_overlays(&self, _editor: &SlideEditor, _scale: f32, _cx: &mut Context<SlideEditor>) -> SheetOverlays;
    fn workspace_shape_design_mode(&self, _editor: &SlideEditor) -> bool;
    fn workspace_shape_has_right_click(&self, _editor: &SlideEditor) -> bool;
    fn workspace_shape_right_down(&self, _editor: &mut SlideEditor, _id: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn workspace_grid_down(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn workspace_key_context(&self) -> &'static str;
    fn workspace_page_shadow(&self) -> bool;
    fn register_actions(&self, _root: Stateful<Div>, _cx: &mut Context<SlideEditor>) -> Stateful<Div>;
    fn cancel_tool_early(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn cancel_tool_late(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn pointer_move_early(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn pointer_move(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn pointer_up_early(&self, _editor: &mut SlideEditor);
    fn pointer_up(&self, _editor: &mut SlideEditor, _event: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn range_pick_move(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn range_pick_down(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn range_pick_up(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn range_pick_key(&self, _editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn range_pick_suspend_drag(&self, _editor: &mut SlideEditor, _window: &mut Window);
    fn root_event_surface(&self, _editor: &SlideEditor, _cx: &Context<SlideEditor>) -> Option<AnyElement>;
    fn format_key(&self, _editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn crop_fields_synced(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn zoom_set(&self, _editor: &mut SlideEditor, _zoom: Option<f32>);
    fn view_from_model(&self, _editor: &mut SlideEditor);
    fn model_from_peer(&self, _editor: &mut SlideEditor);
    fn view_kept(&self, _editor: &SlideEditor) -> Option<Box<dyn std::any::Any>>;
    fn view_put_back(&self, _editor: &mut SlideEditor, _kept: &dyn std::any::Any, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn autorecover_minutes(&self, _editor: &SlideEditor) -> Option<u32>;
    fn set_autorecover_minutes(&self, _editor: &mut SlideEditor, _minutes: u32, _cx: &mut Context<SlideEditor>);
    fn used_fonts(&self, _editor: &SlideEditor) -> Vec<String>;
    fn replace_fonts(&self, _editor: &mut SlideEditor, _target: &str, _replacement: &str, _cx: &mut Context<SlideEditor>) -> usize;
    fn page_when_current_gone(&self, _editor: &SlideEditor) -> Option<SlideId>;
    fn history_applied_early(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn history_applied(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    fn modifiers_changed(&self, _editor: &mut SlideEditor, _event: &ModifiersChangedEvent, _cx: &mut Context<SlideEditor>);
    fn workspace_wheel(&self, _editor: &mut SlideEditor, _event: &ScrollWheelEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn shape_press_early(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _cx: &mut Context<SlideEditor>) -> bool;
    fn shape_press(&self, _editor: &mut SlideEditor, _shape: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn show_start(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn show_end(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn show_running(&self, _editor: &SlideEditor) -> bool;
    fn show_step(&self, _editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn show_to_end(&self, _editor: &mut SlideEditor, _last: bool, _cx: &mut Context<SlideEditor>);
    fn show_element(&self, _editor: &SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<AnyElement>;
    fn show_in_main(&self, _editor: &SlideEditor) -> bool;
    fn open_link_panel_for_page(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn panel_opening(&self, _editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel);
    fn panel_closed(&self, _editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel);
    fn edit_blocked_by_file(&self, _editor: &SlideEditor) -> bool;
    fn add_page_blocked(&self, _editor: &SlideEditor) -> bool;
    fn document_loaded(&self, _editor: &mut SlideEditor);
    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn file_opened(&self, _editor: &mut SlideEditor);
    fn save_refused(&self, _editor: &SlideEditor) -> Option<String>;
    fn model_for_save<'a>(&self, _editor: &'a SlideEditor) -> std::borrow::Cow<'a, Presentation>;
    #[cfg(not(target_family = "wasm"))]
    fn backup_before_write(&self, _editor: &SlideEditor, _path: &std::path::Path);
    fn selected_pages(&self, _editor: &SlideEditor) -> Vec<SlideId>;
    fn clear_page_selection(&self, _editor: &mut SlideEditor);
    fn page_list_row(&self, _editor: &SlideEditor, _index: usize) -> usize;
    fn text_edit_starting(&self, _editor: &mut SlideEditor);
    fn text_key_early(&self, _editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn toggle_text_characters(&self, _editor: &mut SlideEditor, _toggle: crate::text_keys::CharacterToggle, _cx: &mut Context<SlideEditor>) -> bool;
    fn own_text_copy(&self, _editor: &mut SlideEditor, _cut: bool, _cx: &mut Context<SlideEditor>) -> Option<bool>;
    fn own_text_paste(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<bool>;
    fn link_under_pointer(&self, _editor: &SlideEditor, _position: Point<Pixels>) -> bool;
    fn link_pressed(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn selection_context(&self, _editor: &SlideEditor, _state: &mut crate::menu_context::ContextState);
    fn crop_target(&self, _editor: &SlideEditor) -> Option<(crate::crop_mode::CropTarget, EmuRect, ibis_doc_model::Crop)>;
    fn crop_keep(&self, _editor: &mut SlideEditor, _target: &crate::crop_mode::CropTarget, _full: EmuRect, _crop: ibis_doc_model::Crop, _cx: &mut Context<SlideEditor>);
    fn crop_pointer(&self, _editor: &SlideEditor, _target: &crate::crop_mode::CropTarget, _position: Point<Pixels>) -> Option<(f64, f64)>;
    fn open_sidebar(&self, _editor: &mut SlideEditor, _id: &'static str, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn draw_command(&self, _editor: &SlideEditor, _command: crate::draw_menu::DrawCommand) -> Option<crate::chrome::Handler>;
    fn place_pasted_shapes(&self, _editor: &mut SlideEditor, _shapes: &mut [ibis_doc_model::Shape], _stamp: u64);
    fn paste_shapes_in_text(&self, _editor: &mut SlideEditor, _shapes: Vec<ibis_doc_model::Shape>, _cx: &mut Context<SlideEditor>) -> bool;
    fn body_popup_lines(&self, _editor: &SlideEditor) -> Vec<crate::chrome::MenuEntry>;
    fn panel_menu_extras(&self, _editor: &SlideEditor, _panel: crate::shape_state::ShapePanel) -> Vec<crate::chrome::MenuEntry>;
    fn own_edit_open(&self, _editor: &SlideEditor) -> bool;
    fn has_pending_edit(&self, _editor: &SlideEditor) -> bool;
    fn enter_pending_edit(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    fn relayout_if_changed(&self, _editor: &mut SlideEditor);
    fn formula_edit_key(&self, _editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn set_font_size(&self, _editor: &mut SlideEditor, _pt: f32, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn searchable_menu_entries(&self, _editor: &SlideEditor, _id: &str) -> Option<Vec<crate::chrome::MenuEntry>>;
    fn sidebar_sync(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn ruler_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel>;
    fn dialog_background_panel(&self, _editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel>;
    fn dialog_parent_panel(&self, _editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel>;
    fn extra_dialogs(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Vec<ExtraDialog>;
    fn sidebar_blocked_by_dialog(&self, _editor: &SlideEditor) -> bool;
    fn title_mode_button(&self, _editor: &SlideEditor, _weak: &WeakEntity<SlideEditor>, _icon_only: bool) -> Option<AnyElement>;
    fn theme_panel<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::theme_panel::ThemePanel>;
    fn theme_panel_open(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn theme_panel_apply(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn chart_data_window<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::chart_data::DataWindow>;
    fn chart_data_slot<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut Option<crate::chart_data::DataWindow>>;
    fn read_chart_data(&self, _editor: &SlideEditor, _source: &ibis_pptx::Source, _slide: SlideId, _shape: ibis_doc_model::ShapeId) -> Result<ibis_pptx::ChartData, ibis_pptx::Error>;
    fn apply_chart_data(&self, _editor: &mut SlideEditor, _slide: SlideId, _shape: ibis_doc_model::ShapeId, _book: &Presentation, _data: ibis_pptx::ChartData, _cx: &mut Context<SlideEditor>);
    fn chart_panel_apply(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn palette_theme_colors(&self, _editor: &SlideEditor) -> [ibis_doc_model::Rgb; 10];
    fn theme_palettes(&self) -> Vec<(&'static str, &'static str, Vec<(&'static str, ibis_doc_model::Rgb)>)>;
    fn theme_palette_apply(&self, _editor: &mut SlideEditor, _id: &str, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn new_file_shown(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    fn open_print_panel(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool;
    fn prepare_print_or_export(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(&self, editor: &SlideEditor, _printing: bool) -> Result<PdfJob, String>;
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(&self, _editor: &SlideEditor, _path: &std::path::Path, _source: &ibis_pptx::Source, _copy: bool) -> Option<Result<FileBytes, String>>;
    #[cfg(not(target_family = "wasm"))]
    fn new_file_save_folder(&self, _editor: &SlideEditor) -> Option<std::path::PathBuf>;
    fn export_args(&self, _editor: &SlideEditor) -> Vec<&'static str>;
    fn chrome_hidden(&self, _editor: &SlideEditor) -> bool;
    fn box_edit_active(&self, _editor: &SlideEditor) -> bool;
    fn box_text_edit<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::text_edit::TextEditing>;
    fn box_text_edit_mut<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut crate::text_edit::TextEditing>;
    fn box_edit_body<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a ibis_doc_model::TextBody>;
    fn box_edit_commit(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>);
    fn box_edit_splice(&self, _editor: &mut SlideEditor, _start: usize, _remove: usize, _insert: &str, _typing: bool, _cx: &mut Context<SlideEditor>) -> Option<bool>;
    fn box_edit_changed(&self, _editor: &mut SlideEditor);
    fn box_edit_positions(&self, _editor: &SlideEditor) -> Option<crate::render::rich_text::TextPositions>;
    fn box_edit_goal_x(&self, _editor: &SlideEditor) -> Option<(usize, f32)>;
    fn box_edit_set_goal_x(&self, _editor: &mut SlideEditor, _goal: (usize, f32));
    fn note_open_failed(&self, _path: &std::path::Path);
    fn note_opened(&self, _path: &std::path::Path);
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios(&self) -> Vec<(&'static str, fn())>;
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn ignored_scenarios(&self) -> Vec<(&'static str, fn())>;
    fn menus(&self, _editor: &SlideEditor, _menus: &mut Menus);
    fn format_paint(&self, _editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)>;
    fn add_comment(&self, _editor: &SlideEditor) -> Option<crate::chrome::Handler>;
    fn show_comments(&self, _editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)>;
    fn title_buttons(&self, _editor: &SlideEditor) -> Vec<TitleButton>;
    fn toolbar_left(&self, _editor: &SlideEditor) -> Vec<ToolItem>;
    fn toolbar_right(&self, _editor: &SlideEditor) -> Vec<ToolItem>;
    fn menu_items(&self, _editor: &SlideEditor, _part: crate::chrome::MenuPart) -> Vec<crate::chrome::MenuEntry>;
    fn prepare_model(&self, _presentation: &mut Presentation, _resources: &crate::render::shape_view::Resources, _text_system: &TextSystem, _file_name: Option<&str>, _reloaded: bool);
    fn menu_buttons(&self, _editor: &SlideEditor) -> Vec<ToolItem>;
    fn sidebar(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel>;
    fn sidebar_tabs(&self) -> Vec<crate::chrome::SidebarTab>;
    fn sidebar_on_request(&self) -> bool;
    fn sidebar_toggled(&self, _editor: &mut SlideEditor);
    fn ink_panel_open(&self, _editor: &SlideEditor) -> bool;
    fn close_ink_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>);
    fn commands_ran(&self, _editor: &mut SlideEditor, _structural: bool, _cx: &mut Context<SlideEditor>);
    fn delete_own_selection(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> bool;
}

/// A format's [`FormatEditor`] as a [`FormatDriver`]: each method calls the format's own.
pub struct Driver<F>(std::marker::PhantomData<fn() -> F>);

impl<F> Driver<F> {
    pub const fn new() -> Self {
        Driver(std::marker::PhantomData)
    }
}

impl<F: FormatEditor> FormatDriver for Driver<F> {
    fn info(&self) -> &'static FormatInfo {
        F::INFO
    }
    fn page_noun(&self) -> &'static str {
        F::PAGE_NOUN
    }
    fn fit_tooltip(&self) -> &'static str {
        F::FIT_TOOLTIP
    }
    fn slides(&self) -> bool {
        F::SLIDES
    }
    fn shapes(&self) -> bool {
        F::SHAPES
    }
    fn can_fit(&self) -> bool {
        F::CAN_FIT
    }
    fn new_state(&self, presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Box<dyn FormatState> {
        F::new_state(presentation, window, cx)
    }
    fn new_prefs(&self) -> Box<dyn std::any::Any> {
        F::new_prefs()
    }
    fn resources_renewed(&self, _editor: &mut SlideEditor) {
        F::resources_renewed(_editor)
    }
    fn initial_zoom(&self, _editor: &SlideEditor) -> Option<f32> {
        F::initial_zoom(_editor)
    }
    fn workspace_content(&self, editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        F::workspace_content(editor, workspace)
    }
    fn zoom_scroll_y(&self, _editor: &SlideEditor, _before: &WorkspaceContent, _after: &WorkspaceContent, _old: Pixels, _view: Pixels, _anchor: Pixels) -> Option<Pixels> {
        F::zoom_scroll_y(_editor, _before, _after, _old, _view, _anchor)
    }
    fn step_page(&self, _editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::step_page(_editor, _forward, _window, _cx)
    }
    fn show_page(&self, _editor: &mut SlideEditor, _slide: SlideId, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::show_page(_editor, _slide, _window, _cx)
    }
    fn page_changed(&self, _editor: &mut SlideEditor, _from: SlideId, _to: SlideId) {
        F::page_changed(_editor, _from, _to)
    }
    fn before_render(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::before_render(_editor, _window, _cx)
    }
    fn render_body(&self, editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        F::render_body(editor, window, cx)
    }
    fn status_items(&self, _editor: &SlideEditor) -> Vec<SharedString> {
        F::status_items(_editor)
    }
    fn status_items_at_right(&self) -> bool {
        F::status_items_at_right()
    }
    fn status_hint(&self, _editor: &SlideEditor) -> Option<SharedString> {
        F::status_hint(_editor)
    }
    fn status_tab_scroll(&self, _editor: &SlideEditor) -> Option<ScrollHandle> {
        F::status_tab_scroll(_editor)
    }
    fn wants_text_input(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::wants_text_input(_editor, _window, _cx)
    }
    fn text_for_range(&self, _editor: &mut SlideEditor, _range: Range<usize>, _adjusted: &mut Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<String> {
        F::text_for_range(_editor, _range, _adjusted, _window, _cx)
    }
    fn selected_text_range(&self, _editor: &mut SlideEditor, _ignore_disabled_input: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<UTF16Selection> {
        F::selected_text_range(_editor, _ignore_disabled_input, _window, _cx)
    }
    fn marked_text_range(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Range<usize>> {
        F::marked_text_range(_editor, _window, _cx)
    }
    fn unmark_text(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::unmark_text(_editor, _window, _cx)
    }
    fn replace_text_in_range(&self, _editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::replace_text_in_range(_editor, _range, _text, _window, _cx)
    }
    fn replace_and_mark_text_in_range(&self, _editor: &mut SlideEditor, _range: Option<Range<usize>>, _text: &str, _selected: Option<Range<usize>>, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::replace_and_mark_text_in_range(_editor, _range, _text, _selected, _window, _cx)
    }
    fn set_ime_clauses(&self, _editor: &mut SlideEditor, _clauses: Vec<ImeClause>, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::set_ime_clauses(_editor, _clauses, _window, _cx)
    }
    fn bounds_for_range(&self, _editor: &mut SlideEditor, _range: Range<usize>, _element: Bounds<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<Bounds<Pixels>> {
        F::bounds_for_range(_editor, _range, _element, _window, _cx)
    }
    fn text_vertical(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::text_vertical(_editor, _window, _cx)
    }
    fn character_index_for_point(&self, _editor: &mut SlideEditor, _point: Point<Pixels>, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<usize> {
        F::character_index_for_point(_editor, _point, _window, _cx)
    }
    fn insert_symbol(&self, _editor: &mut SlideEditor, _c: char, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::insert_symbol(_editor, _c, _window, _cx)
    }
    fn range_pick_active(&self, _editor: &SlideEditor) -> bool {
        F::range_pick_active(_editor)
    }
    fn finish_range_pick(&self, _editor: &mut SlideEditor, _accept: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::finish_range_pick(_editor, _accept, _window, _cx)
    }
    fn range_pick_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        F::range_pick_panel(_editor, _cx)
    }
    fn close_dialog(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::close_dialog(_editor, _window, _cx)
    }
    fn discard_dialog(&self, _editor: &mut SlideEditor, _key: &'static str) {
        F::discard_dialog(_editor, _key)
    }
    fn keeps_sidebar_after_dialog(&self, _editor: &SlideEditor) -> bool {
        F::keeps_sidebar_after_dialog(_editor)
    }
    fn autocorrect_dialog_parent_open(&self, _editor: &SlideEditor) -> bool {
        F::autocorrect_dialog_parent_open(_editor)
    }
    fn autocorrect_panel_mut<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut crate::autocorrect::AutoCorrectPanel> {
        F::autocorrect_panel_mut(_editor)
    }
    fn dialog_arrow_step(&self, _editor: &mut SlideEditor, _step: i32, _cx: &mut Context<SlideEditor>) -> bool {
        F::dialog_arrow_step(_editor, _step, _cx)
    }
    fn dialog_field_owns_enter(&self, _editor: &SlideEditor, _window: &Window, _cx: &App) -> bool {
        F::dialog_field_owns_enter(_editor, _window, _cx)
    }
    fn panel_message_shown(&self, _editor: &mut SlideEditor) {
        F::panel_message_shown(_editor)
    }
    fn panel_message_dismissed(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::panel_message_dismissed(_editor, _cx)
    }
    fn panel_message_origin(&self, _editor: &SlideEditor) -> Option<Point<Pixels>> {
        F::panel_message_origin(_editor)
    }
    fn panel_message_anchored(&self, _editor: &SlideEditor) -> bool {
        F::panel_message_anchored(_editor)
    }
    fn panel_message_measured(&self, _editor: &mut SlideEditor, _bounds: Bounds<Pixels>, _cx: &mut Context<SlideEditor>) {
        F::panel_message_measured(_editor, _bounds, _cx)
    }
    fn infobar_kind(&self, _editor: &SlideEditor) -> Option<crate::infobar::InfoBarKind> {
        F::infobar_kind(_editor)
    }
    fn infobar_action(&self, _editor: &mut SlideEditor, _kind: crate::infobar::InfoBarKind, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::infobar_action(_editor, _kind, _window, _cx)
    }
    fn infobar_spec(&self, _kind: crate::infobar::InfoBarKind) -> Option<crate::infobar::InfoBarSpec> {
        F::infobar_spec(_kind)
    }
    fn infobar_title_suffix(&self, _editor: &SlideEditor, _kind: crate::infobar::InfoBarKind) -> Option<String> {
        F::infobar_title_suffix(_editor, _kind)
    }
    fn workspace_page_layout_shown(&self, _editor: &SlideEditor) -> bool {
        F::workspace_page_layout_shown(_editor)
    }
    fn workspace_page_layout(&self, _editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        F::workspace_page_layout(_editor, _content, _cx)
    }
    fn workspace_main_pane_lead(&self, _editor: &SlideEditor) -> Point<Pixels> {
        F::workspace_main_pane_lead(_editor)
    }
    fn workspace_cell_editor(&self, _editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        F::workspace_cell_editor(_editor, _content, _cx)
    }
    fn workspace_frozen_panes(&self, _editor: &SlideEditor, _layout: crate::editor::SlideLayout, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Vec<AnyElement> {
        F::workspace_frozen_panes(_editor, _layout, _content, _cx)
    }
    fn workspace_headers(&self, _editor: &mut SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        F::workspace_headers(_editor, _content, _cx)
    }
    fn workspace_split_scrollbars(&self, _editor: &SlideEditor, _content: WorkspaceContent, _cx: &mut Context<SlideEditor>) -> Option<Vec<AnyElement>> {
        F::workspace_split_scrollbars(_editor, _content, _cx)
    }
    fn workspace_page_break_preview_on(&self, _editor: &SlideEditor) -> bool {
        F::workspace_page_break_preview_on(_editor)
    }
    fn workspace_sheet_frame(&self, _editor: &SlideEditor) -> Option<EmuRect> {
        F::workspace_sheet_frame(_editor)
    }
    fn workspace_hide_ink(&self, _editor: &SlideEditor) -> bool {
        F::workspace_hide_ink(_editor)
    }
    fn workspace_sheet_overlays(&self, _editor: &SlideEditor, _scale: f32, _cx: &mut Context<SlideEditor>) -> SheetOverlays {
        F::workspace_sheet_overlays(_editor, _scale, _cx)
    }
    fn workspace_shape_design_mode(&self, _editor: &SlideEditor) -> bool {
        F::workspace_shape_design_mode(_editor)
    }
    fn workspace_shape_has_right_click(&self, _editor: &SlideEditor) -> bool {
        F::workspace_shape_has_right_click(_editor)
    }
    fn workspace_shape_right_down(&self, _editor: &mut SlideEditor, _id: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::workspace_shape_right_down(_editor, _id, _event, _window, _cx)
    }
    fn workspace_grid_down(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::workspace_grid_down(_editor, _event, _window, _cx)
    }
    fn workspace_key_context(&self) -> &'static str {
        F::workspace_key_context()
    }
    fn workspace_page_shadow(&self) -> bool {
        F::workspace_page_shadow()
    }
    fn register_actions(&self, _root: Stateful<Div>, _cx: &mut Context<SlideEditor>) -> Stateful<Div> {
        F::register_actions(_root, _cx)
    }
    fn cancel_tool_early(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::cancel_tool_early(_editor, _window, _cx)
    }
    fn cancel_tool_late(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::cancel_tool_late(_editor, _window, _cx)
    }
    fn pointer_move_early(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::pointer_move_early(_editor, _event, _window, _cx)
    }
    fn pointer_move(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::pointer_move(_editor, _event, _window, _cx)
    }
    fn pointer_up_early(&self, _editor: &mut SlideEditor) {
        F::pointer_up_early(_editor)
    }
    fn pointer_up(&self, _editor: &mut SlideEditor, _event: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::pointer_up(_editor, _event, _window, _cx)
    }
    fn range_pick_move(&self, _editor: &mut SlideEditor, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::range_pick_move(_editor, _event, _window, _cx)
    }
    fn range_pick_down(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::range_pick_down(_editor, _event, _window, _cx)
    }
    fn range_pick_up(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::range_pick_up(_editor, _window, _cx)
    }
    fn range_pick_key(&self, _editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::range_pick_key(_editor, _keystroke, _window, _cx)
    }
    fn range_pick_suspend_drag(&self, _editor: &mut SlideEditor, _window: &mut Window) {
        F::range_pick_suspend_drag(_editor, _window)
    }
    fn root_event_surface(&self, _editor: &SlideEditor, _cx: &Context<SlideEditor>) -> Option<AnyElement> {
        F::root_event_surface(_editor, _cx)
    }
    fn format_key(&self, _editor: &mut SlideEditor, _keystroke: &Keystroke, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::format_key(_editor, _keystroke, _window, _cx)
    }
    fn crop_fields_synced(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::crop_fields_synced(_editor, _window, _cx)
    }
    fn zoom_set(&self, _editor: &mut SlideEditor, _zoom: Option<f32>) {
        F::zoom_set(_editor, _zoom)
    }
    fn view_from_model(&self, _editor: &mut SlideEditor) {
        F::view_from_model(_editor)
    }
    fn model_from_peer(&self, _editor: &mut SlideEditor) {
        F::model_from_peer(_editor)
    }
    fn view_kept(&self, _editor: &SlideEditor) -> Option<Box<dyn std::any::Any>> {
        F::view_kept(_editor)
    }
    fn view_put_back(&self, _editor: &mut SlideEditor, _kept: &dyn std::any::Any, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::view_put_back(_editor, _kept, _window, _cx)
    }
    fn autorecover_minutes(&self, _editor: &SlideEditor) -> Option<u32> {
        F::autorecover_minutes(_editor)
    }
    fn set_autorecover_minutes(&self, _editor: &mut SlideEditor, _minutes: u32, _cx: &mut Context<SlideEditor>) {
        F::set_autorecover_minutes(_editor, _minutes, _cx)
    }
    fn used_fonts(&self, _editor: &SlideEditor) -> Vec<String> {
        F::used_fonts(_editor)
    }
    fn replace_fonts(&self, _editor: &mut SlideEditor, _target: &str, _replacement: &str, _cx: &mut Context<SlideEditor>) -> usize {
        F::replace_fonts(_editor, _target, _replacement, _cx)
    }
    fn page_when_current_gone(&self, _editor: &SlideEditor) -> Option<SlideId> {
        F::page_when_current_gone(_editor)
    }
    fn history_applied_early(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::history_applied_early(_editor, _window, _cx)
    }
    fn history_applied(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::history_applied(_editor, _cx)
    }
    fn modifiers_changed(&self, _editor: &mut SlideEditor, _event: &ModifiersChangedEvent, _cx: &mut Context<SlideEditor>) {
        F::modifiers_changed(_editor, _event, _cx)
    }
    fn workspace_wheel(&self, _editor: &mut SlideEditor, _event: &ScrollWheelEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::workspace_wheel(_editor, _event, _window, _cx)
    }
    fn shape_press_early(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _cx: &mut Context<SlideEditor>) -> bool {
        F::shape_press_early(_editor, _event, _cx)
    }
    fn shape_press(&self, _editor: &mut SlideEditor, _shape: ibis_doc_model::ShapeId, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::shape_press(_editor, _shape, _event, _window, _cx)
    }
    fn show_start(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::show_start(_editor, _window, _cx)
    }
    fn show_end(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::show_end(_editor, _window, _cx)
    }
    fn show_running(&self, _editor: &SlideEditor) -> bool {
        F::show_running(_editor)
    }
    fn show_step(&self, _editor: &mut SlideEditor, _forward: bool, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::show_step(_editor, _forward, _window, _cx)
    }
    fn show_to_end(&self, _editor: &mut SlideEditor, _last: bool, _cx: &mut Context<SlideEditor>) {
        F::show_to_end(_editor, _last, _cx)
    }
    fn show_element(&self, _editor: &SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<AnyElement> {
        F::show_element(_editor, _window, _cx)
    }
    fn show_in_main(&self, _editor: &SlideEditor) -> bool {
        F::show_in_main(_editor)
    }
    fn open_link_panel_for_page(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::open_link_panel_for_page(_editor, _window, _cx)
    }
    fn panel_opening(&self, _editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel) {
        F::panel_opening(_editor, _panel)
    }
    fn panel_closed(&self, _editor: &mut SlideEditor, _panel: crate::shape_state::ShapePanel) {
        F::panel_closed(_editor, _panel)
    }
    fn edit_blocked_by_file(&self, _editor: &SlideEditor) -> bool {
        F::edit_blocked_by_file(_editor)
    }
    fn add_page_blocked(&self, _editor: &SlideEditor) -> bool {
        F::add_page_blocked(_editor)
    }
    fn document_loaded(&self, _editor: &mut SlideEditor) {
        F::document_loaded(_editor)
    }
    fn file_opened(&self, _editor: &mut SlideEditor) {
        F::file_opened(_editor)
    }
    fn save_refused(&self, _editor: &SlideEditor) -> Option<String> {
        F::save_refused(_editor)
    }
    fn model_for_save<'a>(&self, _editor: &'a SlideEditor) -> std::borrow::Cow<'a, Presentation> {
        F::model_for_save(_editor)
    }
    #[cfg(not(target_family = "wasm"))]
    fn backup_before_write(&self, _editor: &SlideEditor, _path: &std::path::Path) {
        F::backup_before_write(_editor, _path)
    }
    fn selected_pages(&self, _editor: &SlideEditor) -> Vec<SlideId> {
        F::selected_pages(_editor)
    }
    fn clear_page_selection(&self, _editor: &mut SlideEditor) {
        F::clear_page_selection(_editor)
    }
    fn page_list_row(&self, _editor: &SlideEditor, _index: usize) -> usize {
        F::page_list_row(_editor, _index)
    }
    fn text_edit_starting(&self, _editor: &mut SlideEditor) {
        F::text_edit_starting(_editor)
    }
    fn text_key_early(&self, _editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::text_key_early(_editor, _event, _window, _cx)
    }
    fn toggle_text_characters(&self, _editor: &mut SlideEditor, _toggle: crate::text_keys::CharacterToggle, _cx: &mut Context<SlideEditor>) -> bool {
        F::toggle_text_characters(_editor, _toggle, _cx)
    }
    fn own_text_copy(&self, _editor: &mut SlideEditor, _cut: bool, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        F::own_text_copy(_editor, _cut, _cx)
    }
    fn own_text_paste(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        F::own_text_paste(_editor, _cx)
    }
    fn link_under_pointer(&self, _editor: &SlideEditor, _position: Point<Pixels>) -> bool {
        F::link_under_pointer(_editor, _position)
    }
    fn link_pressed(&self, _editor: &mut SlideEditor, _event: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::link_pressed(_editor, _event, _window, _cx)
    }
    fn selection_context(&self, _editor: &SlideEditor, _state: &mut crate::menu_context::ContextState) {
        F::selection_context(_editor, _state)
    }
    fn crop_target(&self, _editor: &SlideEditor) -> Option<(crate::crop_mode::CropTarget, EmuRect, ibis_doc_model::Crop)> {
        F::crop_target(_editor)
    }
    fn crop_keep(&self, _editor: &mut SlideEditor, _target: &crate::crop_mode::CropTarget, _full: EmuRect, _crop: ibis_doc_model::Crop, _cx: &mut Context<SlideEditor>) {
        F::crop_keep(_editor, _target, _full, _crop, _cx)
    }
    fn crop_pointer(&self, _editor: &SlideEditor, _target: &crate::crop_mode::CropTarget, _position: Point<Pixels>) -> Option<(f64, f64)> {
        F::crop_pointer(_editor, _target, _position)
    }
    fn open_sidebar(&self, _editor: &mut SlideEditor, _id: &'static str, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::open_sidebar(_editor, _id, _window, _cx)
    }
    fn draw_command(&self, _editor: &SlideEditor, _command: crate::draw_menu::DrawCommand) -> Option<crate::chrome::Handler> {
        F::draw_command(_editor, _command)
    }
    fn place_pasted_shapes(&self, _editor: &mut SlideEditor, _shapes: &mut [ibis_doc_model::Shape], _stamp: u64) {
        F::place_pasted_shapes(_editor, _shapes, _stamp)
    }
    fn paste_shapes_in_text(&self, _editor: &mut SlideEditor, _shapes: Vec<ibis_doc_model::Shape>, _cx: &mut Context<SlideEditor>) -> bool {
        F::paste_shapes_in_text(_editor, _shapes, _cx)
    }
    fn body_popup_lines(&self, _editor: &SlideEditor) -> Vec<crate::chrome::MenuEntry> {
        F::body_popup_lines(_editor)
    }
    fn panel_menu_extras(&self, _editor: &SlideEditor, _panel: crate::shape_state::ShapePanel) -> Vec<crate::chrome::MenuEntry> {
        F::panel_menu_extras(_editor, _panel)
    }
    fn own_edit_open(&self, _editor: &SlideEditor) -> bool {
        F::own_edit_open(_editor)
    }
    fn has_pending_edit(&self, _editor: &SlideEditor) -> bool {
        F::has_pending_edit(_editor)
    }
    fn enter_pending_edit(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::enter_pending_edit(_editor, _cx)
    }
    fn relayout_if_changed(&self, _editor: &mut SlideEditor) {
        F::relayout_if_changed(_editor)
    }
    fn formula_edit_key(&self, _editor: &mut SlideEditor, _event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::formula_edit_key(_editor, _event, _window, _cx)
    }
    fn set_font_size(&self, _editor: &mut SlideEditor, _pt: f32, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::set_font_size(_editor, _pt, _window, _cx)
    }
    fn searchable_menu_entries(&self, _editor: &SlideEditor, _id: &str) -> Option<Vec<crate::chrome::MenuEntry>> {
        F::searchable_menu_entries(_editor, _id)
    }
    fn sidebar_sync(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::sidebar_sync(_editor, _window, _cx)
    }
    fn ruler_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        F::ruler_panel(_editor, _cx)
    }
    fn dialog_background_panel(&self, _editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        F::dialog_background_panel(_editor, _key, _cx)
    }
    fn dialog_parent_panel(&self, _editor: &mut SlideEditor, _key: Option<&'static str>, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        F::dialog_parent_panel(_editor, _key, _cx)
    }
    fn extra_dialogs(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> Vec<ExtraDialog> {
        F::extra_dialogs(_editor, _cx)
    }
    fn sidebar_blocked_by_dialog(&self, _editor: &SlideEditor) -> bool {
        F::sidebar_blocked_by_dialog(_editor)
    }
    fn title_mode_button(&self, _editor: &SlideEditor, _weak: &WeakEntity<SlideEditor>, _icon_only: bool) -> Option<AnyElement> {
        F::title_mode_button(_editor, _weak, _icon_only)
    }
    fn theme_panel<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::theme_panel::ThemePanel> {
        F::theme_panel(_editor)
    }
    fn theme_panel_open(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::theme_panel_open(_editor, _window, _cx)
    }
    fn theme_panel_apply(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::theme_panel_apply(_editor, _window, _cx)
    }
    fn chart_data_window<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::chart_data::DataWindow> {
        F::chart_data_window(_editor)
    }
    fn chart_data_slot<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut Option<crate::chart_data::DataWindow>> {
        F::chart_data_slot(_editor)
    }
    fn read_chart_data(&self, _editor: &SlideEditor, _source: &ibis_pptx::Source, _slide: SlideId, _shape: ibis_doc_model::ShapeId) -> Result<ibis_pptx::ChartData, ibis_pptx::Error> {
        F::read_chart_data(_editor, _source, _slide, _shape)
    }
    fn apply_chart_data(&self, _editor: &mut SlideEditor, _slide: SlideId, _shape: ibis_doc_model::ShapeId, _book: &Presentation, _data: ibis_pptx::ChartData, _cx: &mut Context<SlideEditor>) {
        F::apply_chart_data(_editor, _slide, _shape, _book, _data, _cx)
    }
    fn chart_panel_apply(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::chart_panel_apply(_editor, _window, _cx)
    }
    fn palette_theme_colors(&self, _editor: &SlideEditor) -> [ibis_doc_model::Rgb; 10] {
        F::palette_theme_colors(_editor)
    }
    fn theme_palettes(&self) -> Vec<(&'static str, &'static str, Vec<(&'static str, ibis_doc_model::Rgb)>)> {
        F::theme_palettes()
    }
    fn theme_palette_apply(&self, _editor: &mut SlideEditor, _id: &str, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::theme_palette_apply(_editor, _id, _window, _cx)
    }
    fn new_file_shown(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::new_file_shown(_editor, _cx)
    }
    fn open_print_panel(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> bool {
        F::open_print_panel(_editor, _window, _cx)
    }
    fn prepare_print_or_export(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::prepare_print_or_export(_editor, _cx)
    }
    #[cfg(not(target_family = "wasm"))]
    fn pdf_job(&self, editor: &SlideEditor, _printing: bool) -> Result<PdfJob, String> {
        F::pdf_job(editor, _printing)
    }
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(&self, _editor: &SlideEditor, _path: &std::path::Path, _source: &ibis_pptx::Source, _copy: bool) -> Option<Result<FileBytes, String>> {
        F::bytes_for_path(_editor, _path, _source, _copy)
    }
    #[cfg(not(target_family = "wasm"))]
    fn new_file_save_folder(&self, _editor: &SlideEditor) -> Option<std::path::PathBuf> {
        F::new_file_save_folder(_editor)
    }
    fn export_args(&self, _editor: &SlideEditor) -> Vec<&'static str> {
        F::export_args(_editor)
    }
    fn chrome_hidden(&self, _editor: &SlideEditor) -> bool {
        F::chrome_hidden(_editor)
    }
    fn box_edit_active(&self, _editor: &SlideEditor) -> bool {
        F::box_edit_active(_editor)
    }
    fn box_text_edit<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a crate::text_edit::TextEditing> {
        F::box_text_edit(_editor)
    }
    fn box_text_edit_mut<'a>(&self, _editor: &'a mut SlideEditor) -> Option<&'a mut crate::text_edit::TextEditing> {
        F::box_text_edit_mut(_editor)
    }
    fn box_edit_body<'a>(&self, _editor: &'a SlideEditor) -> Option<&'a ibis_doc_model::TextBody> {
        F::box_edit_body(_editor)
    }
    fn box_edit_commit(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        F::box_edit_commit(_editor, _window, _cx)
    }
    fn box_edit_splice(&self, _editor: &mut SlideEditor, _start: usize, _remove: usize, _insert: &str, _typing: bool, _cx: &mut Context<SlideEditor>) -> Option<bool> {
        F::box_edit_splice(_editor, _start, _remove, _insert, _typing, _cx)
    }
    fn box_edit_changed(&self, _editor: &mut SlideEditor) {
        F::box_edit_changed(_editor)
    }
    fn box_edit_positions(&self, _editor: &SlideEditor) -> Option<crate::render::rich_text::TextPositions> {
        F::box_edit_positions(_editor)
    }
    fn box_edit_goal_x(&self, _editor: &SlideEditor) -> Option<(usize, f32)> {
        F::box_edit_goal_x(_editor)
    }
    fn box_edit_set_goal_x(&self, _editor: &mut SlideEditor, _goal: (usize, f32)) {
        F::box_edit_set_goal_x(_editor, _goal)
    }
    fn note_open_failed(&self, _path: &std::path::Path) {
        F::note_open_failed(_path)
    }
    fn note_opened(&self, _path: &std::path::Path) {
        F::note_opened(_path)
    }
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios(&self) -> Vec<(&'static str, fn())> {
        F::scenarios()
    }
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn ignored_scenarios(&self) -> Vec<(&'static str, fn())> {
        F::ignored_scenarios()
    }
    fn menus(&self, _editor: &SlideEditor, _menus: &mut Menus) {
        F::menus(_editor, _menus)
    }
    fn format_paint(&self, _editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)> {
        F::format_paint(_editor)
    }
    fn add_comment(&self, _editor: &SlideEditor) -> Option<crate::chrome::Handler> {
        F::add_comment(_editor)
    }
    fn show_comments(&self, _editor: &SlideEditor) -> Option<(bool, crate::chrome::Handler)> {
        F::show_comments(_editor)
    }
    fn title_buttons(&self, _editor: &SlideEditor) -> Vec<TitleButton> {
        F::title_buttons(_editor)
    }
    fn toolbar_left(&self, _editor: &SlideEditor) -> Vec<ToolItem> {
        F::toolbar_left(_editor)
    }
    fn toolbar_right(&self, _editor: &SlideEditor) -> Vec<ToolItem> {
        F::toolbar_right(_editor)
    }
    fn menu_items(&self, _editor: &SlideEditor, _part: crate::chrome::MenuPart) -> Vec<crate::chrome::MenuEntry> {
        F::menu_items(_editor, _part)
    }
    fn prepare_model(&self, _presentation: &mut Presentation, _resources: &crate::render::shape_view::Resources, _text_system: &TextSystem, _file_name: Option<&str>, _reloaded: bool) {
        F::prepare_model(_presentation, _resources, _text_system, _file_name, _reloaded)
    }
    fn menu_buttons(&self, _editor: &SlideEditor) -> Vec<ToolItem> {
        F::menu_buttons(_editor)
    }
    fn sidebar(&self, _editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        F::sidebar(_editor, _window, _cx)
    }
    fn sidebar_tabs(&self) -> Vec<crate::chrome::SidebarTab> {
        F::sidebar_tabs()
    }
    fn sidebar_on_request(&self) -> bool {
        F::sidebar_on_request()
    }
    fn sidebar_toggled(&self, _editor: &mut SlideEditor) {
        F::sidebar_toggled(_editor)
    }
    fn ink_panel_open(&self, _editor: &SlideEditor) -> bool {
        F::ink_panel_open(_editor)
    }
    fn close_ink_panel(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) {
        F::close_ink_panel(_editor, _cx)
    }
    fn commands_ran(&self, _editor: &mut SlideEditor, _structural: bool, _cx: &mut Context<SlideEditor>) {
        F::commands_ran(_editor, _structural, _cx)
    }
    fn delete_own_selection(&self, _editor: &mut SlideEditor, _cx: &mut Context<SlideEditor>) -> bool {
        F::delete_own_selection(_editor, _cx)
    }
}

/// The drivers of every format.
pub fn drivers() -> &'static [(Kind, &'static dyn FormatDriver)] {
    crate::app_hooks::drivers()
}

/// The driver of the format of `kind`.
pub fn driver(kind: Kind) -> &'static dyn FormatDriver {
    drivers().iter().find(|(k, _)| *k == kind).map(|(_, d)| *d).unwrap_or_else(|| panic!("no driver for {kind:?}"))
}

/// The names, keys and colours of the format of `kind`.
pub(crate) fn info(kind: Kind) -> &'static FormatInfo {
    driver(kind).info()
}

/// Every format with its names, keys and colours, in the order lists of the formats show them (`FormatInfo::order`).
pub(crate) fn infos() -> Vec<(Kind, &'static FormatInfo)> {
    let mut all: Vec<(Kind, &'static FormatInfo)> = drivers().iter().map(|(kind, driver)| (*kind, driver.info())).collect();
    all.sort_by_key(|(_, info)| info.order);
    all
}

impl SlideEditor {
    /// The kind of the open document.
    pub fn kind(&self) -> Kind {
        self.format.kind()
    }

    /// The names, keys and colours of the open document's format.
    pub(crate) fn format_info(&self) -> &'static FormatInfo {
        with_format!(self, info())
    }

    /// The shell's shape state is this screen's own (`ShapeRules::in_shell`).
    pub(crate) fn uses_shape_edit(&self) -> bool {
        self.format_info().shapes.in_shell
    }

    /// Several shapes can be selected at once (`ShapeRules::several`).
    pub(crate) fn selects_several_shapes(&self) -> bool {
        self.format_info().shapes.several
    }

    /// The screen's format has the CMYK mode (`ShapeRules::cmyk`).
    pub(crate) fn has_cmyk(&self) -> bool {
        self.format_info().shapes.cmyk
    }

    pub fn is_workbook(&self) -> bool {
        self.kind() == Kind::Workbook
    }

    pub fn is_presentation(&self) -> bool {
        self.kind() == Kind::Presentation
    }

    pub fn is_document(&self) -> bool {
        self.kind() == Kind::Document
    }

    /// The open document's pages are slides (see `FormatEditor::SLIDES`).
    pub fn pages_are_slides(&self) -> bool {
        with_format!(self, slides())
    }

    /// A slide show is on (see `FormatEditor::show_running`).
    pub fn show_running(&self) -> bool {
        with_format!(self, show_running(self))
    }

    /// Shapes may be put on the pages (see `FormatEditor::SHAPES`).
    pub(crate) fn allows_shapes(&self) -> bool {
        with_format!(self, shapes())
    }

    /// The open document's format state as the format's own type, when it is that format's document.
    pub fn format_state<T: FormatState>(&self) -> Option<&T> {
        self.format.as_any().downcast_ref::<T>()
    }

    pub fn format_state_mut<T: FormatState>(&mut self) -> Option<&mut T> {
        self.format.as_any_mut().downcast_mut::<T>()
    }

    /// What a format keeps for this window over the files loaded into it (`FormatEditor::new_prefs`); `T` is the
    /// format's own type. It is there whatever kind of file is open.
    pub fn format_prefs<T: 'static>(&self) -> &T {
        self.format_prefs.iter().find_map(|prefs| prefs.downcast_ref::<T>()).expect("every format's settings of the window are made with its editor")
    }

    pub fn format_prefs_mut<T: 'static>(&mut self) -> &mut T {
        self.format_prefs.iter_mut().find_map(|prefs| prefs.downcast_mut::<T>()).expect("every format's settings of the window are made with its editor")
    }
}
