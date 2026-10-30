// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation's editing operations, as the menus, the toolbar, the keys
//! and the pointer ask for them: selection, tools, and edits turned into
//! `ibis_doc_model::Command`s (one undo step per operation). The views call
//! these; the commands and saving know nothing of the UI.

use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{
    AutoShape, Caps, Color, Command, EmuRect, Fill, Geometry, LayoutId, ObjectDefaults, Rgb, Shape, ShapeDefault, ShapeId, ShapeKind, SlideId, Stroke, TextAlign,
    TextBody, TextStyle, VerticalAnchor,
};

use super::State;
use super::interaction;
use crate::shell::editor::SlideEditor;
use crate::shell::format::FormatState;

/// A table's style option (タイトル行・集計行・縞模様など).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableOption {
    FirstRow,
    LastRow,
    FirstCol,
    LastCol,
    BandRow,
    BandCol,
}

/// The table styles offered (表スタイル), with DrawingML GUIDs.
pub(crate) const TABLE_STYLES: &[(&str, Option<&str>, &str)] = &[
    ("pptx.table-style.none", None, ""),
    ("pptx.table-style.grid", Some("{59600109-2475-450F-8C8D-F307F16C7BBF}"), ""),
    ("pptx.table-style.medium2-accent1", Some("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"), ""),
    ("pptx.table-style.medium1", Some("{69CFFB72-C8E0-4CED-8131-404D4D77372C}"), ""),
    ("pptx.table-style.medium1-accent1", Some("{7C4A5A32-3535-4235-85A0-1C43BEF2925F}"), ""),
    ("pptx.table-style.light1", Some("{3990B04E-E470-4E4E-A68B-510787340C36}"), ""),
    ("pptx.table-style.light1-accent1", Some("{21E4AEA9-35BF-4416-8E97-F69590FA4E44}"), ""),
    ("pptx.table-style.dark1", Some("{793D81CF-94F2-401A-BA57-92F5A7B2D0C2}"), ""),
    ("pptx.table-style.dark1-accent1", Some("{0583FA8F-5235-4E83-B732-1A0E07568F1B}"), ""),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    Straight,
    Bent,
    Curved,
    Freehand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrowKind {
    None,
    Single,
    Double,
}

/// What a press on the slide does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    TextBox,
    /// A preset shape (`ST_ShapeType`, such as `"rect"`).
    Shape(&'static str),
    Line { kind: LineKind, arrow: ArrowKind },
}

impl Tool {
    /// The status bar's hint while the tool is in use.
    pub(crate) fn hint(self) -> Option<&'static str> {
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

/// The preset shapes the toolbar and 挿入 > 図形 offer: (preset, label,
/// icon, menu item id).
pub(crate) const SHAPES: &[(&str, &str, &str)] = &[
    ("rect", "rectangle", "shape.rect"),
    ("roundRect", "rounded_corner", "shape.roundRect"),
    ("ellipse", "circle", "shape.ellipse"),
    ("triangle", "change_history", "shape.triangle"),
    ("rtTriangle", "signal_cellular_4_bar", "shape.rtTriangle"),
    ("diamond", "diamond", "shape.diamond"),
    ("pentagon", "pentagon", "shape.pentagon"),
    ("hexagon", "hexagon", "shape.hexagon"),
    ("rightArrow", "arrow_forward", "shape.rightArrow"),
    ("leftArrow", "arrow_back", "shape.leftArrow"),
    ("upArrow", "arrow_upward", "shape.upArrow"),
    ("downArrow", "arrow_downward", "shape.downArrow"),
    ("star5", "star", "shape.star5"),
    ("wedgeRectCallout", "chat_bubble", "shape.wedgeRectCallout"),
    ("cloud", "cloud", "shape.cloud"),
    ("heart", "favorite", "shape.heart"),
];

/// The 図形の変更 menu's item ids, one for each of [`SHAPES`] (their labels
/// are the shapes').
pub(crate) const CHANGE_SHAPE_IDS: [&str; 16] = [
    "change-shape.rect",
    "change-shape.roundRect",
    "change-shape.ellipse",
    "change-shape.triangle",
    "change-shape.rtTriangle",
    "change-shape.diamond",
    "change-shape.pentagon",
    "change-shape.hexagon",
    "change-shape.rightArrow",
    "change-shape.leftArrow",
    "change-shape.upArrow",
    "change-shape.downArrow",
    "change-shape.star5",
    "change-shape.wedgeRectCallout",
    "change-shape.cloud",
    "change-shape.heart",
];

/// PowerPoint's standard colours, for the colour menus: (colour, menu item
/// id; the label is the id's in `i18n/*.tsv`).
pub(crate) const COLORS: &[(u32, &str)] = &[
    (0x000000, "color.black"),
    (0x404040, "color.dark-gray"),
    (0x7f7f7f, "color.gray"),
    (0xd9d9d9, "color.light-gray"),
    (0xffffff, "color.white"),
    (0xc00000, "color.dark-red"),
    (0xff0000, "color.red"),
    (0xffc000, "color.orange"),
    (0xffff00, "color.yellow"),
    (0x92d050, "color.light-green"),
    (0x00b050, "color.green"),
    (0x00b0f0, "color.light-blue"),
    (0x0070c0, "color.blue"),
    (0x002060, "color.dark-blue"),
    (0x7030a0, "color.purple"),
    (0x4472c4, "color.accent1"),
];

/// The outline widths offered, in points.
pub(crate) const WIDTHS: &[f32] = &[0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0];

/// How far an arrow key moves the selection, in EMU (0.2 cm, PowerPoint's
/// grid in Japanese); with ⌥, a point.
const NUDGE: i64 = 72_000;
const FINE_NUDGE: i64 = 12_700;
/// A pasted copy on the same slide is moved this much down and right.
pub(crate) const PASTE_OFFSET: i64 = 182_880;


/// アクション ボタン: each preset with the link it has when drawn (PowerPoint's
/// 動作設定: the buttons that go somewhere), and its menu id.
pub(crate) const ACTION_BUTTONS: &[(&str, &str, &str)] = &[
    ("actionButtonBackPrevious", "pptx.action.previous", ""),
    ("actionButtonForwardNext", "pptx.action.next", ""),
    ("actionButtonBeginning", "pptx.action.first", ""),
    ("actionButtonEnd", "pptx.action.last", ""),
    ("actionButtonHome", "pptx.action.home", ""),
    ("actionButtonReturn", "pptx.action.return", ""),
    ("actionButtonHelp", "pptx.action.help", ""),
    ("actionButtonInformation", "pptx.action.information", "fix.validation.information"),
    ("actionButtonDocument", "pptx.action.document", ""),
    ("actionButtonSound", "pptx.action.sound", "pptx.editing.sound"),
    ("actionButtonMovie", "pptx.action.movie", ""),
    ("actionButtonBlank", "pptx.action.blank", ""),
];

/// The link an action button goes to when drawn: the buttons that move.
pub(crate) fn action_link(preset: &str) -> Option<ibis_doc_model::SlideLink> {
    use ibis_doc_model::SlideLink as L;
    Some(match preset {
        "actionButtonBackPrevious" => L::PreviousSlide,
        "actionButtonForwardNext" => L::NextSlide,
        "actionButtonBeginning" | "actionButtonHome" => L::FirstSlide,
        "actionButtonEnd" => L::LastSlide,
        "actionButtonReturn" => L::LastSlideViewed,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoStylePreset {
    SimpleFrameWhite,
    SimpleFrameBlack,
    Rounded,
    RoundedFrame,
    DropShadow,
    Reflection,
    RoundedReflection,
    RoundedShadow,
    SoftEdge,
    None,
}

/// ワードアートのプリセットスタイル (WordArt styles)
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WordArtPreset {
    pub(crate) id: &'static str,
    pub(crate) insert_id: &'static str,
    pub(crate) apply_id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) color: Rgb,
    pub(crate) bold: bool,
    pub(crate) outline: Option<(Rgb, i64)>,
    pub(crate) shadow: Option<(i32, i64, i64)>,
}

pub(crate) const WORDART_PRESETS: &[WordArtPreset] = &[
    WordArtPreset {
        id: "pptx.wordart.style1",
        insert_id: "pptx.insert.wordart.style1",
        apply_id: "pptx.text.wordart.style1",
        name: "",
        color: Rgb(0x000000),
        bold: true,
        outline: None,
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style2",
        insert_id: "pptx.insert.wordart.style2",
        apply_id: "pptx.text.wordart.style2",
        name: "",
        color: Rgb(0x4472C4),
        bold: true,
        outline: None,
        shadow: Some((2_700_000, 19_050, 25_400)),
    },
    WordArtPreset {
        id: "pptx.wordart.style3",
        insert_id: "pptx.insert.wordart.style3",
        apply_id: "pptx.text.wordart.style3",
        name: "",
        color: Rgb(0xED7D31),
        bold: true,
        outline: Some((Rgb(0xC55A11), 12_700)),
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style4",
        insert_id: "pptx.insert.wordart.style4",
        apply_id: "pptx.text.wordart.style4",
        name: "",
        color: Rgb(0xFFFFFF),
        bold: true,
        outline: Some((Rgb(0x000000), 19_050)),
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style5",
        insert_id: "pptx.insert.wordart.style5",
        apply_id: "pptx.text.wordart.style5",
        name: "",
        color: Rgb(0xFFC000),
        bold: true,
        outline: None,
        shadow: Some((5_400_000, 19_050, 25_400)),
    },
    WordArtPreset {
        id: "pptx.wordart.style6",
        insert_id: "pptx.insert.wordart.style6",
        apply_id: "pptx.text.wordart.style6",
        name: "",
        color: Rgb(0x4472C4),
        bold: true,
        outline: Some((Rgb(0xFFFFFF), 12_700)),
        shadow: Some((2_700_000, 19_050, 25_400)),
    },
];

/// ワードアート・文字変形のプリセット (ECMA-376 a:prstTxWarp)
pub(crate) const TEXT_WARP_PRESETS: &[(&str, &str, &str)] = &[
    ("textArchUp", "pptx.text.warp.arch-up", ""),
    ("textArchDown", "pptx.text.warp.arch-down", ""),
    ("textCircle", "pptx.text.warp.circle", "pptx.common.circle"),
    ("textButton", "pptx.text.warp.button", ""),
    ("textCurveUp", "pptx.text.warp.curve-up", ""),
    ("textCurveDown", "pptx.text.warp.curve-down", ""),
    ("textWave1", "pptx.text.warp.wave1", ""),
    ("textWave2", "pptx.text.warp.wave2", ""),
    ("textDoubleWave1", "pptx.text.warp.double-wave1", ""),
    ("textInflate", "pptx.text.warp.inflate", ""),
    ("textDeflate", "pptx.text.warp.deflate", ""),
    ("textStop", "pptx.text.warp.stop", ""),
    ("textTriangle", "pptx.text.warp.triangle", ""),
    ("textTriangleInverted", "pptx.text.warp.triangle-inverted", ""),
    ("textChevron", "pptx.text.warp.chevron", ""),
    ("textChevronInverted", "pptx.text.warp.chevron-inverted", ""),
    ("textFadeRight", "pptx.text.warp.fade-right", ""),
    ("textFadeLeft", "pptx.text.warp.fade-left", ""),
    ("textFadeUp", "pptx.text.warp.fade-up", ""),
    ("textFadeDown", "pptx.text.warp.fade-down", ""),
];

pub(crate) fn new_shape(id: ShapeId, preset: &str, frame: EmuRect) -> Shape {
    let geometry = ibis_pptx::preset_geometry(preset).unwrap_or(Geometry::Rect);
    let mut auto = AutoShape::new(geometry, Fill::Solid(Color::opaque(Rgb(0x4472c4))), Some(Stroke::solid(Color::opaque(Rgb(0x2f528f)), 12_700)));
    let style = TextStyle { color: Rgb::WHITE, align: TextAlign::Center, ..TextStyle::default() };
    auto.text = Some(TextBody { anchor: VerticalAnchor::Middle, ..TextBody::plain("", style) });
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

/// An empty text body as PowerPoint gives a shape: centred, in the middle.
pub(crate) fn centred_body() -> TextBody {
    let style = TextStyle { color: Rgb::WHITE, align: TextAlign::Center, ..TextStyle::default() };
    TextBody { anchor: VerticalAnchor::Middle, ..TextBody::plain("", style) }
}

/// Returns true if the geometry is a connector/line preset (straight, bent, or curved).
pub(crate) fn is_connector_geometry(geom: &Geometry) -> bool {
    match geom {
        Geometry::Formula(f) => matches!(
            f.preset.as_deref(),
            Some(
                "line"
                    | "lineInv"
                    | "straightConnector1"
                    | "bentConnector2"
                    | "bentConnector3"
                    | "bentConnector4"
                    | "bentConnector5"
                    | "curvedConnector2"
                    | "curvedConnector3"
                    | "curvedConnector4"
                    | "curvedConnector5"
            )
        ),
        _ => false,
    }
}

/// A new line, from its frame and flips.
pub(crate) fn new_line(id: ShapeId, frame: EmuRect, flip_h: bool, flip_v: bool, arrow: bool) -> Shape {
    new_connector(id, frame, flip_h, flip_v, LineKind::Straight, if arrow { ArrowKind::Single } else { ArrowKind::None })
}

/// A new line or connector (straight, bent, curved) with optional single or double arrowheads.
pub(crate) fn new_connector(
    id: ShapeId,
    frame: EmuRect,
    flip_h: bool,
    flip_v: bool,
    kind: LineKind,
    arrow: ArrowKind,
) -> Shape {
    let mut stroke = Stroke::solid(Color::opaque(Rgb(0x4472c4)), 12_700);
    match arrow {
        ArrowKind::None => {}
        ArrowKind::Single => {
            stroke.tail = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
        }
        ArrowKind::Double => {
            stroke.head = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
            stroke.tail = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
        }
    }
    match kind {
        LineKind::Straight => {
            let mut shape = Shape::new(id, frame, ShapeKind::Line(stroke));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Bent => {
            let geom = ibis_pptx::preset_geometry("bentConnector3").unwrap_or(Geometry::Rect);
            let auto = AutoShape::new(geom, Fill::None, Some(stroke));
            let mut shape = Shape::new(id, frame, ShapeKind::Auto(auto));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Curved => {
            let geom = ibis_pptx::preset_geometry("curvedConnector3").unwrap_or(Geometry::Rect);
            let auto = AutoShape::new(geom, Fill::None, Some(stroke));
            let mut shape = Shape::new(id, frame, ShapeKind::Auto(auto));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Freehand => {
            let pts = vec![(0, 0), (frame.width, frame.height)];
            new_freehand_shape(id, frame, &pts, Some(stroke))
        }
    }
}

/// A new freehand scribble shape from its frame, local EMU coordinates, and optional stroke.
pub(crate) fn new_freehand_shape(
    id: ShapeId,
    frame: EmuRect,
    points: &[(i64, i64)],
    stroke: Option<Stroke>,
) -> Shape {
    use ibis_doc_model::{FormulaGeometry, GeometryDef, GeometryPath, PathFill, PathOp};
    let stroke = stroke.unwrap_or_else(|| Stroke::solid(Color::opaque(Rgb(0x4472c4)), 12_700));
    let mut ops = Vec::with_capacity(points.len().max(2));
    if let Some(&(x, y)) = points.first() {
        ops.push(PathOp::MoveTo(x.to_string(), y.to_string()));
    }
    for &(x, y) in points.iter().skip(1) {
        ops.push(PathOp::LineTo(x.to_string(), y.to_string()));
    }
    let path = GeometryPath {
        width: frame.width,
        height: frame.height,
        fill: PathFill::None,
        stroke: true,
        ops,
    };
    let def = GeometryDef {
        adjust: Vec::new(),
        guides: Vec::new(),
        paths: vec![path],
        text_rect: None,
    };
    let geom = FormulaGeometry {
        preset: None,
        adjust: Vec::new(),
        def: std::sync::Arc::new(def),
    };
    let auto = AutoShape::new(Geometry::Formula(geom), Fill::None, Some(stroke));
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

/// What kinds of shapes a selection holds, for the toolbar's right half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionKind {
    None,
    Shape,
    Picture,
    Line,
    Table,
    Chart,
    Multi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BorderPlacement {
    All,
    None,
    Outside,
    Inside,
    Top,
    Bottom,
    Left,
    Right,
    DiagonalDown,
    DiagonalUp,
}

impl SlideEditor {
    pub(crate) fn pptx(&self) -> Option<&State> {
        match &self.format {
            FormatState::Presentation(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn pptx_mut(&mut self) -> Option<&mut State> {
        match &mut self.format {
            FormatState::Presentation(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn cancel_pptx_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let mut cancelled = false;
        if let Some(s) = self.pptx_mut() {
            if let Some(drag) = s.drag.take() {
                match drag {
                    super::slide_view::Drag::Band { before, .. } => {
                        s.selection = before;
                    }
                    super::slide_view::Drag::Text { .. } => {
                        if let Some(edit) = s.edit.as_mut() {
                            edit.caret = edit.anchor;
                        }
                    }
                    _ => {}
                }
                cancelled = true;
            }
            if let Some(pane_drag) = s.pane_drag.take() {
                match pane_drag {
                    super::notes::PaneDrag::List { w0, .. } => {
                        s.pane_width = Some(w0);
                    }
                    super::notes::PaneDrag::Notes { h0, .. } => {
                        s.notes_height = h0;
                    }
                }
                cancelled = true;
            }
            if s.path_draw.take().is_some() {
                cancelled = true;
            }
        }
        if cancelled {
            cx.notify();
        }
        cancelled
    }

    /// The selected shapes of the current slide, back to front.
    pub(crate) fn selected_ids(&self) -> Vec<ShapeId> {
        if let Some(state) = self.pptx() {
            let slide = self.slide();
            slide.shapes.iter().map(|s| s.id).filter(|id| state.selection.contains(id)).collect()
        } else if let Some(state) = self.sheet_state() {
            let slide = self.slide();
            if !state.shape_selection.is_empty() {
                slide.shapes.iter().map(|s| s.id).filter(|id| state.shape_selection.contains(id)).collect()
            } else if let Some(id) = self.selected_sheet_shape() {
                vec![id]
            } else {
                Vec::new()
            }
        } else if let Some(id) = self.selected_sheet_shape() {
            vec![id]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn selected_shapes(&self) -> Vec<&Shape> {
        let ids = self.selected_ids();
        self.slide().shapes.iter().filter(|s| ids.contains(&s.id)).collect()
    }

    pub(crate) fn selection_kind(&self) -> SelectionKind {
        let shapes = self.selected_shapes();
        match shapes.as_slice() {
            [] => SelectionKind::None,
            [one] => match &one.kind {
                ShapeKind::Picture(_) => SelectionKind::Picture,
                ShapeKind::Line(_) => SelectionKind::Line,
                ShapeKind::Auto(auto) if is_connector_geometry(&auto.geometry) => SelectionKind::Line,
                ShapeKind::Table(_) => SelectionKind::Table,
                ShapeKind::Chart(_) => SelectionKind::Chart,
                _ => SelectionKind::Shape,
            },
            _ => SelectionKind::Multi,
        }
    }

    /// Select `ids` (the last is the one the shell's single selection names).
    pub(crate) fn set_selection(&mut self, ids: Vec<ShapeId>) {
        self.selection = ids.last().copied();
        if let Some(state) = self.pptx_mut() {
            state.selection = ids;
        } else if let Some(state) = self.sheet_state_mut() {
            state.shape_selection = ids;
        }
    }

    /// Keep the selection to shapes on the current slide, and in step with
    /// the shell's (which a slide change, a load or text editing may set).
    pub(crate) fn sync_selection(&mut self) {
        let on_slide: Vec<ShapeId> = self.slide().shapes.iter().map(|s| s.id).collect();
        let shell = self.selection;
        if let Some(state) = self.pptx_mut() {
            state.selection.retain(|id| on_slide.contains(id));
            if shell != state.selection.last().copied() {
                state.selection = shell.into_iter().filter(|id| on_slide.contains(id)).collect();
            }
        } else if let Some(state) = self.sheet_state_mut() {
            state.shape_selection.retain(|id| on_slide.contains(id));
            if shell != state.shape_selection.last().copied() {
                state.shape_selection = shell.into_iter().filter(|id| on_slide.contains(id)).collect();
            }
        }
    }

    pub(crate) fn pptx_tool(&self) -> Tool {
        self.pptx().map_or(Tool::Select, |s| s.tool)
    }

    pub(crate) fn set_pptx_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.tool = tool;
            state.drag = None;
        }
        if tool != Tool::Select {
            self.set_selection(Vec::new());
        }
        cx.notify();
    }

    /// Run `commands` as one undo step.
    pub(crate) fn run(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        if commands.is_empty() {
            return;
        }
        let is_admin = commands.iter().all(|c| matches!(c, Command::SetMarkAsFinal { .. } | Command::SetReadOnlyRecommended { .. } | Command::SetModifyVerifier { .. }));
        if !is_admin {
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
        if let Some(edit) = self.pptx_mut().and_then(|s| s.edit.as_mut()) {
            edit.is_new = false;
            edit.typing = false;
        }
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.is_new = false;
            edit.edit.typing = false;
        }
        self.end_notes_typing();
        self.sync_selection();
        if self.is_workbook() {
            self.sync_workbook_peers(cx);
        }
        cx.notify();
    }

    // ----- Selection and shapes ------------------------------------------

    pub(crate) fn select_all_shapes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.slide();
        let ids = slide.shown_shapes().filter(|s| !slide.is_attached(s.id)).map(|s| s.id).collect();
        self.set_selection(ids);
        cx.notify();
    }

    /// Tab (⇧Tab) with a shape selected and no text edited: the next (previous) shape in the
    /// stacking order, from the back, round to the first. `false` when the slide has none.
    pub(crate) fn select_next_shape(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let slide = self.slide();
        let ids: Vec<ShapeId> = slide.shown_shapes().filter(|s| !slide.is_attached(s.id)).map(|s| s.id).collect();
        if ids.is_empty() {
            return false;
        }
        let at = self.selected_ids().first().and_then(|id| ids.iter().position(|x| x == id));
        let next = match (at, forward) {
            (Some(i), true) => (i + 1) % ids.len(),
            (Some(i), false) => (i + ids.len() - 1) % ids.len(),
            (None, true) => 0,
            (None, false) => ids.len() - 1,
        };
        self.set_selection(vec![ids[next]]);
        cx.notify();
        true
    }

    pub(crate) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        // 頂点の編集: Delete takes the vertex away, not the shape.
        if self.is_editing_points() {
            self.delete_selected_vertex(cx);
            return;
        }
        let slide = self.current_slide;
        let commands = self.selected_ids().into_iter().map(|shape| Command::RemoveShape { slide, shape }).collect();
        self.set_selection(Vec::new());
        self.run(commands, cx);
    }

    pub(crate) fn nudge_selected(&mut self, dx: i64, dy: i64, fine: bool, cx: &mut Context<Self>) {
        let step = if fine { FINE_NUDGE } else { NUDGE };
        let slide = self.current_slide;
        let selected = self.selected_shapes();
        let mut commands: Vec<Command> = selected
            .iter()
            .map(|s| Command::SetFrame { slide, shape: s.id, frame: s.frame.translated(dx * step, dy * step) })
            .collect();
        let current_slide = self.slide();
        let updated_shapes: Vec<Shape> = current_slide.shapes.iter().map(|s| {
            if selected.iter().any(|sel| sel.id == s.id) {
                Shape { frame: s.frame.translated(dx * step, dy * step), ..s.clone() }
            } else {
                s.clone()
            }
        }).collect();
        let rerouted = ibis_doc_model::connector::rerouted_on_slide(&updated_shapes, &current_slide.shape_glues);
        for (cid, frame, flip_h, flip_v) in rerouted {
            commands.push(Command::SetConnectorFrame { slide, shape: cid, frame, flip_h, flip_v });
        }
        self.run(commands, cx);
    }

    /// 挿入対象のコンテンツ種類（specific_kind: "tbl", "chart", "pic", "media"）に対して、
    /// 空のプレースホルダー（専用種別または汎用コンテンツ body/obj）を探す。
    pub(crate) fn empty_content_placeholder_for(&self, specific_kind: &str) -> Option<(ShapeId, EmuRect)> {
        let slide = self.slide();
        // 1. 完全一致する専用プレースホルダー（例: "tbl", "chart", "pic", "media"）を最優先
        let specific_match = slide.shapes.iter().find(|sh| {
            slide.placeholders.get(&sh.id).is_some_and(|p| p.placeholder.kind.as_deref() == Some(specific_kind))
                && sh.text_body().is_none_or(ibis_doc_model::TextBody::is_empty)
                && !matches!(sh.kind, ShapeKind::Picture(_) | ShapeKind::Table(_) | ShapeKind::Chart(_))
        });
        if let Some(sh) = specific_match {
            return Some((sh.id, sh.frame));
        }
        // 2. 汎用コンテンツプレースホルダー（body / obj、ただし title や subTitle ではない）
        let generic_match = slide.shapes.iter().find(|sh| {
            slide.placeholders.get(&sh.id).is_some_and(|p| {
                let norm = p.placeholder.normalized();
                let specific = p.placeholder.kind.as_deref();
                norm == "body"
                    && specific != Some("subTitle")
                    && specific != Some("title")
                    && specific != Some("ctrTitle")
            })
            && sh.text_body().is_none_or(ibis_doc_model::TextBody::is_empty)
            && !matches!(sh.kind, ShapeKind::Picture(_) | ShapeKind::Table(_) | ShapeKind::Chart(_))
        });
        generic_match.map(|sh| (sh.id, sh.frame))
    }

    /// Put a new shape on the current slide, on top, and select it.
    pub(crate) fn insert_shape(&mut self, shape: Shape, cx: &mut Context<Self>) -> ShapeId {
        let locked = matches!(shape.kind, ShapeKind::Picture(_));
        self.insert_shape_with(shape, locked, cx)
    }

    /// Insert `shape`, keeping its proportions (縦横比を固定) if `locked`, as one step.
    pub(crate) fn insert_shape_with(&mut self, shape: Shape, locked: bool, cx: &mut Context<Self>) -> ShapeId {
        let slide = self.current_slide;
        let id = shape.id;
        let index = self.slide().shapes.len();
        let mut commands = vec![Command::InsertShape { slide, index, shape }];
        if locked {
            commands.push(Command::SetAspectLocked { slide, shape: id, locked: true });
        }
        self.run(commands, cx);
        self.set_selection(vec![id]);
        id
    }

    pub(crate) fn can_copy_shapes(shapes: &[Shape]) -> bool {
        !shapes.is_empty() && shapes.iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok())
    }

    fn extract_groups_for_shapes(groups: &[ibis_doc_model::ShapeGroup], shape_ids: &[ShapeId]) -> Vec<ibis_doc_model::ShapeGroup> {
        let set: std::collections::HashSet<ShapeId> = shape_ids.iter().copied().collect();
        let mut result = Vec::new();
        for g in groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(g, &mut leaves);
            if !leaves.is_empty() && leaves.iter().all(|id| set.contains(id)) {
                result.push(g.clone());
            }
        }
        result
    }

    fn map_group_hierarchy(
        group: &ibis_doc_model::ShapeGroup,
        id_map: &std::collections::HashMap<ShapeId, ShapeId>,
        used_native_ids: &mut std::collections::HashSet<u32>,
    ) -> ibis_doc_model::ShapeGroup {
        let mut next_native_id = 10000u32;
        while used_native_ids.contains(&next_native_id) {
            next_native_id += 1;
        }
        used_native_ids.insert(next_native_id);

        let new_children = group.children.iter().map(|c| match c {
            ibis_doc_model::GroupMember::Shape(id) => {
                ibis_doc_model::GroupMember::Shape(*id_map.get(id).unwrap_or(id))
            }
            ibis_doc_model::GroupMember::Group(inner) => {
                ibis_doc_model::GroupMember::Group(Box::new(Self::map_group_hierarchy(inner, id_map, used_native_ids)))
            }
        }).collect();

        ibis_doc_model::ShapeGroup {
            has_unmodelled_children: false,
            native_id: next_native_id.to_string(),
            name: group.name.clone(),
            children: new_children,
        }
    }

    /// All selected shapes have been checked before allocating ids or editing.
    fn copies_with_groups(
        &mut self,
        shapes: &[Shape],
        groups: &[ibis_doc_model::ShapeGroup],
        dx: i64,
    ) -> (Vec<Shape>, std::collections::HashMap<ShapeId, ShapeId>, Vec<ibis_doc_model::ShapeGroup>) {
        let number = self.presentation().slides.iter().position(|s| s.id == self.current_slide).unwrap_or(0) + self.presentation().first_slide_number;
        let mut id_map = std::collections::HashMap::new();
        let copies: Vec<Shape> = shapes.iter().map(|s| {
            let mut copy = s.clone();
            let new_id = self.document.ids().new_shape_id();
            id_map.insert(s.id, new_id);
            copy.id = new_id;
            copy.frame = copy.frame.translated(dx, dx);
            if let Some(body) = copy.text_body_mut() { body.set_slide_number(number); }
            if let ShapeKind::Table(table) = &mut copy.kind {
                for row in &mut table.rows { for cell in &mut row.cells { cell.text.set_slide_number(number); } }
            }
            copy
        }).collect();

        let mut used_ids = std::collections::HashSet::new();
        fn collect_ids(groups: &[ibis_doc_model::ShapeGroup], out: &mut std::collections::HashSet<u32>) {
            for g in groups {
                if let Ok(id) = g.native_id.parse::<u32>() {
                    out.insert(id);
                }
                for c in &g.children {
                    if let ibis_doc_model::GroupMember::Group(inner) = c {
                        collect_ids(std::slice::from_ref(inner.as_ref()), out);
                    }
                }
            }
        }
        collect_ids(&self.slide().groups, &mut used_ids);
        let copied_groups = groups.iter().map(|g| Self::map_group_hierarchy(g, &id_map, &mut used_ids)).collect();

        (copies, id_map, copied_groups)
    }

    fn insert_copies(
        &mut self,
        copies: Vec<Shape>,
        copied_groups: Vec<ibis_doc_model::ShapeGroup>,
        cx: &mut Context<Self>,
    ) {
        let slide = self.current_slide;
        let start = self.slide().shapes.len();
        let ids: Vec<ShapeId> = copies.iter().map(|s| s.id).collect();
        let mut commands: Vec<Command> = copies.into_iter().enumerate().map(|(i, shape)| Command::InsertShape { slide, index: start + i, shape }).collect();
        if !copied_groups.is_empty() {
            let mut new_groups = self.slide().groups.clone();
            new_groups.extend(copied_groups);
            commands.push(Command::SetShapeGroups { slide, groups: new_groups });
        }
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(ids);
        }
    }

    /// Option+ドラッグ: copies of the selection where it is, which are then selected (the drag moves them).
    pub(crate) fn duplicate_selected_in_place(&mut self, cx: &mut Context<Self>) -> bool {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) {
            return false;
        }
        let shape_ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let groups = Self::extract_groups_for_shapes(&self.slide().groups, &shape_ids);
        let (copies, _, copied_groups) = self.copies_with_groups(&shapes, &groups, 0);
        self.insert_copies(copies, copied_groups, cx);
        true
    }

    /// 複製（⌘D）: copies of the selection just below and to the right.
    pub(crate) fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        let shape_ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let groups = Self::extract_groups_for_shapes(&self.slide().groups, &shape_ids);
        let (copies, _, copied_groups) = self.copies_with_groups(&shapes, &groups, PASTE_OFFSET);
        self.insert_copies(copies, copied_groups, cx);
    }

    pub(crate) fn copy_selected(&mut self) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        let slide = self.current_slide;
        let shape_ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let groups = Self::extract_groups_for_shapes(&self.slide().groups, &shape_ids);
        if let Some(state) = self.pptx_mut()
            && !shapes.is_empty()
        {
            state.clipboard = shapes;
            state.clipboard_groups = groups;
            state.clipboard_slide = Some(slide);
            state.pastes = 0;
        }
    }

    pub(crate) fn cut_selected(&mut self, cx: &mut Context<Self>) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        self.copy_selected();
        self.write_shapes_to_system_clipboard(cx);
        self.stamp_shape_clipboard(cx);
        if let Some(state) = self.pptx_mut() {
            state.clipboard_slide = None;
        }
        self.delete_selected(cx);
    }

    /// Paste shapes: at their place on another slide, moved down and right
    /// on the slide they were copied from (further each time).
    pub(crate) fn paste_shapes(&mut self, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let Some(state) = self.pptx_mut() else { return false };
        if !Self::can_copy_shapes(&state.clipboard) {
            return false;
        }
        let same = state.clipboard_slide == Some(slide);
        if same {
            state.pastes += 1;
        }
        let dx = if same { PASTE_OFFSET * state.pastes as i64 } else { 0 };
        let shapes = state.clipboard.clone();
        let groups = state.clipboard_groups.clone();
        let (copies, _, copied_groups) = self.copies_with_groups(&shapes, &groups, dx);
        self.insert_copies(copies, copied_groups, cx);
        true
    }

    /// The shapes of the current slide as units: a shape with those
    /// attached to it right after it.
    fn units(&self) -> Vec<Vec<ShapeId>> {
        let slide = self.slide();
        let mut units: Vec<Vec<ShapeId>> = Vec::new();
        for s in &slide.shapes {
            match units.last_mut() {
                Some(unit) if slide.is_attached(s.id) => unit.push(s.id),
                _ => units.push(vec![s.id]),
            }
        }
        units
    }

    /// 順序: `order` is front, forward, backward or back.
    pub(crate) fn order_selected(&mut self, order: Order, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        if selected.is_empty() {
            return;
        }
        let mut units = self.units();
        let is_selected = |u: &Vec<ShapeId>| selected.contains(&u[0]);
        match order {
            Order::Front => {
                let (sel, rest): (Vec<_>, Vec<_>) = units.into_iter().partition(is_selected);
                units = rest.into_iter().chain(sel).collect();
            }
            Order::Back => {
                let (sel, rest): (Vec<_>, Vec<_>) = units.into_iter().partition(is_selected);
                units = sel.into_iter().chain(rest).collect();
            }
            Order::Forward => {
                for i in (0..units.len().saturating_sub(1)).rev() {
                    if is_selected(&units[i]) && !is_selected(&units[i + 1]) {
                        units.swap(i, i + 1);
                    }
                }
            }
            Order::Backward => {
                for i in 1..units.len() {
                    if is_selected(&units[i]) && !is_selected(&units[i - 1]) {
                        units.swap(i, i - 1);
                    }
                }
            }
        }
        // Move each unit to its place, front to back of the new order: the
        // units before it are already in place.
        let slide = self.current_slide;
        let mut commands = Vec::new();
        let mut working = self.units();
        for (i, unit) in units.iter().enumerate() {
            let at = working.iter().position(|u| u[0] == unit[0]).expect("a unit");
            if at != i {
                let index = working[..i].iter().map(Vec::len).sum();
                commands.push(Command::MoveShape { slide, shape: unit[0], index });
                let moved = working.remove(at);
                working.insert(i, moved);
            }
        }
        self.run(commands, cx);
    }

    /// 回転: turn the selection by `degrees` (clockwise).
    pub(crate) fn rotate_selected(&mut self, degrees: i32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let commands = self
            .selected_shapes()
            .into_iter()
            .map(|s| Command::SetTransform { slide, shape: s.id, rotation: s.rotation + degrees * 60_000, flip_h: s.flip_h, flip_v: s.flip_v })
            .collect();
        self.run(commands, cx);
    }

    pub(crate) fn flip_selected(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let commands = self
            .selected_shapes()
            .into_iter()
            .map(|s| Command::SetTransform {
                slide,
                shape: s.id,
                rotation: s.rotation,
                flip_h: s.flip_h ^ horizontal,
                flip_v: s.flip_v ^ !horizontal,
            })
            .collect();
        self.run(commands, cx);
    }

    /// 配置: line up the selection (one shape: on the slide).
    pub(crate) fn align_selected(&mut self, align: Align, cx: &mut Context<Self>) {
        let shapes: Vec<(ShapeId, EmuRect, EmuRect)> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, interaction::bounds(s))).collect();
        if shapes.is_empty() {
            return;
        }
        let area = if shapes.len() == 1 || self.pptx().is_some_and(|s| s.align_to_slide) {
            let size = self.presentation().size_of(self.slide());
            EmuRect::new(0, 0, size.width, size.height)
        } else {
            let l = shapes.iter().map(|s| s.2.x).min().unwrap_or(0);
            let t = shapes.iter().map(|s| s.2.y).min().unwrap_or(0);
            let r = shapes.iter().map(|s| s.2.right()).max().unwrap_or(0);
            let b = shapes.iter().map(|s| s.2.bottom()).max().unwrap_or(0);
            EmuRect::new(l, t, r - l, b - t)
        };
        let slide = self.current_slide;
        let commands = shapes
            .into_iter()
            .filter_map(|(id, frame, b)| {
                let (dx, dy) = match align {
                    Align::Left => (area.x - b.x, 0),
                    Align::Center => (area.x + area.width / 2 - (b.x + b.width / 2), 0),
                    Align::Right => (area.right() - b.right(), 0),
                    Align::Top => (0, area.y - b.y),
                    Align::Middle => (0, area.y + area.height / 2 - (b.y + b.height / 2)),
                    Align::Bottom => (0, area.bottom() - b.bottom()),
                };
                ((dx, dy) != (0, 0)).then(|| Command::SetFrame { slide, shape: id, frame: frame.translated(dx, dy) })
            })
            .collect();
        self.run(commands, cx);
    }

    /// ページの中央: the selection, as a whole, in the middle of the slide.
    pub(crate) fn center_selected(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let shapes: Vec<(ShapeId, EmuRect, EmuRect)> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, interaction::bounds(s))).collect();
        if shapes.is_empty() {
            return;
        }
        let size = self.presentation().size_of(self.slide());
        let l = shapes.iter().map(|s| s.2.x).min().unwrap_or(0);
        let t = shapes.iter().map(|s| s.2.y).min().unwrap_or(0);
        let r = shapes.iter().map(|s| s.2.right()).max().unwrap_or(0);
        let b = shapes.iter().map(|s| s.2.bottom()).max().unwrap_or(0);
        let (dx, dy) = if horizontal { ((size.width - (r - l)) / 2 - l, 0) } else { (0, (size.height - (b - t)) / 2 - t) };
        let slide = self.current_slide;
        let commands = shapes.into_iter().map(|(id, frame, _)| Command::SetFrame { slide, shape: id, frame: frame.translated(dx, dy) }).collect();
        self.run(commands, cx);
    }

    /// 均等に配置: equal gaps between three or more shapes.
    pub(crate) fn distribute_selected(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let mut shapes: Vec<(ShapeId, EmuRect, EmuRect)> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, interaction::bounds(s))).collect();
        if shapes.len() < 3 {
            return;
        }
        let key = |b: &EmuRect| if horizontal { b.x } else { b.y };
        let len = |b: &EmuRect| if horizontal { b.width } else { b.height };
        shapes.sort_by_key(|s| key(&s.2));
        let slide_size = self.presentation().size_of(self.slide());
        let to_slide = self.pptx().is_some_and(|s| s.align_to_slide);
        let (first, last) = if to_slide {
            (0, if horizontal { slide_size.width } else { slide_size.height })
        } else {
            let first = key(&shapes[0].2);
            (first, shapes.last().map(|s| key(&s.2) + len(&s.2)).unwrap_or(first))
        };
        let total: i64 = shapes.iter().map(|s| len(&s.2)).sum();
        let gap = (last - first - total) as f64 / (shapes.len() - 1) as f64;
        let slide = self.current_slide;
        let mut at = first as f64;
        let mut commands = Vec::new();
        for (id, frame, b) in shapes {
            let d = at.round() as i64 - key(&b);
            if d != 0 {
                commands.push(Command::SetFrame { slide, shape: id, frame: if horizontal { frame.translated(d, 0) } else { frame.translated(0, d) } });
            }
            at += len(&b) as f64 + gap;
        }
        self.run(commands, cx);
    }

    pub(crate) fn collect_group_leaves(group: &ibis_doc_model::ShapeGroup, out: &mut Vec<ShapeId>) {
        for child in &group.children {
            match child {
                ibis_doc_model::GroupMember::Shape(id) => out.push(*id),
                ibis_doc_model::GroupMember::Group(inner) => Self::collect_group_leaves(inner, out),
            }
        }
    }

    pub(crate) fn can_group(&self) -> bool {
        let selected = self.selected_ids();
        if selected.len() < 2 { return false; }
        let slide = self.slide();
        let mut top_elements = 0;
        let mut counted_leaves = std::collections::HashSet::new();
        for g in &slide.groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(g, &mut leaves);
            if leaves.iter().any(|id| selected.contains(id)) {
                top_elements += 1;
                counted_leaves.extend(leaves);
            }
        }
        for id in &selected {
            if !counted_leaves.contains(id) {
                top_elements += 1;
            }
        }
        top_elements >= 2
    }

    pub(crate) fn can_ungroup(&self) -> bool {
        let selected = self.selected_ids();
        if selected.is_empty() { return false; }
        let slide = self.slide();
        for g in &slide.groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(g, &mut leaves);
            if leaves.iter().any(|id| selected.contains(id)) {
                return true;
            }
        }
        false
    }

    /// グループ化: group the selected shapes into a new ShapeGroup.
    pub(crate) fn group_selected(&mut self, cx: &mut Context<Self>) {
        let selected_ids = self.selected_ids();
        if selected_ids.len() < 2 { return; }
        let Some(slide) = self.presentation().slide(self.current_slide).cloned() else { return };

        let mut children = Vec::new();
        let mut remaining_groups = Vec::new();
        let mut consumed_leaves = std::collections::HashSet::new();

        for g in &slide.groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(g, &mut leaves);
            if leaves.iter().any(|id| selected_ids.contains(id)) {
                children.push(ibis_doc_model::GroupMember::Group(Box::new(g.clone())));
                consumed_leaves.extend(leaves);
            } else {
                remaining_groups.push(g.clone());
            }
        }

        for shape in &slide.shapes {
            if selected_ids.contains(&shape.id) && !consumed_leaves.contains(&shape.id) {
                children.push(ibis_doc_model::GroupMember::Shape(shape.id));
                consumed_leaves.insert(shape.id);
            }
        }

        if children.len() < 2 { return; }

        children.sort_by_key(|member| match member {
            ibis_doc_model::GroupMember::Shape(id) => slide.shape_index(*id).unwrap_or(usize::MAX),
            ibis_doc_model::GroupMember::Group(g) => {
                let mut leaves = Vec::new();
                Self::collect_group_leaves(g, &mut leaves);
                leaves.iter().filter_map(|id| slide.shape_index(*id)).min().unwrap_or(usize::MAX)
            }
        });

        let mut used_ids = std::collections::HashSet::new();
        fn collect_group_ids(groups: &[ibis_doc_model::ShapeGroup], out: &mut std::collections::HashSet<u32>) {
            for g in groups {
                if let Ok(id) = g.native_id.parse::<u32>() {
                    out.insert(id);
                }
                for c in &g.children {
                    if let ibis_doc_model::GroupMember::Group(inner) = c {
                        collect_group_ids(std::slice::from_ref(inner.as_ref()), out);
                    }
                }
            }
        }
        collect_group_ids(&slide.groups, &mut used_ids);
        let mut next_native_id = 10000u32;
        while used_ids.contains(&next_native_id) {
            next_native_id += 1;
        }

        let mut max_group_num = 0;
        fn collect_names(groups: &[ibis_doc_model::ShapeGroup], max_num: &mut usize) {
            for g in groups {
                if let Some(num_str) = g.name.strip_prefix("グループ ").or_else(|| g.name.strip_prefix("Group ")) { // i18n-ok（保存されたグループ名の接頭辞（日本語・英語の両方を読む））
                    if let Ok(n) = num_str.parse::<usize>() {
                        *max_num = (*max_num).max(n);
                    }
                }
                for c in &g.children {
                    if let ibis_doc_model::GroupMember::Group(inner) = c {
                        collect_names(std::slice::from_ref(inner.as_ref()), max_num);
                    }
                }
            }
        }
        collect_names(&slide.groups, &mut max_group_num);
        let new_name = crate::shell::i18n::format("_pptx.group-n", &[("n", &(max_group_num + 1))]);

        let new_group = ibis_doc_model::ShapeGroup {
            has_unmodelled_children: false,
            native_id: next_native_id.to_string(),
            name: new_name,
            children,
        };

        let mut target_leaves = Vec::new();
        Self::collect_group_leaves(&new_group, &mut target_leaves);
        target_leaves.sort_by_key(|id| slide.shape_index(*id).unwrap_or(usize::MAX));

        let mut sim_shapes: Vec<ShapeId> = slide.shapes.iter().map(|s| s.id).collect();
        let min_idx = sim_shapes.iter().position(|id| target_leaves.contains(id)).unwrap();
        let mut commands = Vec::new();

        for (offset, &leaf_id) in target_leaves.iter().enumerate() {
            let desired_idx = min_idx + offset;
            let current_idx = sim_shapes.iter().position(|&id| id == leaf_id).unwrap();
            if current_idx != desired_idx {
                commands.push(Command::MoveShape {
                    slide: slide.id,
                    shape: leaf_id,
                    index: desired_idx,
                });
                sim_shapes.remove(current_idx);
                sim_shapes.insert(desired_idx, leaf_id);
            }
        }

        remaining_groups.push(new_group);
        commands.push(Command::SetShapeGroups {
            slide: slide.id,
            groups: remaining_groups,
        });

        let new_selection = target_leaves.clone();
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(new_selection);
        }
    }

    /// グループ解除: ungroup any selected ShapeGroup.
    pub(crate) fn ungroup_selected(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        if selected.is_empty() { return; }
        let Some(slide) = self.presentation().slide(self.current_slide).cloned() else { return };
        if slide.groups.is_empty() { return; }

        let mut updated_groups = Vec::new();
        let mut any_ungrouped = false;
        let mut new_selection = Vec::new();

        for g in slide.groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(&g, &mut leaves);
            if leaves.iter().any(|id| selected.contains(id)) {
                any_ungrouped = true;
                new_selection.extend(leaves);
                for child in g.children {
                    match child {
                        ibis_doc_model::GroupMember::Shape(_) => {}
                        ibis_doc_model::GroupMember::Group(inner) => {
                            updated_groups.push(*inner);
                        }
                    }
                }
            } else {
                updated_groups.push(g);
            }
        }

        if !any_ungrouped { return; }

        let commands = vec![Command::SetShapeGroups {
            slide: slide.id,
            groups: updated_groups,
        }];
        if self.apply_pptx_commands_checked(commands, cx) {
            self.set_selection(new_selection);
        }
    }

    /// 塗りつぶしの色.
    pub(crate) fn fill_selected(&mut self, fill: Fill, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        // セルの塗りつぶし: the cell range being edited, else every cell of a
        // selected table.
        if let [id] = self.selected_ids()[..]
            && let Some(ShapeKind::Table(table)) = self.slide().shape(id).map(|s| s.kind.clone())
        {
            let range = self.text_editing().filter(|e| e.shape == id).and_then(|e| e.cell.map(|at| {
                let other = e.cell_anchor.unwrap_or(at);
                (at.0.min(other.0), at.0.max(other.0), at.1.min(other.1), at.1.max(other.1))
            }));
            let mut table = (*table).clone();
            for (r, row) in table.rows.iter_mut().enumerate() {
                for (c, target) in row.cells.iter_mut().enumerate() {
                    if range.is_none_or(|(r0, r1, c0, c1)| (r0..=r1).contains(&r) && (c0..=c1).contains(&c)) {
                        target.fill = fill.clone();
                    }
                }
            }
            return self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
        }
        self.drop_untouched_body();
        let commands = self
            .selected_shapes()
            .into_iter()
            .filter(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            .map(|s| Command::SetFill { slide, shape: s.id, fill: fill.clone() })
            .collect();
        self.run(commands, cx);
    }

    /// 枠線（線）: change the outline of each selected shape with `f`.
    pub(crate) fn stroke_selected(&mut self, f: impl Fn(Option<Stroke>) -> Option<Stroke>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        // セルの罫線: every edge of the cells selected (the edited cell, the
        // range to the Shift+clicked one), else of every cell.
        if let [id] = self.selected_ids()[..]
            && let Some(ShapeKind::Table(table)) = self.slide().shape(id).map(|s| s.kind.clone())
        {
            let (cell, anchor) = self.text_editing().filter(|e| e.shape == id).map_or((None, None), |e| (e.cell, e.cell_anchor));
            let num_rows = table.rows.len();
            let num_cols = table.columns.len();
            let (r0, r1, c0, c1) = match cell {
                Some(a) => {
                    let b = anchor.unwrap_or(a);
                    (a.0.min(b.0), a.0.max(b.0).min(num_rows.saturating_sub(1)), a.1.min(b.1), a.1.max(b.1).min(num_cols.saturating_sub(1)))
                }
                None => (0, num_rows.saturating_sub(1), 0, num_cols.saturating_sub(1)),
            };
            let mut table = (*table).clone();
            let has_any_border = table.rows.iter().enumerate()
                .filter(|(r, _)| (r0..=r1).contains(r))
                .flat_map(|(_, row)| row.cells.iter().enumerate().filter(|(c, _)| (c0..=c1).contains(c)))
                .any(|(_, cell)| cell.borders.top.is_some() || cell.borders.bottom.is_some() || cell.borders.left.is_some() || cell.borders.right.is_some());

            for r in r0..=r1 {
                for c in c0..=c1 {
                    let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)) else { continue };
                    let b = &mut target.borders;
                    if has_any_border {
                        if b.left.is_some() { b.left = f(b.left.clone()); }
                        if b.right.is_some() { b.right = f(b.right.clone()); }
                        if b.top.is_some() { b.top = f(b.top.clone()); }
                        if b.bottom.is_some() { b.bottom = f(b.bottom.clone()); }
                    } else {
                        b.left = f(b.left.clone());
                        b.right = f(b.right.clone());
                        b.top = f(b.top.clone());
                        b.bottom = f(b.bottom.clone());
                    }
                }
            }
            if cell.is_some() {
                if r0 > 0 {
                    for c in c0..=c1 {
                        let top_border = table.rows.get(r0).and_then(|row| row.cells.get(c)).and_then(|cell| cell.borders.top.clone());
                        if let Some(target) = table.rows.get_mut(r0 - 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.bottom = top_border;
                        }
                    }
                }
                if r1 + 1 < num_rows {
                    for c in c0..=c1 {
                        let bottom_border = table.rows.get(r1).and_then(|row| row.cells.get(c)).and_then(|cell| cell.borders.bottom.clone());
                        if let Some(target) = table.rows.get_mut(r1 + 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.top = bottom_border;
                        }
                    }
                }
                if c0 > 0 {
                    for r in r0..=r1 {
                        let left_border = table.rows.get(r).and_then(|row| row.cells.get(c0)).and_then(|cell| cell.borders.left.clone());
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c0 - 1)) {
                            target.borders.right = left_border;
                        }
                    }
                }
                if c1 + 1 < num_cols {
                    for r in r0..=r1 {
                        let right_border = table.rows.get(r).and_then(|row| row.cells.get(c1)).and_then(|cell| cell.borders.right.clone());
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c1 + 1)) {
                            target.borders.left = right_border;
                        }
                    }
                }
            }
            return self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
        }
        let commands = self
            .selected_shapes()
            .into_iter()
            .filter_map(|s| {
                let old = match &s.kind {
                    ShapeKind::Auto(a) => a.stroke.clone(),
                    ShapeKind::Picture(p) => p.stroke.clone(),
                    ShapeKind::Line(l) => Some(l.clone()),
                    ShapeKind::TextBox(_) => None,
                    _ => return None,
                };
                let new = f(old.clone());
                if matches!(s.kind, ShapeKind::Line(_)) && new.is_none() {
                    return None;
                }
                (new != old).then(|| Command::SetStroke { slide, shape: s.id, stroke: new })
            })
            .collect();
        self.run(commands, cx);
    }

    /// 表の罫線の場所（外枠・内側・上下左右・すべて・なし）を設定
    pub(crate) fn apply_table_borders(&mut self, placement: BorderPlacement, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let [id] = self.selected_ids()[..] else { return };
        let Some(ShapeKind::Table(table)) = self.slide().shape(id).map(|s| s.kind.clone()) else { return };
        let (cell, anchor) = self.text_editing().filter(|e| e.shape == id).map_or((None, None), |e| (e.cell, e.cell_anchor));
        let num_rows = table.rows.len();
        let num_cols = table.columns.len();
        if num_rows == 0 || num_cols == 0 { return; }
        let (r0, r1, c0, c1) = match cell {
            Some(a) => {
                let b = anchor.unwrap_or(a);
                (a.0.min(b.0), a.0.max(b.0).min(num_rows - 1), a.1.min(b.1), a.1.max(b.1).min(num_cols - 1))
            }
            None => (0, num_rows - 1, 0, num_cols - 1),
        };

        let current_stroke = table.rows.iter().enumerate()
            .filter(|(r, _)| (r0..=r1).contains(r))
            .flat_map(|(_, row)| row.cells.iter().enumerate().filter(|(c, _)| (c0..=c1).contains(c)))
            .find_map(|(_, cell)| {
                cell.borders.top.as_ref()
                    .or(cell.borders.bottom.as_ref())
                    .or(cell.borders.left.as_ref())
                    .or(cell.borders.right.as_ref())
                    .cloned()
            })
            .or_else(|| {
                table.rows.iter().flat_map(|row| row.cells.iter()).find_map(|cell| {
                    cell.borders.top.as_ref()
                        .or(cell.borders.bottom.as_ref())
                        .or(cell.borders.left.as_ref())
                        .or(cell.borders.right.as_ref())
                        .cloned()
                })
            })
            .unwrap_or_else(|| Stroke::solid(Color::BLACK, 12_700));

        let mut table = (*table).clone();
        for r in r0..=r1 {
            for c in c0..=c1 {
                let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)) else { continue };
                let b = &mut target.borders;
                match placement {
                    BorderPlacement::All => {
                        b.left = Some(current_stroke.clone());
                        b.right = Some(current_stroke.clone());
                        b.top = Some(current_stroke.clone());
                        b.bottom = Some(current_stroke.clone());
                    }
                    BorderPlacement::None => {
                        b.left = None;
                        b.right = None;
                        b.top = None;
                        b.bottom = None;
                        b.tl_to_br = None;
                        b.bl_to_tr = None;
                    }
                    BorderPlacement::Outside => {
                        if r == r0 { b.top = Some(current_stroke.clone()); }
                        if r == r1 { b.bottom = Some(current_stroke.clone()); }
                        if c == c0 { b.left = Some(current_stroke.clone()); }
                        if c == c1 { b.right = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::Inside => {
                        if r > r0 { b.top = Some(current_stroke.clone()); }
                        if r < r1 { b.bottom = Some(current_stroke.clone()); }
                        if c > c0 { b.left = Some(current_stroke.clone()); }
                        if c < c1 { b.right = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::Top => {
                        if r == r0 { b.top = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::Bottom => {
                        if r == r1 { b.bottom = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::Left => {
                        if c == c0 { b.left = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::Right => {
                        if c == c1 { b.right = Some(current_stroke.clone()); }
                    }
                    BorderPlacement::DiagonalDown => {
                        b.tl_to_br = Some(current_stroke.clone());
                    }
                    BorderPlacement::DiagonalUp => {
                        b.bl_to_tr = Some(current_stroke.clone());
                    }
                }
            }
        }
        // Synchronize outer adjacent cells' opposing borders
        match placement {
            BorderPlacement::None => {
                if r0 > 0 {
                    for c in c0..=c1 {
                        if let Some(target) = table.rows.get_mut(r0 - 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.bottom = None;
                        }
                    }
                }
                if r1 + 1 < num_rows {
                    for c in c0..=c1 {
                        if let Some(target) = table.rows.get_mut(r1 + 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.top = None;
                        }
                    }
                }
                if c0 > 0 {
                    for r in r0..=r1 {
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c0 - 1)) {
                            target.borders.right = None;
                        }
                    }
                }
                if c1 + 1 < num_cols {
                    for r in r0..=r1 {
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c1 + 1)) {
                            target.borders.left = None;
                        }
                    }
                }
            }
            BorderPlacement::All | BorderPlacement::Outside | BorderPlacement::Top => {
                if r0 > 0 {
                    for c in c0..=c1 {
                        if let Some(target) = table.rows.get_mut(r0 - 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.bottom = Some(current_stroke.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        match placement {
            BorderPlacement::All | BorderPlacement::Outside | BorderPlacement::Bottom => {
                if r1 + 1 < num_rows {
                    for c in c0..=c1 {
                        if let Some(target) = table.rows.get_mut(r1 + 1).and_then(|row| row.cells.get_mut(c)) {
                            target.borders.top = Some(current_stroke.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        match placement {
            BorderPlacement::All | BorderPlacement::Outside | BorderPlacement::Left => {
                if c0 > 0 {
                    for r in r0..=r1 {
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c0 - 1)) {
                            target.borders.right = Some(current_stroke.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        match placement {
            BorderPlacement::All | BorderPlacement::Outside | BorderPlacement::Right => {
                if c1 + 1 < num_cols {
                    for r in r0..=r1 {
                        if let Some(target) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c1 + 1)) {
                            target.borders.left = Some(current_stroke.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
    }

    /// Change the text format of each selected shape's whole text.
    #[allow(dead_code)]
    pub(crate) fn text_style_selected(&mut self, window: &mut Window, cx: &mut Context<Self>, update: impl Fn(&mut TextStyle)) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let commands = self
            .selected_shapes()
            .into_iter()
            .filter_map(|s| {
                let body = s.text_body()?;
                let mut style = body.style.clone();
                update(&mut style);
                (style != body.style).then(|| Command::SetTextStyle { slide, shape: s.id, style })
            })
            .collect();
        self.run(commands, cx);
    }

    /// The characters a text format goes to: the text selected in the
    /// shape being edited (the edit goes on), else all of each selected
    /// shape's text. (shape, start, end) in characters.
    fn text_targets(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Vec<(ShapeId, usize, usize)> {
        if let Some(edit) = self.text_editing() {
            let range = edit.range();
            if !range.is_empty() {
                return vec![(edit.shape, range.start, range.end)];
            }
            // No selection: the whole text being edited.
            let shape = edit.shape;
            return self.edited_body().map(|b| vec![(shape, 0, b.text().chars().count())]).unwrap_or_default();
        }
        self.selected_shapes().into_iter().filter_map(|s| s.text_body().map(|b| (s.id, 0, b.text().chars().count()))).collect()
    }

    /// 太字・斜体・下線・文字の色 and the like.
    /// フォント サイズの拡大・縮小 (A▲・A▼, ⌘⇧> and ⌘⇧<): the next size of the PowerPoint's list from
    /// that of the selected text (the caret's), for the selected text, or all the text of the shapes selected.
    pub(crate) fn step_font_size_of_text(&mut self, larger: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(style) = self.selected_text_style() else { return };
        let size = ibis_doc_model::step_font_size(style.font_size, larger);
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat { font_size: Some(size), ..Default::default() });
    }

    pub(crate) fn format_characters(&mut self, window: &mut Window, cx: &mut Context<Self>, format: ibis_doc_model::CharacterFormat) {
        let slide = self.current_slide;
        let commands = self
            .text_targets(window, cx)
            .into_iter()
            .map(|(shape, start, end)| Command::SetCharacterFormat { slide, shape, start, end, format: format.clone() })
            .collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    /// 太字 and the like as toggles: on unless the targeted text already
    /// starts with it on (as PowerPoint toggles).
    pub(crate) fn toggle_characters(&mut self, window: &mut Window, cx: &mut Context<Self>, is_on: fn(&TextStyle, &ibis_doc_model::RunStyle) -> bool, format: fn(bool) -> ibis_doc_model::CharacterFormat) {
        let targets = self.text_targets(window, cx);
        let slide = self.current_slide;
        let on = targets.first().and_then(|&(shape, start, _)| {
            let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
            let mut at = 0;
            for p in &body.paragraphs {
                if p.runs.is_empty() && start == at {
                    return Some(is_on(&body.style, &p.end_style));
                }
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if start < at + n || (n > 0 && start == at && at == 0) {
                        return Some(is_on(&body.style, &r.style));
                    }
                    at += n;
                }
                at += 1;
            }
            body.paragraphs.last().map(|p| is_on(&body.style, &p.end_style))
        });
        let format = format(!on.unwrap_or(false));
        let commands = targets.into_iter().map(|(shape, start, end)| Command::SetCharacterFormat { slide, shape, start, end, format: format.clone() }).collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    /// 取り消し線: a single line through the selected characters, or none
    /// when the first of them has one (single or double).
    pub(crate) fn strike_characters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use ibis_doc_model::{CharacterFormat, Strike};
        self.toggle_characters(window, cx, |_, r| r.strike != Strike::None, |on| CharacterFormat { strike: Some(if on { Strike::Single } else { Strike::None }), ..Default::default() });
    }

    pub(crate) fn default_text_shadow() -> ibis_doc_model::Shadow {
        ibis_doc_model::Shadow {
            color: ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 102 },
            blur: 25_400,
            distance: 19_050,
            direction: 5_400_000,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: ibis_doc_model::TileAlign::TopLeft,
            rot_with_shape: true,
        }
    }

    pub(crate) fn toggle_text_shadow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use ibis_doc_model::CharacterFormat;
        self.toggle_characters(
            window,
            cx,
            |_, r| r.shadow.is_some(),
            |on| CharacterFormat {
                shadow: Some(if on { Some(Self::default_text_shadow()) } else { None }),
                ..Default::default()
            },
        );
    }

    pub(crate) fn set_text_shadow(&mut self, shadow: Option<ibis_doc_model::Shadow>, window: &mut Window, cx: &mut Context<Self>) {
        self.format_characters(
            window,
            cx,
            ibis_doc_model::CharacterFormat {
                shadow: Some(shadow),
                ..Default::default()
            },
        );
    }

    pub(crate) fn set_text_outline_color(&mut self, window: &mut Window, cx: &mut Context<Self>, color: Option<ibis_doc_model::Rgb>) {
        let Some(color) = color else {
            self.format_characters(window, cx, ibis_doc_model::CharacterFormat { outline: Some(None), ..Default::default() });
            return;
        };
        let targets = self.text_targets(window, cx);
        let width = targets.first().and_then(|&(shape, start, _)| {
            let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
            let mut at = 0;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if start < at + n || (n > 0 && start == at && at == 0) {
                        if let Some((_, w)) = r.style.outline {
                            return Some(w);
                        }
                    }
                    at += n;
                }
                at += 1;
            }
            None
        }).unwrap_or(12_700);
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat {
            outline: Some(Some((color, width))),
            ..Default::default()
        });
    }

    pub(crate) fn set_text_outline_weight(&mut self, window: &mut Window, cx: &mut Context<Self>, width: i64) {
        let targets = self.text_targets(window, cx);
        let color = targets.first().and_then(|&(shape, start, _)| {
            let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
            let mut at = 0;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if start < at + n || (n > 0 && start == at && at == 0) {
                        if let Some((c, _)) = r.style.outline {
                            return Some(c);
                        }
                        return Some(r.style.color.unwrap_or(body.style.color));
                    }
                    at += n;
                }
                at += 1;
            }
            None
        }).unwrap_or(ibis_doc_model::Rgb(0));
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat {
            outline: Some(Some((color, width))),
            ..Default::default()
        });
    }

    /// Text frame settings do not change character or paragraph formats.
    pub(crate) fn set_text_font_scale(&mut self, scale:u32,cx:&mut Context<Self>) {
        if !(1_000..=100_000).contains(&scale) {return;}
        let targets=if let Some(edit)=self.text_editing() {vec![edit.shape]} else {self.selected_ids().to_vec()};
        let commands=targets.into_iter().filter_map(|shape| {
            let body=if self.text_editing().is_some_and(|e|e.shape==shape) {self.edited_body()?} else {self.slide().shape(shape)?.text_body()?};
            if body.font_scale==scale {return None;}let mut body=body.clone();body.font_scale=scale;Some(Command::SetTextBody {slide:self.current_slide,shape,body})
        }).collect();
        let commands=self.in_edited_cell(commands);self.apply_pptx_commands_checked(commands,cx);
    }

    pub(crate) fn set_text_autofit(&mut self, autofit: ibis_doc_model::TextAutofit, cx: &mut Context<Self>) {
        self.format_text_body(|body| {
            body.autofit = autofit;
            if autofit != ibis_doc_model::TextAutofit::Shrink {
                body.font_scale = 100_000;
            }
        }, cx);
    }

    pub(crate) fn set_text_columns(&mut self, columns: u32, spacing: i64, cx: &mut Context<Self>) {
        let columns = columns.clamp(1, 16);
        let spacing = spacing.max(0);
        self.format_text_body(|body| {
            body.columns = columns;
            body.column_spacing = spacing;
        }, cx);
    }

    pub(crate) fn format_text_body(&mut self, update: impl Fn(&mut TextBody), cx: &mut Context<Self>) {
        let mut commands = Vec::new();
        let slide = self.current_slide;
        if let Some(edit) = self.text_editing() {
            let shape = edit.shape;
            if let Some((r, c)) = edit.cell {
                let (ar, ac) = edit.cell_anchor.unwrap_or((r, c));
                let (r0, r1) = (r.min(ar), r.max(ar));
                let (c0, c1) = (c.min(ac), c.max(ac));
                if let Some(ShapeKind::Table(t)) = self.slide().shape(shape).map(|s| &s.kind) {
                    for row in r0..=r1 {
                        for column in c0..=c1 {
                            if let Some(cell) = t.rows.get(row).and_then(|r_obj| r_obj.cells.get(column)) {
                                let mut changed = cell.text.clone();
                                update(&mut changed);
                                if changed != cell.text {
                                    commands.push(Command::InCell {
                                        row,
                                        column,
                                        command: Box::new(Command::SetTextBody { slide, shape, body: changed }),
                                    });
                                }
                            }
                        }
                    }
                }
            } else if let Some(body) = self.edited_body() {
                let mut changed = body.clone();
                update(&mut changed);
                if changed != *body {
                    commands.push(Command::SetTextBody { slide, shape, body: changed });
                }
            }
        } else {
            for shape in self.selected_ids().to_vec() {
                let Some(s) = self.slide().shape(shape) else { continue };
                if let ShapeKind::Table(t) = &s.kind {
                    for (row, r_obj) in t.rows.iter().enumerate() {
                        for (column, cell) in r_obj.cells.iter().enumerate() {
                            let mut changed = cell.text.clone();
                            update(&mut changed);
                            if changed != cell.text {
                                commands.push(Command::InCell {
                                    row,
                                    column,
                                    command: Box::new(Command::SetTextBody { slide, shape, body: changed }),
                                });
                            }
                        }
                    }
                } else if let Some(body) = s.text_body() {
                    let mut changed = body.clone();
                    update(&mut changed);
                    if changed != *body {
                        commands.push(Command::SetTextBody { slide, shape, body: changed });
                    }
                }
            }
        }
        self.run(commands, cx);
    }

    /// 文字の変形 (`a:prstTxWarp`) を設定・解除する。
    pub(crate) fn set_text_warp(&mut self, warp: Option<&str>, cx: &mut Context<Self>) {
        let warp_owned = warp.map(str::to_string);
        self.format_text_body(|body| {
            body.warp = warp_owned.clone();
        }, cx);
    }

    /// 選択中のテキストボックス・図形の文字変形プリセット名を取得する。
    pub(crate) fn selected_text_warp(&self) -> Option<String> {
        let targets = if let Some(edit) = self.text_editing() { vec![edit.shape] } else { self.selected_ids().to_vec() };
        let shape = *targets.first()?;
        let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
        body.warp.clone()
    }

    /// ワードアート (WordArt) をスライド中央に挿入する。
    pub(crate) fn insert_wordart(&mut self, style_idx: usize, text: Option<&str>, cx: &mut Context<Self>) -> ShapeId {
        let preset = WORDART_PRESETS.get(style_idx).copied().unwrap_or(WORDART_PRESETS[0]);
        let id = self.document.ids().new_shape_id();
        let size = self.presentation().size_of(self.slide());
        let width = (5_080_000i64).min(size.width * 8 / 10);
        let height = 1_270_000i64;
        let frame = EmuRect::new((size.width - width) / 2, (size.height - height) / 2, width, height);

        let default_text = crate::shell::i18n::text("_pptx.wordart-default-text");
        let initial_text = text.unwrap_or(&default_text);
        let mut style = TextStyle::default().with_font_size(3600);
        style.color = preset.color;
        style.bold = preset.bold;

        let shadow = preset.shadow.map(|(dir, dist, blur)| ibis_doc_model::Shadow {
            color: Color { rgb: Rgb(0), alpha: 102 },
            blur,
            distance: dist,
            direction: dir,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: ibis_doc_model::TileAlign::TopLeft,
            rot_with_shape: true,
        });

        let run_style = ibis_doc_model::RunStyle {
            font_size: Some(3600),
            bold: Some(preset.bold),
            color: Some(preset.color),
            outline: preset.outline,
            shadow,
            ..Default::default()
        };

        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![ibis_doc_model::Paragraph {
            runs: vec![ibis_doc_model::Run {
                text: initial_text.to_string(),
                style: run_style,
            }],
            ..Default::default()
        }];

        let shape = Shape::new(id, frame, ShapeKind::TextBox(body));
        self.insert_shape(shape, cx);
        id
    }

    /// 選択中のテキストにワードアートスタイルを適用する。
    pub(crate) fn set_selected_text_wordart_style(&mut self, style_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let preset = WORDART_PRESETS.get(style_idx).copied().unwrap_or(WORDART_PRESETS[0]);
        let shadow = preset.shadow.map(|(dir, dist, blur)| ibis_doc_model::Shadow {
            color: Color { rgb: Rgb(0), alpha: 102 },
            blur,
            distance: dist,
            direction: dir,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: ibis_doc_model::TileAlign::TopLeft,
            rot_with_shape: true,
        });

        self.format_characters(window, cx, ibis_doc_model::CharacterFormat {
            color: Some(preset.color),
            bold: Some(preset.bold),
            outline: Some(preset.outline),
            shadow: Some(shadow),
            ..Default::default()
        });
    }

    /// Copy the first targeted character's resolved, editable formatting.
    pub(crate) fn copy_character_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.text_targets(window, cx);
        let Some(&(shape, start, _)) = targets.first() else { return };
        let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body() } else { self.slide().shape(shape).and_then(|s| s.text_body()) };
        let Some(body) = body else { return };
        let mut at = 0;
        let mut selected = None;
        let mut east_asian = false;
        for paragraph in &body.paragraphs {
            for run in &paragraph.runs {
                let end = at + run.text.chars().count();
                if start < end { selected = Some(run.style.clone()); east_asian = run.text.chars().any(ibis_doc_model::is_east_asian); break; }
                at = end;
            }
            if selected.is_some() { break; }
            if start == at { selected = Some(paragraph.end_style.clone()); break; }
            at += 1;
        }
        let run = selected.unwrap_or_default();
        let look = body.style.with_run(&run);
        let format = ibis_doc_model::CharacterFormat {
            font_size: Some(look.font_size), bold: Some(look.bold), italic: Some(look.italic),
            underline: Some(run.underline), underline_kind: Some(run.underline_kind), color: Some(look.color), font: None,
            script_fonts: Some(if east_asian { (look.fallback_font, look.font) } else { (look.font, look.fallback_font) }),
            highlight: Some(run.highlight), outline: Some(run.outline), baseline: Some(run.baseline), spacing: Some(run.spacing),
            strike: Some(run.strike), caps: Some(run.caps), shadow: Some(run.shadow), link: None, link_tooltip: None,
            lang: Some(run.lang.clone()),
        };
        if let Some(state) = self.pptx_mut() { state.character_format_copy = Some(format); }
        cx.notify();
    }

    pub(crate) fn paste_character_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(format) = self.pptx().and_then(|s| s.character_format_copy.clone()) {
            self.format_characters(window, cx, format);
        }
    }

    pub(crate) fn copy_paragraph_format(&mut self, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let Some((body, mut index, _)) = self.selected_text_position() else { return };
        while index > 0 && body.paragraphs[index].line_break { index -= 1; }
        let mut format = body.paragraphs[index].clone();
        format.align = Some(format.align.unwrap_or(body.style.align));
        let mut last = index;
        while last + 1 < body.paragraphs.len() && body.paragraphs[last + 1].line_break { last += 1; }
        format.space_after = body.paragraphs[last].space_after;
        format.runs.clear(); format.end_style = Default::default(); format.line_break = false;
        if let Some(state) = self.pptx_mut() { state.paragraph_format_copy = Some(format); }
        cx.notify();
    }

    pub(crate) fn paste_paragraph_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let Some(format) = self.pptx().and_then(|s| s.paragraph_format_copy.clone()) else { return };
        let targets = if let Some(edit) = self.text_editing() {
            let range = edit.range(); vec![(edit.shape, range.start, range.end)]
        } else { self.text_targets(window, cx) };
        let commands = targets.into_iter().filter_map(|(shape, start, end)| {
            let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
            if body.paragraphs.is_empty() { return None; }
            let mut changed = body.clone(); let range = changed.paragraphs_in(start, end);
            let (mut first, mut last) = (*range.start(), *range.end());
            while first > 0 && changed.paragraphs[first].line_break { first -= 1; }
            while last + 1 < changed.paragraphs.len() && changed.paragraphs[last + 1].line_break { last += 1; }
            for paragraph in &mut changed.paragraphs[first..=last] {
                let (runs, end_style, line_break) = (std::mem::take(&mut paragraph.runs), paragraph.end_style.clone(), paragraph.line_break);
                *paragraph = ibis_doc_model::Paragraph { runs, end_style, line_break, ..format.clone() };
            }
            changed.unify_lines();
            (changed != *body).then_some(Command::SetTextBody { slide: self.current_slide, shape, body: changed })
        }).collect();
        let commands = self.in_edited_cell(commands); self.run(commands, cx);
    }

    /// 書式をクリア: the targeted text loses its own character formats.
    pub(crate) fn clear_formats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let whole_shape = self.text_editing().is_none();
        let targets = self.text_targets(window, cx);
        let commands = targets
            .into_iter()
            .filter_map(|(shape, start, end)| {
                let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
                let mut body = body.clone();
                if whole_shape {
                    body.style = TextStyle { align: body.style.align, ..TextStyle::default() };
                    body.warp = None;
                    body.warp_adjust.clear();
                } else {
                    let base = body.style.clone();
                    let materialize = |r: &mut ibis_doc_model::RunStyle| {
                        r.font_size.get_or_insert(base.font_size);
                        r.bold.get_or_insert(base.bold);
                        r.italic.get_or_insert(base.italic);
                        r.color.get_or_insert(base.color);
                        if r.font.is_none() {
                            r.font = base.font.clone();
                        }
                        if r.fallback_font.is_none() {
                            r.fallback_font = base.fallback_font.clone();
                        }
                    };
                    for p in &mut body.paragraphs {
                        for run in &mut p.runs {
                            materialize(&mut run.style);
                        }
                        materialize(&mut p.end_style);
                    }
                    body.style = TextStyle { align: body.style.align, ..TextStyle::default() };
                }
                body.clear_run_formats(start, end);
                Some(Command::SetTextBody { slide, shape, body })
            })
            .collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    /// 上付き・下付き as toggles: `raised` is the baseline (30% up, 25% down).
    pub(crate) fn toggle_baseline(&mut self, raised: i32, window: &mut Window, cx: &mut Context<Self>) {
        let on = self.selected_baseline() == raised;
        let format = ibis_doc_model::CharacterFormat { baseline: Some(if on { 0 } else { raised }), ..Default::default() };
        self.format_characters(window, cx, format);
    }

    /// The baseline of the targeted text's first run (0 for none).
    pub(crate) fn selected_baseline(&self) -> i32 {
        let body = self.edited_body().or_else(|| self.selected_shapes().into_iter().find_map(|s| s.text_body()));
        let Some(body) = body else { return 0 };
        let start = self.text_editing().map_or(0, |e| e.range().start);
        let mut at = 0;
        for p in &body.paragraphs {
            for r in &p.runs {
                let n = r.text.chars().count();
                if start < at + n || (start == at && n > 0) {
                    return r.style.baseline;
                }
                at += n;
            }
            at += 1;
        }
        body.paragraphs.first().map_or(0, |p| p.end_style.baseline)
    }

    /// Set character spacing for targeted text (hundredths of a point; negative tightens, positive loosens).
    pub(crate) fn set_slide_character_spacing(&mut self, spacing: i32, window: &mut Window, cx: &mut Context<Self>) {
        let format = ibis_doc_model::CharacterFormat { spacing: Some(spacing), ..Default::default() };
        self.format_characters(window, cx, format);
    }

    /// The character spacing of the targeted text's first run (0 for normal).
    pub(crate) fn selected_spacing(&self) -> i32 {
        let body = self.edited_body().or_else(|| self.selected_shapes().into_iter().find_map(|s| s.text_body()));
        let Some(body) = body else { return 0 };
        let start = self.text_editing().map_or(0, |e| e.range().start);
        let mut at = 0;
        for p in &body.paragraphs {
            for r in &p.runs {
                let n = r.text.chars().count();
                if start < at + n || (start == at && n > 0) {
                    return r.style.spacing;
                }
                at += n;
            }
            at += 1;
        }
        body.paragraphs.first().map_or(0, |p| p.end_style.spacing)
    }

    /// Toggle all-caps or small-caps for the targeted text.
    pub(crate) fn toggle_text_caps(&mut self, target: Caps, window: &mut Window, cx: &mut Context<Self>) {
        let on = self.selected_caps() == target;
        let format = ibis_doc_model::CharacterFormat { caps: Some(if on { Caps::None } else { target }), ..Default::default() };
        self.format_characters(window, cx, format);
    }

    /// The capitals format of the targeted text's first run (`Caps::None` if none).
    pub(crate) fn selected_caps(&self) -> Caps {
        let body = self.edited_body().or_else(|| self.selected_shapes().into_iter().find_map(|s| s.text_body()));
        let Some(body) = body else { return Caps::None };
        let start = self.text_editing().map_or(0, |e| e.range().start);
        let mut at = 0;
        for p in &body.paragraphs {
            for r in &p.runs {
                let n = r.text.chars().count();
                if start < at + n || (start == at && n > 0) {
                    return r.style.caps;
                }
                at += n;
            }
            at += 1;
        }
        body.paragraphs.first().map_or(Caps::None, |p| p.end_style.caps)
    }

    /// The shadow of the targeted text's first run.
    pub(crate) fn selected_shadow(&self) -> Option<ibis_doc_model::Shadow> {
        let body = self.edited_body().or_else(|| self.selected_shapes().into_iter().find_map(|s| s.text_body()))?;
        let start = self.text_editing().map_or(0, |e| e.range().start);
        let mut at = 0;
        for p in &body.paragraphs {
            for r in &p.runs {
                let n = r.text.chars().count();
                if start < at + n || (start == at && n > 0) {
                    return r.style.shadow;
                }
                at += n;
            }
            at += 1;
        }
        body.paragraphs.first().and_then(|p| p.end_style.shadow)
    }

    /// Change the letter case of the targeted text.
    pub(crate) fn change_case(&mut self, case: TextCase, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.text_targets(window, cx);
        let slide = self.current_slide;
        let commands = targets
            .into_iter()
            .filter_map(|(shape, start, end)| {
                let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
                let mut changed = body.clone();
                apply_text_case(&mut changed, start, end, case);
                (changed != *body).then_some(Command::SetTextBody { slide, shape, body: changed })
            })
            .collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    /// 配置・行間・箇条書き・インデント, for the paragraphs of the selection.
    pub(crate) fn format_paragraphs(&mut self, window: &mut Window, cx: &mut Context<Self>, format: ibis_doc_model::ParagraphFormat) {
        let slide = self.current_slide;
        let targets = if let Some(edit) = self.text_editing() {
            let range = edit.range();
            vec![(edit.shape, range.start, range.end)]
        } else { self.text_targets(window, cx) };
        let commands = targets
            .into_iter()
            .map(|(shape, start, end)| Command::SetParagraphFormat { slide, shape, start, end, format: format.clone() })
            .collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    /// 段落のインデント（左余白 margin_left、最初の行のインデント indent）を設定
    pub(crate) fn set_paragraph_indent(&mut self, margin_left: Option<i64>, indent: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        self.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat {
            margin_left,
            indent,
            ..Default::default()
        });
    }

    /// 段落のタブ位置を設定
    pub(crate) fn set_paragraph_tabs(&mut self, tabs: Vec<ibis_doc_model::TextTabStop>, window: &mut Window, cx: &mut Context<Self>) {
        self.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat {
            tabs: Some(tabs),
            ..Default::default()
        });
    }

    /// The first selected character, or the caret's paragraph and local offset.
    fn selected_text_position(&self) -> Option<(&TextBody, usize, usize)> {
        let body = self.edited_body().or_else(|| self.selected_shapes().into_iter().find_map(|s| s.text_body()))?;
        let mut offset = self.text_editing().map(|e| e.range().start).unwrap_or(0);
        for (index, paragraph) in body.paragraphs.iter().enumerate() {
            let len = paragraph.runs.iter().map(|r| r.text.chars().count()).sum::<usize>();
            if offset <= len { return Some((body, index, offset)); }
            offset -= len + 1;
        }
        let index = body.paragraphs.len().checked_sub(1)?;
        Some((body, index, body.paragraphs[index].text().chars().count()))
    }

    /// 既定の図形に設定 / 既定の線に設定: the look of the selected shape (or
    /// 既定の図形に設定 / 既定の線に設定: the look of the selected shape (or
    /// line) is what the next ones drawn get. Saved in the file as `<a:objectDefaults>`.
    pub(crate) fn set_object_default(&mut self, cx: &mut Context<Self>) {
        let Some(shape) = self.selected_shapes().into_iter().next().cloned() else { return };
        let defaults = match &shape.kind {
            ShapeKind::Auto(a) if is_connector_geometry(&a.geometry) => {
                Some(ObjectDefaults { shape: None, line: a.stroke.clone() })
            }
            ShapeKind::Auto(a) => {
                let text = a.text.as_ref().map(|t| t.style.clone()).unwrap_or_default();
                Some(ObjectDefaults { shape: Some(ShapeDefault { fill: a.fill.clone(), stroke: a.stroke.clone(), text, shadow: a.shadow, glow: a.glow, reflection: a.reflection, soft_edge: a.soft_edge }), line: None })
            }
            ShapeKind::Line(stroke) => Some(ObjectDefaults { shape: None, line: Some(stroke.clone()) }),
            _ => None,
        };
        let Some(new) = defaults else { return };
        let mut merged = self.presentation().object_defaults.clone();
        if new.shape.is_some() { merged.shape = new.shape; }
        if new.line.is_some() { merged.line = new.line; }
        if let Some(state) = self.pptx_mut() {
            state.defaults = merged.clone();
        }
        self.run(vec![Command::SetObjectDefaults { defaults: merged }], cx);
        self.set_status(crate::shell::i18n::text("pptx.editing.set-default-shape-line").to_string(), cx);
    }

    /// The defaults set, put on a shape just made.
    pub(crate) fn with_defaults(&self, mut shape: Shape) -> Shape {
        let defaults = &self.presentation().object_defaults;
        match &mut shape.kind {
            ShapeKind::Auto(a) if a.text.is_some() => {
                if let Some(d) = &defaults.shape {
                    a.fill = d.fill.clone();
                    a.stroke = d.stroke.clone();
                    a.shadow = d.shadow;
                    a.glow = d.glow;
                    a.reflection = d.reflection;
                    a.soft_edge = d.soft_edge;
                    if let Some(t) = a.text.as_mut() { t.style = d.text.clone(); }
                }
            }
            ShapeKind::Auto(a) if is_connector_geometry(&a.geometry) => {
                if let Some(d) = &defaults.line {
                    if let Some(s) = &mut a.stroke {
                        let (head, tail) = (s.head, s.tail);
                        *s = d.clone();
                        s.head = head;
                        s.tail = tail;
                    }
                }
            }
            ShapeKind::Line(stroke) => {
                if let Some(d) = &defaults.line {
                    let (head, tail) = (stroke.head.clone(), stroke.tail.clone());
                    *stroke = d.clone();
                    // The arrow ends the tool drew stay as drawn.
                    stroke.head = head;
                    stroke.tail = tail;
                }
            }
            _ => {}
        }
        shape
    }

    /// The list of the active logical paragraph (soft breaks share its list).
    pub(crate) fn selected_list(&self) -> Option<ibis_doc_model::ListKind> {
        let (body, mut index, _) = self.selected_text_position()?;
        while index > 0 && body.paragraphs[index].line_break { index -= 1; }
        Some(ibis_doc_model::ListKind::of(body.paragraphs[index].bullet.as_ref()))
    }

    /// The active character's resolved style and paragraph alignment.
    pub(crate) fn selected_text_style(&self) -> Option<TextStyle> {
        let (body, index, offset) = self.selected_text_position()?;
        let p = &body.paragraphs[index];
        let mut at = 0;
        let run = p.runs.iter().find(|r| {
            at += r.text.chars().count();
            offset < at
        }).or_else(|| p.runs.iter().rev().find(|r| !r.text.is_empty()));
        let style = run.map(|r| &r.style).unwrap_or(&p.end_style);
        Some(TextStyle { align: p.align.unwrap_or(body.style.align), ..body.style.with_run(style) })
    }

    /// The active character's run style (`underline`, `strike`, etc.).
    pub(crate) fn selected_text_run(&self) -> Option<ibis_doc_model::RunStyle> {
        let (body, index, offset) = self.selected_text_position()?;
        let p = &body.paragraphs[index];
        let mut at = 0;
        let run = p.runs.iter().find(|r| {
            at += r.text.chars().count();
            offset < at
        }).or_else(|| p.runs.iter().rev().find(|r| !r.text.is_empty()));
        Some(run.map(|r| r.style.clone()).unwrap_or_else(|| p.end_style.clone()))
    }

    /// The active character's proofing language (a:rPr/@lang), if set.
    pub(crate) fn selected_language(&self) -> Option<String> {
        let (body, index, offset) = self.selected_text_position()?;
        let p = &body.paragraphs.get(index)?;
        let mut at = 0;
        let run = p.runs.iter().find(|r| {
            at += r.text.chars().count();
            offset < at
        }).or_else(|| p.runs.iter().rev().find(|r| !r.text.is_empty()));
        let style = run.map(|r| &r.style).unwrap_or(&p.end_style);
        style.lang.clone()
    }

    /// Set or clear the proofing language for the selected text runs.
    pub(crate) fn set_selection_language(&mut self, lang: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat {
            lang: Some(lang.map(str::to_string)),
            ..Default::default()
        });
    }

    // ----- Pictures -------------------------------------------------------

    /// 画像を挿入 > ファイルから: choose a picture and put it on the slide.
    pub(crate) fn pick_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_picture_file(false, window, cx);
    }

    pub(crate) fn pick_replacement_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_picture_file(true, window, cx);
    }

    fn pick_picture_file(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        // Freeze the targets before the asynchronous file dialog.
        let target = replace.then(|| (self.current_slide, self.selected_ids().to_vec()));
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| this.receive_picture_file(target, &name, data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| this.receive_picture_file(target, &name, data, cx)).ok();
        })
        .detach();
    }

    pub(crate) fn receive_picture_file(&mut self, target: Option<(SlideId, Vec<ShapeId>)>, name: &str, data: Vec<u8>, cx: &mut Context<Self>) {
        let Some((slide, targets)) = target else { self.insert_picture(name, data, cx); return };
        let format = ibis_doc_model::MediaFormat::from_path(name);
        if validated_picture_size(format, &data).is_none() {
            log::warn!("Cannot replace the picture: {name}");
            return;
        }
        let pictures: Vec<_> = targets.into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &self.presentation().slide(slide)?.shape(shape)?.kind else { return None };
            Some((shape, picture.clone()))
        }).collect();
        if pictures.is_empty() { return; }
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let commands = pictures.into_iter().map(|(shape, mut picture)| {
            picture.media = media;
            Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(picture)) }
        }).collect();
        self.run(commands, cx);
    }

    /// Put picture file `data` (named `name`) on the current slide: at its
    /// size at 96 dpi, made smaller to fit the slide, in the middle.
    pub(crate) fn insert_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ShapeId> {
        let format = ibis_doc_model::MediaFormat::from_path(name);
        let Some((w, h)) = validated_picture_size(format, &data) else {
            log::warn!("{name}: invalid or unsupported picture");
            return None;
        };
        let img_aspect = w as f64 / h as f64;
        let placeholder = self.empty_content_placeholder_for("pic");
        let (replaced, frame) = match placeholder {
            Some((old_id, ph_frame)) => {
                let target_w = ph_frame.width;
                let target_h = (target_w as f64 / img_aspect) as i64;
                let (cw, ch) = if target_h <= ph_frame.height {
                    (target_w, target_h)
                } else {
                    let ch = ph_frame.height;
                    ((ch as f64 * img_aspect) as i64, ch)
                };
                let x = ph_frame.x + (ph_frame.width - cw) / 2;
                let y = ph_frame.y + (ph_frame.height - ch) / 2;
                (Some(old_id), EmuRect::new(x, y, cw, ch))
            }
            None => {
                let size = self.presentation().size_of(self.slide());
                // 96 px an inch; no bigger than the slide.
                let (mut cx_emu, mut cy_emu) = (w as i64 * 9_525, h as i64 * 9_525);
                let k = (size.width as f64 / cx_emu as f64).min(size.height as f64 / cy_emu as f64).min(1.0);
                cx_emu = (cx_emu as f64 * k) as i64;
                cy_emu = (cy_emu as f64 * k) as i64;
                (None, EmuRect::new((size.width - cx_emu) / 2, (size.height - cy_emu) / 2, cx_emu, cy_emu))
            }
        };
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let id = self.document.ids().new_shape_id();
        let picture = ibis_doc_model::Picture { media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: None, alt_text: Default::default(), decorative: false, name: name.to_string(), shadow: None, reflection: None, soft_edge: None, geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(old) = replaced {
            commands.push(Command::RemoveShapes { slide, shapes: vec![old] });
        }
        let index = self.slide().shapes.len() - usize::from(replaced.is_some());
        commands.push(Command::InsertShape { slide, index, shape });
        commands.push(Command::SetAspectLocked { slide, shape: id, locked: true });
        self.run(commands, cx);
        self.set_selection(vec![id]);
        Some(id)
    }

    /// Insert a bundled Material icon as an SVG picture onto the slide.
    pub(crate) fn insert_icon(&mut self, icon_name: &str, cx: &mut Context<Self>) -> Option<ShapeId> {
        let raw = crate::shell::icons::icon_svg_bytes(icon_name)?;
        let format = ibis_doc_model::MediaFormat::Svg;
        let placeholder = self.empty_content_placeholder_for("pic");
        let (replaced, frame) = match placeholder {
            Some((old_id, ph_frame)) => {
                let side = ph_frame.width.min(ph_frame.height);
                let x = ph_frame.x + (ph_frame.width - side) / 2;
                let y = ph_frame.y + (ph_frame.height - side) / 2;
                (Some(old_id), EmuRect::new(x, y, side, side))
            }
            None => {
                let size = self.presentation().size_of(self.slide());
                // Default icon size: 1.5 inches square (1,371,600 EMU).
                let side = 1_371_600.min(size.width / 4).min(size.height / 4);
                let x = (size.width - side) / 2;
                let y = (size.height - side) / 2;
                (None, EmuRect::new(x, y, side, side))
            }
        };
        let data = raw.to_vec();
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let id = self.document.ids().new_shape_id();
        let name = format!("Icon: {icon_name}");
        let picture = ibis_doc_model::Picture { media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: None, alt_text: Default::default(), decorative: false, name, shadow: None, reflection: None, soft_edge: None, geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(old) = replaced {
            commands.push(Command::RemoveShapes { slide, shapes: vec![old] });
        }
        let index = self.slide().shapes.len() - usize::from(replaced.is_some());
        commands.push(Command::InsertShape { slide, index, shape });
        commands.push(Command::SetAspectLocked { slide, shape: id, locked: true });
        self.run(commands, cx);
        self.set_selection(vec![id]);
        Some(id)
    }

    /// Insert a screenshot: from clipboard image if available, or captures a window snapshot.
    #[allow(dead_code)]
    pub(crate) fn insert_screenshot(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(id) = self.insert_screenshot_from_clipboard(cx) {
            return Some(id);
        }
        self.insert_screenshot_window(cx)
    }

    /// Insert a screenshot specifically from the clipboard (image or copied image files).
    pub(crate) fn insert_screenshot_from_clipboard(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(item) = cx.read_from_clipboard() {
            for entry in item.entries() {
                match entry {
                    ClipboardEntry::Image(image) => {
                        let ext = match image.format {
                            ImageFormat::Png => "png",
                            ImageFormat::Jpeg => "jpg",
                            ImageFormat::Gif => "gif",
                            ImageFormat::Svg => "svg",
                            ImageFormat::Bmp => "bmp",
                            _ => "png",
                        };
                        let name = format!("Screenshot.{}", ext);
                        return self.insert_picture(&name, image.bytes.clone(), cx);
                    }
                    ClipboardEntry::ExternalPaths(paths) => {
                        for path in paths.paths() {
                            let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg"));
                            if let (true, Ok(data)) = (ok, std::fs::read(path)) {
                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Screenshot.png".into());
                                return self.insert_picture(&name, data, cx);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Insert a screenshot of an available window (window snapshot).
    pub(crate) fn insert_screenshot_window(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        let (data, _, _) = ibis_pptx::screenshot_snapshot(640, 360);
        self.insert_picture("Screenshot_Window.png", data, cx)
    }

    /// Insert a screen clipping screenshot (e.g. cropped screen area).
    pub(crate) fn insert_screenshot_clipping(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(id) = self.insert_screenshot_from_clipboard(cx) {
            return Some(id);
        }
        let (data, _, _) = ibis_pptx::screenshot_snapshot(480, 270);
        self.insert_picture("Screen_Clipping.png", data, cx)
    }

    /// Choose an image file to fill the selected shapes (AutoShape or Table cells).
    pub(crate) fn pick_fill_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = (self.current_slide, self.selected_ids().to_vec());
        let cells = self.text_editing().and_then(|e| e.cell.map(|at| {
            let other = e.cell_anchor.unwrap_or(at);
            (e.shape, at.0.min(other.0), at.0.max(other.0), at.1.min(other.1), at.1.max(other.1))
        }));
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("pptx.common.fill").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| this.receive_fill_picture_file(target, cells, &name, data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| this.receive_fill_picture_file(target, cells, &name, data, cx)).ok();
        })
        .detach();
    }

    pub(crate) fn receive_fill_picture_file(&mut self, target: (SlideId, Vec<ShapeId>), cells: Option<(ShapeId, usize, usize, usize, usize)>, name: &str, data: Vec<u8>, cx: &mut Context<Self>) {
        self.fill_target_with_picture(target, cells, name, data, cx);
    }

    pub(crate) fn fill_selected_with_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ibis_doc_model::MediaId> {
        let target = (self.current_slide, self.selected_ids().to_vec());
        let cells = self.text_editing().and_then(|e| e.cell.map(|at| {
            let other = e.cell_anchor.unwrap_or(at);
            (e.shape, at.0.min(other.0), at.0.max(other.0), at.1.min(other.1), at.1.max(other.1))
        }));
        self.fill_target_with_picture(target, cells, name, data, cx)
    }

    pub(crate) fn fill_target_with_picture(
        &mut self,
        target: (SlideId, Vec<ShapeId>),
        cells: Option<(ShapeId, usize, usize, usize, usize)>,
        name: &str,
        data: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> Option<ibis_doc_model::MediaId> {
        let format = ibis_doc_model::MediaFormat::from_path(name);
        if validated_picture_size(format, &data).is_none() {
            log::warn!("Invalid image file: {name}");
            return None;
        }
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let fill = Fill::Image(media);

        let (slide, ids) = target;
        if let [id] = ids[..]
            && let Some(ShapeKind::Table(table)) = self.presentation().slide(slide).and_then(|s| s.shape(id)).map(|s| s.kind.clone())
        {
            let mut table = (*table).clone();
            for (r, row) in table.rows.iter_mut().enumerate() {
                for (c, cell) in row.cells.iter_mut().enumerate() {
                    if cells.is_none_or(|(target_id, r0, r1, c0, c1)| target_id != id || ((r0..=r1).contains(&r) && (c0..=c1).contains(&c))) {
                        cell.fill = fill.clone();
                    }
                }
            }
            self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
            return Some(media);
        }

        let commands = ids
            .into_iter()
            .filter_map(|id| self.presentation().slide(slide).and_then(|s| s.shape(id)))
            .filter(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            .map(|s| Command::SetFill { slide, shape: s.id, fill: fill.clone() })
            .collect();
        self.run(commands, cx);
        Some(media)
    }

    /// Choose an image file to set as the current slide's background.
    pub(crate) fn pick_background_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.background").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| this.receive_background_picture_file(slide, &name, data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| this.receive_background_picture_file(slide, &name, data, cx)).ok();
        })
        .detach();
    }

    pub(crate) fn receive_background_picture_file(&mut self, slide: SlideId, name: &str, data: Vec<u8>, cx: &mut Context<Self>) {
        let format = ibis_doc_model::MediaFormat::from_path(name);
        if validated_picture_size(format, &data).is_none() {
            log::warn!("Invalid image file: {name}");
            return;
        }
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        if self.current_slide == slide {
            self.set_background(Fill::Image(media), cx);
        } else {
            self.run(vec![Command::SetBackground { slide, fill: Fill::Image(media) }], cx);
        }
    }

    pub(crate) fn set_background_with_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ibis_doc_model::MediaId> {
        let format = ibis_doc_model::MediaFormat::from_path(name);
        if validated_picture_size(format, &data).is_none() {
            log::warn!("Invalid image file: {name}");
            return None;
        }
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        self.set_background(Fill::Image(media), cx);
        Some(media)
    }

    // ----- Slides ---------------------------------------------------------

    pub(crate) fn set_background(&mut self, fill: Fill, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if self.slide().background != fill {
            self.run(vec![Command::SetBackground { slide, fill }], cx);
        }
    }

    /// すべてに適用: the current slide's background (and 背景グラフィックを
    /// 表示しない) on every slide.
    pub(crate) fn apply_background_to_all(&mut self, cx: &mut Context<Self>) {
        let fill = self.slide().background.clone();
        let hidden = self.slide().hide_backdrop;
        let mut commands: Vec<Command> = self.presentation().slides.iter().filter(|s| s.background != fill).map(|s| Command::SetBackground { slide: s.id, fill: fill.clone() }).collect();
        commands.extend(self.presentation().slides.iter().filter(|s| s.hide_backdrop != hidden).map(|s| Command::SetBackdropHidden { slide: s.id, hidden }));
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// 図形の変更: the selected auto shapes made preset `preset` (their text,
    /// fill and outline kept).
    pub(crate) fn change_selected_shapes(&mut self, preset: &str, cx: &mut Context<Self>) {
        let Some(geometry) = ibis_pptx::preset_geometry(preset) else { return };
        let slide = self.current_slide;
        let commands: Vec<Command> = self
            .selected_shapes()
            .into_iter()
            .filter_map(|shape| match &shape.kind {
                ShapeKind::Auto(a) if a.geometry != geometry => {
                    let mut a = a.clone();
                    a.geometry = geometry.clone();
                    Some(Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(a)) })
                }
                _ => None,
            })
            .collect();
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// スライドのコピー: the current slide, as it is now, for pasting here or
    /// in another window (its file as it is now goes along for that).
    pub(crate) fn copy_current_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let chosen = self.selected_slide_ids();
        // The file as it is now, and the slides' parts in it.
        let saved = self.source.as_ref().and_then(|source| {
            let data: std::sync::Arc<[u8]> = source.save(self.presentation(), ibis_pptx::SaveOptions::default()).ok()?.into();
            let opened = ibis_pptx::open(data.clone()).ok()?;
            let parts = opened.source.slide_parts(&opened.presentation);
            Some((data, parts))
        });
        let slides = chosen
            .iter()
            .filter_map(|id| {
                let slide = self.presentation().slide(*id)?.clone();
                let layout = slide.layout.and_then(|l| self.presentation().layout(l)).map(|l| l.name.clone());
                let index = self.presentation().slide_index(*id)?;
                let file = saved.as_ref().and_then(|(data, parts)| Some(ibis_pptx::ForeignSlide { data: data.clone(), part: parts.get(index).cloned().flatten()?, keep_source: false }));
                Some((slide, layout, file))
            })
            .collect();
        *slide_clipboard() = Some(CopiedSlides { origin: self.slide_origin(cx), slides });
    }

    /// Which document slides are copied from: the window and the file it
    /// has open (another file opened in the window is another; one closed
    /// is none, though its window's id or memory be used again).
    fn slide_origin(&self, cx: &Context<Self>) -> SlideOrigin {
        SlideOrigin { window: cx.entity_id(), file: self.source.as_ref().map(std::sync::Arc::downgrade) }
    }

    /// スライドの切り取り: copied, then deleted (not the last one).
    pub(crate) fn cut_current_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.presentation().slides.len() < 2 {
            return;
        }
        self.copy_current_slide(window, cx);
        self.delete_slide(window, cx);
    }

    /// スライドの貼り付け: the copied slides after the current one. From
    /// this file, as copies of them (their shapes keep their ids: saved as
    /// copies of their parts, their own formatting kept); from another,
    /// their parts copied over, following this file's layout of the same
    /// name (else its first): 貼り付け先のテーマを使用.
    pub(crate) fn paste_slides(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.paste_slides_as(false, window, cx)
    }

    /// スライドの貼り付け（元の書式を保持）: from another file, its master
    /// (layouts, theme) along; from this one, as [`paste_slides`].
    pub(crate) fn paste_slides_keeping_formatting(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.paste_slides_as(true, window, cx)
    }

    fn paste_slides_as(&mut self, keep_source: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(copied) = slide_clipboard().clone() else { return false };
        self.end_text_edit(window, cx);
        let here = copied.origin.is(&self.slide_origin(cx));
        let mut index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        let mut commands = Vec::new();
        let mut last = None;
        for (slide, layout_name, file) in copied.slides {
            let id = self.document.ids().new_slide_id();
            let pasted = if here {
                ibis_doc_model::Slide { id, ..slide }
            } else {
                let (Some(source), Some(mut file)) = (self.source.clone(), file) else {
                    log::warn!("This slide cannot be pasted (no file)");
                    continue;
                };
                let Some(layout) = matching_layout(self.presentation(), layout_name.as_deref(), &slide) else { continue };
                // The slides pasted before this one are in the file already.
                let mut current = self.presentation().clone();
                for command in &commands {
                    if let Command::InsertSlide { index, slide } = command {
                        current.slides.insert(*index, slide.clone());
                    }
                }
                file.keep_source = keep_source;
                match source.paste_slide(&current, index, id, layout, file, || self.document.ids().new_shape_id()) {
                    Ok((mut pasted, media, layouts)) => {
                        // Its own layouts (keeping its formatting) join the presentation's.
                        self.document.ids().layouts.extend(layouts);
                        for m in media {
                            if self.presentation().media(m.id).is_none() {
                                self.resources.add_picture(m.id, m.format, &m.data);
                                self.document.ids().media.push(m);
                            }
                        }
                        pasted.notes = slide.notes.clone();
                        pasted
                    }
                    Err(err) => {
                        log::warn!("Cannot paste the slide: {err}");
                        continue;
                    }
                }
            };
            commands.push(Command::InsertSlide { index, slide: pasted });
            index += 1;
            last = Some(id);
        }
        if commands.is_empty() {
            return false;
        }
        self.run(commands, cx);
        if let Some(id) = last {
            self.select_slide(id, window, cx);
        }
        true
    }

    pub(crate) fn duplicate_current_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        // The slides selected, their copies together after the last of them.
        let chosen = self.selected_slide_ids();
        let mut index = chosen.iter().filter_map(|id| self.presentation().slide_index(*id)).max().map_or(0, |i| i + 1);
        let mut commands = Vec::new();
        let mut first_copy = None;
        for id in &chosen {
            let copy_id = self.document.ids().new_slide_id();
            let Some(copy) = self.presentation().duplicate_slide(*id, copy_id) else { continue };
            first_copy.get_or_insert(copy_id);
            commands.push(Command::InsertSlide { index, slide: copy });
            index += 1;
        }
        if commands.is_empty() {
            return;
        }
        self.run(commands, cx);
        if let Some(id) = first_copy {
            self.select_slide(id, window, cx);
        }
    }

    // ----- Tables ---------------------------------------------------------

    /// 行の挿入・削除、列の挿入・削除 for the selected table, at the cell
    /// being edited (else its last row and column): `change` gives the new
    /// table, how much the frame grows (width, height) and the cell to edit
    /// after. The new cells are as the table's style makes them (the table
    /// read back through the file, when there is one).
    pub(crate) fn change_table(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl Fn(&ibis_doc_model::Table, (usize, usize)) -> Option<(ibis_doc_model::Table, (i64, i64), (usize, usize))>) {
        let [id] = self.selected_ids()[..] else { return };
        let Some(shape) = self.slide().shape(id).cloned() else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let editing = self.text_editing().filter(|e| e.shape == id).and_then(|e| e.cell);
        let at = editing.unwrap_or((table.rows.len().saturating_sub(1), table.columns.len().saturating_sub(1)));
        let Some((new, (dw, dh), next)) = change(table, at) else { return };
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let mut shaped = shape.clone();
        shaped.kind = ShapeKind::Table(Box::new(new));
        shaped.frame.width = (shaped.frame.width + dw).max(1);
        shaped.frame.height = (shaped.frame.height + dh).max(1);
        if let Some(source) = &self.source {
            let mut changed = self.presentation().clone();
            if let Some(i) = changed.slide_index(slide)
                && let Some(target) = changed.slides[i].shapes.iter_mut().find(|s| s.id == id)
            {
                *target = shaped.clone();
                match source.as_read(&changed, slide, id) {
                    Ok(read) => shaped.kind = read.kind,
                    Err(err) => log::warn!("table not read back: {err}"),
                }
            }
        }
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(shaped.kind) }, Command::SetFrame { slide, shape: id, frame: shaped.frame }], cx);
        if editing.is_some() {
            self.begin_table_cell_edit(id, next, window, cx);
        }
    }

    /// 列の幅・行の高さ: the border right of `column` moved `dx` (the column
    /// on its right gives the room, as in PowerPoint), or the border below
    /// `row` moved `dy` (the table grows). Columns and rows stay at least
    /// 0.1 inch.
    pub(crate) fn move_table_border(&mut self, id: ShapeId, column: Option<usize>, row: Option<usize>, dx: i64, dy: i64, cx: &mut Context<Self>) {
        const LEAST: i64 = 91_440;
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id).cloned() else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let mut table = (**table).clone();
        let mut frame = shape.frame;
        if let Some(c) = column {
            if c + 1 >= table.columns.len() {
                return;
            }
            let (a, b) = (table.columns[c], table.columns[c + 1]);
            let d = dx.clamp(LEAST - a, b - LEAST);
            if d == 0 {
                return;
            }
            table.columns[c] = a + d;
            table.columns[c + 1] = b - d;
        } else if let Some(r) = row {
            let heights = table.row_heights(frame.height);
            let Some(&h) = heights.get(r) else { return };
            let d = dy.max(LEAST - h);
            if d == 0 {
                return;
            }
            // The rows as drawn, the moved one taller or shorter.
            for (row, height) in table.rows.iter_mut().zip(&heights) {
                row.height = *height;
            }
            table.rows[r].height = h + d;
            frame.height += d;
        } else {
            return;
        }
        let mut commands = vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }];
        if frame != shape.frame {
            commands.push(Command::SetFrame { slide, shape: id, frame });
        }
        self.run(commands, cx);
    }

    /// 表の挿入: a new table of `rows` × `columns` in the empty content placeholder
    /// (which it takes the place of), else in the middle of the slide.
    pub(crate) fn insert_slide_table(&mut self, rows: usize, columns: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.insert_slide_table_with(rows, columns, None, window, cx);
    }

    /// A table of `rows` by `columns`, its cells holding `contents` (by row) when given.
    pub(crate) fn insert_slide_table_with(&mut self, rows: usize, columns: usize, contents: Option<&[Vec<String>]>, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let placeholder = self.empty_content_placeholder_for("tbl");
        let (replaced, frame) = match placeholder {
            Some((old_id, ph_frame)) => (Some(old_id), ph_frame),
            None => {
                let size = self.presentation().size_of(self.slide());
                let width = (columns as i64 * 1_219_200).min(size.width * 8 / 10);
                let height = rows as i64 * 370_840;
                (None, EmuRect::new((size.width - width) / 2, (size.height - height) / 2, width, height))
            }
        };
        let id = self.document.ids().new_shape_id();
        let mut table = ibis_doc_model::Table::new_grid(rows, columns, frame.width, frame.height);
        if let Some(contents) = contents {
            for (r, line) in contents.iter().enumerate() {
                for (c, text) in line.iter().enumerate() {
                    if let Some(cell) = table.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)) {
                        let len = cell.text.text().chars().count();
                        cell.text.splice(0, len, text);
                    }
                }
            }
        }
        let shape = Shape::new(id, frame, ShapeKind::Table(Box::new(table)));
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(old) = replaced {
            commands.push(Command::RemoveShapes { slide, shapes: vec![old] });
        }
        let index = self.slide().shapes.len() - usize::from(replaced.is_some());
        commands.push(Command::InsertShape { slide, index, shape });
        self.run(commands, cx);
        self.set_selection(vec![id]);
        if contents.is_none() {
            self.begin_table_cell_edit(id, (0, 0), window, cx);
        }
    }

    /// 高さを揃える (`rows`) / 幅を揃える: the selected table's rows made
    /// the same height (the table's height shared), or its columns the same
    /// width (its width shared; what is left goes to the last).
    pub(crate) fn distribute_table(&mut self, rows: bool, window: &mut Window, cx: &mut Context<Self>) {
        let [id] = self.selected_ids()[..] else { return };
        let Some(shape) = self.slide().shape(id).cloned() else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let mut table = (**table).clone();
        if rows {
            let n = table.rows.len() as i64;
            if n == 0 {
                return;
            }
            let each = shape.frame.height / n;
            for (i, row) in table.rows.iter_mut().enumerate() {
                row.height = if i as i64 == n - 1 { shape.frame.height - each * (n - 1) } else { each };
            }
        } else {
            let n = table.columns.len() as i64;
            if n == 0 {
                return;
            }
            let total: i64 = table.columns.iter().sum();
            let each = total / n;
            for (i, w) in table.columns.iter_mut().enumerate() {
                *w = if i as i64 == n - 1 { total - each * (n - 1) } else { each };
            }
        }
        if ShapeKind::Table(Box::new(table.clone())) == shape.kind {
            return;
        }
        let slide = self.current_slide;
        self.end_text_edit(window, cx);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
    }

    /// セルの結合: the cells selected (the edited cell to the Shift+clicked one).
    pub(crate) fn merge_table_cells(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, Some(a), Some(b))) = self.text_editing().map(|e| (e.shape, e.cell, e.cell_anchor)) else { return };
        let Some(ShapeKind::Table(table)) = self.slide().shape(id).map(|s| s.kind.clone()) else { return };
        let Some(merged) = table.with_merged(a, b) else { return };
        let slide = self.current_slide;
        self.end_text_edit(window, cx);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(merged))) }], cx);
        self.begin_table_cell_edit(id, (a.0.min(b.0), a.1.min(b.1)), window, cx);
    }

    /// セルの分割: the merged cell being edited back into its cells.
    pub(crate) fn split_table_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, Some(at))) = self.text_editing().map(|e| (e.shape, e.cell)) else { return };
        let Some(ShapeKind::Table(table)) = self.slide().shape(id).map(|s| s.kind.clone()) else { return };
        let Some(split) = table.with_split(at) else { return };
        let slide = self.current_slide;
        self.end_text_edit(window, cx);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(split))) }], cx);
        self.begin_table_cell_edit(id, at, window, cx);
    }

    pub(crate) fn table_insert_row(&mut self, below: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.change_table(window, cx, |t, (r, c)| {
            let at = if below { r + 1 } else { r };
            let height = t.rows.get(r).map_or(0, |row| row.height);
            Some((t.with_row_inserted(at)?, (0, height), (at, c)))
        });
    }

    pub(crate) fn table_insert_column(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.change_table(window, cx, |t, (r, c)| {
            let at = if right { c + 1 } else { c };
            let width = t.columns.get(c).copied().unwrap_or(0);
            Some((t.with_column_inserted(at)?, (width, 0), (r, at)))
        });
    }

    pub(crate) fn table_remove_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change_table(window, cx, |t, (r, c)| {
            let height = t.rows.get(r).map_or(0, |row| row.height);
            let new = t.with_row_removed(r)?;
            let next = (r.min(new.rows.len() - 1), c);
            Some((new, (0, -height), next))
        });
    }

    pub(crate) fn table_remove_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change_table(window, cx, |t, (r, c)| {
            let width = t.columns.get(c).copied().unwrap_or(0);
            let new = t.with_column_removed(c)?;
            let next = (r, c.min(new.columns.len() - 1));
            Some((new, (-width, 0), next))
        });
    }

    /// 表のスタイルオプション（タイトル行・集計行・縞模様など）を切り替える.
    pub(crate) fn toggle_table_option(&mut self, option: TableOption, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(shape) = self.selected_shapes().into_iter().find(|s| matches!(s.kind, ShapeKind::Table(_))) else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let mut table = (**table).clone();
        match option {
            TableOption::FirstRow => table.style_options.first_row = !table.style_options.first_row,
            TableOption::LastRow => table.style_options.last_row = !table.style_options.last_row,
            TableOption::FirstCol => table.style_options.first_col = !table.style_options.first_col,
            TableOption::LastCol => table.style_options.last_col = !table.style_options.last_col,
            TableOption::BandRow => table.style_options.band_row = !table.style_options.band_row,
            TableOption::BandCol => table.style_options.band_col = !table.style_options.band_col,
        }
        let id = shape.id;
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
    }

    /// 表のスタイルオプションが有効かどうか.
    pub(crate) fn table_option_checked(&self, option: TableOption) -> bool {
        let Some(shape) = self.selected_shapes().into_iter().find(|s| matches!(s.kind, ShapeKind::Table(_))) else { return false };
        let ShapeKind::Table(table) = &shape.kind else { return false };
        match option {
            TableOption::FirstRow => table.style_options.first_row,
            TableOption::LastRow => table.style_options.last_row,
            TableOption::FirstCol => table.style_options.first_col,
            TableOption::LastCol => table.style_options.last_col,
            TableOption::BandRow => table.style_options.band_row,
            TableOption::BandCol => table.style_options.band_col,
        }
    }

    /// 表のスタイル ID（tableStyleId）を設定する.
    pub(crate) fn set_table_style_id(&mut self, style_id: Option<String>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(shape) = self.selected_shapes().into_iter().find(|s| matches!(s.kind, ShapeKind::Table(_))) else { return };
        let ShapeKind::Table(table) = &shape.kind else { return };
        let mut table = (**table).clone();
        table.style_id = style_id;
        let id = shape.id;
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Table(Box::new(table))) }], cx);
    }

    /// 現在選択されている表のスタイル ID を取得する.
    pub(crate) fn table_style_id(&self) -> Option<String> {
        let shape = self.selected_shapes().into_iter().find(|s| matches!(s.kind, ShapeKind::Table(_)))?;
        let ShapeKind::Table(table) = &shape.kind else { return None };
        table.style_id.clone()
    }

    /// 画面切り替え: the current slide's transition becomes `effect` (its
    /// timing kept), or none.
    pub(crate) fn set_transition(&mut self, effect: Option<&str>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let old = self.slide().transition.clone();
        let transition = effect.map(|e| match &old {
            Some(t) => ibis_doc_model::Transition { effect: e.to_string(), direction: None, ..t.clone() },
            None => ibis_doc_model::Transition::new(e),
        }).or_else(|| old.filter(|t| t.after.is_some() || !t.on_click).map(|t| ibis_doc_model::Transition { effect: String::new(), direction: None, ..t }));
        self.run(vec![Command::SetTransition { slide, transition }], cx);
    }

    /// How long the current slide's transition takes, milliseconds.
    pub(crate) fn set_transition_duration(&mut self, duration: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(t) = self.slide().transition.clone() else { return };
        self.run(vec![Command::SetTransition { slide, transition: Some(ibis_doc_model::Transition { duration, ..t }) }], cx);
    }

    /// 画面切り替えの効果オプション（方向）: the current slide's transition's
    /// direction becomes `direction`.
    pub(crate) fn set_transition_direction(&mut self, direction: Option<&str>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(mut t) = self.slide().transition.clone() else { return };
        t.direction = direction.map(str::to_string);
        self.run(vec![Command::SetTransition { slide, transition: Some(t) }], cx);
    }

    pub(crate) fn set_transition_after(&mut self, after: Option<u32>, cx: &mut Context<Self>) {
        let mut transition = self.slide().transition.clone().unwrap_or_else(|| ibis_doc_model::Transition::new(""));
        transition.after = after;
        self.run(vec![Command::SetTransition { slide: self.current_slide, transition: Some(transition) }], cx);
    }

    pub(crate) fn toggle_transition_click(&mut self, cx: &mut Context<Self>) {
        let mut transition = self.slide().transition.clone().unwrap_or_else(|| ibis_doc_model::Transition::new(""));
        transition.on_click = !transition.on_click;
        self.run(vec![Command::SetTransition { slide: self.current_slide, transition: Some(transition) }], cx);
    }

    /// すべてのスライドに適用: every slide gets the current slide's transition.
    pub(crate) fn apply_transition_to_all(&mut self, cx: &mut Context<Self>) {
        let transition = self.slide().transition.clone();
        let commands = self
            .presentation()
            .slides
            .iter()
            .filter(|s| s.transition != transition)
            .map(|s| Command::SetTransition { slide: s.id, transition: transition.clone() })
            .collect();
        self.run(commands, cx);
    }

    /// 画面切り替えのサウンド: the current slide's transition sound becomes `sound`.
    pub(crate) fn set_transition_sound(&mut self, sound: Option<ibis_doc_model::TransitionSound>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut transition = self.slide().transition.clone().unwrap_or_else(|| ibis_doc_model::Transition::new(""));
        transition.sound = sound;
        self.run(vec![Command::SetTransition { slide, transition: Some(transition) }], cx);
    }

    /// 次のサウンドまで繰り返す（loop）のトグル切り替え.
    pub(crate) fn toggle_transition_sound_loop(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(mut transition) = self.slide().transition.clone() else { return };
        if let Some(ibis_doc_model::TransitionSound::Play { loop_sound, .. }) = &mut transition.sound {
            *loop_sound = !*loop_sound;
            self.run(vec![Command::SetTransition { slide, transition: Some(transition) }], cx);
        }
    }

    /// Change the canvas without scaling or moving its contents.
    pub(crate) fn set_slide_size(&mut self, size: ibis_doc_model::EmuSize, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.presentation().slide_size != size {
            self.run(vec![Command::SetSlideSize { size }], cx);
        }
    }

    /// Change the canvas and scale what is on it by `mode` (最大化・サイズに
    /// 合わせて調整: all of one scale, centred): the slides' shapes and
    /// backdrops, the layouts (and, saved, their parts and the masters').
    pub(crate) fn set_slide_size_scaled(&mut self, size: ibis_doc_model::EmuSize, mode: super::size_panel::ScaleMode, window: &mut Window, cx: &mut Context<Self>) {
        use super::size_panel::ScaleMode;
        use ibis_doc_model::LayoutScale;
        if mode == ScaleMode::Keep { return self.set_slide_size(size, window, cx); }
        self.end_text_edit(window, cx);
        let p = self.presentation();
        let old = p.slide_size;
        if old == size { return; }
        let by = LayoutScale::between(old, size, mode == ScaleMode::Maximize);
        let mut commands = vec![Command::SetSlideSize { size }];
        commands.extend(p.slides.iter().map(|s| Command::SetSlide { slide: s.scaled(by) }));
        commands.push(Command::SetLayouts { layouts: p.layouts.iter().map(|l| l.scaled(by)).collect(), scale: p.layout_scale.then(by) });
        self.run(commands, cx);
    }

    /// Crop selected pictures without changing their frames, media or other effects.
    pub(crate) fn crop_selected_pictures(&mut self, crop: ibis_doc_model::Crop, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.crop == crop { return None; }
            let mut picture = picture.clone();
            picture.crop = crop;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Crop selected pictures to a target aspect ratio (`num:den`, e.g. 1:1, 16:9), centred.
    pub(crate) fn crop_selected_pictures_aspect(&mut self, num: u64, den: u64, cx: &mut Context<Self>) {
        if num == 0 || den == 0 { return; }
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            let (orig_w, orig_h) = self.presentation().media.iter()
                .find(|m| m.id == picture.media)
                .and_then(|m| picture_size(&m.data))
                .map(|(w, h)| (w as u64, h as u64))
                .unwrap_or_else(|| (shape.frame.width.max(1) as u64, shape.frame.height.max(1) as u64));
            let crop = crop_for_aspect(orig_w, orig_h, num, den);
            if picture.crop == crop { return None; }
            let mut picture = picture.clone();
            picture.crop = crop;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Crop selected pictures to a rounded-corner shape (corner in 100,000ths of shorter side; 0 for square).
    pub(crate) fn crop_selected_pictures_corner(&mut self, corner: u32, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.corner == corner { return None; }
            let mut picture = picture.clone();
            picture.corner = corner;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Recolour selected pictures with a two-colour duotone effect (dark, light).
    pub(crate) fn set_selected_pictures_duotone(&mut self, duotone: Option<(ibis_doc_model::Rgb, ibis_doc_model::Rgb)>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.duotone == duotone { return None; }
            let mut picture = picture.clone();
            picture.duotone = duotone;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Set bright and contrast correction for selected pictures (in 100 000ths, -100 000..=100 000).
    pub(crate) fn set_selected_pictures_luminance(&mut self, luminance: Option<(i32, i32)>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.luminance == luminance { return None; }
            let mut picture = picture.clone();
            picture.luminance = luminance;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Set or clear artistic effect for selected pictures.
    pub(crate) fn set_selected_pictures_artistic_effect(&mut self, effect: Option<ibis_doc_model::ArtisticEffect>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.artistic_effect == effect { return None; }
            let mut picture = picture.clone();
            picture.artistic_effect = effect;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// 選択中の SVG 画像を図形（フリーフォーム・Office カスタム図形）に変換する
    pub(crate) fn convert_selected_svg_to_shape(&mut self, cx: &mut Context<Self>) -> bool {
        let selected_svgs: Vec<(ShapeId, EmuRect, std::sync::Arc<[u8]>)> = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            let media = self.presentation().media(picture.media)?;
            if media.format == ibis_doc_model::MediaFormat::Svg {
                Some((shape.id, shape.frame, media.data.clone()))
            } else {
                None
            }
        }).collect();

        if selected_svgs.is_empty() {
            return false;
        }

        let slide_id = self.current_slide;
        let mut commands = Vec::new();
        let mut new_shape_ids = Vec::new();

        for (shape_id, frame, svg_data) in selected_svgs {
            let shape_index = self.slide().shapes.iter().position(|s| s.id == shape_id).unwrap_or(0);
            match ibis_pptx::convert_svg_to_shapes(|| self.document.ids().new_shape_id(), &svg_data, frame) {
                Ok(new_shapes) if !new_shapes.is_empty() => {
                    commands.push(Command::RemoveShape { slide: slide_id, shape: shape_id });
                    for (i, new_shape) in new_shapes.into_iter().enumerate() {
                        new_shape_ids.push(new_shape.id);
                        commands.push(Command::InsertShape {
                            slide: slide_id,
                            index: shape_index + i,
                            shape: new_shape,
                        });
                    }
                }
                _ => {}
            }
        }

        if commands.is_empty() {
            return false;
        }

        self.run(commands, cx);
        if !new_shape_ids.is_empty() {
            self.set_selection(new_shape_ids);
        }
        cx.notify();
        true
    }

    /// 選択図形の中に SVG 画像が含まれているか
    pub(crate) fn has_selected_svg_picture(&self) -> bool {
        self.selected_shapes().iter().any(|s| {
            matches!(&s.kind, ShapeKind::Picture(pic) if self.presentation().media(pic.media).is_some_and(|m| m.format == ibis_doc_model::MediaFormat::Svg))
        })
    }

    pub(crate) fn set_shape_glow(&mut self, glow: Option<(Color,i64)>, cx: &mut Context<Self>) {
        let commands=self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};if auto.glow==glow {return None;}
            let mut auto=auto.clone();auto.glow=glow;Some(Command::SetKind {slide:self.current_slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(auto))})
        }).collect();
        self.apply_pptx_commands_checked(commands,cx);
    }
    pub(crate) fn set_shape_shadow(&mut self, shadow: Option<ibis_doc_model::Shadow>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            match &shape.kind {
                ShapeKind::Auto(auto) => {
                    if auto.shadow == shadow { return None; }
                    let mut auto = auto.clone();
                    auto.shadow = shadow;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
                }
                ShapeKind::Picture(picture) => {
                    if picture.shadow == shadow { return None; }
                    let mut picture = picture.clone();
                    picture.shadow = shadow;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
                }
                _ => None,
            }
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(crate) fn set_shape_reflection(&mut self, reflection: Option<ibis_doc_model::Reflection>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            match &shape.kind {
                ShapeKind::Auto(auto) => {
                    if auto.reflection == reflection { return None; }
                    let mut auto = auto.clone();
                    auto.reflection = reflection;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
                }
                ShapeKind::Picture(picture) => {
                    if picture.reflection == reflection { return None; }
                    let mut picture = picture.clone();
                    picture.reflection = reflection;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
                }
                _ => None,
            }
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(crate) fn set_shape_soft_edge(&mut self, soft_edge: Option<i64>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            match &shape.kind {
                ShapeKind::Auto(auto) => {
                    if auto.soft_edge == soft_edge { return None; }
                    let mut auto = auto.clone();
                    auto.soft_edge = soft_edge;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
                }
                ShapeKind::Picture(picture) => {
                    if picture.soft_edge == soft_edge { return None; }
                    let mut picture = picture.clone();
                    picture.soft_edge = soft_edge;
                    Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
                }
                _ => None,
            }
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(crate) fn set_shape_bevel(&mut self, bevel: Option<ibis_doc_model::Bevel>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto) = &shape.kind else { return None };
            if auto.bevel == bevel { return None; }
            let mut auto = auto.clone();
            auto.bevel = bevel;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    #[allow(dead_code)]
    pub(crate) fn set_shape_tilt(&mut self, tilt: Option<ibis_doc_model::Tilt>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto) = &shape.kind else { return None };
            if auto.tilt == tilt { return None; }
            let mut auto = auto.clone();
            auto.tilt = tilt;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(crate) fn set_shape_3d_rotation_preset(&mut self, preset: Option<ibis_doc_model::CameraPreset>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto) = &shape.kind else { return None };
            let new_tilt = match preset {
                Some(p) => {
                    let mut t = auto.tilt.unwrap_or_else(|| ibis_doc_model::Tilt::from_preset(p));
                    let (lat, lon, rev) = p.default_angles();
                    t.camera_preset = Some(p);
                    t.lat = lat;
                    t.lon = lon;
                    t.rev = rev;
                    Some(t)
                }
                None => {
                    if let Some(mut t) = auto.tilt {
                        if t.extrusion_h > 0 || t.depth > 0 {
                            t.lat = 0;
                            t.lon = 0;
                            t.rev = 0;
                            t.camera_preset = None;
                            Some(t)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };
            if auto.tilt == new_tilt { return None; }
            let mut auto = auto.clone();
            auto.tilt = new_tilt;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(crate) fn set_shape_3d_depth(&mut self, depth_emu: i64, color: Option<ibis_doc_model::Color>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto) = &shape.kind else { return None };
            let mut t = auto.tilt.unwrap_or_else(|| ibis_doc_model::Tilt::from_preset(ibis_doc_model::CameraPreset::OrthographicFront));
            t.extrusion_h = depth_emu;
            if color.is_some() {
                t.extrusion_color = color;
            }
            let new_tilt = if t.extrusion_h == 0 && t.lat == 0 && t.lon == 0 && t.rev == 0 && t.depth == 0 {
                None
            } else {
                Some(t)
            };
            if auto.tilt == new_tilt { return None; }
            let mut auto = auto.clone();
            auto.tilt = new_tilt;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Auto(auto)) })
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }
    pub(super) fn apply_pptx_commands_checked(&mut self,commands:Vec<Command>,cx:&mut Context<Self>) -> bool {
        if commands.is_empty() {return true;}
        if self.is_workbook() {
            self.run(commands, cx);
            return true;
        }
        self.drop_untouched_body();
        // Preview serialization before changing any live target.
        let mut preview=self.presentation().clone();
        for command in &commands {
            if preview.apply(command.clone()).is_err() {
                return false;
            }
        }
        if let Some(source)=&self.source {
            if let Err(error)=source.save(&preview,ibis_pptx::SaveOptions::default()) {log::warn!("Cannot apply the edit: {error}");return false;}
        }
        self.run(commands,cx);
        true
    }

    /// Remove only the modelled picture colour effects, keeping crop and framing.
    pub(crate) fn reset_picture_color_effects(&mut self, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.duotone.is_none() && picture.clear.is_none() && picture.luminance.is_none() && picture.artistic_effect.is_none() { return None; }
            let mut picture = picture.clone(); picture.duotone = None; picture.clear = None; picture.luminance = None; picture.artistic_effect = None;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// Reset picture formatting (stroke, corner, duotone, clear, luminance, shadow, reflection, soft_edge, artistic_effect), keeping crop and frame.
    pub(crate) fn reset_picture_effects(&mut self, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.duotone.is_none()
                && picture.clear.is_none()
                && picture.luminance.is_none()
                && picture.stroke.is_none()
                && picture.corner == 0
                && picture.shadow.is_none()
                && picture.reflection.is_none()
                && picture.soft_edge.is_none()
                && picture.artistic_effect.is_none()
            {
                return None;
            }
            let mut picture = picture.clone();
            picture.duotone = None;
            picture.clear = None;
            picture.luminance = None;
            picture.stroke = None;
            picture.corner = 0;
            picture.shadow = None;
            picture.reflection = None;
            picture.soft_edge = None;
            picture.artistic_effect = None;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }


    /// Apply a predefined video style preset to selected pictures.
    pub(crate) fn set_selected_pictures_video_style(&mut self, style: VideoStylePreset, cx: &mut Context<Self>) {
        use ibis_doc_model::{Color, Rgb, Stroke, Shadow, Reflection};
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            let mut pic = picture.clone();
            match style {
                VideoStylePreset::None => {
                    pic.stroke = None;
                    pic.corner = 0;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::SimpleFrameWhite => {
                    pic.stroke = Some(Stroke::solid(Color::opaque(Rgb(0xffffff)), 25_400));
                    pic.corner = 0;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::SimpleFrameBlack => {
                    pic.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 19_050));
                    pic.corner = 0;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::Rounded => {
                    pic.stroke = None;
                    pic.corner = 20_000;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::RoundedFrame => {
                    pic.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 19_050));
                    pic.corner = 20_000;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::DropShadow => {
                    pic.stroke = None;
                    pic.corner = 0;
                    pic.shadow = Some(Shadow::simple(Color { rgb: Rgb(0), alpha: 128 }, 50_800, 50_800, 2_700_000));
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::Reflection => {
                    pic.stroke = None;
                    pic.corner = 0;
                    pic.shadow = None;
                    pic.reflection = Some(Reflection::standard(0, 35_000));
                    pic.soft_edge = None;
                }
                VideoStylePreset::RoundedReflection => {
                    pic.stroke = None;
                    pic.corner = 20_000;
                    pic.shadow = None;
                    pic.reflection = Some(Reflection::standard(0, 35_000));
                    pic.soft_edge = None;
                }
                VideoStylePreset::RoundedShadow => {
                    pic.stroke = None;
                    pic.corner = 20_000;
                    pic.shadow = Some(Shadow::simple(Color { rgb: Rgb(0), alpha: 128 }, 50_800, 50_800, 2_700_000));
                    pic.reflection = None;
                    pic.soft_edge = None;
                }
                VideoStylePreset::SoftEdge => {
                    pic.stroke = None;
                    pic.corner = 0;
                    pic.shadow = None;
                    pic.reflection = None;
                    pic.soft_edge = Some(63_500);
                }
            }
            if pic == *picture { return None; }
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(pic)) })
        }).collect();
        self.apply_pptx_commands_checked(commands, cx);
    }

    /// Set or clear transparent color for selected pictures.
    #[allow(dead_code)]
    pub(crate) fn set_selected_pictures_clear(&mut self, clear: Option<ibis_doc_model::Rgb>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.clear == clear { return None; }
            let mut picture = picture.clone();
            picture.clear = clear;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    /// スライドをスキップ: the current slide is skipped in the slide show,
    /// or shown again.
    pub(crate) fn toggle_skip_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let chosen = self.selected_slide_ids();
        let Some(hidden) = self.presentation().slide(self.current_slide).map(|s| !s.hidden) else { return };
        let commands: Vec<Command> = chosen
            .iter()
            .filter_map(|id| self.presentation().slide(*id).cloned())
            .map(|mut slide| {
                slide.hidden = hidden;
                Command::SetSlide { slide }
            })
            .collect();
        self.run(commands, cx);
    }

    /// Reset placeholders' placement, preserving their text and formatting.
    pub(crate) fn reset_placeholder_frames(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if let Some(slide) = self.presentation().reset_slide_frames(self.current_slide) {
            if slide != *self.slide() { self.run(vec![Command::SetSlide { slide }], cx); }
        }
    }

    /// スライドのリセット (Reset Slide): reset placeholders' placement and direct formatting back to layout defaults.
    pub(crate) fn reset_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide_id = self.current_slide;
        let Some(layout_id) = self.slide().layout else { return };
        let new = match &self.source {
            Some(source) => {
                if let Some(reset) = self.presentation().reset_slide(slide_id) {
                    let mut temp = self.presentation().clone();
                    if let Some(idx) = temp.slide_index(slide_id) {
                        temp.slides[idx] = reset;
                    }
                    source.with_layout(&temp, slide_id, layout_id).map_err(|e| e.to_string())
                } else {
                    Err("cannot reset slide".to_string())
                }
            }
            None => self.presentation().reset_slide(slide_id).ok_or_else(|| "cannot reset slide".to_string()),
        };
        match new {
            Ok(new) if Some(&new) != self.presentation().slide(slide_id) => {
                self.run(vec![Command::SetSlide { slide: new }], cx);
            }
            Ok(_) => {}
            Err(err) => log::warn!("Cannot reset the slide: {err}"),
        }
    }

    /// レイアウトの適用: the current slide follows `layout`.
    pub(crate) fn apply_layout(&mut self, layout: LayoutId, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        // Read back through the file when it has one, for what the new
        // layout gives its text (formats, theme colours).
        // A layout copied and not yet saved is not in the file to read.
        let unsaved = self.presentation().layout(layout).is_some_and(|l| l.copy_of.is_some());
        let new = match &self.source {
            Some(source) if !unsaved => source.with_layout(self.presentation(), slide, layout).map_err(|e| e.to_string()),
            _ => self.presentation().with_layout(slide, layout).ok_or_else(|| "no such layout".to_string()),
        };
        match new {
            Ok(new) if Some(&new) != self.presentation().slide(slide) => self.run(vec![Command::SetSlide { slide: new }], cx),
            Ok(_) => {}
            Err(err) => log::warn!("Cannot apply the layout: {err}"),
        }
    }

    pub(crate) fn add_slide_with_layout(&mut self, layout: LayoutId, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_pptx_mark_as_final() || self.is_pptx_read_only_blocked() {
            return;
        }
        self.end_text_edit(window, cx);
        if self.presentation().layout(layout).is_none() {
            return;
        }
        let id = self.document.ids().new_slide_id();
        let index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        let slide = self.new_slide(index, id, Some(layout));
        self.run(vec![Command::InsertSlide { index, slide }], cx);
        self.select_slide(id, window, cx);
    }

    /// Move the current slide by `delta` places in the list.
    pub(crate) fn move_current_slide(&mut self, delta: i32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(index) = self.presentation().slide_index(slide) else { return };
        let target = (index as i64 + delta as i64).clamp(0, self.presentation().slides.len() as i64 - 1) as usize;
        if target != index {
            self.run(vec![Command::MoveSlide { slide, index: target }], cx);
        }
    }

    pub(crate) fn is_pptx_gridlines_enabled(&self) -> bool {
        self.pptx().is_some_and(|s| s.gridlines)
    }

    pub(crate) fn toggle_pptx_gridlines(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.gridlines = !s.gridlines;
            cx.notify();
        }
    }

    pub(crate) fn is_pptx_guides_enabled(&self) -> bool {
        self.pptx().is_some_and(|s| s.guides)
    }

    pub(crate) fn toggle_pptx_guides(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.guides = !s.guides;
            cx.notify();
        }
    }

    pub(crate) fn is_pptx_snap_to_grid_enabled(&self) -> bool {
        self.pptx().is_some_and(|s| s.snap_to_grid)
    }

    pub(crate) fn toggle_pptx_snap_to_grid(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.snap_to_grid = !s.snap_to_grid;
            cx.notify();
        }
    }

    pub(crate) fn is_pptx_snap_to_shape_enabled(&self) -> bool {
        self.pptx().is_some_and(|s| s.snap_to_shape)
    }

    pub(crate) fn toggle_pptx_snap_to_shape(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.snap_to_shape = !s.snap_to_shape;
            cx.notify();
        }
    }
}

