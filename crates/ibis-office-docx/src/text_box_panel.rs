// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テキスト ボックス (Word's 挿入 > テキスト ボックス): inserting a text box
//! at the caret, and the sidebar panel that edits the text of the text box
//! at the caret.

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind, Rgb, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// What the panel shows of a text box: where it is, its text, size, place and text format.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxInfo {
    pub at: FlowPos,
    pub(crate) text: String,
    /// Width, height and offset (points, as the panel's fields hold them).
    pub geometry: [String; 4],
    pub(crate) font_size: String,
    pub color: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) floating: bool,
    /// Whether it holds text (a text box); a drawn shape does not.
    pub(crate) has_text: bool,
    /// The fill and the line's colours (`RRGGBB`, empty for none) and the line's width (points).
    pub fill: String,
    pub line: String,
    pub(crate) line_width: String,
    /// A straight line (it has arrow ends, no fill).
    pub(crate) is_line: bool,
    /// The outline as it is (the line's, or the shape's), for a change to start from.
    pub(crate) stroke: Option<ibis_doc_model::Stroke>,
    /// The dash pattern of the outline, whether the fill is a gradient, whether the shape has a shadow.
    pub dash: ibis_doc_model::LineDash,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    /// A glow, soft edges, a reflection.
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    /// Vertical anchor of the text (top, middle, bottom).
    pub anchor: ibis_doc_model::VerticalAnchor,
    pub effects:super::text_box_effects::EffectModel,
}

/// The panel's fields, and the text box they edit.
pub struct TextBoxState {
    pub text: Entity<TextareaState>,
    /// Width, height, left and top (points).
    pub geometry: [Entity<InputState>; 4],
    pub(crate) font_size: Entity<InputState>,
    pub color: Entity<InputState>,
    pub fill: Entity<InputState>,
    pub line: Entity<InputState>,
    pub line_width: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// A line's arrows at its start and its end.
    pub(crate) head: bool,
    pub(crate) tail: bool,
    /// The outline's dash pattern, a gradient fill, a shadow.
    pub dash_ui: crate::shell::shape_format_pane::DashUi,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    pub effects:super::text_box_effects::EffectEditor,
    /// Whether the text's outline, shadow and glow are on (the text tab's checkboxes).
    pub text_enabled: [bool; 3],
    pub(crate) anchor: ibis_doc_model::VerticalAnchor,
    /// What was shown when the panel opened or last applied: the changes are those from it.
    pub(crate) initial: TextBoxInfo,
    /// A field is being typed in: its next change joins the step the last one made (one step of undo per edit, not per key).
    pub(crate) typing: Option<EntityId>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

/// A text box's text being edited in place (a double click on it): the caret and selection
/// painted over the box on the page, using the same `TextEditing` + `CaretMarks` component as PowerPoint and Excel.
pub struct BoxEdit {
    pub at: FlowPos,
    pub page: usize,
    /// The box on its page (points).
    pub frame: (f32, f32, f32, f32),
    pub(crate) edit: crate::shell::text_edit::TextEditing,
    /// Where the box's text was painted on the page (for caret/selection and mouse hit-testing).
    pub positions: crate::render::rich_text::TextPositions,
    pub(crate) goal_x: Option<(usize, f32)>,
    /// Page point `(x, y)` from a double-click that opened `BoxEdit`, to place the caret once painted.
    pub(crate) pending_page_click: Option<(f32, f32)>,
    /// Character anchor when dragging a selection inside the box.
    pub(crate) drag_anchor: Option<usize>,
}

/// Points as the fields show them (a whole number without the decimals).
fn points(emu: i64) -> String {
    let v = emu as f64 / 12_700.0;
    if (v - v.round()).abs() < 0.005 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

fn emu(text: &str) -> Option<i64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() < 10_000.0).map(|v| (v * 12_700.0).round() as i64)
}

/// What a drag of a floating text box does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DragMode {
    Move,
    /// A handle: which side it takes along, each axis -1 (the left / top), 0 or 1 (the right / bottom).
    Resize(i8, i8),
}

/// The wrapping a floating shape's panel offers.
const WRAPS: [(&str, &str, ibis_doc_model::PictureWrapType); 5] = [
    ("shape-wrap-square", "docx.picture.wrap.square", ibis_doc_model::PictureWrapType::Square),
    ("shape-wrap-tight", "docx.picture.wrap.tight", ibis_doc_model::PictureWrapType::Tight),
    ("shape-wrap-top-bottom", "docx.picture.wrap.top-and-bottom", ibis_doc_model::PictureWrapType::TopAndBottom),
    ("shape-wrap-behind", "docx.picture.wrap.behind-text", ibis_doc_model::PictureWrapType::BehindText),
    ("shape-wrap-front", "docx.picture.wrap.in-front-of-text", ibis_doc_model::PictureWrapType::InFrontOfText),
];

/// A drag moving or resizing a floating text box or picture.
#[derive(Clone, Copy, Debug)]
pub struct ShapeDrag {
    pub(crate) at: FlowPos,
    pub(crate) mode: DragMode,
    pub(crate) page: usize,
    /// Where the box was on its page when the drag began (points).
    pub(crate) frame: (f32, f32, f32, f32),
    /// Where the mouse went down on the page, and how far it has moved (points).
    pub(crate) start: (f32, f32),
    pub delta: (f32, f32),
    /// The box's offset from its anchor when it began (EMU).
    pub(crate) offset: (i64, i64),
    /// Whether the dragged target's corner handles preserve aspect ratio (e.g., pictures).
    pub(crate) keep_aspect_on_corners: bool,
    /// Whether the dragged target is an inline picture (`FlowRunKind::Picture` / `PictureControl` or non-floating `ShapeKind::Picture`).
    pub(crate) is_inline_picture: bool,
}

impl ShapeDrag {
    /// The box as the drag now has it (points): moved, or with its sides taken along.
    pub fn frame_now(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.frame;
        let (dx, dy) = self.delta;
        match self.mode {
            DragMode::Move => {
                if self.is_inline_picture {
                    (x, y, w, h)
                } else {
                    (x + dx, y + dy, w, h)
                }
            }
            DragMode::Resize(hx, hy) => {
                if self.keep_aspect_on_corners && hx != 0 && hy != 0 && w > 0.0 && h > 0.0 {
                    let ratio = w / h;
                    let raw_w = (w + hx as f32 * dx).max(12.0);
                    let raw_h = (h + hy as f32 * dy).max(12.0);
                    let (nw, nh) = if (raw_w / w - 1.0).abs() >= (raw_h / h - 1.0).abs() {
                        let nw = raw_w;
                        let nh = (nw / ratio).max(12.0);
                        (nh * ratio, nh)
                    } else {
                        let nh = raw_h;
                        let nw = (nh * ratio).max(12.0);
                        (nw, nw / ratio)
                    };
                    let nx = if hx < 0 { x + w - nw } else { x };
                    let ny = if hy < 0 { y + h - nh } else { y };
                    return (nx, ny, nw, nh);
                }
                let (mut nx, mut nw) = (x, w);
                match hx {
                    -1 => {
                        nw = (w - dx).max(12.0);
                        nx = x + w - nw;
                    }
                    1 => nw = (w + dx).max(12.0),
                    _ => {}
                }
                let (mut ny, mut nh) = (y, h);
                match hy {
                    -1 => {
                        nh = (h - dy).max(12.0);
                        ny = y + h - nh;
                    }
                    1 => nh = (h + dy).max(12.0),
                    _ => {}
                }
                (nx, ny, nw, nh)
            }
        }
    }
}

/// Whether `shape` is how the layout draws the inline picture of `media` that is cut, outlined or recoloured (a shape of its
/// own, with no id and no anchor).
fn is_laid_out_inline_picture(shape: &ibis_doc_model::Shape, media: ibis_doc_model::MediaId) -> bool {
    shape.id == ibis_doc_model::ShapeId(0) && shape.floating.is_none() && matches!(&shape.kind, ShapeKind::Picture(p) if p.media == media)
}

