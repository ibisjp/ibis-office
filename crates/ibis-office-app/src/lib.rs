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
pub(crate) use ibis_office_base::clock;
// The document screen is its own crate (tasks/doc/crate-split-plan.md, 15); the app and the tests name it `crate::docx`.
pub(crate) use ibis_office_docx as docx;
#[cfg(windows)]
mod open_with;
#[cfg(not(target_family = "wasm"))]
mod single_instance;
// The presentation screen is its own crate (tasks/doc/crate-split-plan.md, 14); the app and its tests name it `crate::pptx`.
pub(crate) use ibis_office_pptx as pptx;
mod render;
mod register_hooks;
pub use register_hooks::register_hooks;
mod format_drivers;
// The shell is its own crate (tasks/doc/crate-split-plan.md, 12); the formats and the tests name it `crate::shell`.
pub(crate) use ibis_office_shell as shell;
pub(crate) use shell::actions::*;
pub(crate) use shell::app::*;
#[cfg(not(target_family = "wasm"))]
pub(crate) use shell::read_file::*;
#[cfg(not(target_family = "wasm"))]
pub(crate) use shell::timing;
// What the shell took down from here (tasks/doc/crate-split-plan.md, 12.1), under the names the formats use.
pub(crate) use shell::linux_desktop;
// The formats are crates with their own aliases: only the app's checks name it.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) use shell::native_shape_clipboard;
#[cfg(all(headless_export, any(test, feature = "scenarios")))]
pub(crate) use shell::pdf_picture;
// The workbook screen is its own crate (tasks/doc/crate-split-plan.md, 16); the app and the tests name it `crate::xlsx`.
pub(crate) use ibis_office_xlsx as xlsx;
#[cfg(any(headless_export, target_family = "wasm"))]
mod export;
#[cfg(any(headless_export, target_family = "wasm"))]
mod export_print;
#[cfg(any(headless_export, target_family = "wasm"))]
mod export_text;
#[cfg(any(headless_export, target_family = "wasm"))]
mod pdf_vector;
#[cfg(target_family = "wasm")]
mod web_pdf;
#[cfg(target_family = "wasm")]
mod web_pdf_fonts;
#[cfg(headless_export)]
mod ui_snapshot;

pub use shell::editor::SlideEditor;

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/shell/native_quit.rs"]
mod native_quit_scenarios;

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/offscreen_3d.rs"]
mod offscreen_3d_scenarios;

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
mod scenario_list;

#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
mod pptx_scenarios;

// The text rectangle check of a slide (`crate::export`, the app's): a `#[test]` on Windows, a scenario elsewhere, so it is declared here and
// not under `pptx_scenarios` (which only Mac and Linux build).
#[cfg(all(headless_export, any(all(test, windows), all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))))]
#[path = "tests/pptx/text_rect.rs"]
mod pptx_text_rect;

// The document's layout checks that export a page (`crate::export`, the app's): they stay in the app (tasks/doc/crate-split-plan.md, 15.2).
#[cfg(any(all(test, windows), all(any(target_os = "macos", target_os = "linux"), feature = "scenarios")))]
#[path = "tests/docx/layout_tests.rs"]
pub(crate) mod docx_layout_tests;
#[cfg(any(all(test, windows), all(any(target_os = "macos", target_os = "linux"), feature = "scenarios")))]
#[path = "tests/docx/layout_tests_grid.rs"]
pub(crate) mod docx_layout_tests_grid;
#[cfg(test)]
#[path = "tests/docx/probe.rs"]
mod docx_probe;
// The workbook's checks that export a page or draw headless (`crate::export`, the app's): they stay in the app (tasks/doc/crate-split-plan.md, 16.2).
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "tests/xlsx/corpus_report.rs"]
pub(crate) mod xlsx_corpus_report;
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[path = "tests/xlsx/scenario_issue189.rs"]
pub(crate) mod xlsx_issue189;
#[cfg(all(test, target_os = "windows", feature = "export"))]
#[path = "tests/xlsx/print_bounds.rs"]
mod xlsx_print_bounds;

#[cfg(test)]
#[path = "tests/shell/menu_context_table.rs"]
mod menu_context_table;

