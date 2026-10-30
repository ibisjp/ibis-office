// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "scenarios"), allow(dead_code))]
//! ibisOffice presentation editor front end.
//!
//! The same code runs as a native window (`src/main.rs`) and in the browser as
//! wasm (`run`, exported through wasm-bindgen).


// The editor window and what all formats share (shell/), drawing the
// model (render/), and each format's own editing and views: presentations
// (pptx/), workbooks (xlsx/) and documents (docx/). A format's code stays in
// its directory; see shell/format.rs for how the shell hands over to it.
pub(crate) mod clock;
pub(crate) use clock::debug_timing;
mod docx;
mod linux_desktop;
mod media_player;
mod native_shape_clipboard;
#[cfg(windows)]
mod open_with;
mod pptx;
mod render;
mod spelling;
mod grammar;
mod shell;
mod xlsx;
#[cfg(headless_export)]
mod export;
#[cfg(headless_export)]
mod export_print;
#[cfg(all(target_os = "macos", feature = "export"))]
mod export_text;
#[cfg(headless_export)]
mod pdf_vector;
#[cfg(headless_export)]
mod ui_snapshot;

pub use shell::editor::SlideEditor;

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/shell/native_quit.rs"]
mod native_quit_scenarios;

/// Run the scenario tests (`tests/scenarios.rs`; see `shell/scenario.rs`).
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
pub fn run_scenarios(filter: &[String]) -> i32 {
    shell::scenario::run(filter)
}
use shell::editor;
use pptx::slideshow;
use xlsx::sheet_ui;

use gpui_kit::component::Root;
use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::*;

actions!(
    ibis_office,
    [
        AddPresentationSlide,
        DuplicatePresentationSelection,
        /// Undo the last edit.
        Undo,
        /// Redo the last undone edit.
        Redo,
        /// Delete the selected shape.
        DeleteSelection,
        /// Leave text editing, the insert tool, or the selection, in that order.
        CancelTool,
        /// Open a PowerPoint, Excel or Word file.
        OpenFile,
        /// Make a new, empty Word document (ファイル > 新規作成 > ドキュメント).
        NewDocument,
        /// Open the start window (ファイル > スタートウィンドウ).
        OpenStartWindow,
        /// Make a new presentation from a template (ファイル > 新規 > テンプレートから新規作成…).
        NewFromTemplate,
        /// Close the open file and show the start screen (スタート画面).
        CloseFile,
        /// Save the open file (to the file it was opened from).
        Save,
        /// Save the open file under another name.
        SaveAs,
        /// Print the open file (ファイル > 印刷…, ⌘P).
        PrintFile,
        /// Hide or show the title row and the menu bar (the toolbar's ^).
        ToggleMenus,
        /// Show the next slide.
        NextSlide,
        /// Show the previous slide.
        PreviousSlide,
        /// Show the first slide.
        FirstSlide,
        /// Show the last slide.
        LastSlide,
        /// Start the slide show from the slide shown.
        StartSlideshow,
        /// Start the slide show from the first slide.
        StartSlideshowFromBeginning,
        /// Start the slide show from the slide shown with the presenter's
        /// view (発表者ビュー).
        StartPresenterView,
        InsertComment,
        /// 挿入 > リンク (a presentation's hyperlink).
        InsertHyperlink,
        /// Black out the slide show, or show it again.
        BlackScreen,
        /// White out the slide show, or show it again.
        WhiteScreen,
        /// Quit the application.
        Quit,
        /// Open the settings window (ibisOffice > 設定…, ⌘,).
        OpenSettings,
        /// Open the About window (ibisOffice > ibisOffice について).
        OpenAbout,
        /// Select the cell above the selected one on a worksheet.
        SelectCellUp,
        /// Select the cell below the selected one on a worksheet.
        SelectCellDown,
        /// Select the cell to the left of the selected one on a worksheet.
        SelectCellLeft,
        /// Select the cell to the right of the selected one on a worksheet.
        SelectCellRight,
        /// Show the next worksheet of a workbook.
        NextSheet,
        /// Show the previous worksheet of a workbook.
        PreviousSheet,
        /// Stretch the selected range of a worksheet a cell up.
        ExtendCellsUp,
        /// Stretch the selected range of a worksheet a cell down.
        ExtendCellsDown,
        /// Stretch the selected range of a worksheet a cell to the left.
        ExtendCellsLeft,
        /// Stretch the selected range of a worksheet a cell to the right.
        ExtendCellsRight,
        /// Scroll a document up by a few lines.
        ScrollLineUp,
        /// Scroll a document down by a few lines.
        ScrollLineDown,
        /// Scroll to the start of a document.
        ScrollToStart,
        /// Scroll to the end of a document.
        ScrollToEnd,
        /// Switch to laser pointer mode in the slide show.
        LaserPointer,
        /// Switch to pen drawing mode in the slide show.
        PenPointer,
        /// Switch to highlighter drawing mode in the slide show.
        HighlighterPointer,
        /// Switch back to arrow pointer mode in the slide show.
        ArrowPointer,
        /// Switch to eraser mode in the slide show.
        EraserPointer,
        /// Clear all ink annotations on the current slide.
        ClearSlideInks,
        /// Toggle the all slides grid overview in the slide show.
        ToggleSlideshowGrid,
        /// Zoom in the slide show view.
        ZoomInSlideshow,
        /// Zoom out the slide show view.
        ZoomOutSlideshow,
        /// Reset zoom in the slide show view to normal 1.0x.
        ResetSlideshowZoom,
        /// Start rehearsing slide show timings.
        RehearseTimings,
        /// 表示 > 拡大・縮小・100%・ウィンドウに合わせる（⌘+・⌘-・⌘0・⌥⌘0）.
        ZoomInView,
        ZoomOutView,
        ZoomActualView,
        ZoomFitView,
        /// Toggle the slide sorter grid view in the presentation editor.
        ToggleSlideGridView,
        /// Start or exit reading view in the presentation editor.
        ToggleReadingView,
        /// Switch to normal view in the presentation editor.
        SwitchToNormalView,
        /// Toggle the outline view in the presentation editor.
        ToggleOutlineView,
        /// Switch to outline view in the presentation editor.
        SwitchToOutlineView,
        /// Toggle the notes page view in the presentation editor.
        ToggleNotesPageView,
        /// Switch to notes page view in the presentation editor.
        SwitchToNotesPageView,
        /// Open the zoom dialog.
        OpenZoomDialog,
        /// Open a new window duplicating current presentation or empty.
        NewEditorWindow,
        /// Arrange all editor windows evenly.
        ArrangeAllWindows,
        /// Cascade all editor windows.
        CascadeWindows,
        /// Copy animations of selected shape (Animation Painter).
        CopyAnimation,
        /// Paste animations onto selected shapes (Animation Painter).
        PasteAnimation,
    ]
);