/// The handles of a selected text box: the corners and the middles of the sides.
const HANDLES: &[(i8, i8)] = &[(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// A new text box's size: 6 cm × 3 cm.
const NEW_SIZE: (i64, i64) = (2_160_000, 1_080_000);

/// The shape styles: fill colour, line colour and line width (pt); an empty colour is none.
pub(crate) const SHAPE_STYLES: [(&str, &str, &str); 8] = [
    ("FFFFFF", "4472C4", "1"),
    ("D9E2F3", "4472C4", "1"),
    ("4472C4", "2F528F", "1"),
    ("ED7D31", "AE5A21", "1"),
    ("A5A5A5", "787878", "1"),
    ("FFC000", "BC8C00", "1"),
    ("70AD47", "507E32", "1"),
    ("000000", "000000", "2.25"),
];

#[ibis_office_macros::editor_ext(TextBoxPanel)]
impl SlideEditor {
    /// The text box (a shape with text) at the caret, or just before it.
    pub fn text_box_at_caret(&self) -> Option<TextBoxInfo> {
        let (start, _) = self.ordered_selection()?;
        self.text_box_near(start)
    }

    /// The text box at `start` (or just before it).
    pub fn text_box_near(&self, start: FlowPos) -> Option<TextBoxInfo> {
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::Shape(shape) = &run.kind {
                let body = match &shape.kind {
                    ShapeKind::Auto(a) => a.text.as_ref(),
                    ShapeKind::TextBox(body) => Some(body),
                    _ => None,
                };
                if matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_)) {
                    let default_body = ibis_doc_model::TextBody::default();
                    let text_body = body.unwrap_or(&default_body);
                    let first = text_body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
                    let bold = first.and_then(|r| r.style.bold).unwrap_or(text_body.style.bold);
                    let italic = first.and_then(|r| r.style.italic).unwrap_or(text_body.style.italic);
                    let size = first.and_then(|r| r.style.font_size).unwrap_or(text_body.style.font_size);
                    let color = first.and_then(|r| r.style.color).unwrap_or(text_body.style.color);
                    let (x, y) = shape.floating.as_ref().map_or((0, 0), |f| (f.offset_x, f.offset_y));
                    let (fill, line, line_width) = match &shape.kind {
                        ShapeKind::Auto(a) => (
                            match &a.fill {
                                ibis_doc_model::Fill::Solid(c) => format!("{:06X}", c.rgb.0 & 0xffffff),
                                _ => String::new(),
                            },
                            a.stroke.as_ref().map_or(String::new(), |s| format!("{:06X}", s.color.rgb.0 & 0xffffff)),
                            a.stroke.as_ref().map_or("1".to_string(), |s| points(s.width)),
                        ),
                        ShapeKind::Line(s) => (String::new(), format!("{:06X}", s.color.rgb.0 & 0xffffff), points(s.width)),
                        _ => (String::new(), String::new(), "1".to_string()),
                    };
                    let stroke = match &shape.kind {
                        ShapeKind::Auto(a) => a.stroke.clone(),
                        ShapeKind::Line(s) => Some(s.clone()),
                        _ => None,
                    };
                    let (gradient, shadow, glow, soft_edge, reflection) = match &shape.kind {
                        ShapeKind::Auto(a) => (matches!(a.fill, ibis_doc_model::Fill::Linear { .. }), a.shadow.is_some(), a.glow.is_some(), a.soft_edge.is_some(), a.reflection.is_some()),
                        _ => (false, false, false, false, false),
                    };
                    let dash = stroke.as_ref().map_or(ibis_doc_model::LineDash::Solid, |s| s.dash);
                    let found = Some(TextBoxInfo {
                        at: FlowPos::new(p.id, offset),
                        text: text_body.text(),
                        geometry: [points(shape.frame.width), points(shape.frame.height), points(x), points(y)],
                        font_size: format!("{}", size as f32 / 100.0),
                        color: format!("{:06X}", color.0 & 0xffffff),
                        bold,
                        italic,
                        floating: shape.floating.is_some(),
                        has_text: matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)),
                        fill,
                        line,
                        line_width,
                        is_line: matches!(shape.kind, ShapeKind::Line(_)),
                        stroke,
                        dash,
                        gradient,
                        shadow,
                        glow,
                        soft_edge,
                        reflection,
                        effects:{
                            let mut model=match &shape.kind{ShapeKind::Auto(a)=>super::text_box_effects::EffectModel{fill:a.fill.clone(),shadow:a.shadow.clone(),glow:a.glow,soft:a.soft_edge,reflection:a.reflection.clone(),..Default::default()},_=>Default::default()};
                            if let Some(run)=first {model.text_outline=run.style.outline;model.text_shadow=run.style.shadow.clone();model.text_glow=run.style.glow;}
                            model
                        },
                        anchor: text_body.anchor,
                    });
                    if offset == start.offset {
                        return found;
                    }
                    if offset + 1 == start.offset {
                        previous = found;
                    }
                }
            }
            offset += run.len();
        }
        previous
    }

    /// The floating text box, shape, or picture under the point (`x`, `y` points) of page `page`, in front of the
    /// text: where it is anchored and its offset.
    pub(crate) fn floating_text_box_at(&self, page: usize, x: f32, y: f32) -> Option<(FlowPos, (i64, i64), (f32, f32, f32, f32))> {
        let doc = self.flow()?;
        let laid = super::flow_layout::pages(doc, &self.resources);
        let page = laid.pages.get(page)?;
        fn find_in_blocks<F>(blocks: &[ibis_doc_model::Block], f: &mut F) -> Option<(FlowPos, (i64, i64))>
        where
            F: FnMut(u32, usize, &ibis_doc_model::FlowRun) -> Option<(FlowPos, (i64, i64))>,
        {
            for block in blocks {
                match block {
                    ibis_doc_model::Block::Paragraph(p) => {
                        let mut offset = 0;
                        for run in &p.runs {
                            if let Some(found) = f(p.id, offset, run) {
                                return Some(found);
                            }
                            offset += run.len();
                        }
                    }
                    ibis_doc_model::Block::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(found) = find_in_blocks(&cell.blocks, f) {
                                    return Some(found);
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        for item in page.items.iter().rev() {
            match item {
                super::flow_layout::Item::Shape { shape, x: sx, y: sy, w, h } => {
                    let selectable = matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_) | ShapeKind::Picture(_));
                    // A line or ink stroke has little thickness: 4 points about it are on it.
                    let slack = if matches!(shape.kind, ShapeKind::Line(_)) || super::ink::is_docx_ink_shape(shape) { 4.0 } else { 0.0 };
                    if !selectable || x < *sx - slack || x > sx + w + slack || y < *sy - slack || y > sy + h + slack {
                        continue;
                    }
                    let mut check = |para_id: u32, offset: usize, run: &ibis_doc_model::FlowRun| {
                        if let FlowRunKind::Shape(own) = &run.kind
                            && own.id == shape.id
                        {
                            if let Some(anchor) = own.floating.as_ref() {
                                return Some((FlowPos::new(para_id, offset), (anchor.offset_x, anchor.offset_y)));
                            } else if own.floating.is_none() && matches!(own.kind, ShapeKind::Picture(_)) {
                                return Some((FlowPos::new(para_id, offset), (0, 0)));
                            }
                        }
                        // An inline picture that is cut, outlined or recoloured is laid out as a shape without an id of its own.
                        if let FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. } = &run.kind
                            && is_laid_out_inline_picture(shape, *media)
                        {
                            return Some((FlowPos::new(para_id, offset), (0, 0)));
                        }
                        None
                    };
                    if let Some((pos, off)) = find_in_blocks(&doc.body, &mut check)
                        .or_else(|| find_in_blocks(&doc.header, &mut check))
                        .or_else(|| find_in_blocks(&doc.footer, &mut check))
                    {
                        return Some((pos, off, (*sx, *sy, *w, *h)));
                    }
                }
                super::flow_layout::Item::Picture { media, x: sx, y: sy, w, h } => {
                    if x < *sx || x > sx + w || y < *sy || y > sy + h {
                        continue;
                    }
                    let mut check = |para_id: u32, offset: usize, run: &ibis_doc_model::FlowRun| {
                        if let FlowRunKind::Picture { media: m, .. } | FlowRunKind::PictureControl { media: m, .. } = &run.kind
                            && m == media
                        {
                            return Some((FlowPos::new(para_id, offset), (0, 0)));
                        }
                        None
                    };
                    if let Some((pos, off)) = find_in_blocks(&doc.body, &mut check)
                        .or_else(|| find_in_blocks(&doc.header, &mut check))
                        .or_else(|| find_in_blocks(&doc.footer, &mut check))
                    {
                        return Some((pos, off, (*sx, *sy, *w, *h)));
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The floating text box, shape, or picture at `at`: its page, frame (points), and anchor offset (EMU).
    pub fn floating_text_box_frame_of(&self, at: FlowPos) -> Option<(usize, (f32, f32, f32, f32), (i64, i64))> {
        let doc = self.flow()?;
        let paragraph = self.paragraph_of(at.para)?;
        let mut offset = 0;
        enum Target {
            Shape(ibis_doc_model::ShapeId, (i64, i64)),
            Picture(ibis_doc_model::MediaId),
        }
        let mut found = None;
        for run in &paragraph.runs {
            if offset == at.offset {
                match &run.kind {
                    FlowRunKind::Shape(shape) => {
                        if let Some(anchor) = shape.floating.as_ref() {
                            found = Some(Target::Shape(shape.id, (anchor.offset_x, anchor.offset_y)));
                        } else if shape.floating.is_none() && matches!(shape.kind, ShapeKind::Picture(_)) {
                            found = Some(Target::Shape(shape.id, (0, 0)));
                        }
                    }
                    FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. } => {
                        found = Some(Target::Picture(*media));
                    }
                    _ => {}
                }
            }
            offset += run.len();
        }
        let target = found?;
        let laid = super::flow_layout::pages(doc, &self.resources);
        for (page, pg) in laid.pages.iter().enumerate() {
            for item in &pg.items {
                match (&target, item) {
                    (Target::Shape(id, anchor_offset), super::flow_layout::Item::Shape { shape, x, y, w, h }) if shape.id == *id => {
                        return Some((page, (*x, *y, *w, *h), *anchor_offset));
                    }
                    (Target::Picture(media), super::flow_layout::Item::Picture { media: m, x, y, w, h }) if m == media => {
                        return Some((page, (*x, *y, *w, *h), (0, 0)));
                    }
                    (Target::Picture(media), super::flow_layout::Item::Shape { shape, x, y, w, h }) if is_laid_out_inline_picture(shape, *media) => {
                        return Some((page, (*x, *y, *w, *h), (0, 0)));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Returns `(is_picture, is_inline_picture)` for the run at `at`.
    pub fn is_picture_at(&self, at: FlowPos) -> (bool, bool) {
        let Some(p) = self.paragraph_of(at.para) else { return (false, false) };
        let mut off = 0;
        for run in &p.runs {
            if off == at.offset {
                return match &run.kind {
                    FlowRunKind::Picture { .. } | FlowRunKind::PictureControl { .. } => (true, true),
                    FlowRunKind::Shape(s) if matches!(s.kind, ShapeKind::Picture(_)) => (true, s.floating.is_none()),
                    _ => (false, false),
                };
            }
            off += run.len();
        }
        (false, false)
    }

    /// The text box or picture at the caret (the one selected): its page, frame (points), where it is anchored
    /// and its offset.
    pub fn selected_text_box_frame(&self) -> Option<(usize, (f32, f32, f32, f32), FlowPos, (i64, i64))> {
        let at = self.text_box_at_caret().map(|i| i.at).or_else(|| {
            let (start, _) = self.ordered_selection()?;
            self.floating_text_box_frame_of(start)
                .map(|_| start)
                .or_else(|| (start.offset > 0).then(|| FlowPos::new(start.para, start.offset - 1)).filter(|&prev| self.floating_text_box_frame_of(prev).is_some()))
        })?;
        let (page, frame, anchor_offset) = self.floating_text_box_frame_of(at)?;
        Some((page, frame, at, anchor_offset))
    }

    /// All currently selected shapes/pictures (the primary one at the caret plus any Shift+clicked shapes).
    pub fn all_selected_shapes(&self) -> Vec<(usize, (f32, f32, f32, f32), FlowPos, (i64, i64))> {
        let mut out = Vec::new();
        if let Some(primary) = self.selected_text_box_frame() {
            out.push(primary);
        }
        if let Some(state) = self.doc_state() {
            for &at in &state.selected_shapes {
                if out.iter().any(|(_, _, a, _)| *a == at) {
                    continue;
                }
                if let Some((page, frame, offset)) = self.floating_text_box_frame_of(at) {
                    out.push((page, frame, at, offset));
                }
            }
        }
        out
    }

    /// The picture, chart or shape that is selected in a document (the caret on exactly one object, or a floating
    /// shape with its handles).
    pub(crate) fn selected_document_objects(&self) -> Vec<crate::shell::menu_context::ObjectKind> {
        use crate::shell::menu_context::ObjectKind;
        let mut out = Vec::new();
        let on_one_position = self.ordered_selection().is_some_and(|(a, b)| a.para == b.para && b.offset == a.offset + 1);
        if on_one_position {
            if self.selected_doc_chart().is_some() {
                out.push(ObjectKind::Chart);
            } else if self.picture_at_caret().is_some() {
                out.push(ObjectKind::Picture);
            }
        }
        if out.is_empty() && self.doc_state().is_some_and(|s| s.shape_selected || !s.selected_shapes.is_empty()) {
            let n = self.selected_shape_positions().len().max(1);
            out.extend(std::iter::repeat_n(ObjectKind::Shape, n));
        }
        out
    }

    /// All currently selected shape/text-box `FlowPos` positions (primary at caret + `selected_shapes`).
    pub(crate) fn selected_shape_positions(&self) -> Vec<FlowPos> {
        let mut out = Vec::new();
        if let Some(edit) = self.doc_state().and_then(|s| s.box_edit.as_ref()) {
            out.push(edit.at);
            return out;
        }
        if let Some(info) = self.text_box_at_caret() {
            out.push(info.at);
        } else if let Some((_, _, at, _)) = self.selected_text_box_frame() {
            out.push(at);
        }
        if let Some(state) = self.doc_state() {
            for &at in &state.selected_shapes {
                if !out.contains(&at) {
                    out.push(at);
                }
            }
        }
        out
    }

    /// Apply fill color (`Some(rgb)` or `None` for no fill) to all currently selected shapes.
    pub(crate) fn apply_selected_shapes_fill(&mut self, fill: Option<Rgb>, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.selected_shape_positions();
        let mut commands = Vec::new();
        for at in targets {
            if let Some(info) = self.text_box_near(at)
                && !info.is_line
            {
                commands.push(FlowCommand::SetShapeStyle { at: info.at, fill: Some(fill), line: None });
            }
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
                state.selected_shapes = keep_multi;
            }
            self.sync_text_box_panel(window, cx);
        }
    }

    /// Apply outline color (`Some(rgb)` or `None` for no outline) to all currently selected shapes.
    pub(crate) fn apply_selected_shapes_line_color(&mut self, line_color: Option<Rgb>, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.selected_shape_positions();
        let mut commands = Vec::new();
        for at in targets {
            let Some(info) = self.text_box_near(at) else { continue };
            let line_w = info.stroke.as_ref().map_or(12_700, |s| s.width);
            if info.is_line {
                let stroke = line_color.map(|c| {
                    let mut s = info.stroke.clone().unwrap_or_else(|| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(c), line_w));
                    s.color = ibis_doc_model::Color::opaque(c);
                    s
                });
                commands.push(FlowCommand::SetShapeStroke { at: info.at, stroke });
            } else {
                commands.push(FlowCommand::SetShapeStyle {
                    at: info.at,
                    fill: None,
                    line: Some(line_color.map(|c| (c, line_w))),
                });
            }
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
                state.selected_shapes = keep_multi;
            }
            self.sync_text_box_panel(window, cx);
        }
    }

    /// Shift+click on a floating shape/text box/picture to add or toggle it in the multi-selection.
    pub fn shift_click_shape(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        let Some((at, _, _)) = self.floating_text_box_at(page, x, y) else { return false };
        let current_primary = self.selected_text_box_frame().map(|(_, _, a, _)| a);
        if !self.doc_state().is_some_and(|s| s.shape_selected) || current_primary.is_none() {
            self.set_caret(at, false, cx);
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = true;
                state.selected_shapes = vec![at];
            }
            cx.notify();
            return true;
        }
        if let Some(primary) = current_primary
            && let Some(state) = self.doc_state_mut()
        {
            if !state.selected_shapes.contains(&primary) {
                state.selected_shapes.push(primary);
            }
            if let Some(idx) = state.selected_shapes.iter().position(|&p| p == at) {
                if state.selected_shapes.len() > 1 {
                    state.selected_shapes.remove(idx);
                    if primary == at
                        && let Some(&next) = state.selected_shapes.first()
                    {
                        self.set_caret(next, false, cx);
                    }
                }
            } else {
                state.selected_shapes.push(at);
            }
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = true;
            }
        }
        cx.notify();
        true
    }

    pub(crate) fn shape_at(&self, at: FlowPos) -> Option<&ibis_doc_model::Shape> {
        let p = self.paragraph_of(at.para)?;
        let mut off = 0;
        for run in &p.runs {
            if off == at.offset
                && let FlowRunKind::Shape(shape) = &run.kind
            {
                return Some(shape);
            }
            off += run.len();
        }
        None
    }

    pub fn shape_text_body_at(&self, at: FlowPos) -> Option<&ibis_doc_model::TextBody> {
        let shape = self.shape_at(at)?;
        match &shape.kind {
            ShapeKind::Auto(a) => a.text.as_ref(),
            ShapeKind::TextBox(body) => Some(body),
            _ => None,
        }
    }

    /// Synchronize `self.resources.edited_box` with the active `box_edit` state (and any IME composing body).
    pub(crate) fn sync_docx_edited_box_resource(&mut self) {
        let next = self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| {
            let id = b.edit.shape;
            let positions = b.positions.clone();
            let composing = self.composing_body_as(id, true);
            (id, positions, composing)
        });
        self.resources.edited_box = next;
    }

    /// The character formatting at the current selection (or caret) inside `box_edit` (or the selected shape).
    pub(crate) fn box_edit_current_format(&self) -> Option<(ibis_doc_model::RunFormat, ibis_doc_model::FlowRunStyle)> {
        let state = self.doc_state()?;
        let (at, range) = if let Some(edit) = state.box_edit.as_ref() {
            (edit.at, edit.edit.range())
        } else if state.shape_selected {
            let info = self
                .text_box_at_caret()
                .filter(|i| i.has_text)
                .or_else(|| state.selected_shapes.iter().find_map(|&at| self.text_box_near(at).filter(|i| i.has_text)))?;
            (info.at, 0..0)
        } else {
            return None;
        };
        let body = self.shape_text_body_at(at)?;
        let char_off = range.start;
        let is_caret = range.is_empty();
        let mut pos = 0;
        let mut found_run = None;
        'outer: for para in &body.paragraphs {
            for r in &para.runs {
                let n = r.text.chars().count();
                if char_off < pos + n || (is_caret && n > 0 && char_off == pos + n) {
                    found_run = Some(&r.style);
                    break 'outer;
                }
                pos += n;
            }
            pos += 1;
        }
        let bold = found_run.and_then(|s| s.bold).unwrap_or(body.style.bold);
        let italic = found_run.and_then(|s| s.italic).unwrap_or(body.style.italic);
        let underline = found_run.is_some_and(|s| s.underline);
        let underline_style = found_run
            .map(|s| ibis_doc_model::UnderlineStyle::from_drawing_underline(s.underline_kind))
            .unwrap_or(ibis_doc_model::UnderlineStyle::Single);
        let size = found_run.and_then(|s| s.font_size).unwrap_or(body.style.font_size);
        let color = found_run.and_then(|s| s.color).unwrap_or(body.style.color);
        let highlight = found_run.and_then(|s| s.highlight);
        let strike = found_run.is_some_and(|s| s.strike != ibis_doc_model::Strike::None);
        let double_strike = found_run.is_some_and(|s| s.strike == ibis_doc_model::Strike::Double);
        let spacing_hundredths = found_run.map_or(0, |s| s.spacing);
        let spacing = spacing_hundredths as i64 * 127;
        let font = found_run.and_then(|s| s.font.clone()).or_else(|| body.style.font.clone());
        let format = ibis_doc_model::RunFormat {
            font: font.clone(),
            size: Some(size),
            bold: Some(bold),
            italic: Some(italic),
            underline: Some(underline),
            underline_style: underline.then_some(underline_style),
            color: Some(color),
            highlight: Some(highlight),
            strike: Some(strike),
            double_strike: Some(double_strike),
            spacing: (spacing != 0).then_some(spacing),
            ..Default::default()
        };
        let style = ibis_doc_model::FlowRunStyle {
            font,
            size,
            bold,
            italic,
            underline,
            underline_style,
            color,
            highlight,
            strike,
            double_strike,
            spacing,
            ..Default::default()
        };
        Some((format, style))
    }

    /// The handle of the selected text box under the point (`x`, `y` points) of page `page`.
    fn text_box_handle_at(&self, page: usize, x: f32, y: f32) -> Option<(i8, i8)> {
        if !self.doc_state().is_some_and(|s| s.shape_selected) {
            return None;
        }
        let (p, (fx, fy, fw, fh), _, _) = self.selected_text_box_frame()?;
        if p != page {
            return None;
        }
        HANDLES.iter().copied().find(|&(hx, hy)| {
            let (cx, cy) = (fx + (hx + 1) as f32 * fw / 2.0, fy + (hy + 1) as f32 * fh / 2.0);
            (x - cx).abs() <= 5.0 && (y - cy).abs() <= 5.0
        })
    }

    /// A press on a floating text box or picture: on a handle of the selected one it begins resizing, else it selects the box
    /// (the caret goes before it) and starts moving it. Whether the press was on one.
    pub fn begin_shape_drag(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if let Some((hx, hy)) = self.text_box_handle_at(page, x, y)
            && let Some((_, frame, at, offset)) = self.selected_text_box_frame()
        {
            let (is_picture, is_inline_picture) = self.is_picture_at(at);
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = true;
                state.shape_drag = Some(ShapeDrag {
                    at,
                    mode: DragMode::Resize(hx, hy),
                    page,
                    frame,
                    start: (x, y),
                    delta: (0.0, 0.0),
                    offset,
                    keep_aspect_on_corners: is_picture,
                    is_inline_picture,
                });
            }
            cx.notify();
            return true;
        }
        let Some((at, offset, frame)) = self.floating_text_box_at(page, x, y) else { return false };
        let (is_picture, is_inline_picture) = self.is_picture_at(at);
        let keep_multi = self.doc_state().filter(|s|s.selected_shapes.contains(&at)).map(|s|s.selected_shapes.clone()).unwrap_or_default();
        self.set_caret(at, false, cx);
        if let Some(state) = self.doc_state_mut() {
            state.shape_selected = true;
            state.selected_shapes = keep_multi;
            state.shape_drag = Some(ShapeDrag {
                at,
                mode: DragMode::Move,
                page,
                frame,
                start: (x, y),
                delta: (0.0, 0.0),
                offset,
                keep_aspect_on_corners: is_picture,
                is_inline_picture,
            });
        }
        cx.notify();
        true
    }

    /// A double click on a floating text box (including near its handle/border): its text is edited in place.
    pub(crate) fn begin_box_edit_or_handle(&mut self, page: usize, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(state) = self.doc_state_mut() {
            state.shape_drag = None;
        }
        self.begin_box_edit(page, x, y, window, cx)
    }

    /// Start editing the floating text box at `at` in place.
    pub(crate) fn start_box_edit_at(&mut self, at: FlowPos, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((page, frame, _)) = self.floating_text_box_frame_of(at) else { return false };
        let Some(info) = self.text_box_near(at).filter(|i| i.has_text) else { return false };
        let Some(shape_id) = self.shape_at(at).map(|s| s.id) else { return false };
        self.commit_box_edit_keep_focus(window, cx);
        self.set_caret(at, false, cx);
        let len = info.text.chars().count();
        let positions = crate::render::rich_text::TextPositions::default();
        if let Some(state) = self.doc_state_mut() {
            state.shape_selected = false;
            state.selected_shapes.clear();
            state.box_edit = Some(BoxEdit {
                at,
                page,
                frame,
                edit: crate::shell::text_edit::TextEditing {
                    shape: shape_id,
                    anchor: len,
                    caret: len,
                    marked: None,
                    marked_clauses: Vec::new(),
                    is_new: false,
                    typing: false,
                    cell: None,
                    cell_anchor: None,
            added_body: None,
                },
                positions,
                goal_x: None,
                pending_page_click: None,
                drag_anchor: None,
            });
        }
        self.sync_docx_edited_box_resource();
        self.restart_caret_blink(cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn place_box_edit_caret_at_pending_click(&mut self, cx: &mut Context<Self>) {
        let Some((page, (click_x, click_y))) = self.doc_state_mut().and_then(|s| {
            let edit = s.box_edit.as_mut()?;
            let click = edit.pending_page_click.take()?;
            Some((edit.page, click))
        }) else { return };
        let is_top = self.doc_split_active();
        let Some((origin, k)) = self.page_origin_pane(page, is_top) else { return };
        let win_pos = point(origin.x + px(click_x * k), origin.y + px(click_y * k));
        if let Some(char_idx) = self.text_index_at(win_pos) {
            self.shape_move_caret(char_idx, false, cx);
        }
    }

    /// A double click on a floating text box: its text is edited in place.
    pub(crate) fn begin_box_edit(&mut self, page: usize, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((at, _, _)) = self.floating_text_box_at(page, x, y) else { return false };
        if !self.start_box_edit_at(at, window, cx) {
            return false;
        }
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
            edit.pending_page_click = Some((x, y));
        }
        true
    }

    /// The in-place edit ends (Esc or click outside): the page has the focus.
    pub fn commit_box_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_box_edit_keep_focus(window, cx) {
            self.focus_handle.focus(window, cx);
        }
    }

    /// End `box_edit` without changing focus. Whether there was an edit.
    fn commit_box_edit_keep_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.doc_state().and_then(|s| s.box_edit.as_ref()).is_some_and(|e| e.edit.marked.is_some()) {
            self.commit_marked(cx);
        }
        let Some(_edit) = self.doc_state_mut().and_then(|s| {
            let e = s.box_edit.take()?;
            s.shape_selected = true;
            Some(e)
        }) else { return false };
        self.resources.edited_box = None;
        self.sync_text_box_panel(window, cx);
        cx.notify();
        true
    }

    fn shape_range_bold_italic(&self, at: FlowPos, char_offset: usize) -> (bool, bool, bool) {
        let Some(body) = self.shape_text_body_at(at) else { return (false, false, false) };
        let mut pos = 0;
        for para in &body.paragraphs {
            for r in &para.runs {
                let n = r.text.chars().count();
                if char_offset < pos + n || (n > 0 && char_offset == pos && pos == 0) {
                    return (
                        r.style.bold.unwrap_or(body.style.bold),
                        r.style.italic.unwrap_or(body.style.italic),
                        r.style.underline,
                    );
                }
                pos += n;
            }
            pos += 1;
        }
        (body.style.bold, body.style.italic, false)
    }

    /// ⌘B ⌘I ⌘U while a text box is edited in place (the shell's `on_text_key`, through `toggle_text_characters`).
    pub(crate) fn toggle_box_edit_characters(&mut self, toggle: crate::shell::text_keys::CharacterToggle, cx: &mut Context<Self>) {
        use crate::shell::text_keys::CharacterToggle;
        let change = match toggle {
            CharacterToggle::Bold => ibis_doc_model::RunChange::Bold(true),
            CharacterToggle::Italic => ibis_doc_model::RunChange::Italic(true),
            CharacterToggle::Underline => {
                let cur_u = self.box_edit_current_format().and_then(|(f, _)| f.underline).unwrap_or(false);
                ibis_doc_model::RunChange::Underline(!cur_u)
            }
        };
        self.apply_box_edit_run_change(&change, cx);
    }

    /// Character formatting (`Bold`, `Italic`, `Underline`, `Size`, `Color`, `Font`, `Highlight`, `Strike`)
    /// applied while editing a text box in place (`box_edit`), while a shape is selected (`shape_selected`),
    /// or to a selection inside the text box sidebar panel (`text_box`).
    /// The text a character change goes to: the box edited in place, a selection in the panel's text, or the selected shape (its whole text);
    /// and the panel's text when it is not in the document yet.
    fn box_edit_target(&mut self, cx: &mut Context<Self>) -> (Option<(FlowPos, usize, usize, bool)>, Option<String>) {
        let mut sync_panel_text = None;
        let target = if let Some(edit) = self.doc_state().and_then(|s| s.box_edit.as_ref()) {
            let r = edit.edit.range();
            Some((edit.at, r.start, r.end, true))
        } else if let Some(tb) = self.doc_state().and_then(|s| s.text_box.as_ref()) {
            let text = tb.text.read(cx).value().to_string();
            let range = tb.text.read(cx).selected_range();
            let from = text[..range.start.min(text.len())].chars().count();
            let to = text[..range.end.min(text.len())].chars().count();
            if from < to {
                if text != tb.initial.text {
                    sync_panel_text = Some(text);
                }
                Some((tb.initial.at, from, to, false))
            } else if self.doc_state().is_some_and(|s| s.shape_selected) {
                self.text_box_at_caret()
                    .filter(|i| i.has_text)
                    .or_else(|| self.doc_state().and_then(|s| s.selected_shapes.iter().find_map(|&at| self.text_box_near(at).filter(|i| i.has_text))))
                    .map(|i| (i.at, 0, 0, false))
            } else {
                None
            }
        } else if self.doc_state().is_some_and(|s| s.shape_selected) {
            self.text_box_at_caret()
                .filter(|i| i.has_text)
                .or_else(|| self.doc_state().and_then(|s| s.selected_shapes.iter().find_map(|&at| self.text_box_near(at).filter(|i| i.has_text))))
                .map(|i| (i.at, 0, 0, false))
        } else {
            None
        };
        (target, sync_panel_text)
    }

    pub fn apply_box_edit_run_change(&mut self, change: &ibis_doc_model::RunChange, cx: &mut Context<Self>) -> bool {
        let (target, sync_panel_text) = self.box_edit_target(cx);
        let Some((at, from, to, is_box_edit)) = target else { return false };
        if matches!(change, ibis_doc_model::RunChange::Clear) {
            return self.mutate_active_shape_text(
                |body, range| {
                    let (s, e) = range.unwrap_or_else(|| (0, body.text().chars().count()));
                    body.clear_run_formats(s, e);
                    let touched = body.paragraphs_in(s, e);
                    for i in touched {
                        let p = &mut body.paragraphs[i];
                        p.align = None;
                        p.bullet = None;
                        p.margin_left = 0;
                        p.indent = 0;
                        p.line_spacing = None;
                        p.space_before = 0;
                        p.space_after = 0;
                        p.level = 0;
                    }
                    if range.is_none() {
                        body.style = ibis_doc_model::TextStyle {
                            font_size: 1050,
                            ..Default::default()
                        };
                    }
                },
                cx,
            );
        }
        let (cur_bold, cur_italic, _cur_underline) = self.shape_range_bold_italic(at, if from < to { from } else { 0 });
        let mut format = ibis_doc_model::CharacterFormat::default();
        match change {
            ibis_doc_model::RunChange::Bold(_) => format.bold = Some(!cur_bold),
            ibis_doc_model::RunChange::Italic(_) => format.italic = Some(!cur_italic),
            ibis_doc_model::RunChange::Underline(v) => {
                format.underline = Some(*v);
                if *v {
                    format.underline_kind = Some(ibis_doc_model::Underline::Single);
                }
            }
            ibis_doc_model::RunChange::Strike(v) => {
                format.strike = Some(if *v { ibis_doc_model::Strike::Single } else { ibis_doc_model::Strike::None });
            }
            ibis_doc_model::RunChange::DoubleStrike(v) => {
                format.strike = Some(if *v { ibis_doc_model::Strike::Double } else { ibis_doc_model::Strike::None });
            }
            ibis_doc_model::RunChange::Position(pos) => {
                format.baseline = Some(match pos {
                    ibis_doc_model::VerticalPosition::Baseline => 0,
                    ibis_doc_model::VerticalPosition::Superscript => 30_000,
                    ibis_doc_model::VerticalPosition::Subscript => -25_000,
                });
            }
            ibis_doc_model::RunChange::Size(s) | ibis_doc_model::RunChange::ComplexScriptSize(s) => format.font_size = Some(*s),
            ibis_doc_model::RunChange::Color(c) | ibis_doc_model::RunChange::ThemeColor { color: c, .. } => format.color = Some(*c),
            ibis_doc_model::RunChange::Highlight(c) => format.highlight = Some(*c),
            ibis_doc_model::RunChange::Font(f) | ibis_doc_model::RunChange::AllFonts(f) | ibis_doc_model::RunChange::EastAsianFont(f) | ibis_doc_model::RunChange::ComplexScriptFont(f) => {
                format.font = Some(Some(f.clone()));
            }
            ibis_doc_model::RunChange::Spacing(s) => {
                format.spacing = Some(s.map_or(0, |emu| (emu as f64 / 127.0).round() as i32));
            }
            _ => return false,
        }
        self.run_box_edit_format(at, from, to, is_box_edit, sync_panel_text, format, cx)
    }

    /// A character format (the text effects of the panel's text tab) given to the box's selected characters, or to its whole text.
    pub(crate) fn apply_box_edit_character_format(&mut self, format: ibis_doc_model::CharacterFormat, cx: &mut Context<Self>) -> bool {
        let (target, sync_panel_text) = self.box_edit_target(cx);
        let Some((at, from, to, is_box_edit)) = target else { return false };
        self.run_box_edit_format(at, from, to, is_box_edit, sync_panel_text, format, cx)
    }

    fn run_box_edit_format(&mut self, at: FlowPos, from: usize, to: usize, is_box_edit: bool, sync_panel_text: Option<String>, format: ibis_doc_model::CharacterFormat, cx: &mut Context<Self>) -> bool {
        let mut commands = Vec::new();
        if let Some(text) = sync_panel_text {
            commands.push(FlowCommand::SetShapeText { at, text });
        }
        if !is_box_edit && from >= to {
            let mut targets = self.selected_shape_positions();
            if !targets.contains(&at) {
                targets.insert(0, at);
            }
            for target_at in targets {
                if self.shape_text_body_at(target_at).is_some() {
                    commands.push(FlowCommand::FormatShapeText {
                        at: target_at,
                        range: None,
                        format: format.clone(),
                    });
                }
            }
        } else {
            commands.push(FlowCommand::FormatShapeText {
                at,
                range: (from < to).then_some((from, to)),
                format,
            });
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
                state.selected_shapes = keep_multi;
            }
            if is_box_edit
                && let Some(edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut())
            {
                edit.edit.typing = false;
                edit.edit.is_new = false;
            }
            if let Some(info) = self.text_box_near(at)
                && let Some(tb) = self.doc_state_mut().and_then(|s| s.text_box.as_mut())
                && tb.initial.at == at
            {
                tb.initial = info;
            }
        }
        self.sync_docx_edited_box_resource();
        cx.notify();
        true
    }

    /// Returns `(at, Some((from, to)))` if a shape's text is being edited in place (`box_edit`),
    /// or `(at, None)` if a shape with text is selected (`shape_selected`).
    pub(crate) fn active_shape_text_target(&self) -> Option<(FlowPos, Option<(usize, usize)>, bool)> {
        let state = self.doc_state()?;
        if let Some(edit) = state.box_edit.as_ref() {
            let r = edit.edit.range();
            Some((edit.at, (r.start < r.end).then_some((r.start, r.end)), true))
        } else if state.shape_selected {
            let info = self
                .text_box_at_caret()
                .filter(|i| i.has_text)
                .or_else(|| state.selected_shapes.iter().find_map(|&at| self.text_box_near(at).filter(|i| i.has_text)))?;
            Some((info.at, None, false))
        } else {
            None
        }
    }

    /// Mutates the [`ibis_doc_model::TextBody`] of the active shape (either in `box_edit` or all selected shapes in `shape_selected`)
    /// as an undoable `FlowCommand`.
    pub(crate) fn mutate_active_shape_text<F>(&mut self, mut f: F, cx: &mut Context<Self>) -> bool
    where
        F: FnMut(&mut ibis_doc_model::TextBody, Option<(usize, usize)>),
    {
        let Some((at, range, is_box_edit)) = self.active_shape_text_target() else { return false };
        let Some(doc) = self.flow() else { return false };
        let commands: Vec<FlowCommand> = if is_box_edit || range.is_some() {
            ibis_doc_model::flow_command::mutate_shape_text_body(doc, at, |body| f(body, range)).into_iter().collect()
        } else {
            let mut targets = self.selected_shape_positions();
            if !targets.contains(&at) {
                targets.insert(0, at);
            }
            ibis_doc_model::flow_command::mutate_shapes_text_bodies(doc, &targets, |body| f(body, None))
        };
        if commands.is_empty() {
            return false;
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
                state.selected_shapes = keep_multi;
            }
            if is_box_edit
                && let Some(edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut())
            {
                edit.edit.typing = false;
                edit.edit.is_new = false;
            }
            if let Some(info) = self.text_box_near(at)
                && let Some(tb) = self.doc_state_mut().and_then(|s| s.text_box.as_mut())
                && tb.initial.at == at
            {
                tb.initial = info;
            }
        }
        self.sync_docx_edited_box_resource();
        cx.notify();
        true
    }

    /// The in-place editing overlay over the box being edited on page `index`.
    fn box_edit_overlay(&self, index: usize, k: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.doc_state()?.box_edit.as_ref().filter(|e| e.page == index)?;
        let edit_at = edit.at;
        let pending = edit.pending_page_click.is_some();
        self.ensure_caret_blink(cx);
        let live_frame = self.floating_text_box_frame_of(edit_at).map(|(_, f, _)| f).unwrap_or(edit.frame);
        let active_drag = self.doc_state().and_then(|s| s.shape_drag).filter(|d| d.page == index && d.at == edit_at);
        let (x, y, w, h) = active_drag.map_or(live_frame, |d| d.frame_now());
        let marks = self.caret_marks();
        let theme = cx.theme();
        let mut selected_color = theme.primary;
        selected_color.a = 0.25;
        let caret_color = theme.foreground;
        let entity = cx.entity();

        let blue = rgb(0x1a73e8);
        let edit_handle_fill = rgb(0x80d4ff);
        let points=ibis_doc_model::ResizeHandle::ALL.map(|handle|{let (ax,ay)=handle.anchor();(handle,point(px(ax as f32*w*k/2.),px(ay as f32*h*k/2.)))});
        let handles:Vec<AnyElement>=crate::shell::selection_handles::group("docx-edit-handles".into(),points,None,Default::default(),blue.into(),edit_handle_fill.into()).into_iter().map(|(handle,element)|{
            let (ax,ay)=handle.unwrap().anchor();let (hx,hy)=(ax as i8-1,ay as i8-1);
            element                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                            let is_top = this.doc_split_active();
                            let Some((px_pt, py_pt)) = this.page_point_pane(index, event.position, is_top) else { return };
                            let Some((_, frame, offset)) = this.floating_text_box_frame_of(edit_at) else { return };
                            let (is_picture, is_inline_picture) = this.is_picture_at(edit_at);
                            if let Some(state) = this.doc_state_mut() {
                                state.shape_drag = Some(ShapeDrag {
                                    at: edit_at,
                                    mode: DragMode::Resize(hx, hy),
                                    page: index,
                                    frame,
                                    start: (px_pt, py_pt),
                                    delta: (0.0, 0.0),
                                    offset,
                                    keep_aspect_on_corners: is_picture,
                                    is_inline_picture,
                                });
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            })
            .collect();

        Some(
            div()
                .absolute()
                .left(px(x * k))
                .top(px(y * k))
                .w(px(w * k))
                .h(px(h * k))
                .border_1()
                .border_dashed()
                .border_color(blue)
                .cursor_text()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, cx| {
                            if pending {
                                let entity = entity.clone();
                                cx.defer(move |cx| entity.update(cx, |this, cx| this.place_box_edit_caret_at_pending_click(cx)));
                            }
                            let Some(marks) = marks.as_ref() else { return };
                            let Some((caret, selection)) = marks.polygons(false) else { return };
                            let mut paint = |q: [(f32, f32); 4], color: Hsla| {
                                let points: Vec<Point<Pixels>> = q.iter().map(|&(px_x, px_y)| point(px(px_x), px(px_y))).collect();
                                if let Some(p) = crate::render::shape_view::polygon(&points, None, true) {
                                    window.paint_path(p, color);
                                }
                            };
                            for q in selection {
                                paint(q, selected_color);
                            }
                            if let Some(q) = caret.filter(|_| crate::shell::text_edit::caret_blink_visible()) {
                                paint(q, caret_color);
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.focus_handle.focus(window, cx);
                        if let Some(char_idx) = this.text_index_at(event.position) {
                            match event.click_count {
                                1 => {
                                    this.shape_move_caret(char_idx, event.modifiers.shift, cx);
                                    if let Some(edit) = this.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                                        edit.drag_anchor = Some(edit.edit.anchor);
                                    }
                                }
                                n => {
                                    let (a, b) = this.text_unit_around(char_idx, n >= 3);
                                    this.shape_move_caret(a, false, cx);
                                    this.shape_move_caret(b, true, cx);
                                    if let Some(edit) = this.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                                        edit.drag_anchor = None;
                                    }
                                }
                            }
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if this.doc_state().is_some_and(|s| s.shape_drag.is_some()) {
                        if event.pressed_button == Some(MouseButton::Left) {
                            let is_top = this.doc_split_active();
                            if let Some((px_pt, py_pt)) = this.page_point_pane(index, event.position, is_top) {
                                this.drag_shape(px_pt, py_pt, cx);
                            }
                        } else if let Some(state) = this.doc_state_mut() {
                            state.shape_drag = None;
                        }
                        return;
                    }
                    if event.pressed_button != Some(MouseButton::Left) {
                        return;
                    }
                    if this.doc_state().and_then(|s| s.box_edit.as_ref()).is_some_and(|e| e.drag_anchor.is_some())
                        && let Some(char_idx) = this.text_index_at(event.position)
                    {
                        this.shape_move_caret(char_idx, true, cx);
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        let border_click_exit = this
                            .doc_state()
                            .and_then(|s| s.shape_drag)
                            .is_some_and(|d| d.mode == DragMode::Move && d.delta.0.abs() < 2.0 && d.delta.1.abs() < 2.0);
                        if this.doc_state().is_some_and(|s| s.shape_drag.is_some()) {
                            this.end_shape_drag(cx);
                            this.sync_text_box_panel(window, cx);
                        }
                        if border_click_exit {
                            this.commit_box_edit(window, cx);
                            return;
                        }
                        if let Some(edit) = this.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                            edit.drag_anchor = None;
                        }
                        cx.notify();
                    }),
                )
                .child(crate::shell::selection_handles::border_region("docx-edit-frame",Bounds::new(point(px(0.),px(0.)),size(px(w*k),px(h*k))),0,cx.listener(move|this,event:&MouseDownEvent,_window,cx|{
                    // Dragging the border moves the box while its text stays in edit (a plain click on it ends the edit: the mouse-up).
                    cx.stop_propagation();
                    let is_top=this.doc_split_active();
                    let Some((px_pt,py_pt))=this.page_point_pane(index,event.position,is_top) else { return };
                    let Some((_,frame,offset))=this.floating_text_box_frame_of(edit_at) else { return };
                    let (is_picture,is_inline_picture)=this.is_picture_at(edit_at);
                    if let Some(state)=this.doc_state_mut() {
                        if let Some(edit)=state.box_edit.as_mut(){edit.edit.is_new=false;}
                        state.shape_drag=Some(ShapeDrag{at:edit_at,mode:DragMode::Move,page:index,frame,start:(px_pt,py_pt),delta:(0.0,0.0),offset,keep_aspect_on_corners:is_picture,is_inline_picture});
                    }
                    cx.notify();
                })))
                .children(handles)
                .into_any_element(),
        )
    }

    /// The mouse moved to (`x`, `y`) points of the page with the box held.
    pub(crate) fn drag_shape(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if let Some(drag) = self.doc_state_mut().and_then(|s| s.shape_drag.as_mut()) {
            drag.delta = (x - drag.start.0, y - drag.start.1);
            cx.notify();
        }
    }

    /// The mouse went up: the box moves or changes size by what it was dragged (a click that moved less than
    /// 2 points only selects).
    pub fn end_shape_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.doc_state_mut().and_then(|s| s.shape_drag.take()) else { return };
        if drag.delta.0.abs() < 2.0 && drag.delta.1.abs() < 2.0 {
            cx.notify();
            return;
        }
        let (nx, ny, nw, nh) = drag.frame_now();
        let (ox, oy, _, _) = drag.frame;
        let emu = |v: f32| (v * 12_700.0) as i64;
        let dx_emu = emu(nx - ox);
        let dy_emu = emu(ny - oy);
        let offset = (drag.offset.0 + dx_emu, drag.offset.1 + dy_emu);
        let size = (drag.mode != DragMode::Move).then(|| ibis_doc_model::EmuSize::new(emu(nw), emu(nh)));
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        let mut commands = Vec::new();
        if drag.is_inline_picture {
            if let Some(size) = size {
                commands.push(FlowCommand::SetPictureSize { at: drag.at, size });
            }
        } else {
            commands.push(FlowCommand::SetShapeGeometry { at: drag.at, size, offset: Some(offset) });
            if drag.mode == DragMode::Move && keep_multi.len() > 1 {
                for &other_at in &keep_multi {
                    if other_at == drag.at {
                        continue;
                    }
                    let (_, is_inline) = self.is_picture_at(other_at);
                    if !is_inline
                        && let Some((_, _, other_off)) = self.floating_text_box_frame_of(other_at)
                    {
                        commands.push(FlowCommand::SetShapeGeometry {
                            at: other_at,
                            size: None,
                            offset: Some((other_off.0 + dx_emu, other_off.1 + dy_emu)),
                        });
                    }
                }
            }
            commands.push(FlowCommand::RerouteConnectors);
        }
        if !commands.is_empty()
            && self.run_flow(commands, cx)
            && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut())
        {
            state.anchor = Some(a);
            state.caret = Some(c);
            state.selected_shapes = keep_multi;
        }
        if let Some(edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut())
            && edit.at == drag.at
        {
            edit.frame = (nx, ny, nw, nh);
            edit.edit.is_new = false;
            edit.edit.typing = false;
        }
        self.sync_docx_edited_box_resource();
        cx.notify();
    }

    /// The selected text box's handles, and the outline of the box while it is dragged, on page `index`
    /// (`k` pixels to the point).
    pub(crate) fn text_box_overlay(&self, index: usize, k: f32, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        // トリミング on the page (`shell/crop_mode.rs`) takes the place of the selection.
        if let Some(crop) = self.render_flow_crop_mode(index, k, cx) {
            out.push(crop.into_any_element());
            return out;
        }
        out.extend(self.draw_table_overlay(index, k));
        out.extend(self.wrap_points_overlay(index, k));
        out.extend(self.docx_ink_overlay(index, k));
        if let Some(editing) = self.box_edit_overlay(index, k, cx) {
            out.push(editing);
            return out;
        }
        let blue = rgb(0x1a73e8);
        if let Some(drag) = self.doc_state().and_then(|s| s.shape_drag).filter(|d| d.page == index && (d.delta.0 != 0.0 || d.delta.1 != 0.0)) {
            let (x, y, w, h) = drag.frame_now();
            out.push(div().absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).border_1().border_color(blue).bg(hsla(0.6, 0.8, 0.5, 0.12)).into_any_element());
            if drag.mode == DragMode::Move {
                let (ox, oy, _, _) = drag.frame;
                let (dx, dy) = (x - ox, y - oy);
                for (page, (sx, sy, sw, sh), at, _) in self.all_selected_shapes() {
                    if page == index && at != drag.at {
                        out.push(
                            div()
                                .absolute()
                                .left(px((sx + dx) * k))
                                .top(px((sy + dy) * k))
                                .w(px(sw * k))
                                .h(px(sh * k))
                                .border_1()
                                .border_color(blue)
                                .bg(hsla(0.6, 0.8, 0.5, 0.12))
                                .into_any_element(),
                        );
                    }
                }
            }
            return out;
        }
        if !self.doc_state().is_some_and(|s| s.shape_selected) {
            return out;
        }
        for (page, (x, y, w, h), _, _) in self.all_selected_shapes() {
            if page != index {
                continue;
            }
            out.push(crate::shell::selection_handles::outline(Bounds::new(point(px(x*k),px(y*k)),size(px(w*k),px(h*k))),blue.into()).into_any_element());
            out.push(crate::shell::selection_handles::border_region(format!("docx-frame-{x}-{y}"),Bounds::new(point(px(x*k),px(y*k)),size(px(w*k),px(h*k))),0,cx.listener(move|e,event:&MouseDownEvent,w,cx|{if let Some(edit)=e.doc_state_mut().and_then(|s|s.box_edit.as_mut()){edit.edit.is_new=false;}let mut event=event.clone();event.click_count=1;e.on_page_down(index,&event,w,e.doc_split_active(),cx);})).into_any_element());
            let points=ibis_doc_model::ResizeHandle::ALL.map(|handle|{let (ax,ay)=handle.anchor();(handle,point(px((x+ax as f32*w/2.)*k),px((y+ay as f32*h/2.)*k)))});
            out.extend(crate::shell::selection_handles::group(format!("docx-shape-handles-{x}-{y}").into(),points,None,Default::default(),blue.into(),rgb(0xffffff).into()).into_iter().map(|(_,element)|element.into_any_element()));
        }
        out
    }

    /// 挿入 > テキスト ボックス: an empty text box at the caret, immediately entering in-place `TextArea` mode.
    pub fn doc_insert_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let size = ibis_doc_model::EmuSize::new(NEW_SIZE.0, NEW_SIZE.1);
        if self.run_flow(vec![FlowCommand::InsertTextBox { at: caret, size, text: String::new() }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            // The panel is for the box just inserted, not another one next to the caret (#14).
            if let Some(info) = self.text_box_near(caret) {
                self.open_text_box_for(info, false, window, cx);
            }
            self.start_box_edit_at(caret, window, cx);
        }
    }

    /// 挿入 > 図形: a floating shape of preset `preset` at the caret (Word's blue with a darker line), selected.
    /// The command that inserts a preset shape (`size` in EMU) at `caret`.
    pub(crate) fn shape_command(preset: &str, caret: FlowPos, size: ibis_doc_model::EmuSize) -> Option<FlowCommand> {
        let geometry = ibis_pptx::preset_geometry(preset)?;
        Some(FlowCommand::InsertShape { at: caret, size, geometry, fill: Some(Rgb(0x4472C4)), line: Some((Rgb(0x2F528F), 12_700)) })
    }
    pub fn doc_insert_shape(&mut self, preset: &str, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(command) = Self::shape_command(preset, caret, ibis_doc_model::EmuSize::new(1_440_000, 1_080_000)) else { return };
        if self.run_flow(vec![command], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            if let Some(state) = self.doc_state_mut() {
                state.box_edit = None;
                state.shape_selected = true;
            }
            cx.notify();
        }
    }
    /// The command that inserts a connector (a line when `geometry` is `None`) at `caret`.
    pub(crate) fn connector_command(geometry: Option<ibis_doc_model::Geometry>, arrow: bool, caret: FlowPos, size: ibis_doc_model::EmuSize, flip_h: bool, flip_v: bool, glue: ibis_doc_model::Glue) -> FlowCommand {
        let mut stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(Rgb(0x2F528F)), 19_050);
        if arrow {
            stroke.tail = ibis_doc_model::LineEnd::parse(Some("triangle"), None, None);
        }
        FlowCommand::InsertConnector { at: caret, size, stroke, geometry, flip_h, flip_v, glue }
    }
    /// 挿入 > 図形 > 直線・コネクタ clicked on the page: at the usual size, at the caret.
    pub(crate) fn doc_insert_connector(&mut self, preset: Option<&str>, arrow: bool, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let geometry = preset.and_then(ibis_pptx::preset_geometry);
        let size = if geometry.is_some() { ibis_doc_model::EmuSize::new(2_160_000, 1_080_000) } else { ibis_doc_model::EmuSize::new(2_160_000, 0) };
        let command = Self::connector_command(geometry, arrow, caret, size, false, false, Default::default());
        if self.run_flow(vec![command], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            if let Some(state) = self.doc_state_mut() {
                state.box_edit = None;
                state.shape_selected = true;
            }
            cx.notify();
        }
    }

    /// Replace the text of the text box at `at`.
    pub(crate) fn set_text_box_text(&mut self, at: FlowPos, text: &str, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetShapeText { at, text: text.to_string() }], cx);
        cx.notify();
    }

    /// 挿入 > テキスト ボックスの文字…: the panel, with the text box at the caret.
    pub fn open_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(info) = self.text_box_at_caret() else {
            self.status = Some(crate::shell::i18n::text("docx.text-box-panel.place-cursor-before-after-text"));
            cx.notify();
            return;
        };
        self.open_text_box_for(info, true, window, cx);
    }

    /// The panel open (`focus` puts the caret in its text), already for `info`'s text box.
    pub(crate) fn open_text_box_for(&mut self, info: TextBoxInfo, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let text = cx.new(|cx| TextareaState::new(window, cx));
        text.update(cx, |a, cx| {
            a.set_value(info.text.clone(), window, cx);
            if focus {
                a.focus(window, cx);
            }
        });
        let field = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_string();
            cx.new(|cx| {
                let mut s = InputState::new(window, cx);
                s.set_value(value, window, cx);
                s
            })
        };
        let geometry = [field(&info.geometry[0], window, cx), field(&info.geometry[1], window, cx), field(&info.geometry[2], window, cx), field(&info.geometry[3], window, cx)];
        let font_size = field(&info.font_size, window, cx);
        let color = field(&info.color, window, cx);
        let fill = field(&info.fill, window, cx);
        let line = field(&info.line, window, cx);
        let line_width = field(&info.line_width, window, cx);
        // Every field applies as it changes (no 適用 button); the same field's following changes join one step of undo.
        let mut subscriptions = vec![{
            let id = text.entity_id();
            cx.subscribe_in(&text, window, move |this, _, event: &InputEvent, _, cx| this.text_box_field_event(id, event, cx))
        }];
        for input in geometry.iter().chain([&font_size, &color, &fill, &line, &line_width]) {
            let id = input.entity_id();
            subscriptions.push(cx.subscribe_in(input, window, move |this, _, event: &InputEvent, _, cx| this.text_box_field_event(id, event, cx)));
        }
        let(effects,effect_subscriptions)=super::text_box_effects::new_editor(&info.effects,window,cx);
        subscriptions.extend(effect_subscriptions);
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.ruby = None;
            state.helper = Default::default();
            state.text_box = Some(TextBoxState {
                effects,
                text_enabled: [info.effects.text_outline.is_some(), info.effects.text_shadow.is_some(), info.effects.text_glow.is_some()],
                text,
                geometry,
                font_size,
                color,
                fill,
                line,
                line_width,
                bold: info.bold,
                italic: info.italic,
                head: info.stroke.as_ref().is_some_and(|s| s.head != ibis_doc_model::LineEnd::None),
                tail: info.stroke.as_ref().is_some_and(|s| s.tail != ibis_doc_model::LineEnd::None),
                dash_ui: crate::shell::shape_format_pane::DashUi::new(info.dash),
                gradient: info.gradient,
                shadow: info.shadow,
                glow: info.glow,
                soft_edge: info.soft_edge,
                reflection: info.reflection,
                anchor: info.anchor,
                initial: info,
                typing: None,
                _subscriptions: subscriptions,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The open panel follows the box: after a drag its size and place are the box's now; with
    /// another box selected (a click on it), it is that box's (GitHub issue #14).
    pub(crate) fn sync_text_box_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let (shown_at, shown_geometry, shown_text, fields, text_input) = (
            state.initial.at,
            state.initial.geometry.clone(),
            state.initial.text.clone(),
            state.geometry.clone(),
            state.text.clone(),
        );
        let Some(info) = self.text_box_at_caret() else { return };
        if info.at != shown_at {
            self.open_text_box_for(info, false, window, cx);
        } else {
            let effects_changed=self.doc_state().and_then(|s|s.text_box.as_ref()).is_some_and(|s|s.initial.effects!=info.effects);
            if effects_changed {
                let(editor,subs)=super::text_box_effects::new_editor(&info.effects,window,cx);
                if let Some(s)=self.doc_state_mut().and_then(|s|s.text_box.as_mut()){
                    let tab=s.effects.tab;let open=s.effects.open;s.effects=editor;s.effects.tab=tab;s.effects.open=open;s._subscriptions.extend(subs);
                    s.text_enabled=[info.effects.text_outline.is_some(),info.effects.text_shadow.is_some(),info.effects.text_glow.is_some()];s.gradient=info.gradient;s.shadow=info.shadow;s.glow=info.glow;s.soft_edge=info.soft_edge;s.reflection=info.reflection;
                }
            }
            if let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                state.initial = info.clone();
            }
            if info.geometry != shown_geometry {
                for (field, value) in fields.iter().zip(info.geometry.iter()) {
                    field.update(cx, |i, cx| i.set_value(value.clone(), window, cx));
                }
            }
            if info.text != shown_text {
                text_input.update(cx, |i, cx| i.set_value(info.text, window, cx));
            }
        }
        cx.notify();
    }

    /// A change in one of the panel's fields applies at once; leaving the field ends the step.
    fn text_box_field_event(&mut self, field: EntityId, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Change => self.apply_text_box_typing(field, cx),
            InputEvent::Blur | InputEvent::PressEnter { .. } => {
                if let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    state.typing = None;
                }
            }
            _ => {}
        }
    }

    /// The panel's changes made to its text box, as one step: the text, the size and place, the
    /// text's size, colour, bold and italic.
    pub fn apply_text_box(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
            state.typing = None;
        }
        self.apply_text_box_step(false, None, cx);
    }

    /// A field's change: a new step the first time, then joined to it while the field is typed in.
    fn apply_text_box_typing(&mut self, field: EntityId, cx: &mut Context<Self>) {
        let joins = self.doc_state().and_then(|s| s.text_box.as_ref()).is_some_and(|s| s.typing == Some(field));
        self.apply_text_box_step(joins, Some(field), cx);
    }

    fn apply_text_box_step(&mut self, amend: bool, from_field: Option<EntityId>, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let text = state.text.read(cx).value().to_string();
        let geometry = [value(&state.geometry[0]), value(&state.geometry[1]), value(&state.geometry[2]), value(&state.geometry[3])];
        let (font_size, color) = (value(&state.font_size), value(&state.color));
        let (bold, italic) = (state.bold, state.italic);
        let initial = state.initial.clone();
        let at = initial.at;
        let mut commands = Vec::new();
        if text != initial.text {
            commands.push(FlowCommand::SetShapeText { at, text: text.clone() });
        }
        let size = (geometry[0] != initial.geometry[0] || geometry[1] != initial.geometry[1])
            .then(|| emu(&geometry[0]).zip(emu(&geometry[1])).filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| ibis_doc_model::EmuSize::new(w, h)))
            .flatten();
        let offset = (initial.floating && (geometry[2] != initial.geometry[2] || geometry[3] != initial.geometry[3]))
            .then(|| emu(&geometry[2]).zip(emu(&geometry[3])))
            .flatten();
        if size.is_some() || offset.is_some() {
            commands.push(FlowCommand::SetShapeGeometry { at, size, offset });
            commands.push(FlowCommand::RerouteConnectors);
        }
        let new_size = (font_size != initial.font_size).then(|| font_size.parse::<f32>().ok().filter(|v| (1.0..=1638.0).contains(v)).map(|v| (v * 2.0).round() as u32 * 50)).flatten();
        let new_color = (color != initial.color).then(|| u32::from_str_radix(color.trim_start_matches('#'), 16).ok().filter(|_| color.trim_start_matches('#').len() == 6).map(Rgb)).flatten();
        let (new_bold, new_italic) = ((bold != initial.bold).then_some(bold), (italic != initial.italic).then_some(italic));
        // The fill and the line (an empty colour: none).
        let hex = |t: &str| (t.trim_start_matches('#').len() == 6).then(|| u32::from_str_radix(t.trim_start_matches('#'), 16).ok()).flatten().map(Rgb);
        let (fill_now, line_now, width_now) = (value(&state.fill), value(&state.line), value(&state.line_width));
        let new_fill = (!state.gradient && fill_now != initial.fill).then(|| if fill_now.is_empty() { Some(None) } else { hex(&fill_now).map(Some) }).flatten();
        let width = emu(&width_now).filter(|w| *w > 0).unwrap_or(12_700);
        let new_line = (line_now != initial.line || width_now != initial.line_width).then(|| if line_now.is_empty() { Some(None) } else { hex(&line_now).map(|c| Some((c, width))) }).flatten();
        let (head_now, tail_now) = (state.head, state.tail);
        let ends_before = (initial.stroke.as_ref().is_some_and(|s| s.head != ibis_doc_model::LineEnd::None), initial.stroke.as_ref().is_some_and(|s| s.tail != ibis_doc_model::LineEnd::None));
        if initial.is_line {
            // A line: its whole outline, with the colour, width and arrows the panel says.
            if let Some(mut stroke) = initial.stroke.clone() {
                let changed = line_now != initial.line || width_now != initial.line_width || (head_now, tail_now) != ends_before;
                if changed {
                    if let Some(c) = hex(&line_now) {
                        stroke.color = ibis_doc_model::Color::opaque(c);
                    }
                    stroke.width = width;
                    let arrow = |on: bool, was: ibis_doc_model::LineEnd| if !on { ibis_doc_model::LineEnd::None } else if was != ibis_doc_model::LineEnd::None { was } else { ibis_doc_model::LineEnd::parse(Some("triangle"), None, None) };
                    stroke.head = arrow(head_now, stroke.head);
                    stroke.tail = arrow(tail_now, stroke.tail);
                    commands.push(FlowCommand::SetShapeStroke { at, stroke: Some(stroke) });
                }
            }
        } else if new_fill.is_some() || new_line.is_some() {
            commands.push(FlowCommand::SetShapeStyle { at, fill: new_fill, line: new_line });
        }
        // The dash pattern of the outline, a gradient fill, a shadow (after the fill and the line, which start them again).
        if state.dash_ui.dash != initial.dash && let Some(base) = initial.stroke.clone().or_else(|| hex(&line_now).map(|c| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(c), width))) {
            let mut stroke = base;
            if let Some(c) = hex(&line_now) {
                stroke.color = ibis_doc_model::Color::opaque(c);
            }
            stroke.width = width;
            commands.push(FlowCommand::SetShapeStroke { at, stroke: Some(stroke.with_dash(state.dash_ui.dash)) });
        }
        let mut text_effects = ibis_doc_model::CharacterFormat::default();
        if !initial.is_line && let Some(effects)=super::text_box_effects::model(state,cx) {
            let (a,b)=(&effects,&initial.effects);
            if (&a.fill,&a.shadow,&a.glow,&a.soft,&a.reflection)!=(&b.fill,&b.shadow,&b.glow,&b.soft,&b.reflection) {commands.push(FlowCommand::SetShapeEffects{at,
                fill:(effects.fill!=initial.effects.fill).then_some(effects.fill.clone()),
                shadow:(effects.shadow!=initial.effects.shadow).then_some(effects.shadow.clone()),
                glow:(effects.glow!=initial.effects.glow).then_some(effects.glow),
                soft_edge:(effects.soft!=initial.effects.soft).then_some(effects.soft),
                reflection:(effects.reflection!=initial.effects.reflection).then_some(effects.reflection.clone())});}
            // The text's outline, shadow and glow go to the selected characters (or the whole text), below.
            text_effects.outline=(a.text_outline!=b.text_outline).then_some(a.text_outline);
            text_effects.shadow=(a.text_shadow!=b.text_shadow).then(||a.text_shadow.clone());
            text_effects.glow=(a.text_glow!=b.text_glow).then_some(a.text_glow);
        }
        if new_size.is_some() || new_color.is_some() || new_bold.is_some() || new_italic.is_some() {
            commands.push(FlowCommand::SetShapeTextFormat { at, bold: new_bold, italic: new_italic, size: new_size, color: new_color });
        }
        if state.anchor != initial.anchor {
            commands.push(FlowCommand::SetShapeAnchor { at, anchor: state.anchor });
        }
        if text_effects != ibis_doc_model::CharacterFormat::default() && self.shape_text_body_at(at).is_some() {
            let range = self.box_edit_target(cx).0.and_then(|(target, from, to, _)| (target == at && from < to).then_some((from, to)));
            commands.push(FlowCommand::FormatShapeText { at, range, format: text_effects });
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let done = if amend {
            let result = self.document.amend_last(commands.into_iter().map(ibis_doc_model::Command::Flow).collect());
            if result.is_ok() {
                self.sync_peers(cx);
            }
            result.is_ok()
        } else {
            self.run_flow(commands, cx)
        };
        if done {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            if let Some(info) = self.text_box_at_caret() {
                let updated_frame = self.floating_text_box_frame_of(info.at);
                if let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    state.initial = info.clone();
                    state.typing = from_field;
                }
                if let Some(edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut())
                    && edit.at == info.at
                {
                    if let Some((page, frame, _)) = updated_frame {
                        edit.page = page;
                        edit.frame = frame;
                    }
                    let len = info.text.chars().count();
                    edit.edit.anchor = edit.edit.anchor.min(len);
                    edit.edit.caret = edit.edit.caret.min(len);
                    edit.edit.typing = false;
                    edit.edit.is_new = false;
                }
            }
            self.sync_docx_edited_box_resource();
        }
        cx.notify();
    }

    /// The panel's font size, colour, bold and italic given to the characters selected in its text only (DOCX-IMAGE-015); the
    /// rest of the text keeps its formatting. One step of undo.
    #[allow(dead_code)]
    pub(crate) fn apply_text_box_selection(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let text = state.text.read(cx).value().to_string();
        let range = state.text.read(cx).selected_range();
        let (from, to) = (text[..range.start.min(text.len())].chars().count(), text[..range.end.min(text.len())].chars().count());
        if from >= to {
            return;
        }
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (font_size, color) = (value(&state.font_size), value(&state.color));
        let (bold, italic) = (state.bold, state.italic);
        let initial = state.initial.clone();
        let at = initial.at;
        let size = font_size.parse::<f32>().ok().filter(|v| (1.0..=1638.0).contains(v)).map(|v| (v * 2.0).round() as u32 * 50);
        let color = (color.trim_start_matches('#').len() == 6).then(|| u32::from_str_radix(color.trim_start_matches('#'), 16).ok().map(Rgb)).flatten();
        let mut commands = Vec::new();
        if text != initial.text {
            commands.push(FlowCommand::SetShapeText { at, text: text.clone() });
        }
        commands.push(FlowCommand::SetShapeRangeFormat { at, from, to, bold: Some(bold), italic: Some(italic), size, color });
        let keep = self.text_selection();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            if let Some(info) = self.text_box_at_caret()
                && let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut())
            {
                state.initial = info;
            }
        }
        cx.notify();
    }

    /// 文字列の折り返し of the text box or shape at the caret (it floats: not inline).
    pub(crate) fn set_shape_wrap(&mut self, wrap: ibis_doc_model::PictureWrapType, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else { return };
        if self.run_flow(vec![FlowCommand::SetPictureWrap { at: info.at, wrap }], cx) {
            cx.notify();
        }
    }

    /// 図形のスタイル (DOCX-IMAGE-024): a choice of fill, line colour and line width; given to the panel's fields and applied in
    /// one step.
    pub(crate) fn apply_shape_style(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&(fill, line, width)) = SHAPE_STYLES.get(index) else { return };
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let (fill_input, line_input, width_input) = (state.fill.clone(), state.line.clone(), state.line_width.clone());
        fill_input.update(cx, |i, cx| i.set_value(fill, window, cx));
        line_input.update(cx, |i, cx| i.set_value(line, window, cx));
        width_input.update(cx, |i, cx| i.set_value(width, window, cx));
        self.apply_text_box(cx);
    }

    /// A right press at page point (`x`, `y`) of page `index`: when it is on a shape, a picture or a text box (the box's body, the hand
    /// band of a selected one, or the text of the box being edited), that object is the selection (a selected one stays, with the
    /// others of a multi-selection) and this returns true. The menu's 図形の書式設定… then has a shape to show (K0027).
    pub fn select_shape_for_menu(&mut self, index: usize, x: f32, y: f32, is_top: bool, cx: &mut Context<Self>) -> bool {
        // The text of the box being edited: the box stays as it is.
        if let Some(edit) = self.doc_state().and_then(|s| s.box_edit.as_ref())
            && edit.page == index
        {
            let (fx, fy, fw, fh) = edit.frame;
            if x >= fx && x <= fx + fw && y >= fy && y <= fy + fh {
                return true;
            }
        }
        let k = self.page_origin_pane(index, is_top).map_or(1.0, |(_, k)| k);
        let slop = crate::shell::selection_handles::BORDER_SLOP_PX / k;
        // The hand band of a selected shape keeps the selection (the same test as the hand cursor).
        if self.doc_state().is_some_and(|s| s.shape_selected)
            && self.all_selected_shapes().iter().any(|(page, f, _, _)| *page == index && crate::shell::selection_handles::on_border(*f, 0, (x, y), slop))
        {
            return true;
        }
        let Some((at, _, _)) = self.floating_text_box_at(index, x, y) else { return false };
        let already = self.doc_state().is_some_and(|s| s.shape_selected && (s.selected_shapes.contains(&at) || self.selected_text_box_frame().is_some_and(|(_, _, a, _)| a == at)));
        if !already {
            let keep_multi = Vec::new();
            self.set_caret(at, false, cx);
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = true;
                state.selected_shapes = keep_multi;
            }
        }
        true
    }

    /// 図形の書式設定… of the context menu: the 図形の書式 pane for the shape or text box at the caret (its text box panel), or for the
    /// selected picture the picture's pane.
    pub fn open_doc_shape_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_box_at_caret().is_some() {
            self.open_text_box(window, cx);
        } else if self.picture_at_caret().is_some() {
            self.open_picture_edit(window, cx);
        }
    }

    pub fn text_box_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use crate::shell::shape_format_pane as pane;
        let state = self.doc_state()?.text_box.as_ref()?;
        let (floating, has_text, is_line) = (state.initial.floating, state.initial.has_text, state.initial.is_line);
        let (head, tail_arrow) = (state.head, state.tail);
        let fields = pane::ShapeFormatFields {
            unit: pane::SizeUnit::Point,
            width: state.geometry[0].clone(),
            height: state.geometry[1].clone(),
            rotation: None,
            left: floating.then(|| state.geometry[2].clone()),
            top: floating.then(|| state.geometry[3].clone()),
            fill: (!is_line).then(|| state.fill.clone()),
            line: Some(state.line.clone()),
            line_width: Some(state.line_width.clone()),
            font_size: has_text.then(|| state.font_size.clone()),
            text_color: has_text.then(|| state.color.clone()),
        };
        let dash_ui = state.dash_ui;
        let tab = state.effects.tab;
        let enabled = super::text_box_effects::enabled_flags(state);
        let mut lead: Vec<AnyElement> = Vec::new();
        if !is_line {
            lead.push(v_flex().gap_1().child(div().text_sm().child(crate::shell::i18n::text("docx.text-box-panel.shape-styles"))).child(h_flex().gap_1().flex_wrap().children(SHAPE_STYLES.iter().enumerate().map(|(i, &(f, l, _))| {
                let hex = |c: &str| u32::from_str_radix(c, 16).unwrap_or(0);
                div().id(("shape-style", i)).w(px(26.)).h(px(20.)).rounded_sm().border_2().border_color(rgb(hex(l))).bg(rgb(hex(f))).cursor_pointer().on_click(cx.listener(move |this, _, window, cx| this.apply_shape_style(i, window, cx)))
            }))).into_any_element());
        }
        let mut after_place: Vec<AnyElement> = Vec::new();
        if floating {
            after_place.push(h_flex().flex_wrap().gap_1().children(WRAPS.iter().map(|&(id, label, wrap)| crate::shell::chrome::action_button(id).small().label(crate::shell::i18n::text(label)).on_click(cx.listener(move |this, _, _, cx| this.set_shape_wrap(wrap, cx))))).into_any_element());
        }
        let toggle = |id: &'static str, label: &'static str, on: bool, f: fn(&mut TextBoxState)| {
            crate::shell::chrome::checkbox_button(id,crate::shell::i18n::text(label),on,false,cx.listener(move |this, _, _, cx| {
                if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    f(state);
                }
                this.apply_text_box(cx);
            }))
        };
        let after_line: Vec<AnyElement> = is_line.then(|| h_flex().gap_1().child(toggle("line-head", "docx.text-box-panel.arrow-start", head, |s| s.head = !s.head)).child(toggle("line-tail", "docx.text-box-panel.arrow-end", tail_arrow, |s| s.tail = !s.tail)).into_any_element()).into_iter().collect();
        let hooks = pane::ShapeFormatHooks {
            set_tab: |e, index| { if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) { s.effects.tab = index; } },
            dash_ui: |e| e.doc_state_mut().and_then(|d| d.text_box.as_mut()).map(|s| &mut s.dash_ui),
            apply: |e, _, cx| e.apply_text_box(cx),
        };
        let state = self.doc_state()?.text_box.as_ref()?;
        let spec = pane::ShapeFormatSpec {
            id: "docx",
            plain_title_key: "shape.effects.panel-title",
            tab,
            fields,
            hooks,
            dash: Some(dash_ui),
            effects: Some(pane::ShapeFormatEffects { editor: &state.effects, enabled, hooks: super::text_box_effects::hooks(), scroll: self.sidebar_scroll.clone() }),
            lead,
            after_place,
            after_line,
            tail: Vec::new(),
        };
        Some(pane::shape_format_panel(spec, cx))
    }
}