/// Run the scenario tests (`tests/scenarios.rs`; see `shell/scenario.rs`).
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
pub fn run_scenarios(filter: &[String]) -> i32 {
    register_hooks();
    shell::scenario::run(filter, scenario_list::all, scenario_list::ignored)
}
use shell::editor;
use pptx::slideshow;
use xlsx::sheet_ui;

use gpui_kit::*;


/// Register components, theme and key bindings. Call once, before opening a window.
pub fn init(cx: &mut App) {
    register_hooks();
    shell::settings::migrate_on_startup();
    #[cfg(not(target_family = "wasm"))]
    shell::read_file::register_query_workbook_loader();
    // The window knows the system's fonts; the words (ibis-office-base) ask it for their English names.
    ibis_office_base::i18n::set_english_font_name(shell::font_list::english_name);
    gpui_kit::init(cx);
    shell::dialog::init(cx);
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
        KeyBinding::new(&secondary("+"), ViewZoomIn, context),
        KeyBinding::new(&secondary("="), ViewZoomIn, context),
        KeyBinding::new(&secondary("-"), ViewZoomOut, context),
        KeyBinding::new(&secondary("0"), ViewZoomActual, context),
        KeyBinding::new(&secondary("alt-0"), ViewZoomFit, context),
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
        KeyBinding::new("shift-pagedown", sheet_ui::ExtendPageDown, Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("shift-pageup", sheet_ui::ExtendPageUp, Some(editor::SHEET_CONTEXT)),
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
        // A space typed in a text field inside a popover (the search field of a font list) is a character of the field. The popover binds
        // Space to Confirm in its own context (a trigger's activation key); the deeper context of the field takes the key off it.
        KeyBinding::new("space", gpui::NoAction, Some("Input")),
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
        KeyBinding::new(&secondary("o"), Open, None),
        KeyBinding::new(&secondary("n"), NewDocument, None),
        KeyBinding::new(&secondary("s"), Save, None),
        KeyBinding::new(&secondary("shift-s"), SaveAs, None),
        KeyBinding::new(&secondary("p"), Print, None),
        KeyBinding::new("ctrl-shift-f", ToggleMenus, context),
        // As in Office: Command-W (Control-W elsewhere) closes the file.
        KeyBinding::new(&secondary("w"), CloseFile, None),
        // The start window's own keys (a window with no file open has `start_menu::START_CONTEXT`): ⌘P is still Print in a document.
        KeyBinding::new(&secondary("d"), StartNewDocument, Some(shell::start_menu::START_CONTEXT)),
        KeyBinding::new(&secondary("e"), StartNewWorkbook, Some(shell::start_menu::START_CONTEXT)),
        KeyBinding::new(&secondary("p"), StartNewPresentation, Some(shell::start_menu::START_CONTEXT)),
        KeyBinding::new(&secondary("q"), Quit, None),
        KeyBinding::new(&secondary(","), OpenSettings, None),
    ]);
    // ウィンドウ > しまう and ibisOffice > ibisOffice を隠す: the keys of a Mac's apps.
    if is_mac() {
        // ⌘M is a document's own key (PowerPoint's new slide, `pptx.new-slide`): Minimize has it in the start window only (`start_menu::START_CONTEXT`).
        cx.bind_keys([KeyBinding::new("cmd-m", MinimizeWindow, Some(shell::start_menu::START_CONTEXT)), KeyBinding::new("cmd-h", HideApp, None)]);
    }
    cx.on_action(|_: &MinimizeWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });
    cx.on_action(|_: &HideApp, cx| cx.hide());
    #[cfg(not(target_family = "wasm"))]
    cx.on_action(|_: &CheckForUpdates, cx| shell::update::check_now(cx));
    if is_mac(){cx.bind_keys([
        KeyBinding::new("ctrl-shift-up",sheet_ui::ExtendJumpUp,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-down",sheet_ui::ExtendJumpDown,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-left",sheet_ui::ExtendJumpLeft,Some(editor::SHEET_CONTEXT)),
        KeyBinding::new("ctrl-shift-right",sheet_ui::ExtendJumpRight,Some(editor::SHEET_CONTEXT)),
    ]);}
    // Switching between the open windows (menu bar redesign, stage 4): Ctrl+Tab lists them, Ctrl+F6 and Ctrl+Shift+F6 step
    // through them (Windows and Linux; Office's own keys). On a Mac the system's Window menu has the ⌘` and ⌘⇧` keys. The
    // browser has one document and no windows to switch.
    #[cfg(not(target_family = "wasm"))]
    {
        cx.bind_keys([
            KeyBinding::new("ctrl-tab", WindowList, context),
            KeyBinding::new("ctrl-shift-tab", WindowListBack, context),
        ]);
        if !is_mac() {
            cx.bind_keys([KeyBinding::new("ctrl-f6", NextWindow, context), KeyBinding::new("ctrl-shift-f6", PreviousWindow, context)]);
        }
    }
    // A document's text keys (the document担当's, in `docx/editing.rs`).
    cx.bind_keys(docx::key_bindings());
    cx.bind_keys(xlsx::key_bindings());
    cx.on_action(|_: &Quit, cx| quit_asking_to_save(cx));
    // The Dock's Quit bypasses the app menu's Quit action. macOS asks before stopping its
    // event loop; other platform layers currently call on_quit only after their loop has ended.
    #[cfg(any(target_os = "macos", feature = "scenarios"))]
    cx.on_quit_requested(native_quit_allowed).detach();
    cx.on_action(|_: &OpenSettings, cx| crate::shell::settings_dialog::open(cx));
    cx.on_action(|_: &OpenAbout, cx| crate::shell::about::open(cx));
    cx.on_action(|_: &OpenStartWindow, cx| crate::open_editor(None, None, cx));
    // File > New with no window open (a window handles it itself otherwise): the start window.
    cx.on_action(|_: &NewDocument, cx| crate::open_editor(None, None, cx));
    set_app_menus(cx);
}