/// Register components, theme and key bindings. Call once, before opening a window.
pub fn init(cx: &mut App) {
    shell::settings::migrate_on_startup();
    gpui_kit::init(cx);
    shell::panel_dialog::init(cx);
    // Questions drawn as ibisOffice's dialogs where the system has none (Linux).
    linux_desktop::install_prompts(cx);
    // The metafile pictures put their text by the metrics of the fonts.
    render::metafile_fonts::install(cx);
    // The chrome follows the system's appearance (light or dark); what is in the document (the pages, the cells,
    // the slides) stays as it is, white paper as in Office. Scrollbars stay hidden until the view scrolls or the
    // pointer is over one (not the system's setting, which may show them always), unless the setting
    // スクロール バーを常に表示 is on.
    shell::palette::follow(cx.window_appearance(), None, cx);
    shell::palette::apply_theme(None, cx);

    let context = Some(editor::CONTEXT);
    let secondary = |key: &str| format!("{}-{key}", secondary_modifier());
    cx.bind_keys([
        KeyBinding::new(&secondary("z"), Undo, context),
        KeyBinding::new(&secondary("y"), Redo, context),
        KeyBinding::new(&secondary("shift-z"), Redo, context),
        KeyBinding::new("delete", DeleteSelection, context),
        KeyBinding::new("backspace", DeleteSelection, context),
        KeyBinding::new("escape", CancelTool, context),
        // 表示の拡大・縮小（メニューの表示どおり。⌘= は ⌘+ のシフトなし）.
        KeyBinding::new(&secondary("+"), ZoomInView, context),
        KeyBinding::new(&secondary("="), ZoomInView, context),
        KeyBinding::new(&secondary("-"), ZoomOutView, context),
        KeyBinding::new(&secondary("0"), ZoomActualView, context),
        KeyBinding::new(&secondary("alt-0"), ZoomFitView, context),
        KeyBinding::new("ctrl-alt-1", ToggleSlideGridView, context),
        KeyBinding::new("ctrl-alt-2", ToggleOutlineView, context),
        KeyBinding::new("ctrl-alt-3", ToggleNotesPageView, context),
        KeyBinding::new(&secondary("shift-n"), NewEditorWindow, context),
        KeyBinding::new(&secondary("alt-shift-c"), CopyAnimation, context),
        KeyBinding::new(&secondary("alt-shift-v"), PasteAnimation, context),
        KeyBinding::new("pagedown", NextSlide, context),
        KeyBinding::new("pageup", PreviousSlide, context),
        KeyBinding::new("down", NextSlide, Some(editor::WORKSPACE_CONTEXT)),
        KeyBinding::new("up", PreviousSlide, Some(editor::WORKSPACE_CONTEXT)),
        KeyBinding::new("down", NextSlide, Some(editor::LIST_CONTEXT)),
        KeyBinding::new("up", PreviousSlide, Some(editor::LIST_CONTEXT)),
        KeyBinding::new("up", SelectCellUp, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("down", SelectCellDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("left", SelectCellLeft, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("right", SelectCellRight, Some(editor::SHEET_CONTEXT)),
        // As in Excel: Control+PageDown / PageUp (and ⌥→ / ⌥← on a Mac)
        // change sheets; PageDown / PageUp alone scroll a screen.
        KeyBinding::new("ctrl-pagedown", NextSheet, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-pageup", PreviousSheet, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("alt-right", NextSheet, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("alt-left", PreviousSheet, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-up", ExtendCellsUp, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-down", ExtendCellsDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-left", ExtendCellsLeft, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-right", ExtendCellsRight, Some(editor::SHEET_CONTEXT)),
        // As in Excel: Enter moves down and Tab right, with Shift back.
        KeyBinding::new("enter", SelectCellDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-enter", SelectCellUp, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("tab", SelectCellRight, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-tab", SelectCellLeft, Some(editor::SHEET_CONTEXT)),
        // As in Excel: ジャンプ with Control+G or F5 (on a Mac too); to the
        // edge of the data region with Control+arrow (⌘+arrow on a Mac),
        // column A with Home, A1 with Control+Home and the last cell in use
        // with Control+End (Fn+← / Fn+→ on a Mac keyboard give Home / End);
        // Shift stretches the range instead.
        KeyBinding::new("ctrl-g", sheet_ui::GoTo, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("f5", sheet_ui::GoTo, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("up"), sheet_ui::JumpCellUp, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("down"), sheet_ui::JumpCellDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("left"), sheet_ui::JumpCellLeft, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("right"), sheet_ui::JumpCellRight, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("shift-up"), sheet_ui::ExtendJumpUp, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("shift-down"), sheet_ui::ExtendJumpDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("shift-left"), sheet_ui::ExtendJumpLeft, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new(&secondary("shift-right"), sheet_ui::ExtendJumpRight, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("home", sheet_ui::SelectRowStart, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("end", sheet_ui::SelectRowEnd, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-end", sheet_ui::ExtendRowEnd, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-home", sheet_ui::ExtendRowStart, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-home", sheet_ui::SelectFirstCell, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-home", sheet_ui::ExtendFirstCell, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-end", sheet_ui::SelectLastCell, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-end", sheet_ui::ExtendLastCell, Some(editor::SHEET_CONTEXT)),
        // Calculation: F9 recalculates all sheets, Shift+F9 the active sheet.
        KeyBinding::new("f9", sheet_ui::CalculateNow, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-f9", sheet_ui::CalculateSheet, Some(editor::SHEET_CONTEXT)),
        // A document scrolls, as in Word; PageUp / PageDown (above) go by pages.
        KeyBinding::new("up", ScrollLineUp, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new("down", ScrollLineDown, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new("home", ScrollToStart, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new("end", ScrollToEnd, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new(&secondary("home"), ScrollToStart, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new(&secondary("end"), ScrollToEnd, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new(&secondary("up"), ScrollToStart, Some(editor::DOCUMENT_CONTEXT)),
        KeyBinding::new(&secondary("down"), ScrollToEnd, Some(editor::DOCUMENT_CONTEXT)),
        // The slide show, as in PowerPoint (Esc ends it, as `CancelTool`).
        KeyBinding::new("right", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("down", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("space", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("enter", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("left", PreviousSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("up", PreviousSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("backspace", PreviousSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("n", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("p", PreviousSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("pagedown", NextSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("pageup", PreviousSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("home", FirstSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("end", LastSlide, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("b", BlackScreen, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new(".", BlackScreen, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("w", WhiteScreen, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new(",", WhiteScreen, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-l", LaserPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-l", LaserPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-p", PenPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-p", PenPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-i", HighlighterPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-i", HighlighterPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-a", ArrowPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-a", ArrowPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-e", EraserPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-e", EraserPointer, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("e", ClearSlideInks, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-s", ToggleSlideshowGrid, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-s", ToggleSlideshowGrid, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("g", ToggleSlideshowGrid, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("+", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("=", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-+", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-+", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-=", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-=", ZoomInSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("-", ZoomOutSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl--", ZoomOutSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd--", ZoomOutSlideshow, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("0", ResetSlideshowZoom, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("ctrl-0", ResetSlideshowZoom, Some(slideshow::SLIDESHOW_CONTEXT)),
        KeyBinding::new("cmd-0", ResetSlideshowZoom, Some(slideshow::SLIDESHOW_CONTEXT)),
        // Slides, as in PowerPoint: Home and End go to the first and last.
        KeyBinding::new("tab", pptx::slide_view::NextTableCell, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("shift-tab", pptx::slide_view::PreviousTableCell, Some(pptx::slide_view::CONTEXT)),
        // フォント サイズ（PowerPoint: ⌘⇧> で大きく、⌘⇧< で小さく、⌘] で大きく、⌘[ で小さく）.
        KeyBinding::new("cmd-shift->", pptx::slide_view::IncreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-shift-.", pptx::slide_view::IncreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-shift-<", pptx::slide_view::DecreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-shift-,", pptx::slide_view::DecreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new(&secondary("]"), pptx::slide_view::IncreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new(&secondary("["), pptx::slide_view::DecreaseFontSize, Some(pptx::slide_view::CONTEXT)),
        // 配置（PowerPoint: ⌘L 左・⌘E 中央・⌘R 右・⌘J 両端）、書式のコピー・貼り付け（⌥⌘C・⌥⌘V）.
        KeyBinding::new("cmd-l", pptx::slide_view::AlignTextLeft, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-e", pptx::slide_view::AlignTextCenter, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-r", pptx::slide_view::AlignTextRight, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("cmd-j", pptx::slide_view::AlignTextJustify, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("alt-cmd-c", pptx::slide_view::CopyTextFormat, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("alt-cmd-v", pptx::slide_view::PasteTextFormat, Some(pptx::slide_view::CONTEXT)),
        KeyBinding::new("home", FirstSlide, Some(editor::WORKSPACE_CONTEXT)),
        KeyBinding::new("end", LastSlide, Some(editor::WORKSPACE_CONTEXT)),
        KeyBinding::new("home", FirstSlide, Some(editor::LIST_CONTEXT)),
        KeyBinding::new("end", LastSlide, Some(editor::LIST_CONTEXT)),
        // Starting the slide show: PowerPoint for Mac's Command-Shift-Return
        // (from the beginning) and Command-Return (from the slide shown);
        // F5 and Shift-F5 elsewhere, as PowerPoint for Windows.
        KeyBinding::new(if is_mac() { "cmd-shift-enter" } else { "f5" }, StartSlideshowFromBeginning, context),
        KeyBinding::new(if is_mac() { "alt-cmd-enter" } else { "alt-f5" }, StartPresenterView, context),
        // 挿入 > コメント: PowerPoint and Word for Mac's Option-Command-M
        // (Ctrl+Alt+M on Windows).
        KeyBinding::new(if is_mac() { "alt-cmd-m" } else { "ctrl-alt-m" }, InsertComment, context),
        // 挿入 > リンク: ⌘K (Ctrl+K), as PowerPoint.
        KeyBinding::new(&secondary("k"), InsertHyperlink, context),
        KeyBinding::new(if is_mac() { "cmd-enter" } else { "shift-f5" }, StartSlideshow, context),
        KeyBinding::new(&secondary("o"), OpenFile, None),
        KeyBinding::new(&secondary("n"), NewDocument, None),
        KeyBinding::new(&secondary("s"), Save, None),
        KeyBinding::new(&secondary("shift-s"), SaveAs, None),
        KeyBinding::new(&secondary("p"), PrintFile, None),
        KeyBinding::new("ctrl-shift-f", ToggleMenus, context),
        // As in Office: Command-W (Control-W elsewhere) closes the file.
        KeyBinding::new(&secondary("w"), CloseFile, None),
        KeyBinding::new(&secondary("q"), Quit, None),
        KeyBinding::new(&secondary(","), OpenSettings, None),
    ]);
    if is_mac(){cx.bind_keys([
        KeyBinding::new("ctrl-shift-up",sheet_ui::ExtendJumpUp,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-down",sheet_ui::ExtendJumpDown,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-left",sheet_ui::ExtendJumpLeft,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-right",sheet_ui::ExtendJumpRight,Some(editor::SHEET_CONTEXT)),
    ]);}
    // A document's text keys (the document担当's, in `docx/editing.rs`).
    cx.bind_keys(docx::key_bindings());
    cx.bind_keys(xlsx::key_bindings());
    cx.on_action(|_: &Quit, cx| quit_asking_to_save(cx));
    // The Dock's Quit bypasses the app menu's Quit action. macOS asks before stopping its
    // event loop; other platform layers currently call on_quit only after their loop has ended.
    #[cfg(any(target_os = "macos", feature = "scenarios"))]
    cx.on_quit_requested(native_quit_allowed).detach();
    cx.on_action(|_: &OpenSettings, cx| crate::shell::settings_window::open(cx));
    cx.on_action(|_: &OpenAbout, cx| crate::shell::about_window::open(cx));
    cx.on_action(|_: &OpenStartWindow, cx| crate::open_editor(None, None, cx));
    // File > New with no window open (a window handles it itself otherwise): the start window.
    cx.on_action(|_: &NewDocument, cx| crate::open_editor(None, None, cx));
    set_app_menus(cx);
}

/// The last window has closed (⌘W / ファイル > 閉じる on the start window, or the red button): the app ends with it,
/// on a Mac too. The scenarios record the request in `QUIT_REQUESTED` instead of ending.
pub(crate) fn quit_if_last_window_closed(cx: &mut App) {
    if !cx.windows().is_empty() {
        return;
    }
    #[cfg(feature = "scenarios")]
    QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(feature = "scenarios"))]
    cx.quit();
}

#[cfg(feature = "scenarios")]
pub(crate) static QUIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A native quit request must not destroy windows before their save questions are answered.
/// Once all documents have been saved/discarded, the next request is allowed synchronously.
pub(crate) fn native_quit_allowed(cx: &mut App) -> bool {
    let mut unsaved = false;
    for handle in cx.windows() {
        let Some(root) = handle.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok());
        match editor {
            Ok(Some(editor)) => unsaved |= editor.read(cx).is_dirty(),
            Ok(None) => {},
            // A borrowed window must never be mistaken for a clean window.
            Err(_) => unsaved = true,
        }
    }
    if unsaved {
        // The Dock can ask while the app is inactive; bring its save question into view.
        cx.activate(true);
        quit_asking_to_save(cx);
        false
    } else {
        true
    }
}

/// ibisOffice > 終了 (⌘Q): the windows with edits not saved stay and ask whether to save them (each goes once
/// answered); the others close. The app ends with its last window (`quit_if_last_window_closed`), or at once when
/// nothing is left to ask (GitHub issue #27).
pub(crate) fn quit_asking_to_save(cx: &mut App) {
    // ⌘Q comes while the window with the keys is being updated, when its editor cannot be read (the window would be
    // passed over and the app end without asking): ask once the event is done.
    cx.defer(quit_asking_to_save_now);
}

fn quit_asking_to_save_now(cx: &mut App) {
    let mut asking = false;
    for handle in cx.windows() {
        let Some(root) = handle.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        let Some(editor) = editor else { continue };
        // A window that may close goes now; one that may not has opened the question.
        let close = editor.update(cx, |editor, cx| editor.window_should_close(cx));
        if close {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        } else {
            asking = true;
        }
    }
    if !asking {
        #[cfg(feature = "scenarios")]
        QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
        #[cfg(not(feature = "scenarios"))]
        cx.quit();
    }
}

/// The app's menu bar (macOS's), in the UI's language: set again when the
/// language changes.
pub(crate) fn set_app_menus(cx: &mut App) {
    use crate::shell::i18n::text;
    cx.set_menus([
        Menu::new("ibisOffice").items([MenuItem::action(text("help.about"), OpenAbout), MenuItem::separator(), MenuItem::action(text("app.settings"), OpenSettings), MenuItem::separator(), MenuItem::action(text("app.quit"), Quit)]),
        Menu::new(text("menu.file")).items([
            MenuItem::action(text("file.start-window"), OpenStartWindow),
            MenuItem::separator(),
            MenuItem::action(text("app.file.new"), NewDocument),
            MenuItem::action(text("app.file.open"), OpenFile),
            MenuItem::action(text("app.file.save"), Save),
            MenuItem::action(text("app.file.save-as"), SaveAs),
            MenuItem::action(text("app.file.print"), PrintFile),
            MenuItem::action(text("app.file.close"), CloseFile),
        ]),
    ]);
}

/// The modifier of shortcuts such as undo: Command on a Mac, Control elsewhere.
/// GPUI's `secondary` picks it by the build target, so in the browser (not
/// macOS) it is always Control; ask the browser for its platform instead.
fn secondary_modifier() -> &'static str {
    #[cfg(target_family = "wasm")]
    return if web::is_mac() { "cmd" } else { "ctrl" };
    #[cfg(not(target_family = "wasm"))]
    "secondary"
}

/// Whether the keys are a Mac's (the build target, or the browser's platform).
fn is_mac() -> bool {
    #[cfg(target_family = "wasm")]
    return web::is_mac();
    #[cfg(not(target_family = "wasm"))]
    cfg!(target_os = "macos")
}

/// Open the editor window on `document` (a presentation and its file name),
/// showing slide number `slide` (1-based), or on the start view without one.
#[derive(Default)]
struct StartWindows(std::collections::HashMap<gpui_kit::EntityId,(gpui_kit::AnyWindowHandle,bool)>);
impl gpui_kit::Global for StartWindows {}
/// Keep start-view identity outside an editor borrow, so its own command can reuse it.
pub(crate) fn track_start_window(id:gpui_kit::EntityId,handle:Option<gpui_kit::AnyWindowHandle>,start:bool,cx:&mut App){
    if cx.try_global::<StartWindows>().is_none(){cx.set_global(StartWindows::default());}
    cx.update_global::<StartWindows,_>(|windows,_|{if let Some(handle)=handle{windows.0.insert(id,(handle,start));}else if let Some((_,state))=windows.0.get_mut(&id){*state=start;}});
}
pub fn open_editor(document: Option<(ibis_doc_model::Presentation, SharedString)>, slide: Option<usize>, cx: &mut App) {
    if document.is_none(){
        let alive=cx.windows();
        if cx.try_global::<StartWindows>().is_some(){cx.update_global::<StartWindows,_>(|s,_|s.0.retain(|_,(w,_)|alive.contains(w)));}
        let existing=cx.try_global::<StartWindows>().and_then(|s|s.0.values().find(|(w,start)|*start&&alive.contains(w)).map(|(w,_)|*w));
        if let Some(handle)=existing{let _=handle.update(cx,|_,w,_|w.activate_window());cx.activate(true);return;}
    }
    // A start window is as wide as its view (shell/start_view.rs).
    let width = if document.is_none() { shell::start_view::START_WINDOW_WIDTH } else { 1280. };
    open_window_sized(width, cx, move |editor, window, cx| {
        if let Some((presentation, name)) = document {
            editor.load(presentation, name, window, cx);
        }
        if let Some(number) = slide {
            editor.show_slide_number(number, window, cx);
        }
    });
}

/// A new editor window, its editor set up by `setup`.
pub(crate) fn open_window_with(cx: &mut App, setup: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) {
    open_window_sized(1280., cx, setup);
}

/// A new editor window `width` pixels wide, its editor set up by `setup`.
fn open_window_sized(width: f32, cx: &mut App, setup: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) {
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(800.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    linux_desktop::identify(&mut options);
    cx.open_window(options, |window, cx| {
        shell::palette::watch(window, cx);
        let view = cx.new(|cx| {
            let mut editor = SlideEditor::new(ibis_doc_model::Presentation::new(), window, cx);
            setup(&mut editor, window, cx);
            editor
        });
        let focus = view.read(cx).focus_handle.clone();
        focus.focus(window, cx);
        // The window's close button asks whether to save edits not saved (GitHub issue #27).
        let editor = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| editor.update(cx, |editor, cx| editor.window_should_close(cx)).unwrap_or(true));
        // Root hosts gpui-component's overlays (tooltips, popovers, dialogs).
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("failed to open the editor window");
    cx.activate(true);
}

/// Command line:
///
/// ```text
/// ibisOffice [--slide <number>] [<file.pptx|file.xlsx|file.docx>]
/// ibisOffice --export png|pdf|pdf-image [--out-dir <dir>] [--width <px>] [--slides 1,3-5] [--layout] <file.pptx|file.xlsx|file.docx>
/// ibisOffice --export ui [--out-dir <dir>] [--steps start,pagedown,end] [<file.pptx|file.xlsx|file.docx>]
/// ibisOffice [--timing] [--quit-after-first-frame] [<file>]
/// ```
///
/// `--timing` prints to stderr, as `ibisOffice timing: <stage> <ms>`, the
/// milliseconds since the process started (as the system recorded it) at
/// each stage: `main`, `app` (the application running), `window` (the
/// window made, with an empty document of the file's kind), `first-frame`
/// (the window drawn once), `second-frame`, `read` (the file read, on
/// another thread), `parsed`, `loaded` (the document in the window, laid
/// out), `document-frame` (the document drawn), `document-second-frame`,
/// `slide-pictures` (the pictures of the slide shown decoded) and
/// `all-pictures` (every picture decoded, the slide list drawn).
/// `--quit-after-first-frame` quits after `document-second-frame`, or
/// `second-frame` without a file (pictures decode in the background and
/// may still be missing then), for timing starts one after another.
///
/// `--export ui` draws the whole editor window headless after each step
/// (see `ui_snapshot.rs`), for checking the window without a screen.
#[cfg(not(target_family = "wasm"))]
#[derive(Default)]
struct Args {
    path: Option<std::path::PathBuf>,
    slide: Option<usize>,
    export: Option<String>,
    out_dir: Option<std::path::PathBuf>,
    width: Option<u32>,
    slides: Option<String>,
    layout: bool,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    print_hidden_text: bool,
    /// 印刷オプション: 変更履歴とコメントを印刷する (DOCX-SEC-014).
    no_print_markup: bool,
    /// 印刷の設定 (PPTX-RECORD-017): `gray` or `bw`.
    print_color: Option<String>,
    /// 配布資料: slides to a page (PPTX-RECORD-016).
    handout: Option<u8>,
    /// スライドに枠を付けて印刷 (PPTX-RECORD-018).
    print_frame: bool,
    /// 配布資料: a landscape page, and what it carries (header, date, footer, page numbers).
    handout_landscape: bool,
    notes_pages: bool,
    outline_pages: bool,
    handout_header: Option<String>,
    handout_date: Option<String>,
    handout_footer: Option<String>,
    handout_number: bool,
    steps: Option<String>,
    /// `--timing`: print when each stage of starting was reached.
    timing: bool,
    /// `--quit-after-first-frame`: quit once the window has been drawn.
    quit_after_first_frame: bool,
    /// `--read-only`: open document in read-only viewing mode.
    read_only: bool,
}

#[cfg(not(target_family = "wasm"))]
fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = std::env::args_os().skip(1);
    while let Some(arg) = iter.next() {
        let arg = arg.to_string_lossy().into_owned();
        let mut value = || iter.next().map(|v| v.to_string_lossy().into_owned()).ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--read-only" => args.read_only = true,
            "--slide" => args.slide = Some(value()?.parse().map_err(|_| "--slide needs a slide number".to_string())?),
            "--export" => args.export = Some(value()?),
            "--out-dir" => args.out_dir = Some(value()?.into()),
            "--width" => args.width = Some(value()?.parse().map_err(|_| "--width needs a width in pixels".to_string())?),
            "--slides" => args.slides = Some(value()?),
            "--layout" => args.layout = true,
            "--print-backgrounds" => args.print_backgrounds = true,
            "--print-hidden-text" => args.print_hidden_text = true,
            "--no-print-markup" => args.no_print_markup = true,
            "--print-color" => args.print_color = Some(value()?),
            "--handout" => args.handout = Some(value()?.parse().ok().filter(|n| matches!(n, 1 | 2 | 3 | 4 | 6 | 8 | 9 | 16)).ok_or_else(|| "--handout needs one of 1, 2, 3, 4, 6, 8, 9 or 16".to_string())?),
            "--print-frame" => args.print_frame = true,
            "--handout-landscape" => args.handout_landscape = true,
            "--notes-pages" => args.notes_pages = true,
            "--outline-pages" => args.outline_pages = true,
            "--handout-header" => args.handout_header = Some(value()?),
            "--handout-date" => args.handout_date = Some(value()?),
            "--handout-footer" => args.handout_footer = Some(value()?),
            "--handout-number" => args.handout_number = true,
            "--steps" => args.steps = Some(value()?),
            "--timing" => args.timing = true,
            "--quit-after-first-frame" => args.quit_after_first_frame = true,
            other if other.starts_with("--") => return Err(format!("unknown option: {other}")),
            _ => args.path = Some(arg.into()),
        }
    }
    Ok(args)
}

/// `--export`: write the slides of the file and quit, without a window.
#[cfg(not(target_family = "wasm"))]
fn run_export(args: &Args, format: &str) -> Result<(), String> {
    #[cfg(headless_export)]
    {
        let format = match format {
            "ui" => {
                // Without a file, the start view.
                let out_dir = args.out_dir.clone().unwrap_or_else(|| ".".into());
                std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
                let document = match args.path.as_deref() {
                    Some(path) => Some(read_document(path).ok_or_else(|| format!("cannot open {}", path.display()))?),
                    None => None,
                };
                let stem = args
                    .path
                    .as_deref()
                    .and_then(|path| path.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "start".into());
                return ui_snapshot::run(document, &out_dir, &stem, args.steps.as_deref().unwrap_or("start"));
            }
            "png" => export::Format::Png,
            "pdf" => export::Format::Pdf,
            "pdf-image" => export::Format::PdfImage,
            other => return Err(format!("--export takes png, pdf, pdf-image or ui: {other}")),
        };
        let path = args.path.as_deref().ok_or("name the .pptx, .xlsx or .docx file to export")?;
        let out_dir = args.out_dir.clone().unwrap_or_else(|| ".".into());
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "slides".into());
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let presentation = ibis_pptx::read(&data).map_err(|e| format!("{}: {e}", path.display()))?;
        let slides = args.slides.as_deref().map(export::parse_slides).transpose()?.unwrap_or_default();
        let file_name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let options = export::Options {
            format,
            out_dir,
            name,
            width: args.width.unwrap_or(1920),
            slides,
            layout: args.layout,
            file_name,
            print_backgrounds: args.print_backgrounds,
            print_hidden_text: args.print_hidden_text,
            print_markup: !args.no_print_markup,
            color: match args.print_color.as_deref() {
                None | Some("color") => export_print::PrintColor::Color,
                Some("gray") => export_print::PrintColor::Gray,
                Some("bw") => export_print::PrintColor::BlackWhite,
                Some(other) => return Err(format!("--print-color takes color, gray or bw: {other}")),
            },
            handout: args.handout,
            frame: args.print_frame,
            handout_landscape: args.handout_landscape,
            notes_pages: args.notes_pages,
            outline_pages: args.outline_pages,
            notes_source: args.notes_pages.then(|| data.clone()),
            handout_marks: export_print::HandoutMarks { header: args.handout_header.clone(), date: args.handout_date.clone(), footer: args.handout_footer.clone(), number: args.handout_number, layout: ibis_pptx::handout_master::layout(&data).ok() },
        };
        export::export(presentation, &options)
    }
    #[cfg(not(headless_export))]
    {
        let _ = (args, format);
        Err("this executable cannot export (build it on macOS or Windows with --features export)".into())
    }
}

/// A file read for editing: its model, and what saving it needs.
#[cfg(not(target_family = "wasm"))]
pub(crate) struct Loaded {
    pub(crate) presentation: ibis_doc_model::Presentation,
    pub(crate) source: std::sync::Arc<ibis_pptx::Source>,
    /// The password a protected file was opened with: saving protects it again.
    pub(crate) password: Option<String>,
    pub(crate) csv_stream: Option<std::sync::mpsc::Receiver<ibis_pptx::CsvProgressiveChunk>>,
}

/// What `read_file` says when the file is protected and needs its password.
pub(crate) const NEEDS_PASSWORD: &str = "\u{1}needs-password";
/// What it says when the password given is not the file's.
pub(crate) const WRONG_PASSWORD: &str = "\u{1}wrong-password";

/// The file at `path`, read and parsed. Its pictures and fonts start
/// decoding on other threads (`preload`).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn read_file(path: &std::path::Path) -> Result<Loaded, String> {
    read_file_with(path, None)
}

/// As `read_file`; `password` opens a protected file (パスワードを使用して暗号化).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn read_file_with(path: &std::path::Path, password: Option<&str>) -> Result<Loaded, String> {
    timing::mark_read_start();
    let read_t0 = crate::clock::Instant::now();
    let mut data = std::fs::read(path).map_err(|e| e.to_string())?;
    let read_fs_ms = read_t0.elapsed().as_secs_f64() * 1000.0;
    timing::mark("read");
    let mut used = None;
    if ibis_pptx::is_encrypted(&data) {
        let Some(password) = password else { return Err(NEEDS_PASSWORD.to_string()) };
        data = ibis_pptx::decrypt(&data, password).map_err(|e| if e == ibis_pptx::DecryptError::WrongPassword { WRONG_PASSWORD.to_string() } else { e.to_string() })?;
        used = Some(password.to_string());
    }
    let open_t0 = crate::clock::Instant::now();
    let external_document_format=ibis_pptx::docx_convert::DocxConvertFormat::from_path(path);
    let open_result=if let Some(format)=external_document_format {
        ibis_pptx::docx_convert::import_docx_format(&data,format).map(|opened|(opened,None))
    } else {ibis_pptx::open_progressive(data,100_000)};
    let (opened, csv_stream) = open_result.map_err(|e| {
        xlsx::repair::note_open_failed(path);
        docx::repair::note_open_failed(path);
        e.to_string()
    })?;
    xlsx::repair::note_opened(path);
    docx::repair::note_opened(path);
    let open_ms = open_t0.elapsed().as_secs_f64() * 1000.0;
    timing::mark("parsed");
    let total_file_ms = read_t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("ibisOffice timing: read_file total: {total_file_ms:.1} ms (fs::read: {read_fs_ms:.1} ms, open: {open_ms:.1} ms)");
    render::preload::Preload::start_for_next(&opened.presentation);
    Ok(Loaded { presentation: opened.presentation, source: std::sync::Arc::new(opened.source), password: used, csv_stream })
}

/// The file at `path`, read and parsed, with its name.
#[cfg(headless_export)]
fn read_document(path: &std::path::Path) -> Option<(ibis_doc_model::Presentation, SharedString)> {
    let name: SharedString = path.file_name()?.to_string_lossy().into_owned().into();
    let loaded = read_file(path).map_err(|e| log::error!("{}: {e}", path.display())).ok()?;
    Some((loaded.presentation, name))
}

/// The local path of a `file://` URL.
#[cfg(not(target_family = "wasm"))]
fn url_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // Percent-decoding.
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(std::path::PathBuf::from(String::from_utf8(out).ok()?))
}

/// Open `path` in the app: in the editor window if it shows no document
/// (the start view), else in a new window.
#[cfg(not(target_family = "wasm"))]
fn open_path_in_app(path: std::path::PathBuf, cx: &mut App) {
    for window in cx.windows() {
        let Some(root) = window.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        if let Some(editor) = editor
            && !editor.read(cx).has_document
        {
            window
                .update(cx, |_, window, cx| editor.update(cx, |e, cx| e.open_path(path.clone(), None, window, cx)))
                .ok();
            return;
        }
    }
    open_editor_for(path, None, cx);
}

/// A new editor window that opens `path`, showing an empty document of its
/// kind while it loads, then slide `slide` (1-based) if given.
#[cfg(not(target_family = "wasm"))]
fn open_editor_for(path: std::path::PathBuf, slide: Option<usize>, cx: &mut App) {
    open_window_with(cx, move |editor, window, cx| editor.open_path(path, slide, window, cx));
}

/// `--timing`: stages of starting, in milliseconds since the process
/// started.
#[cfg(not(target_family = "wasm"))]
mod timing {
    use std::sync::OnceLock;
    use std::time::{Duration, SystemTime};

    static START: OnceLock<Option<SystemTime>> = OnceLock::new();

    /// Start timing if `on`, from when the system says the process began
    /// (before the executable was loaded), or failing that from now.
    pub(crate) fn enable(on: bool) {
        START.get_or_init(|| on.then(|| process_start().unwrap_or_else(crate::clock::system_now)));
    }

    static QUIT_AFTER_DOCUMENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static FILE_READ_START: OnceLock<SystemTime> = OnceLock::new();

    /// Mark the start of reading the file.
    pub(crate) fn mark_read_start() {
        FILE_READ_START.get_or_init(crate::clock::system_now);
        mark("read-start");
    }

    /// Quit once the document opened at launch has been drawn twice.
    pub(crate) fn quit_after_document(on: bool) {
        QUIT_AFTER_DOCUMENT.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// The file is loaded into `window`: mark it and the next two frames,
    /// the document drawn, and quit then if asked to.
    pub(crate) fn document_loaded(window: &mut gpui_kit::Window) {
        mark("loaded");
        if let (Some(Some(_)), Some(read_start)) = (START.get(), FILE_READ_START.get()) {
            let ms = crate::clock::system_now().duration_since(*read_start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
            eprintln!("ibisOffice timing: read-start to loaded: {ms:.1} ms ({:.3} s)", ms / 1000.0);
        }
        window.on_next_frame(|window, _| {
            mark("document-frame");
            if let (Some(Some(_)), Some(read_start)) = (START.get(), FILE_READ_START.get()) {
                let ms = crate::clock::system_now().duration_since(*read_start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
                eprintln!("ibisOffice timing: read-start to document-frame (render complete): {ms:.1} ms ({:.3} s)", ms / 1000.0);
            }
            window.on_next_frame(|_, cx| {
                mark("document-second-frame");
                if QUIT_AFTER_DOCUMENT.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    cx.quit();
                }
            });
            window.refresh();
        });
        window.refresh();
    }

    pub(crate) fn mark(stage: &str) {
        if let Some(Some(start)) = START.get() {
            let ms = crate::clock::system_now().duration_since(*start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
            eprintln!("ibisOffice timing: {stage} {ms:.1}");
        }
    }

    /// When this process was started, from `sysctl kern.proc.pid` (macOS).
    #[cfg(target_os = "macos")]
    fn process_start() -> Option<SystemTime> {
        // struct kinfo_proc begins with kp_proc, whose first member, the
        // union p_un, holds __p_starttime (a timeval) at offset 0.
        const CTL_KERN: i32 = 1;
        const KERN_PROC: i32 = 14;
        const KERN_PROC_PID: i32 = 1;
        const KINFO_PROC_SIZE: usize = 648;
        const STARTTIME_OFFSET: usize = 0;
        unsafe extern "C" {
            fn sysctl(name: *const i32, namelen: u32, oldp: *mut u8, oldlenp: *mut usize, newp: *const u8, newlen: usize) -> i32;
        }
        let mib = [CTL_KERN, KERN_PROC, KERN_PROC_PID, std::process::id() as i32];
        let mut buf = [0u8; KINFO_PROC_SIZE];
        let mut len = buf.len();
        // SAFETY: the buffer and its length are valid for the call; the
        // kernel writes at most `len` bytes.
        let ok = unsafe { sysctl(mib.as_ptr(), mib.len() as u32, buf.as_mut_ptr(), &mut len, std::ptr::null(), 0) } == 0;
        if !ok || len < STARTTIME_OFFSET + 16 {
            return None;
        }
        let secs = i64::from_ne_bytes(buf[STARTTIME_OFFSET..STARTTIME_OFFSET + 8].try_into().ok()?);
        let usecs = i32::from_ne_bytes(buf[STARTTIME_OFFSET + 8..STARTTIME_OFFSET + 12].try_into().ok()?);
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64) + Duration::from_micros(usecs.max(0) as u64);
        // Sanity: within the last minute.
        crate::clock::system_now().duration_since(start).ok().filter(|d| *d < Duration::from_secs(60)).map(|_| start)
    }

    #[cfg(not(target_os = "macos"))]
    fn process_start() -> Option<SystemTime> {
        None
    }
}

/// Native entry point; see [`Args`] for the command line.
#[cfg(not(target_family = "wasm"))]
pub fn run_native() {
    // TODAY, NOW and a date typed without its year read the local time.
    ibis_calc::eval::convert::set_clock(clock::calc_clock);
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("ibisOffice: {err}");
            std::process::exit(2);
        }
    };
    if let Some(format) = &args.export {
        if let Err(err) = run_export(&args, format) {
            eprintln!("ibisOffice: {err}");
            std::process::exit(1);
        }
        // Done: the caches of what was drawn (hundreds of thousands of
        // shaped lines for a large workbook) are not freed one by one.
        std::process::exit(0);
    }
    // Explorer's "Open with" list gets ibisOffice for csv and tsv (Windows).
    #[cfg(windows)]
    open_with::register();
    timing::enable(args.timing);
    timing::mark("main");
    // The file named is read and parsed while the application starts.
    let early_read = args.path.clone().map(|path| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let read_path = path.clone();
        std::thread::spawn(move || sender.send(read_file(&read_path)).ok());
        (path, receiver)
    });
    let slide = args.slide;
    let quit_after_first_frame = args.quit_after_first_frame;
    timing::quit_after_document(quit_after_first_frame);
    // Files macOS hands over (a double-click in Finder, `open -a`): opened in
    // the window, or in a new one; queued until the app is running.
    let opener: std::rc::Rc<std::cell::RefCell<(Option<AsyncApp>, Vec<std::path::PathBuf>)>> = Default::default();
    let handed = opener.clone();
    let application = gpui_kit::application().with_assets(crate::shell::icons::Assets::new(""));
    application.on_open_urls(move |urls| {
        let paths: Vec<std::path::PathBuf> = urls.iter().filter_map(|u| url_path(u)).collect();
        let mut state = handed.borrow_mut();
        match state.0.clone() {
            Some(cx) => {
                drop(state);
                cx.update(|cx| paths.into_iter().for_each(|p| open_path_in_app(p, cx)));
            }
            None => state.1.extend(paths),
        }
    });
    application
        .run(move |cx| {
            {
                let mut state = opener.borrow_mut();
                state.0 = Some(cx.to_async());
            }
            let pending: Vec<std::path::PathBuf> = std::mem::take(&mut opener.borrow_mut().1);
            init(cx);
            // macOS keeps an app running after its last window closes; quit instead.
            cx.on_window_closed(|cx, _| quit_if_last_window_closed(cx)).detach();
            timing::mark("app");
            // A file handed over at launch stands in for one named on the
            // command line. The window opens at once, with an empty
            // document of the file's kind until the file is read.
            let (first, rest) = match (early_read, pending.split_first()) {
                (Some(read), _) => (Some(read), pending.clone()),
                (None, Some((first, rest))) => {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let read_path = first.clone();
                    std::thread::spawn(move || sender.send(read_file(&read_path)).ok());
                    (Some((first.clone(), receiver)), rest.to_vec())
                }
                (None, None) => (None, Vec::new()),
            };
            let opens_file = first.is_some();
            let read_only = args.read_only;
            match first {
                Some((path, receiver)) => open_window_with(cx, move |editor, window, cx| {
                    editor.read_only = read_only;
                    let read = move || receiver.recv().unwrap_or_else(|_| Err(crate::shell::i18n::text("shell.read-stopped").to_string()));
                    editor.open_read(path, read, slide, window, cx)
                }),
                None => open_window_with(cx, move |editor, _, _| editor.read_only = read_only),
            }
            rest.into_iter().for_each(|p| open_path_in_app(p, cx));
            timing::mark("window");
            // Look for a newer version in the background (not in the timing runs).
            if !quit_after_first_frame {
                crate::shell::update::start(cx);
                // Offer to put the AppImage in the desktop's list of apps (Linux).
                linux_desktop::start(cx);
            }
            // The frame after the one scheduled now: the window drawn once.
            if let Some(window) = cx.windows().first().copied() {
                window
                    .update(cx, move |_, window, _| {
                        window.on_next_frame(move |window, _| {
                            timing::mark("first-frame");
                            window.on_next_frame(move |_, cx| {
                                timing::mark("second-frame");
                                // With a file, after it is drawn (`timing::document_loaded`).
                                if quit_after_first_frame && !opens_file {
                                    cx.quit();
                                }
                            });
                            window.refresh();
                        });
                        window.refresh();
                    })
                    .ok();
            }
        });
}

#[cfg(target_family = "wasm")]
mod web {
    use std::borrow::Cow;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui_kit::component::theme::Theme;
    use gpui_kit::web::{CanvasFontFallback, WebBackendPreference, WebPlatform};
    use gpui_kit::*;
    use wasm_bindgen::prelude::*;

    thread_local! {
        static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
    }

    /// UI font bundled with the app. The browser has no system fonts GPUI can
    /// load, and GPUI's `.SystemUIFont` alias resolves to IBM Plex Sans on web.
    const UI_FONT: &[u8] = include_bytes!("../fonts/IBMPlexSans-Regular.ttf");

    /// Start the editor in the page. Icons are fetched from
    /// `<asset_base>/assets/icons/*.svg`. Pass an absolute URL such as
    /// `location.origin`: the icon loader fetches with reqwest, which rejects
    /// relative URLs, so `None` (an empty base) loads no icons.
    ///
    /// `file` is a .pptx, .xlsx or .docx to open, named `file_name`; without it (or if it
    /// cannot be read) the editor shows the start view.
    #[wasm_bindgen]
    pub fn run(asset_base: Option<String>, file_name: Option<String>, file: Option<Vec<u8>>) -> Result<(), JsValue> {
        let document = file.and_then(|data| match ibis_pptx::read(&data) {
            Ok(presentation) => Some((presentation, file_name.unwrap_or_default().into())),
            Err(err) => {
                log::error!("{err}");
                None
            }
        });
        gpui_kit::platform::web_init();

        // Single-threaded: needs no SharedArrayBuffer, so no COOP/COEP headers.
        // Japanese text (and emoji) the bundled font lacks is drawn by the
        // browser's Canvas 2D with the viewer's local fonts, so no multi-MB
        // CJK font has to ship in the wasm.
        let platform = Rc::new(WebPlatform::new_with_backend_and_font_fallback(
            false,
            WebBackendPreference::Auto,
            CanvasFontFallback::EmojiAndCjk,
        ));
        // Dates typed without a year take this year, and TODAY and NOW the
        // time now: the calculator cannot read the clock in the browser, so the
        // editor gives it the browser's (`clock::calc_clock`, in the browser's
        // time zone; see `SlideEditor::new`).
        ibis_calc::eval::convert::set_clock(crate::clock::calc_clock);
        let http_client = Arc::new(platform.fetch_http_client());
        // The art page borders' pictures are fetched from the same place.
        crate::docx::art_border::set_web_source(asset_base.clone().unwrap_or_default(), http_client.clone());
        crate::render::web_image::set_http_client(http_client.clone());
        let app = Application::with_platform(platform)
            .with_http_client(http_client)
            .with_assets(crate::shell::icons::Assets::new(
                asset_base.unwrap_or_default(),
            ));

        let handle = app.run_embedded(|cx| {
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(UI_FONT)])
                .expect("failed to load the UI font");
            super::init(cx);
            Theme::global_mut(cx).font_family = "IBM Plex Sans".into();
            super::open_editor(document, None, cx);
        });
        APPLICATION.with(|app| *app.borrow_mut() = Some(handle));
        Ok(())
    }

    /// Whether the browser runs on a Mac (or an iPad, which reports itself as
    /// one), where shortcuts use Command. Same test as gpui-pre-web's.
    pub(crate) fn is_mac() -> bool {
        let navigator = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("navigator")).ok();
        let field = |name: &str| {
            navigator
                .as_ref()
                .and_then(|navigator| js_sys::Reflect::get(navigator, &JsValue::from_str(name)).ok())
                .and_then(|value| value.as_string())
                .unwrap_or_default()
        };
        field("platform").contains("Mac") || field("userAgent").contains("Mac")
    }

    #[wasm_bindgen]
    extern "C" {
        /// Defined by `www/main.ts`: shows the browser's file picker and
        /// resolves to `{ name, bytes }`, or to `null` when cancelled.
        #[wasm_bindgen(catch, js_name = ibisOfficePickFile)]
        fn pick_file_js() -> Result<js_sys::Promise, JsValue>;
    }

    /// Let the user choose a file; its name and contents, or `None` if cancelled.
    pub(crate) async fn pick_file() -> Result<Option<(String, Vec<u8>)>, String> {
        let describe = |err: JsValue| err.as_string().unwrap_or_else(|| format!("{err:?}"));
        let promise = pick_file_js().map_err(describe)?;
        let value = wasm_bindgen_futures::JsFuture::from(promise).await.map_err(describe)?;
        if value.is_null() || value.is_undefined() {
            return Ok(None);
        }
        let field = |name: &str| js_sys::Reflect::get(&value, &JsValue::from_str(name)).map_err(describe);
        let name = field("name")?.as_string().unwrap_or_default();
        let bytes = js_sys::Uint8Array::new(&field("bytes")?).to_vec();
        Ok(Some((name, bytes)))
    }
}
