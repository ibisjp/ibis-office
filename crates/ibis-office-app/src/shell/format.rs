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
use ibis_doc_model::{Presentation, SlideId};
use ibis_pptx::Kind;

use crate::shell::chrome::{Menus, SidebarPanel, ToolItem};
use crate::shell::editor::{SlideEditor, WorkspaceContent};
use crate::{docx, pptx, xlsx};

/// The state of the open document that only its format has.
pub(crate) enum FormatState {
    Presentation(pptx::State),
    Workbook(xlsx::State),
    Document(docx::State),
}

impl FormatState {
    /// The state for `presentation`, just read: its kind is told by its
    /// content (worksheets, a flowing document, or slides).
    pub(crate) fn for_model(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        if presentation.flow.is_some() {
            <docx::Document as FormatEditor>::new_state(presentation, window, cx)
        } else if presentation.slides.iter().any(|slide| crate::shell::editor::sheet_of(slide).is_some()) {
            <xlsx::Workbook as FormatEditor>::new_state(presentation, window, cx)
        } else {
            <pptx::Presentation as FormatEditor>::new_state(presentation, window, cx)
        }
    }

    pub(crate) fn kind(&self) -> Kind {
        match self {
            FormatState::Presentation(_) => Kind::Presentation,
            FormatState::Workbook(_) => Kind::Workbook,
            FormatState::Document(_) => Kind::Document,
        }
    }
}

/// Call `FormatEditor::$f(args)` (or read the constant `$c`) of the open
/// document's format.
macro_rules! with_format {
    ($editor:expr, $f:ident ( $($arg:expr),* )) => {
        match $editor.format.kind() {
            ibis_pptx::Kind::Presentation => <crate::pptx::Presentation as crate::shell::format::FormatEditor>::$f($($arg),*),
            ibis_pptx::Kind::Workbook => <crate::xlsx::Workbook as crate::shell::format::FormatEditor>::$f($($arg),*),
            ibis_pptx::Kind::Document => <crate::docx::Document as crate::shell::format::FormatEditor>::$f($($arg),*),
        }
    };
    ($editor:expr, $c:ident) => {
        match $editor.format.kind() {
            ibis_pptx::Kind::Presentation => <crate::pptx::Presentation as crate::shell::format::FormatEditor>::$c,
            ibis_pptx::Kind::Workbook => <crate::xlsx::Workbook as crate::shell::format::FormatEditor>::$c,
            ibis_pptx::Kind::Document => <crate::docx::Document as crate::shell::format::FormatEditor>::$c,
        }
    };
}
pub(crate) use with_format;

/// What a format's editor provides to the shell. The defaults are a
/// presentation's behaviour.
pub(crate) trait FormatEditor {
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
    fn new_state(presentation: &Presentation, window: &mut Window, cx: &mut Context<SlideEditor>) -> FormatState;

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

    /// Before each render.
    fn before_render(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {}

    /// The window's content between the toolbar and the status bar (the sidebar goes on its right).
    fn render_body(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement;

    /// Items the status bar shows after the page number.
    fn status_items(_editor: &SlideEditor) -> Vec<SharedString> {
        Vec::new()
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

    // ----- Scenario tests (see `shell/scenario.rs`) ------------------------

    /// The format's scenario tests: (name, function), run by
    /// `tests/scenarios.rs` on the main thread. Put them in
    /// `src/<format>/scenarios.rs` (under `#[cfg(feature = "scenarios")]`).
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    fn scenarios() -> Vec<(&'static str, fn())> {
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
    fn format_paint(_editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        None
    }

    /// The title bar's コメントを追加 button: what a click does (⌥⌘M too,
    /// when the format handles the key). `None` leaves it disabled.
    fn add_comment(_editor: &SlideEditor) -> Option<crate::shell::chrome::Handler> {
        None
    }

    /// The title bar's コメントを表示 button: whether the comments are shown
    /// (the button is drawn filled), and what a click does. `None` leaves
    /// it disabled.
    fn show_comments(_editor: &SlideEditor) -> Option<(bool, crate::shell::chrome::Handler)> {
        None
    }

    /// Buttons after the toolbar's common left part (新しいスライド, …).
    fn toolbar_left(_editor: &SlideEditor) -> Vec<ToolItem> {
        Vec::new()
    }

    /// The toolbar's right half, for what is selected. `None` keeps the
    /// shell's stand-in for the format (`chrome_defaults::toolbar_right`).
    fn toolbar_right(_editor: &SlideEditor) -> Option<Vec<ToolItem>> {
        None
    }

    /// The sidebar's content. `None` keeps the shell's stand-in
    /// (`chrome_defaults::sidebar`), which may be nothing (no sidebar).
    fn sidebar(_editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        None
    }

    /// The tabs across the sidebar's top, for the panels that stand side by
    /// side (a workbook's 書式設定・グラフ エディター・データの入力規則).
    fn sidebar_tabs() -> Vec<crate::shell::chrome::SidebarTab> {
        Vec::new()
    }

    /// Whether the right pane has something to show when it is opened with no panel chosen (a workbook: what is
    /// selected; a presentation: the slide's or the shape's size and position). A document's shows only a panel opened
    /// for it (書式・コメント・検索…), so its title-row button cannot open the pane by itself.
    fn sidebar_on_request() -> bool {
        false
    }
}

impl SlideEditor {
    /// The kind of the open document.
    pub(crate) fn kind(&self) -> Kind {
        self.format.kind()
    }

    pub(crate) fn is_workbook(&self) -> bool {
        self.kind() == Kind::Workbook
    }

    pub(crate) fn is_presentation(&self) -> bool {
        self.kind() == Kind::Presentation
    }

    pub(crate) fn is_document(&self) -> bool {
        self.kind() == Kind::Document
    }

    /// The open document's pages are slides (see `FormatEditor::SLIDES`).
    pub(crate) fn pages_are_slides(&self) -> bool {
        with_format!(self, SLIDES)
    }

    /// Shapes may be put on the pages (see `FormatEditor::SHAPES`).
    pub(crate) fn allows_shapes(&self) -> bool {
        with_format!(self, SHAPES)
    }

    /// A workbook's state, when a workbook is open.
    pub(crate) fn sheet_state(&self) -> Option<&xlsx::State> {
        match &self.format {
            FormatState::Workbook(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn sheet_state_mut(&mut self) -> Option<&mut xlsx::State> {
        match &mut self.format {
            FormatState::Workbook(state) => Some(state),
            _ => None,
        }
    }

    /// A document's state, when a document is open.
    pub(crate) fn doc_state(&self) -> Option<&docx::State> {
        match &self.format {
            FormatState::Document(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn doc_state_mut(&mut self) -> Option<&mut docx::State> {
        match &mut self.format {
            FormatState::Document(state) => Some(state),
            _ => None,
        }
    }
}