/// Command line:
///
/// ```text
/// ibisOffice [--slide <number>] [<file.pptx|file.xlsx|file.docx>]
/// ibisOffice --export png|pdf|pdf-image [--out-dir <dir>] [--width <px>] [--range 1,3-5] [--include-layout] <file.pptx|file.xlsx|file.docx>
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
#[derive(Default)]
#[cfg_attr(target_family = "wasm", allow(dead_code))]
struct Args {
    path: Option<std::path::PathBuf>,
    slide: Option<usize>,
    export: Option<String>,
    out_dir: Option<std::path::PathBuf>,
    /// `--export row-heights <file> [out.tsv]`: the second file name.
    out_file: Option<std::path::PathBuf>,
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
    no_handout_date: bool,
    no_handout_number: bool,
    handout_order_vertical: bool,
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
    parse_args_from(std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()))
}

/// The arguments `--export` takes, from a list of them (the browser version's PDF is made with the arguments the print settings give
/// the desktop's `--export` run: `SlideEditor::write_pdf_with`).
fn parse_args_from(arguments: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = arguments.into_iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--read-only" => args.read_only = true,
            "--slide" => args.slide = Some(value()?.parse().map_err(|_| "--slide needs a slide number".to_string())?),
            "--export" => args.export = Some(value()?),
            "--out-dir" => args.out_dir = Some(value()?.into()),
            "--width" => args.width = Some(value()?.parse().map_err(|_| "--width needs a width in pixels".to_string())?),
            "--range" => args.slides = Some(value()?),
            // K0038: the old names still work, with a warning (`--slides` is pages for a document, and a singular `--slide` opens at one slide).
            "--slides" => {
                eprintln!("warning: --slides is the old name of --range");
                args.slides = Some(value()?)
            }
            "--include-layout" => args.layout = true,
            "--layout" => {
                eprintln!("warning: --layout is the old name of --include-layout");
                args.layout = true
            }
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
            "--no-handout-date" => args.no_handout_date = true,
            "--no-handout-number" => args.no_handout_number = true,
            "--handout-order" => {
                args.handout_order_vertical = match value()?.as_str() {
                    "horizontal" => false,
                    "vertical" => true,
                    other => return Err(format!("--handout-order takes horizontal or vertical: {other}")),
                }
            }
            "--steps" => args.steps = Some(value()?),
            "--timing" => args.timing = true,
            "--quit-after-first-frame" => args.quit_after_first_frame = true,
            other if other.starts_with("--") => return Err(format!("unknown option: {other}")),
            _ if args.path.is_some() && args.export.as_deref() == Some("row-heights") => args.out_file = Some(arg.into()),
            _ => args.path = Some(arg.into()),
        }
    }
    Ok(args)
}