/// Parse width and height from SVG data (attributes `width`, `height`, or `viewBox`).
pub(crate) fn svg_pixels(data: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(data).ok()?;
    let svg_start = text.find("<svg")?;
    let svg_tag = &text[svg_start..];
    let tag_end = svg_tag.find('>')?;
    let header = &svg_tag[..tag_end];

    let parse_dim = |attr_name: &str| -> Option<f64> {
        let pattern_dq = format!("{attr_name}=\"");
        let pattern_sq = format!("{attr_name}='");
        let (val_str, quote) = if let Some(pos) = header.find(&pattern_dq) {
            (&header[pos + pattern_dq.len()..], '"')
        } else if let Some(pos) = header.find(&pattern_sq) {
            (&header[pos + pattern_sq.len()..], '\'')
        } else {
            return None;
        };
        let end = val_str.find(quote)?;
        let raw = val_str[..end].trim();
        if raw.ends_with('%') {
            return None;
        }
        let num_str = raw.trim_end_matches(|c: char| c.is_alphabetic());
        let val = num_str.parse::<f64>().ok()?;
        if raw.ends_with("pt") {
            Some(val * 96.0 / 72.0)
        } else if raw.ends_with("in") {
            Some(val * 96.0)
        } else if raw.ends_with("mm") {
            Some(val * 96.0 / 25.4)
        } else if raw.ends_with("cm") {
            Some(val * 96.0 / 2.54)
        } else {
            Some(val)
        }
    };

    let w = parse_dim("width");
    let h = parse_dim("height");
    if let (Some(w), Some(h)) = (w, h) {
        if w > 0.0 && h > 0.0 {
            return Some((w.round() as u32, h.round() as u32));
        }
    }

    // Fallback to viewBox: "min-x min-y width height"
    let pattern_dq = "viewBox=\"";
    let pattern_sq = "viewBox='";
    let (val_str, quote) = if let Some(pos) = header.find(pattern_dq) {
        (&header[pos + pattern_dq.len()..], '"')
    } else if let Some(pos) = header.find(pattern_sq) {
        (&header[pos + pattern_sq.len()..], '\'')
    } else {
        return None;
    };
    let end = val_str.find(quote)?;
    let raw = val_str[..end].trim();
    let parts: Vec<f64> = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0 {
        Some((parts[2].round() as u32, parts[3].round() as u32))
    } else {
        None
    }
}

