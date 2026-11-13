// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The state of editing shapes and their text, which every format has the window keep (`SlideEditor::shape_edit`): the selection,
//! the tool and the drag in progress, the shape clipboard, the guides and the text being edited. A presentation's editor reads
//! and writes it as the other formats' do when they put shapes on their pages. It was in `pptx::State`.

use gpui_kit::*;
use ibis_doc_model::{EmuRect, ResizeHandle, ShapeId};

use crate::chrome::MenuEntry;
use crate::editor::SlideEditor;
use crate::shape_items::{ArrowKind, LineKind};
use crate::text_edit::TextEditing;

/// What a press on the slide does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    TextBox,
    /// A preset shape (`ST_ShapeType`, such as `"rect"`).
    Shape(&'static str),
    Line { kind: LineKind, arrow: ArrowKind },
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Select
    }
}

impl Tool {
    /// The status bar's hint while the tool is in use.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Tool::Select => None,
            Tool::TextBox => Some("pptx.editing.click-drag-place-text-box"),
            Tool::Shape(_) => Some("pptx.editing.drag-place-shape-shift-square"),
            Tool::Line { kind: LineKind::Straight, .. } => Some("_pptx.drag-to-draw-a-line-shift-for-45-steps-esc-to-ca"),
            Tool::Line { kind: LineKind::Bent, .. } => Some("_pptx.drag-to-draw-an-elbow-connector-esc-to-cancel"),
            Tool::Line { kind: LineKind::Curved, .. } => Some("_pptx.drag-to-draw-a-curved-connector-esc-to-cancel"),
            Tool::Line { kind: LineKind::Freehand, .. } => Some("_pptx.drag-to-draw-freehand-esc-to-cancel"),
        }
    }
}

/// A pointer drag on the slide, previewed while it goes and turned into one
/// edit when it ends.
#[derive(Clone, Debug)]
pub enum Drag {
    Move { start: Point<Pixels>, current: Point<Pixels>, origins: Vec<(ShapeId, EmuRect)> },
    Resize { shape: ShapeId, handle: ResizeHandle, start: Point<Pixels>, current: Point<Pixels>, origin: EmuRect, rotation: i32, keep_aspect_on_corners: bool },
    Rotate { shape: ShapeId, start: Point<Pixels>, current: Point<Pixels>, origin: i32, center: Point<Pixels> },
    Band { start: Point<Pixels>, current: Point<Pixels>, before: Vec<ShapeId> },
    Place { tool: Tool, start: Point<Pixels>, current: Point<Pixels>, points: Vec<Point<Pixels>> },
    /// Selecting the edited text.
    Text { start: Point<Pixels>, current: Point<Pixels> },
    /// A table's border between columns (`column`: the one left of it) or
    /// rows (`row`: the one above it) moved: 列の幅・行の高さ.
    TableBorder { shape: ShapeId, column: Option<usize>, row: Option<usize>, start: Point<Pixels>, current: Point<Pixels> },
    /// 頂点の編集: vertex `index` of outline `part` dragged.
    Vertex { part: usize, index: usize, handle: u8, start: Point<Pixels>, current: Point<Pixels> },
    /// ガイド線: guide `index` dragged (out of the slide, it is removed).
    Guide { index: usize, start: Point<Pixels>, current: Point<Pixels>, alt: bool },
}

impl Drag {
    pub fn current_mut(&mut self) -> &mut Point<Pixels> {
        match self {
            Drag::Move { current, .. } | Drag::Resize { current, .. } | Drag::Rotate { current, .. } | Drag::Band { current, .. } | Drag::Place { current, .. } | Drag::Text { current, .. } | Drag::TableBorder { current, .. } | Drag::Vertex { current, .. } | Drag::Guide { current, .. } => current,
        }
    }

    pub fn start_current(&self) -> (Point<Pixels>, Point<Pixels>) {
        match self {
            Drag::Move { start, current, .. } | Drag::Resize { start, current, .. } | Drag::Rotate { start, current, .. } | Drag::Band { start, current, .. } | Drag::Place { start, current, .. } | Drag::Text { start, current } | Drag::TableBorder { start, current, .. } | Drag::Vertex { start, current, .. } | Drag::Guide { start, current, .. } => (*start, *current),
        }
    }
}

/// See the module's words.
/// The shell's panels a format has panels of its own give way to (`FormatEditor::panel_opening`, `panel_closed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapePanel {
    AltText,
    Chart,
    Compress,
    Crop,
    Find,
    Frame,
    Glow,
    Gradient,
    HeaderFooter,
    Insets,
    Paragraph,
    ReplaceFonts,
    Shadow,
    Size,
    Spelling,
    Symbols,
}