/// What a handout, notes page or outline carries besides the slides. PowerPoint puts today's date and the page number on them unless
/// the handout master's `p:hf` turns them off (the header and footer need a text, which only the arguments give); `--no-handout-date`
/// and `--no-handout-number` turn them off.
#[cfg(any(headless_export, target_family = "wasm"))]
fn handout_marks(args: &Args, data: &[u8]) -> export_print::HandoutMarks {
    let flags = ibis_formats::handout_master::header_footer_flags(data).ok();
    let (date_on, number_on) = flags.map_or((true, true), |f| (f.date, f.number));
    let date = args.handout_date.clone().or_else(|| (date_on && !args.no_handout_date).then(shell::header_footer::today));
    let number = args.handout_number || (number_on && !args.no_handout_number);
    export_print::HandoutMarks { header: args.handout_header.clone(), date: date.filter(|_| !args.no_handout_date), footer: args.handout_footer.clone(), number, layout: ibis_formats::handout_master::layout(data).ok(), notes: ibis_formats::handout_master::notes_frames(data).ok(), vertical_order: args.handout_order_vertical, slide_numbers: Vec::new(), slide_size_pt: None }
}

/// The options of an export from the arguments `--export` takes (`file` the file's bytes, for the notes' formatting and the handout master).
#[cfg(any(headless_export, target_family = "wasm"))]
fn export_options(args: &Args, format: export::Format, out_dir: std::path::PathBuf, name: String, file_name: Option<String>, data: &[u8]) -> Result<export::Options, String> {
    let slides = args.slides.as_deref().map(export::parse_slides).transpose()?.unwrap_or_default();
    Ok(export::Options {
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
        notes_source: args.notes_pages.then(|| data.to_vec()),
        handout_marks: handout_marks(args, data),
    })
}

/// `--export`: write the slides of the file and quit, without a window.
#[cfg(not(target_family = "wasm"))]
fn run_export(args: &Args, format: &str) -> Result<(), String> {
    if matches!(format, "md" | "markdown" | "lawxml" | "lawtext") {
        let path = args.path.as_deref().ok_or("name the .docx, .md, .xml, or .lawtext file to export")?;
        let out_dir = args.out_dir.clone().unwrap_or_else(|| ".".into());
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
        let raw_data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // A Law XML or Lawtext file goes through Markdown, as it does when opened.
        let law_md = ibis_formats::lawtext::path_input_to_markdown(path, &raw_data).map_err(|e| format!("{}: {e}", path.display()))?;
        let is_text_md = law_md.is_some() || path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"));
        let data = law_md.map(String::into_bytes).unwrap_or(raw_data);
        let presentation = if is_text_md {
            ibis_formats::markdown::open_markdown_with_base_dir(data, path.parent()).map_err(|e| format!("{}: {e}", path.display()))?.presentation
        } else {
            ibis_odf::read(&data).map_err(|e| format!("{}: {e}", path.display()))?
        };
        let (ext, content) = match format {
            "lawxml" => ("xml", ibis_formats::lawtext::export_presentation_to_law_xml(&presentation)),
            "lawtext" => ("lawtext", ibis_formats::lawtext::export_presentation_to_lawtext(&presentation)),
            _ => ("md", ibis_formats::markdown::export_presentation_to_markdown(&presentation)),
        };
        let content = content.map_err(|e| format!("{}: {e}", path.display()))?;
        let out_path = out_dir.join(format!("{stem}.{ext}"));
        std::fs::write(&out_path, content.as_bytes()).map_err(|e| format!("{}: {e}", out_path.display()))?;
        return Ok(());
    }
    #[cfg(headless_export)]
    {
        let format = match format {
            "row-heights" => {
                let path = args.path.as_deref().ok_or("name the .xlsx or .xls file")?;
                // Read as the PDF export reads it, so the rows are the ones its pages are laid out from.
                let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                let presentation = ibis_odf::read(&data).map_err(|e| format!("{}: {e}", path.display()))?;
                return export::row_heights(presentation, args.out_file.as_deref());
            }
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
            other => return Err(format!("--export takes png, pdf, pdf-image, md, row-heights or ui: {other}")),
        };
        let path = args.path.as_deref().ok_or("name the .pptx, .xlsx or .docx file to export")?;
        let out_dir = args.out_dir.clone().unwrap_or_else(|| ".".into());
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "slides".into());
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let presentation = ibis_odf::read(&data).map_err(|e| format!("{}: {e}", path.display()))?;
        let file_name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let options = export_options(args, format, out_dir, name, file_name, &data)?;
        export::export(presentation, &options)
    }
    #[cfg(not(headless_export))]
    {
        let _ = (args, format);
        Err("this executable cannot export (build it on macOS or Windows with --features export)".into())
    }
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