fn is_svg_header(data: &[u8]) -> bool {
    data.starts_with(b"<svg")
        || data.starts_with(b"<?xml")
        || (data.len() > 4 && data[..1024.min(data.len())].windows(4).any(|w| w == b"<svg"))
}

/// The pixel size of a PNG, JPEG, GIF, BMP or SVG picture, from its header.
pub(crate) fn validated_picture_size(format: ibis_doc_model::MediaFormat, data: &[u8]) -> Option<(u32, u32)> {
    use ibis_doc_model::MediaFormat;
    let signature = match format {
        MediaFormat::Png => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        MediaFormat::Jpeg => data.starts_with(&[0xff, 0xd8, 0xff]),
        MediaFormat::Gif => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        MediaFormat::Bmp => data.starts_with(b"BM"),
        MediaFormat::Svg => is_svg_header(data),
        _ => false,
    };
    if !signature { return None; }
    let (width, height) = picture_size(data)?;
    if width == 0 || height == 0 { return None; }
    if format == MediaFormat::Png {
        // Reject truncated containers on every platform before storing media.
        let mut at = 8usize;
        let mut have_data = false;
        let mut have_end = false;
        while at < data.len() {
            let length = u32::from_be_bytes(data.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize;
            let kind = data.get(at + 4..at + 8)?;
            let end = at.checked_add(12)?.checked_add(length)?;
            if end > data.len() { return None; }
            if at == 8 && (kind != b"IHDR" || length != 13) { return None; }
            have_data |= kind == b"IDAT";
            if kind == b"IEND" { have_end = length == 0 && end == data.len(); break; }
            at = end;
        }
        if !have_data || !have_end { return None; }
    }
    #[cfg(all(target_os = "macos", feature = "export"))]
    {
        if format == MediaFormat::Svg {
            return Some((width, height));
        }
        // All four codecs are already enabled by the native dependency graph.
        let decoder = match format { MediaFormat::Png => image::ImageFormat::Png, MediaFormat::Jpeg => image::ImageFormat::Jpeg, MediaFormat::Gif => image::ImageFormat::Gif, MediaFormat::Bmp => image::ImageFormat::Bmp, _ => return None };
        let decoded = image::load_from_memory_with_format(data, decoder).ok()?;
        Some((decoded.width(), decoded.height()))
    }
    #[cfg(not(all(target_os = "macos", feature = "export")))]
    Some((width, height))
}

pub(crate) fn picture_size(data: &[u8]) -> Option<(u32, u32)> {
    if is_svg_header(data) {
        if let Some((w, h)) = svg_pixels(data) {
            return Some((w, h));
        }
    }
    let be32 = |i: usize| Some(u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?));
    let le = |i: usize, n: usize| -> Option<u32> { Some(data.get(i..i + n)?.iter().rev().fold(0u32, |a, &b| (a << 8) | b as u32)) };
    if data.starts_with(b"\x89PNG") {
        return Some((be32(16)?, be32(20)?));
    }
    if data.starts_with(b"GIF8") {
        return Some((le(6, 2)?, le(8, 2)?));
    }
    if data.starts_with(b"BM") {
        return Some((le(18, 4)?, le(22, 4)?.min(i32::MAX as u32)));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        // The first start-of-frame marker.
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            let len = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                let h = ((data[i + 5] as u32) << 8) | data[i + 6] as u32;
                let w = ((data[i + 7] as u32) << 8) | data[i + 8] as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

/// Calculate centered Crop for given original dimensions and target aspect ratio (`num:den`).
pub(crate) fn crop_for_aspect(orig_w: u64, orig_h: u64, num: u64, den: u64) -> ibis_doc_model::Crop {
    if orig_w == 0 || orig_h == 0 || num == 0 || den == 0 {
        return ibis_doc_model::Crop::default();
    }
    let w_scaled = orig_w * den;
    let h_scaled = orig_h * num;
    if w_scaled > h_scaled {
        let fraction = 1.0 - (h_scaled as f64) / (w_scaled as f64);
        let lr = ((fraction / 2.0) * 100_000.0).round() as i32;
        ibis_doc_model::Crop { left: lr, right: lr, top: 0, bottom: 0 }
    } else if w_scaled < h_scaled {
        let fraction = 1.0 - (w_scaled as f64) / (h_scaled as f64);
        let tb = ((fraction / 2.0) * 100_000.0).round() as i32;
        ibis_doc_model::Crop { left: 0, right: 0, top: tb, bottom: tb }
    } else {
        ibis_doc_model::Crop::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    Front,
    Forward,
    Backward,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// Slides copied (スライドのコピー), for any window: the window they were
/// copied in, and each slide as it was, its layout's name and its file.
#[derive(Clone)]
pub(crate) struct CopiedSlides {
    pub(crate) origin: SlideOrigin,
    pub(crate) slides: Vec<(ibis_doc_model::Slide, Option<String>, Option<ibis_pptx::ForeignSlide>)>,
}

/// The slide clipboard, shared by the app's windows.
pub(crate) fn slide_clipboard() -> std::sync::MutexGuard<'static, Option<CopiedSlides>> {
    static CLIPBOARD: std::sync::Mutex<Option<CopiedSlides>> = std::sync::Mutex::new(None);
    CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// The layout of `presentation` a slide pasted from another file follows:
/// the one of the same name, else the one whose placeholders are most like
/// the slide's (by kind: title, body…), else the first.
fn matching_layout(presentation: &ibis_doc_model::Presentation, name: Option<&str>, slide: &ibis_doc_model::Slide) -> Option<ibis_doc_model::LayoutId> {
    let layouts = &presentation.layouts;
    if let Some(found) = layouts.iter().find(|l| Some(l.name.as_str()) == name) {
        return Some(found.id);
    }
    let mut kinds: Vec<&str> = slide.placeholders.values().map(|p| p.placeholder.normalized()).filter(|k| !matches!(*k, "dt" | "ftr" | "sldNum" | "hdr")).collect();
    kinds.sort_unstable();
    let score = |l: &ibis_doc_model::Layout| {
        let mut theirs: Vec<&str> = l.placeholders.iter().map(|p| p.placeholder.normalized()).filter(|k| !matches!(*k, "dt" | "ftr" | "sldNum" | "hdr")).collect();
        theirs.sort_unstable();
        let mut matched = 0usize;
        let mut rest = theirs.clone();
        for k in &kinds {
            if let Some(i) = rest.iter().position(|t| t == k) {
                rest.remove(i);
                matched += 1;
            }
        }
        // Most kinds matched, then fewest left over on either side.
        (matched, std::cmp::Reverse(rest.len() + kinds.len() - matched))
    };
    layouts.iter().max_by_key(|l| score(l)).or(layouts.first()).map(|l| l.id)
}

/// Where slides were copied: a window and the file open in it.
#[derive(Clone)]
pub(crate) struct SlideOrigin {
    window: gpui_kit::EntityId,
    file: Option<std::sync::Weak<ibis_pptx::Source>>,
}

impl SlideOrigin {
    /// The same document still open (a file is told by its source while
    /// it lives; a window without one by the window only).
    fn is(&self, other: &SlideOrigin) -> bool {
        self.window == other.window
            && match (&self.file, &other.file) {
                (Some(a), Some(b)) => a.upgrade().is_some() && a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            }
    }
}



/// The case transform to apply to selected text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextCase {
    Sentence,
    Lower,
    Upper,
    Title,
    Toggle,
}

pub(crate) fn transform_text_case(text: &str, case: TextCase) -> String {
    match case {
        TextCase::Upper => text.to_uppercase(),
        TextCase::Lower => text.to_lowercase(),
        TextCase::Toggle => {
            let mut out = String::with_capacity(text.len());
            for c in text.chars() {
                if c.is_uppercase() {
                    for l in c.to_lowercase() {
                        out.push(l);
                    }
                } else if c.is_lowercase() {
                    for u in c.to_uppercase() {
                        out.push(u);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
        TextCase::Title => {
            let mut out = String::with_capacity(text.len());
            let mut new_word = true;
            for c in text.chars() {
                if c.is_alphabetic() {
                    if new_word {
                        for u in c.to_uppercase() {
                            out.push(u);
                        }
                        new_word = false;
                    } else {
                        for l in c.to_lowercase() {
                            out.push(l);
                        }
                    }
                } else {
                    out.push(c);
                    if c.is_whitespace() || c == '-' || c == '/' || c == '_' || c == '(' || c == '[' || c == '{' || c == '"' || c == '“' {
                        new_word = true;
                    }
                }
            }
            out
        }
        TextCase::Sentence => {
            let mut out = String::with_capacity(text.len());
            let mut new_sentence = true;
            for c in text.chars() {
                if c.is_alphabetic() {
                    if new_sentence {
                        for u in c.to_uppercase() {
                            out.push(u);
                        }
                        new_sentence = false;
                    } else {
                        for l in c.to_lowercase() {
                            out.push(l);
                        }
                    }
                } else {
                    out.push(c);
                    if c == '.' || c == '!' || c == '?' || c == '\n' || c == '\r' {
                        new_sentence = true;
                    }
                }
            }
            out
        }
    }
}

pub(crate) fn apply_text_case(body: &mut TextBody, start: usize, end: usize, case: TextCase) {
    if start >= end {
        return;
    }
    let all_text = body.text();
    let total = all_text.chars().count();
    let start = start.min(total);
    let end = end.min(total);
    if start >= end {
        return;
    }

    let selected: String = all_text.chars().skip(start).take(end - start).collect();
    let converted = transform_text_case(&selected, case);
    if converted == selected {
        return;
    }

    let orig_chars: Vec<char> = selected.chars().collect();
    let conv_chars: Vec<char> = converted.chars().collect();

    if orig_chars.len() == conv_chars.len() {
        let mut at = 0;
        let p_count = body.paragraphs.len();
        for (p_idx, p) in body.paragraphs.iter_mut().enumerate() {
            for r in &mut p.runs {
                let r_len = r.text.chars().count();
                let r_start = at;
                let r_end = at + r_len;
                if r_end > start && r_start < end {
                    let mut new_text = String::with_capacity(r.text.len());
                    for (i, c) in r.text.chars().enumerate() {
                        let global_idx = r_start + i;
                        if global_idx >= start && global_idx < end {
                            new_text.push(conv_chars[global_idx - start]);
                        } else {
                            new_text.push(c);
                        }
                    }
                    r.text = new_text;
                }
                at = r_end;
            }
            if p_idx + 1 < p_count {
                at += 1;
            }
        }
    } else {
        body.splice(start, end - start, &converted);
    }
}

impl SlideEditor {
    /// Add or update a custom show (目的別スライド ショー).
    pub(crate) fn add_custom_show(&mut self, name: &str, slides: Vec<ibis_doc_model::SlideId>, cx: &mut Context<Self>) -> u32 {
        let mut shows = self.presentation().custom_shows.clone();
        let id = self.presentation().new_custom_show_id();
        shows.push(ibis_doc_model::CustomShow {
            id,
            name: name.to_string(),
            slides,
        });
        self.run(vec![ibis_doc_model::Command::SetCustomShows { custom_shows: shows }], cx);
        id
    }

    /// Remove a custom show by its numeric identifier.
    pub(crate) fn remove_custom_show(&mut self, id: u32, cx: &mut Context<Self>) {
        let mut shows = self.presentation().custom_shows.clone();
        shows.retain(|s| s.id != id);
        let mut cmds = vec![ibis_doc_model::Command::SetCustomShows { custom_shows: shows }];
        if matches!(self.presentation().show_properties.slide_range, ibis_doc_model::ShowSlideRange::Custom(cid) if cid == id) {
            let mut props = self.presentation().show_properties.clone();
            props.slide_range = ibis_doc_model::ShowSlideRange::All;
            cmds.push(ibis_doc_model::Command::SetShowProperties { properties: props });
        }
        self.run(cmds, cx);
    }

    /// プレゼンテーションが最終版としてマークされているか（PowerPoint の「最終版にする」）。
    pub(crate) fn is_pptx_mark_as_final(&self) -> bool {
        self.presentation().mark_as_final
    }

    /// 読み取り専用を推奨する設定になっているか（PowerPoint の「読み取り専用を推奨する」）。
    pub(crate) fn is_pptx_read_only_recommended(&self) -> bool {
        self.presentation().read_only_recommended
    }

    /// 読み取り専用推奨の情報バーで「編集を有効にする」を押したか。
    pub(crate) fn is_pptx_read_only_dismissed(&self) -> bool {
        self.pptx().is_some_and(|s| s.read_only_dismissed)
    }

    /// 読み取り専用推奨の情報バーを閉じて編集を有効にする。
    pub(crate) fn dismiss_pptx_read_only(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.read_only_dismissed = true;
            cx.notify();
        }
    }

    /// 最終版としてマークを設定・解除する。
    pub(crate) fn set_pptx_mark_as_final(&mut self, mark: bool, cx: &mut Context<Self>) {
        self.run(vec![ibis_doc_model::Command::SetMarkAsFinal { mark_as_final: mark }], cx);
    }

    /// 最終版としてマークをトグル切替する。
    pub(crate) fn toggle_pptx_mark_as_final(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_pptx_mark_as_final();
        self.set_pptx_mark_as_final(next, cx);
    }

    /// 最終版マークを解除して編集を可能にする（PowerPoint の「それでも編集する」）。
    pub(crate) fn pptx_edit_anyway(&mut self, cx: &mut Context<Self>) {
        self.set_pptx_mark_as_final(false, cx);
    }

    /// 読み取り専用を推奨を設定・解除する。
    pub(crate) fn set_pptx_read_only_recommended(&mut self, ro: bool, cx: &mut Context<Self>) {
        self.run(vec![ibis_doc_model::Command::SetReadOnlyRecommended { read_only_recommended: ro }], cx);
    }

    /// 読み取り専用を推奨をトグル切替する。
    pub(crate) fn toggle_pptx_read_only_recommended(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_pptx_read_only_recommended();
        self.set_pptx_read_only_recommended(next, cx);
    }

    /// PowerPoint 標準の情報バー（最終版 / 読み取り専用の推奨）。
    pub(crate) fn render_pptx_infobar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.is_pptx_mark_as_final() {
            Some(
                div()
                    .id("pptx-infobar-final")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("pptx.infobar.final.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("pptx.infobar.final.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-edit-anyway")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("pptx.infobar.final.edit-anyway"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pptx_edit_anyway(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_pptx_modify_locked() {
            Some(
                div()
                    .id("pptx-infobar-modify")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(0x323130)).child(crate::shell::i18n::text("pptx.infobar.modify.title")))
                            .child(div().text_size(px(12.)).text_color(rgb(0x605e5c)).child(crate::shell::i18n::text("pptx.infobar.modify.message"))),
                    )
                    .child(
                        div()
                            .id("infobar-btn-modify-password")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("pptx.infobar.modify.unlock"))
                            .on_click(cx.listener(|this, _, window, cx| this.ask_modify_password_to_unlock(window, cx))),
                    )
                    .into_any_element(),
            )
        } else if self.is_pptx_read_only_recommended() && !self.is_pptx_read_only_dismissed() {
            Some(
                div()
                    .id("pptx-infobar-readonly")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("pptx.infobar.readonly.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("pptx.infobar.readonly.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-enable-editing")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("pptx.infobar.readonly.enable-editing"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_pptx_read_only(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else {
            None
        }
    }

    /// 非表示スライド タイトルを追加する (Add Hidden Slide Title: PPTX-ACCESS-003).
    /// スライドの表示キャンバス外（上端の外側オフスクリーン）にタイトルプレースホルダーを配置し、
    /// スライド名（p:cSld/@name）にもタイトルを設定することで、視覚的なレイアウトを崩さずに
    /// スクリーンリーダー利用者向けにスライドタイトルを提供し、アクセシビリティ検査のエラーを解消する。
    pub(crate) fn add_hidden_slide_title(
        &mut self,
        slide_id: SlideId,
        title_text: Option<&str>,
        cx: &mut Context<Self>,
    ) -> ShapeId {
        let (size, shape_len, slide_idx) = {
            let slide = self.presentation().slide(slide_id).expect("slide exists");
            let size = self.presentation().size_of(slide);
            let idx = self.presentation().slide_index(slide_id).unwrap_or(0);
            (size, slide.shapes.len(), idx)
        };
        let title = match title_text {
            Some(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => crate::shell::i18n::format("_pptx.slide-n", &[("n", &(slide_idx + 1))]),
        };
        // オフスクリーン（スライド上端の外側）にタイトルプレースホルダーを配置
        let frame = EmuRect::new(0, -1_500_000, size.width, 800_000);
        let shape_id = self.document.ids().new_shape_id();
        let shape = Shape::text_box(shape_id, frame, &title);
        let ph = ibis_doc_model::SlidePlaceholder {
            placeholder: ibis_doc_model::PlaceholderRef { kind: Some("title".to_string()), index: None },
            own_frame: true,
        };
        let commands = vec![
            Command::InsertShape { slide: slide_id, index: shape_len, shape },
            Command::SetPlaceholder { slide: slide_id, shape: shape_id, placeholder: Some(ph) },
            Command::SetSlideName { slide: slide_id, name: Some(title) },
        ];
        self.run(commands, cx);
        shape_id
    }

    /// スライド名（非表示タイトル）を設定する
    #[allow(dead_code)]
    pub(crate) fn set_slide_name(
        &mut self,
        slide_id: SlideId,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.run(vec![Command::SetSlideName { slide: slide_id, name }], cx);
    }
}

/// A category grouping of bundled icons for the Insert Icon menu.
pub(crate) struct IconCategory {
    pub label_key: &'static str,
    pub icon: &'static str,
    pub items: &'static [(&'static str, &'static str)],
}

pub(crate) const ICON_CATEGORIES: &[IconCategory] = &[
    IconCategory {
        label_key: "pptx.icon-category.arrows",
        icon: "arrow_forward",
        items: &[
            ("arrow_forward", "pptx.icon.arrow-right"),
            ("arrow_left", "pptx.icon.arrow-left"),
            ("arrow_upward", "pptx.icon.arrow-up"),
            ("arrow_downward", "pptx.icon.arrow-down"),
            ("arrow_right_alt", "pptx.icon.arrow-right-alt"),
            ("turn_right", "pptx.icon.turn-right"),
            ("swap_horiz", "pptx.icon.swap-horiz"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.business",
        icon: "description",
        items: &[
            ("description", "pptx.icon.document"),
            ("folder_open", "pptx.icon.folder"),
            ("insert_chart", "pptx.icon.chart"),
            ("table_chart", "pptx.icon.table"),
            ("bar_chart", "pptx.icon.bar-chart"),
            ("schedule", "pptx.icon.schedule"),
            ("edit", "pptx.icon.edit"),
            ("save", "pptx.icon.save"),
            ("print", "pptx.icon.print"),
            ("search", "pptx.icon.search"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.media",
        icon: "image",
        items: &[
            ("image", "pptx.icon.image"),
            ("movie", "pptx.icon.movie"),
            ("music_note", "pptx.icon.music"),
            ("photo_camera", "pptx.icon.camera"),
            ("play_circle", "pptx.icon.play"),
            ("link", "pptx.icon.link"),
            ("language", "pptx.icon.globe"),
            ("chat_bubble", "pptx.icon.chat"),
            ("co_present", "pptx.icon.presentation"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.symbols",
        icon: "auto_awesome",
        items: &[
            ("check", "pptx.icon.check"),
            ("close", "pptx.icon.close"),
            ("add", "pptx.icon.add"),
            ("info", "pptx.icon.info"),
            ("help", "pptx.icon.help"),
            ("settings", "pptx.icon.settings"),
            ("palette", "pptx.icon.palette"),
            ("lock", "pptx.icon.lock"),
            ("auto_awesome", "pptx.icon.sparkle"),
            ("new_releases", "pptx.icon.badge"),
        ],
    },
];