/// A context menu over the page or one of its panels, open.
pub struct ContextPopup {
    pub menu: Entity<gpui_kit::component::menu::PopupMenu>,
    pub position: Point<Pixels>,
    pub _dismiss: Subscription,
}

#[derive(Default)]
pub struct ShapeEditState {
    pub selection: Vec<ShapeId>,
    pub tool: Tool,
    pub drag: Option<Drag>,
    /// Shift is held (during a drag).
    pub shift: bool,
    pub clipboard: Vec<ibis_doc_model::Shape>,
    pub clipboard_groups: Vec<ibis_doc_model::ShapeGroup>,
    /// The text being edited in place, where its characters are, the x the caret keeps moving up or down, and the click that
    /// started the edit.
    pub edit: Option<TextEditing>,
    pub positions: crate::render::rich_text::TextPositions,
    pub(crate) goal_x: Option<(usize, f32)>,
    pub pending_click: Option<Point<Pixels>>,
    /// The edited text last checked for spelling, and the words found wrong (as ranges of characters).
    pub(crate) spell_cache: std::cell::RefCell<Option<(String, Vec<std::ops::Range<usize>>)>>,
    pub guides: bool,
    pub gridlines: bool,
    pub snap_to_grid: bool,
    pub snap_to_shape: bool,
    pub align_to_slide: bool,
    pub chart_panel: Option<crate::chart_panel::ChartPanel>,
    pub chart_data: Option<crate::chart_data::DataWindow>,
    pub compress_panel: Option<crate::compress::CompressPanel>,
    pub theme_panel: Option<crate::theme_panel::ThemePanel>,
    pub autocorrect: Option<crate::autocorrect::AutoCorrectPanel>,
    pub symbols: Option<usize>,
    pub character_format_copy: Option<ibis_doc_model::CharacterFormat>,
    pub paragraph_format_copy: Option<ibis_doc_model::Paragraph>,
    pub(crate) clipboard_stamp: Option<u64>,
    pub read_only_dismissed: bool,
    pub defaults: ibis_doc_model::ObjectDefaults,
    pub link_mode: Option<ibis_doc_model::ShapeId>,
    pub(crate) pending_chain_copies: Vec<ibis_doc_model::TextChain>,
    /// The text being composed in a chain laid out (`SlideEditor::chain_display`), for the editing state it was made for.
    pub(crate) chain_display_cache: std::cell::RefCell<Option<((crate::text_edit::TextEditing, usize, ibis_doc_model::SlideId, bool), Option<std::rc::Rc<crate::text_chains::ChainDisplay>>)>>,
    /// Select All took the text of the whole chain, not only of the box being edited: the box and the selection it had then, which the
    /// selection must still be for it to count (`SlideEditor::chain_select_all_active`).
    pub(crate) chain_select_all: Option<(ibis_doc_model::ShapeId, std::ops::Range<usize>)>,
    pub pastes: u32,
    pub clipboard_slide: Option<ibis_doc_model::SlideId>,
    pub crop_panel: Option<crate::crop_panel::CropPanel>,
    pub frame_panel: Option<crate::frame_panel::FramePanel>,
    pub find: Option<crate::find::FindState>,
    pub spelling: Option<crate::spelling_panel::SpellPanel>,
    pub header_footer: Option<crate::header_footer::HeaderFooterPanel>,
    pub selection_pane: Option<crate::selection_pane::SelectionPane>,
    pub thesaurus: Option<crate::thesaurus_panel::PptxThesaurusPanel>,
    pub size_panel: Option<crate::size_panel::SizePanel>,
    pub insets_panel: Option<crate::insets_panel::InsetsPanel>,
    pub glow_panel: Option<crate::glow_panel::GlowPanel>,
    pub shadow_panel: Option<crate::shadow_panel::ShadowPanel>,
    pub gradient_panel: Option<crate::gradient_panel::GradientPanel>,
    pub paragraph_panel: Option<crate::paragraph_panel::ParagraphPanel>,
    pub alt_text_panel: Option<crate::alt_text_panel::AltTextPanel>,
    /// The context menu (of the page, its list or a panel), while open. The format's view draws it.
    pub popup: Option<ContextPopup>,
}

impl SlideEditor {
    /// A menu of `entries` open at `position` (the window's), as the slide's context menu is.
    pub fn open_popup_entries(&mut self, entries: Vec<MenuEntry>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = std::rc::Rc::new(entries);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.shape_edit.popup = None;
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.shape_edit.popup = Some(ContextPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }
}