/// Native entry point; see [`Args`] for the command line.
#[cfg(all(not(target_family = "wasm"), not(target_os = "android"), not(target_os = "ios")))]
pub fn run_native() {
    // `ibisOffice --mcp ...`: the MCP server (GitHub issue #159), headless: no window, no hooks of the windows. (On Windows the GUI program has no standard
    // input and output: `ibisOffice-mcp.exe` is the same server.)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        ibis_calc::eval::convert::set_clock(clock::calc_clock);
        std::process::exit(ibis_mcp::run_from_args(std::env::args().skip(2)));
    }
    register_hooks();
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
    // One ibisOffice to a user (GitHub issue #239): where one is running, a second start (the app's icon, a file association, the command line) hands its
    // file, or a new window, to it and ends. macOS does that itself (Launch Services). Not for a run that is measured or read only, nor with
    // `IBIS_NO_SINGLE_INSTANCE` set (the tests and the scenario runs that start the program).
    #[cfg(any(windows, target_os = "linux"))]
    let single = !args.quit_after_first_frame
        && !args.timing
        && !args.read_only
        && args.slide.is_none()
        && std::env::var_os("IBIS_NO_SINGLE_INSTANCE").is_none();
    #[cfg(any(windows, target_os = "linux"))]
    if single && single_instance::hand_over(&single_instance::requests_for(args.path.as_deref())) {
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
            // This process answers the second starts of the user (see above); each request is handed to the app on its own thread's behalf.
            #[cfg(any(windows, target_os = "linux"))]
            if single {
                let (sender, mut receiver) = futures::channel::mpsc::unbounded::<Vec<single_instance::Request>>();
                let sender = std::sync::Mutex::new(sender);
                match single_instance::serve(move |requests| {
                    if let Ok(sender) = sender.lock() {
                        sender.unbounded_send(requests).ok();
                    }
                }) {
                    Ok(()) => {
                        cx.spawn(async move |cx| {
                            use futures::StreamExt;
                            while let Some(requests) = receiver.next().await {
                                cx.update(|cx| {
                                    for request in requests {
                                        match request {
                                            single_instance::Request::Open(path) => open_path_in_app(path, cx),
                                            single_instance::Request::NewWindow => open_window_with(cx, |_, _, _| {}),
                                        }
                                    }
                                    cx.activate(true);
                                });
                            }
                        })
                        .detach();
                        cx.on_app_quit(|_| async { single_instance::stop() }).detach();
                    }
                    Err(err) => log::warn!("cannot answer second starts: {err}"),
                }
            }
            // The files that are open are watched for changes by other apps (shell/file_watch.rs).
            shell::file_watch::start(cx);
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
                // The fonts that can be downloaded, asked about at the first start of a version (セットアップ).
                crate::shell::font_setup_dialog::start(cx);
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

    /// Runs `f` with the app (what the host's calls act on). Nothing happens before the editor started.
    fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
        APPLICATION.with(|app| app.borrow().as_ref().map(|handle| handle.update(f)))
    }

    /// Whether the editor window exists, so the host's calls (`host_load` and the others) act on it. `run` returns before that: the app
    /// starts when the GPU is ready. The page waits for this before it tells the host `ready`.
    #[wasm_bindgen]
    pub fn host_editor_ready() -> bool {
        with_app(|cx| crate::shell::host_editor::with_editor(cx, |_, _, _| ()).is_some()).unwrap_or(false)
    }

    /// The host gives a document (`IbisOfficeHandle.load`): shown as the page's document, tied to the host's file when `bound`.
    #[wasm_bindgen]
    pub fn host_load(name: String, bytes: Vec<u8>, version: String, folder_id: Option<String>, read_only: bool, read_only_message: Option<String>, bound: bool) -> Result<(), JsValue> {
        let result = with_app(|cx| crate::shell::host_editor::with_editor(cx, |editor, window, cx| editor.host_load(name, bytes, version, folder_id, read_only, read_only_message, bound, window, cx)));
        match result.flatten() {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => Err(JsValue::from_str(&error)),
            None => Err(JsValue::from_str("the editor is not running")),
        }
    }

    /// The host switches read-only on or off (`IbisOfficeHandle.setReadOnly`).
    #[wasm_bindgen]
    pub fn host_set_read_only(read_only: bool, message: Option<String>) {
        with_app(|cx| crate::shell::host_editor::with_editor(cx, |editor, window, cx| editor.host_set_read_only(read_only, message, window, cx)));
    }

    /// A notice of the host for the information bar (`IbisOfficeHandle.notify`; no `level` clears them).
    #[wasm_bindgen]
    pub fn host_notify(level: Option<String>, message: Option<String>, id: Option<String>) {
        use crate::shell::host_editor::{HostNotice, HostNoticeLevel};
        let notice = match (level.as_deref(), message) {
            (Some(level), Some(message)) => Some(HostNotice {
                level: match level {
                    "error" => HostNoticeLevel::Error,
                    "warn" => HostNoticeLevel::Warn,
                    _ => HostNoticeLevel::Info,
                },
                message,
                id,
            }),
            _ => None,
        };
        with_app(|cx| crate::shell::host_editor::with_editor(cx, |editor, _, cx| editor.host_notify(notice, cx)));
    }

    /// The document as a save would write it (`IbisOfficeHandle.snapshot`).
    #[wasm_bindgen]
    pub fn host_snapshot() -> Result<Vec<u8>, JsValue> {
        match with_app(|cx| crate::shell::host_editor::with_editor(cx, |editor, window, cx| editor.host_bytes(window, cx))).flatten() {
            Some(Ok(bytes)) => Ok(bytes),
            Some(Err(error)) => Err(JsValue::from_str(&error)),
            None => Err(JsValue::from_str("the editor is not running")),
        }
    }

    /// The file name of `host_snapshot`.
    #[wasm_bindgen]
    pub fn host_snapshot_name() -> String {
        with_app(|cx| crate::shell::host_editor::with_editor(cx, |editor, _, _| editor.file_name.as_ref().map(|n| n.to_string()).unwrap_or_default())).flatten().unwrap_or_default()
    }

    /// Start the editor in the page. Icons are fetched from
    /// `<asset_base>/assets/icons/*.svg`. Pass an absolute URL such as
    /// `location.origin`: the icon loader fetches with reqwest, which rejects
    /// relative URLs, so `None` (an empty base) loads no icons.
    ///
    /// `file` is a .pptx, .xlsx or .docx to open, named `file_name`; without it (or if it
    /// cannot be read) the editor shows the start view.
    #[wasm_bindgen]
    pub async fn run(asset_base: Option<String>, file_name: Option<String>, file: Option<Vec<u8>>) -> Result<(), JsValue> {
        let document = file.and_then(|data| match ibis_odf::read(&data) {
            Ok(presentation) => Some((presentation, file_name.unwrap_or_default().into())),
            Err(err) => {
                log::error!("{err}");
                None
            }
        });
        crate::register_hooks();
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
        crate::export::set_web_text_system(platform.text_system());
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
            // The blocks of the fonts the page's manifest lists (GitHub issue #157, stage 2): the fonts are known before a document opens, the base block of each comes behind.
            crate::shell::web::load_subset_blocks(cx, Vec::new(), true);
            let blank = document.is_none().then(crate::shell::web::new_kind_of_address).flatten();
            super::open_editor(document, None, cx);
            if let Some(kind) = blank {
                crate::shell::app::new_file_in_page(kind, cx);
            }
            // The fonts that can be downloaded, asked about at the first start of a version (セットアップ).
            crate::shell::font_setup_dialog::start(cx);
            // The fonts downloaded with セットアップ (kept in the browser's IndexedDB) are read behind the first drawing, not before it (#157).
            crate::shell::web::load_downloaded_fonts_in_background(cx);
        });
        APPLICATION.with(|app| *app.borrow_mut() = Some(handle));
        Ok(())
    }
}

