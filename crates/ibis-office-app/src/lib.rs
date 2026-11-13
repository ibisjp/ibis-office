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
#[cfg(headless_export)]
mod export;
#[cfg(headless_export)]
mod export_print;
#[cfg(headless_export)]
mod export_text;
#[cfg(headless_export)]
mod pdf_vector;
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
            _ if args.path.is_some() && args.export.as_deref() == Some("row-heights") => args.out_file = Some(arg.into()),
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
            "row-heights" => {
                let path = args.path.as_deref().ok_or("name the .xlsx or .xls file")?;
                // Read as the PDF export reads it, so the rows are the ones its pages are laid out from.
                let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                let presentation = ibis_pptx::read(&data).map_err(|e| format!("{}: {e}", path.display()))?;
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
            other => return Err(format!("--export takes png, pdf, pdf-image, row-heights or ui: {other}")),
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
#[cfg(not(target_family = "wasm"))]
pub fn run_native() {
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

    /// Start the editor in the page. Icons are fetched from
    /// `<asset_base>/assets/icons/*.svg`. Pass an absolute URL such as
    /// `location.origin`: the icon loader fetches with reqwest, which rejects
    /// relative URLs, so `None` (an empty base) loads no icons.
    ///
    /// `file` is a .pptx, .xlsx or .docx to open, named `file_name`; without it (or if it
    /// cannot be read) the editor shows the start view.
    #[wasm_bindgen]
    pub async fn run(asset_base: Option<String>, file_name: Option<String>, file: Option<Vec<u8>>) -> Result<(), JsValue> {
        let document = file.and_then(|data| match ibis_pptx::read(&data) {
            Ok(presentation) => Some((presentation, file_name.unwrap_or_default().into())),
            Err(err) => {
                log::error!("{err}");
                None
            }
        });
        // The fonts downloaded with セットアップ are kept in the browser's IndexedDB; they are given to the text system when the
        // first document is drawn (render::shape_view), so they are read before the editor starts.
        crate::render::shape_view::set_web_fonts(crate::shell::web::load_downloaded_fonts().await);
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
            let blank = document.is_none().then(crate::shell::web::new_kind_of_address).flatten();
            super::open_editor(document, None, cx);
            if let Some(kind) = blank {
                crate::shell::app::new_file_in_page(kind, cx);
            }
            // The fonts that can be downloaded, asked about at the first start of a version (セットアップ).
            crate::shell::font_setup_dialog::start(cx);
        });
        APPLICATION.with(|app| *app.borrow_mut() = Some(handle));
        Ok(())
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