#[cfg(target_os = "android")]
mod android_entry {
    const UI_FONT: &[u8] = include_bytes!("../fonts/IBMPlexSans-Regular.ttf");

    #[unsafe(no_mangle)]
    pub extern "Rust" fn android_main(android_app: gpui_android::android_activity::AndroidApp) {
        gpui_android::init_logging();
        super::register_hooks();
        ibis_calc::eval::convert::set_clock(super::clock::calc_clock);

        let platform = std::rc::Rc::new(gpui_android::AndroidPlatform::new(android_app));
        let app = super::Application::with_platform(platform)
            .with_assets(crate::shell::icons::Assets::new(""));
        let mut fonts: Vec<std::borrow::Cow<'static, [u8]>> =
            vec![std::borrow::Cow::Borrowed(UI_FONT)];
        for path in [
            "/system/fonts/NotoSansCJK-Regular.ttc",
            "/system/fonts/NotoSerifCJK-Regular.ttc",
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/Roboto-Bold.ttf",
            "/system/fonts/DroidSansMono.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                log::info!("Loaded Android system font: {path} ({} bytes)", bytes.len());
                fonts.push(std::borrow::Cow::Owned(bytes));
            }
        }
        if let Err(err) = app.text_system().add_fonts(fonts) {
            log::error!("Failed to register fonts on Android: {err}");
        }
        let names = app.text_system().all_font_names();
        let ui_font = ["Noto Sans CJK JP", "Noto Sans JP", "IBM Plex Sans"]
            .into_iter()
            .find(|name| names.iter().any(|n| n == name))
            .unwrap_or("IBM Plex Sans")
            .to_string();
        log::info!("Android UI font selected: {ui_font} (total {} families)", names.len());

        let opener: std::rc::Rc<
            std::cell::RefCell<(Option<super::AsyncApp>, Vec<std::path::PathBuf>)>,
        > = Default::default();
        let handed = opener.clone();
        app.on_open_urls(move |urls| {
            let paths: Vec<std::path::PathBuf> =
                urls.iter().filter_map(|u| super::url_path(u)).collect();
            let mut state = handed.borrow_mut();
            match state.0.clone() {
                Some(cx) => {
                    drop(state);
                    cx.update(|cx| {
                        paths
                            .into_iter()
                            .for_each(|p| super::open_path_in_app(p, cx))
                    });
                }
                None => state.1.extend(paths),
            }
        });
        app.run(move |cx| {
            opener.borrow_mut().0 = Some(cx.to_async());
            let pending: Vec<std::path::PathBuf> = std::mem::take(&mut opener.borrow_mut().1);
            super::init(cx);
            super::shell::file_watch::start(cx);
            gpui_kit::component::Theme::global_mut(cx).font_family = ui_font.into();
            if let Some((first, rest)) = pending.split_first() {
                super::open_path_in_app(first.clone(), cx);
                for p in rest {
                    super::open_path_in_app(p.clone(), cx);
                }
            } else {
                super::open_editor(None, None, cx);
            }
        });
    }
}

#[cfg(target_os = "ios")]
mod ios_entry {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::rc::Rc;

    const UI_FONT: &[u8] = include_bytes!("../fonts/IBMPlexSans-Regular.ttf");

    thread_local! {
        static PLATFORM: RefCell<Option<Rc<gpui_ios::IosPlatform>>> = const { RefCell::new(None) };
        static APP_HANDLE: RefCell<Option<gpui_kit::gpui::ApplicationHandle>> = const { RefCell::new(None) };
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_start(
        ui_view: *mut c_void,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) {
        if PLATFORM.with(|p| p.borrow().is_some()) {
            return;
        }
        let Some(view) = gpui_ios::make_non_null_view(ui_view) else {
            return;
        };
        super::register_hooks();
        ibis_calc::eval::convert::set_clock(super::clock::calc_clock);

        let platform = Rc::new(gpui_ios::IosPlatform::new(
            view,
            logical_width,
            logical_height,
            scale_factor,
        ));
        PLATFORM.with(|p| *p.borrow_mut() = Some(Rc::clone(&platform)));

        let app = super::Application::with_platform(platform)
            .with_assets(crate::shell::icons::Assets::new(""));
        let mut fonts: Vec<std::borrow::Cow<'static, [u8]>> =
            vec![std::borrow::Cow::Borrowed(UI_FONT)];
        unsafe extern "C" {
            fn ibis_ios_find_font_path(name: *const std::ffi::c_char) -> *mut std::ffi::c_char;
            fn ibis_ios_free_c_string(ptr: *mut std::ffi::c_char);
        }
        for font_name in [
            c"HiraginoSans-W3",
            c"HiraginoSans-W6",
            c"PingFangSC-Regular",
            c".AppleSystemUIFont",
        ] {
            let ptr = unsafe { ibis_ios_find_font_path(font_name.as_ptr()) };
            if !ptr.is_null() {
                let path = unsafe { std::ffi::CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { ibis_ios_free_c_string(ptr) };
                if let Ok(bytes) = std::fs::read(&path) {
                    fonts.push(std::borrow::Cow::Owned(bytes));
                }
            }
        }
        let _ = app.text_system().add_fonts(fonts);
        let names = app.text_system().all_font_names();
        let ui_font = [
            "Hiragino Sans",
            "Hiragino Kaku Gothic ProN",
            "PingFang SC",
            "IBM Plex Sans",
        ]
        .into_iter()
        .find(|name| names.iter().any(|n| n == name))
        .unwrap_or("IBM Plex Sans")
        .to_string();

        let opener: Rc<RefCell<(Option<super::AsyncApp>, Vec<std::path::PathBuf>)>> =
            Default::default();
        let handed = opener.clone();
        app.on_open_urls(move |urls| {
            let paths: Vec<std::path::PathBuf> =
                urls.iter().filter_map(|u| super::url_path(u)).collect();
            let mut state = handed.borrow_mut();
            match state.0.clone() {
                Some(cx) => {
                    drop(state);
                    cx.update(|cx| {
                        paths
                            .into_iter()
                            .for_each(|p| super::open_path_in_app(p, cx))
                    });
                }
                None => state.1.extend(paths),
            }
        });

        let handle = app.run_embedded(move |cx| {
            opener.borrow_mut().0 = Some(cx.to_async());
            let pending: Vec<std::path::PathBuf> = std::mem::take(&mut opener.borrow_mut().1);
            super::init(cx);
            super::shell::file_watch::start(cx);
            gpui_kit::component::Theme::global_mut(cx).font_family = ui_font.into();
            if let Some((first, rest)) = pending.split_first() {
                super::open_path_in_app(first.clone(), cx);
                for p in rest {
                    super::open_path_in_app(p.clone(), cx);
                }
            } else {
                super::open_editor(None, None, cx);
            }
        });
        APP_HANDLE.with(|slot| *slot.borrow_mut() = Some(handle));
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_tick() {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.tick();
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_resize(logical_width: f32, logical_height: f32, scale_factor: f32) {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.resize(logical_width, logical_height, scale_factor);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_touch_began(x: f32, y: f32) {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.touch_began(x, y);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_touch_moved(x: f32, y: f32) {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.touch_moved(x, y);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_touch_ended(x: f32, y: f32, cancelled: bool) {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.touch_ended(x, y, cancelled);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_pinch(x: f32, y: f32, delta: f32, phase: u8) {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.pinch(x, y, delta, phase);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_lifecycle(phase: u8) {
        let phase = match phase {
            0 => gpui_kit::gpui::AppLifecyclePhase::Active,
            1 => gpui_kit::gpui::AppLifecyclePhase::Inactive,
            2 => gpui_kit::gpui::AppLifecyclePhase::Background,
            _ => gpui_kit::gpui::AppLifecyclePhase::Foreground,
        };
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.lifecycle(phase);
            }
        });
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ibis_ios_memory_warning() {
        PLATFORM.with(|p| {
            if let Some(platform) = p.borrow().as_ref() {
                platform.memory_warning();
            }
        });
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod read_guard_tests {
    /// C1 (review 0.8.5): a reader that panics closes that file with an error; the process, and the next read, go on.
    #[test]
    fn a_panicking_reader_becomes_an_open_error() {
        crate::register_hooks();
        let path = std::path::Path::new("panics.xlsx");
        let failed: Result<u8, String> = super::catch_read_panic(path, || panic!("a reader's mistake"));
        assert_eq!(failed.unwrap_err(), crate::shell::i18n::text("file.read-crashed").to_string());
        assert_eq!(super::catch_read_panic(path, || Ok::<u8, String>(7)), Ok(7));
    }
}
