// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テキスト ボックス (Word's 挿入 > テキスト ボックス): inserting a text box
//! at the caret, and the sidebar panel that edits the text of the text box
//! at the caret.

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind, Rgb, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the panel shows of a text box: where it is, its text, size, place and text format.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBoxInfo {
    pub(crate) at: FlowPos,
    pub(crate) text: String,
    /// Width, height and offset (points, as the panel's fields hold them).
    pub(crate) geometry: [String; 4],
    pub(crate) font_size: String,
    pub(crate) color: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) floating: bool,
    /// Whether it holds text (a text box); a drawn shape does not.
    pub(crate) has_text: bool,
    /// The fill and the line's colours (`RRGGBB`, empty for none) and the line's width (points).
    pub(crate) fill: String,
    pub(crate) line: String,
    pub(crate) line_width: String,
    /// A straight line (it has arrow ends, no fill).
    pub(crate) is_line: bool,
    /// The outline as it is (the line's, or the shape's), for a change to start from.
    pub(crate) stroke: Option<ibis_doc_model::Stroke>,
    /// The dash pattern of the outline, whether the fill is a gradient, whether the shape has a shadow.
    pub(crate) dash: ibis_doc_model::LineDash,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    /// A glow, soft edges, a reflection.
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    /// Vertical anchor of the text (top, middle, bottom).
    pub(crate) anchor: ibis_doc_model::VerticalAnchor,
    pub(crate) effects:super::text_box_effects::EffectModel,
}

/// The panel's fields, and the text box they edit.
pub(crate) struct TextBoxState {
    pub(crate) text: Entity<TextareaState>,
    /// Width, height, left and top (points).
    pub(crate) geometry: [Entity<InputState>; 4],
    pub(crate) font_size: Entity<InputState>,
    pub(crate) color: Entity<InputState>,
    pub(crate) fill: Entity<InputState>,
    pub(crate) line: Entity<InputState>,
    pub(crate) line_width: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// A line's arrows at its start and its end.
    pub(crate) head: bool,
    pub(crate) tail: bool,
    /// The outline's dash pattern, a gradient fill, a shadow.
    pub(crate) dash: ibis_doc_model::LineDash,
    pub(crate) line_dash_open: bool,
    pub(crate) hovered_dash: Option<ibis_doc_model::LineDash>,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    pub(crate) effects:super::text_box_effects::EffectEditor,
    pub(crate) anchor: ibis_doc_model::VerticalAnchor,
    /// What was shown when the panel opened or last applied: the changes are those from it.
    pub(crate) initial: TextBoxInfo,
    /// A field is being typed in: its next change joins the step the last one made (one step of undo per edit, not per key).
    pub(crate) typing: Option<EntityId>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

/// A text box's text being edited in place (a double click on it): the caret and selection
/// painted over the box on the page, using the same `TextEditing` + `CaretMarks` component as PowerPoint and Excel.
pub(crate) struct BoxEdit {
    pub(crate) at: FlowPos,
    pub(crate) page: usize,
    /// The box on its page (points).
    pub(crate) frame: (f32, f32, f32, f32),
    pub(crate) edit: crate::pptx::text_edit::TextEditing,
    /// Where the box's text was painted on the page (for caret/selection and mouse hit-testing).
    pub(crate) positions: crate::render::rich_text::TextPositions,
    pub(crate) goal_x: Option<(usize, f32)>,
    /// Page point `(x, y)` from a double-click that opened `BoxEdit`, to place the caret once painted.
    pub(crate) pending_page_click: Option<(f32, f32)>,
    /// Character anchor when dragging a selection inside the box.
    pub(crate) drag_anchor: Option<usize>,
}

fn handle_cursor(hx: i8, hy: i8) -> CursorStyle {
    match (hx, hy) {
        (-1, -1) | (1, 1) => CursorStyle::ResizeUpLeftDownRight,
        (1, -1) | (-1, 1) => CursorStyle::ResizeUpRightDownLeft,
        (0, -1) | (0, 1) => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeLeftRight,
    }
}

/// The outline's dash patterns the panel offers.
const DASHES: [ibis_doc_model::LineDash; 4] = [
    ibis_doc_model::LineDash::Solid,
    ibis_doc_model::LineDash::Dash,
    ibis_doc_model::LineDash::Dot,
    ibis_doc_model::LineDash::DashDot,
];

fn dash_label(dash: ibis_doc_model::LineDash) -> &'static str {
    match dash {
        ibis_doc_model::LineDash::Solid => "docx.text-box-panel.line-solid",
        ibis_doc_model::LineDash::Dash | ibis_doc_model::LineDash::LgDash | ibis_doc_model::LineDash::SysDash => "docx.text-box-panel.line-dash",
        ibis_doc_model::LineDash::Dot | ibis_doc_model::LineDash::SysDot => "docx.text-box-panel.line-dot",
        _ => "docx.text-box-panel.line-dash-dot",
    }
}

fn render_line_dash_graphic(dash: ibis_doc_model::LineDash, width_px: f32, color: Hsla) -> AnyElement {
    let mut row = h_flex().w(px(width_px)).h(px(12.)).items_center(). gap(px(3.));
    match dash {
        ibis_doc_model::LineDash::Solid => {
            row = row.child(div().w(px(width_px)).h(px(2.)).bg(color));
        }
        ibis_doc_model::LineDash::Dash | ibis_doc_model::LineDash::LgDash | ibis_doc_model::LineDash::SysDash => {
            let seg = 10.0;
            let count = ((width_px + 3.0) / (seg + 3.0)).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row.child(div().w(px(seg)).h(px(2.)).bg(color));
            }
        }
        ibis_doc_model::LineDash::Dot | ibis_doc_model::LineDash::SysDot => {
            let seg = 3.0;
            let count = ((width_px + 3.0) / (seg + 3.0)).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row.child(div().w(px(seg)).h(px(2.)).bg(color));
            }
        }
        _ => {
            let unit = 10.0 + 3.0 + 3.0 + 3.0;
            let count = ((width_px + 3.0) / unit).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row
                    .child(div().w(px(10.)).h(px(2.)).bg(color))
                    .child(div().w(px(3.)).h(px(2.)).bg(color));
            }
        }
    }
    row.into_any_element()
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
pub(crate) struct ShapeDrag {
    pub(crate) at: FlowPos,
    pub(crate) mode: DragMode,
    pub(crate) page: usize,
    /// Where the box was on its page when the drag began (points).
    pub(crate) frame: (f32, f32, f32, f32),
    /// Where the mouse went down on the page, and how far it has moved (points).
    pub(crate) start: (f32, f32),
    pub(crate) delta: (f32, f32),
    /// The box's offset from its anchor when it began (EMU).
    pub(crate) offset: (i64, i64),
    /// Whether the dragged target's corner handles preserve aspect ratio (e.g., pictures).
    pub(crate) keep_aspect_on_corners: bool,
    /// Whether the dragged target is an inline picture (`FlowRunKind::Picture` / `PictureControl` or non-floating `ShapeKind::Picture`).
    pub(crate) is_inline_picture: bool,
}

impl ShapeDrag {
    /// The box as the drag now has it (points): moved, or with its sides taken along.
    pub(crate) fn frame_now(&self) -> (f32, f32, f32, f32) {
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

impl SlideEditor {
    /// The text box (a shape with text) at the caret, or just before it.
    pub(crate) fn text_box_at_caret(&self) -> Option<TextBoxInfo> {
        let (start, _) = self.ordered_selection()?;
        self.text_box_near(start)
    }

    /// The text box at `start` (or just before it).
    pub(crate) fn text_box_near(&self, start: FlowPos) -> Option<TextBoxInfo> {
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
                        effects:match &shape.kind{ShapeKind::Auto(a)=>super::text_box_effects::EffectModel{fill:a.fill.clone(),shadow:a.shadow.clone(),glow:a.glow,soft:a.soft_edge,reflection:a.reflection.clone()},_=>Default::default()},
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
    pub(crate) fn floating_text_box_frame_of(&self, at: FlowPos) -> Option<(usize, (f32, f32, f32, f32), (i64, i64))> {
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
                    _ => {}
                }
            }
        }
        None
    }

    /// Returns `(is_picture, is_inline_picture)` for the run at `at`.
    pub(crate) fn is_picture_at(&self, at: FlowPos) -> (bool, bool) {
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
    pub(crate) fn selected_text_box_frame(&self) -> Option<(usize, (f32, f32, f32, f32), FlowPos, (i64, i64))> {
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
    pub(crate) fn all_selected_shapes(&self) -> Vec<(usize, (f32, f32, f32, f32), FlowPos, (i64, i64))> {
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
    pub(crate) fn shift_click_shape(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
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

    pub(crate) fn shape_text_body_at(&self, at: FlowPos) -> Option<&ibis_doc_model::TextBody> {
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
            let composing = self.composing_body(id);
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
    pub(crate) fn begin_shape_drag(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
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
                edit: crate::pptx::text_edit::TextEditing {
                    shape: shape_id,
                    anchor: len,
                    caret: len,
                    marked: None,
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
        let is_top = self.doc_split_active;
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
    pub(crate) fn commit_box_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// Character formatting (`Bold`, `Italic`, `Underline`, `Size`, `Color`, `Font`, `Highlight`, `Strike`)
    /// applied while editing a text box in place (`box_edit`), while a shape is selected (`shape_selected`),
    /// or to a selection inside the text box sidebar panel (`text_box`).
    pub(crate) fn apply_box_edit_run_change(&mut self, change: &ibis_doc_model::RunChange, cx: &mut Context<Self>) -> bool {
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
        let mut commands = Vec::new();
        if let Some(text) = sync_panel_text {
            commands.push(FlowCommand::SetShapeText { at, text });
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
                            let is_top = this.doc_split_active;
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
                            if let Some(q) = caret.filter(|_| crate::pptx::text_edit::caret_blink_visible()) {
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
                            let is_top = this.doc_split_active;
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
                    let is_top=this.doc_split_active;
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
    pub(crate) fn end_shape_drag(&mut self, cx: &mut Context<Self>) {
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
            out.push(crate::shell::selection_handles::border_region(format!("docx-frame-{x}-{y}"),Bounds::new(point(px(x*k),px(y*k)),size(px(w*k),px(h*k))),0,cx.listener(move|e,event:&MouseDownEvent,w,cx|{if let Some(edit)=e.doc_state_mut().and_then(|s|s.box_edit.as_mut()){edit.edit.is_new=false;}let mut event=event.clone();event.click_count=1;e.on_page_down(index,&event,w,e.doc_split_active,cx);})).into_any_element());
            let points=ibis_doc_model::ResizeHandle::ALL.map(|handle|{let (ax,ay)=handle.anchor();(handle,point(px((x+ax as f32*w/2.)*k),px((y+ay as f32*h/2.)*k)))});
            out.extend(crate::shell::selection_handles::group(format!("docx-shape-handles-{x}-{y}").into(),points,None,Default::default(),blue.into(),rgb(0xffffff).into()).into_iter().map(|(_,element)|element.into_any_element()));
        }
        out
    }

    /// 挿入 > テキスト ボックス: an empty text box at the caret, immediately entering in-place `TextArea` mode.
    pub(crate) fn doc_insert_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(crate) fn doc_insert_shape(&mut self, preset: &str, cx: &mut Context<Self>) {
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
    pub(crate) fn open_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let(effects,effect_subscriptions)=super::text_box_effects::EffectEditor::new(&info.effects,window,cx);
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
                dash: info.dash,
                line_dash_open: false,
                hovered_dash: None,
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
                let(editor,subs)=super::text_box_effects::EffectEditor::new(&info.effects,window,cx);
                if let Some(s)=self.doc_state_mut().and_then(|s|s.text_box.as_mut()){
                    let tab=s.effects.tab;let open=s.effects.open;s.effects=editor;s.effects.tab=tab;s.effects.open=open;s._subscriptions.extend(subs);
                    s.gradient=info.gradient;s.shadow=info.shadow;s.glow=info.glow;s.soft_edge=info.soft_edge;s.reflection=info.reflection;
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
    pub(crate) fn apply_text_box(&mut self, cx: &mut Context<Self>) {
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
        if state.dash != initial.dash && let Some(base) = initial.stroke.clone().or_else(|| hex(&line_now).map(|c| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(c), width))) {
            let mut stroke = base;
            if let Some(c) = hex(&line_now) {
                stroke.color = ibis_doc_model::Color::opaque(c);
            }
            stroke.width = width;
            commands.push(FlowCommand::SetShapeStroke { at, stroke: Some(stroke.with_dash(state.dash)) });
        }
        if !initial.is_line && let Some(effects)=super::text_box_effects::model(state,cx) {
            if effects!=initial.effects {commands.push(FlowCommand::SetShapeEffects{at,
                fill:(effects.fill!=initial.effects.fill).then_some(effects.fill),
                shadow:(effects.shadow!=initial.effects.shadow).then_some(effects.shadow),
                glow:(effects.glow!=initial.effects.glow).then_some(effects.glow),
                soft_edge:(effects.soft!=initial.effects.soft).then_some(effects.soft),
                reflection:(effects.reflection!=initial.effects.reflection).then_some(effects.reflection)});}
        }
        if new_size.is_some() || new_color.is_some() || new_bold.is_some() || new_italic.is_some() {
            commands.push(FlowCommand::SetShapeTextFormat { at, bold: new_bold, italic: new_italic, size: new_size, color: new_color });
        }
        if state.anchor != initial.anchor {
            commands.push(FlowCommand::SetShapeAnchor { at, anchor: state.anchor });
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let done = if amend {
            let result = self.document.amend_last(commands.into_iter().map(ibis_doc_model::Command::Flow).collect());
            if result.is_ok() {
                self.sync_document_peers(cx);
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

    pub(crate) fn text_box_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let tab=self.doc_state()?.text_box.as_ref()?.effects.tab;
        let tabs=crate::shell::chrome::tab_control_with_mode([crate::shell::i18n::text("shape.effects.basic"),crate::shell::i18n::text("shape.effects.decoration")],tab,"text-box-tabs",crate::shell::chrome::TabMode::Intrinsic,|e,index,cx|{if let Some(s)=e.doc_state_mut().and_then(|d|d.text_box.as_mut()){s.effects.tab=index;}cx.notify();},cx);
        if tab==1 {let body=crate::shell::chrome::panel_form().p_0().child(tabs).child(self.text_box_decoration_panel(cx)?);return Some(SidebarPanel{title:crate::shell::i18n::text("_shell.text-box"),body:body.into_any_element()});}
        let state = self.doc_state()?.text_box.as_ref()?;
        let (geometry, font_size, color, floating) = (state.geometry.clone(), state.font_size.clone(), state.color.clone(), state.initial.floating);
        let (fill, line, line_width, has_text, is_line) = (state.fill.clone(), state.line.clone(), state.line_width.clone(), state.initial.has_text, state.initial.is_line);
        let (head, tail) = (state.head, state.tail);
        let (dash, line_dash_open, hovered_dash, gradient, shadow) = (state.dash, state.line_dash_open, state.hovered_dash, state.gradient, state.shadow);
        let (glow, soft_edge, reflection) = (state.glow, state.soft_edge, state.reflection);
        let weak = cx.weak_entity();
        let row_el = |label: &'static str, field_el: AnyElement| {
            h_flex()
                .w_full()
                .justify_between()
                .gap_2()
                .items_center()
                .child(div().flex_shrink_0().text_sm().child(crate::shell::i18n::text(label)))
                .child(div().w(crate::shell::chrome::FIELD_WIDTH+px(30.)).flex_shrink_0().flex().justify_start().child(field_el))
        };
        let parse_rgb = |input: &Entity<InputState>, cx: &Context<Self>| -> Option<Rgb> {
            let v = input.read(cx).value();
            let s = v.trim().trim_start_matches('#');
            (s.len() == 6).then(|| u32::from_str_radix(s, 16).ok()).flatten().map(Rgb)
        };
        let fill_rgb = parse_rgb(&fill, cx);
        let line_rgb = parse_rgb(&line, cx);
        let color_rgb = parse_rgb(&color, cx);

        const STD_COLORS: &[(&str, u32, &str)] = &[
            ("_shell.black", 0x000000, "tb.color.black"),
            ("_shell.dark-red", 0xc00000, "tb.color.dark-red"),
            ("_shell.red", 0xff0000, "tb.color.red"),
            ("_shell.orange", 0xffc000, "tb.color.orange"),
            ("_shell.yellow", 0xffff00, "tb.color.yellow"),
            ("_shell.light-green", 0x92d050, "tb.color.light-green"),
            ("_shell.green", 0x00b050, "tb.color.green"),
            ("_shell.light-blue", 0x00b0f0, "tb.color.light-blue"),
            ("_shell.blue", 0x0070c0, "tb.color.blue"),
            ("_shell.dark-blue", 0x002060, "tb.color.dark-blue"),
            ("_shell.purple", 0x7030a0, "tb.color.purple"),
        ];
        const THEME_10: [Rgb; 10] = [
            Rgb(0xffffff),
            Rgb(0x000000),
            Rgb(0xe7e6e6),
            Rgb(0x44546a),
            Rgb(0x4472c4),
            Rgb(0xed7d31),
            Rgb(0xa5a5a5),
            Rgb(0xffc000),
            Rgb(0x5b9bd5),
            Rgb(0x70ad47),
        ];
        let std_list: Vec<(SharedString, Rgb, &'static str)> = STD_COLORS
            .iter()
            .map(|&(k, c, id)| (crate::shell::i18n::text(k), Rgb(c), id))
            .collect();
        let make_palette = |input: Entity<InputState>, current: Option<Rgb>, allow_none: bool| {
            let pick_input = input.clone();
            let pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(move |this, chosen, window, cx| {
                if let Some(c) = chosen {
                    this.note_recent_color(c);
                }
                let hex_str = chosen.map(|c| format!("{:06X}", c.0 & 0xffffff)).unwrap_or_default();
                pick_input.update(cx, |s, cx| s.set_value(hex_str, window, cx));
                this.apply_text_box(cx);
            });
            let none_item = allow_none.then(|| {
                let none_input = input.clone();
                crate::shell::chrome::MenuItem::new("tb.color.none", "_shell.no-fill")
                    .checked(current.is_none())
                    .on(move |this, window, cx| {
                        none_input.update(cx, |s, cx| s.set_value(String::new(), window, cx));
                        this.apply_text_box(cx);
                    })
            });
            crate::shell::color_palette::simple_palette(none_item, Some(&THEME_10), &std_list, current, pick)
        };

        let w_el = crate::shell::chrome::text_field(&geometry[0])
            .unit("pt",px(22.))
            .spinner("tb-w", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 9999.0).integer_only(), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        let h_el = crate::shell::chrome::text_field(&geometry[1])
            .unit("pt",px(22.))
            .spinner("tb-h", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 9999.0).integer_only(), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        let x_el = crate::shell::chrome::text_field(&geometry[2])
            .unit("pt",px(22.))
            .spinner("tb-x", crate::shell::chrome::SpinnerSpec::new(1.0).range(-9999.0, 9999.0).integer_only(), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        let y_el = crate::shell::chrome::text_field(&geometry[3])
            .unit("pt",px(22.))
            .spinner("tb-y", crate::shell::chrome::SpinnerSpec::new(1.0).range(-9999.0, 9999.0).integer_only(), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        const LINE_WIDTH_STEPS: &[f32] = &[0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0];
        let lw_el = crate::shell::chrome::text_field(&line_width)
            .unit("pt",px(22.))
            .spinner("tb-lw", crate::shell::chrome::SpinnerSpec::new(0.5).steps(LINE_WIDTH_STEPS).range(0.0, 100.0), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        const FONT_STEPS: &[f32] = &[6.0, 7.0, 8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0];
        let fs_el = crate::shell::chrome::text_field(&font_size)
            .unit("pt",px(22.))
            .spinner("tb-fs", crate::shell::chrome::SpinnerSpec::new(1.0).steps(FONT_STEPS).range(1.0, 1638.0), cx, |this, _, _, cx| this.apply_text_box(cx))
            .into_any_element();
        let fill_el = div()
            .w(crate::shell::chrome::FIELD_WIDTH)
            .child(crate::shell::chrome::color_text_field("tb-fill", &fill, fill_rgb, make_palette(fill.clone(), fill_rgb, true), &weak))
            .into_any_element();
        let line_el = div()
            .w(crate::shell::chrome::FIELD_WIDTH)
            .child(crate::shell::chrome::color_text_field("tb-line", &line, line_rgb, make_palette(line.clone(), line_rgb, true), &weak))
            .into_any_element();
        let color_el = div()
            .w(crate::shell::chrome::FIELD_WIDTH)
            .child(crate::shell::chrome::color_text_field("tb-color", &color, color_rgb, make_palette(color.clone(), color_rgb, false), &weak))
            .into_any_element();

        let theme = cx.theme();
        let fg = theme.foreground;
        let border_col = theme.border;
        let popover_bg = theme.popover;
        let accent_bg = theme.accent;
        let mut line_style_wrap = div().relative().w(crate::shell::chrome::FIELD_WIDTH+px(30.));
        let combo_btn = h_flex()
            .id("line-dash")
            .w(crate::shell::chrome::FIELD_WIDTH+px(30.))
            .h(px(28.))
            .px_2()
            .items_center()
            .justify_between()
            .rounded(px(4.))
            .border_1()
            .border_color(border_col)
            .bg(popover_bg)
            .cursor_pointer()
            .child(render_line_dash_graphic(dash, 84.0, fg))
            .child(gpui_kit::component::Icon::new(crate::shell::icons::Material("arrow_drop_down")).size(px(16.)).text_color(fg))
            .when(!line_dash_open, |el| {
                el.on_click(cx.listener(|this, _, _, cx| {
                    if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                        state.line_dash_open = true;
                        state.hovered_dash = None;
                    }
                    cx.notify();
                }))
            });
        line_style_wrap = line_style_wrap.child(combo_btn);
        if line_dash_open {
            let mut list_col = v_flex()
                .id("line-dash-list")
                .w(crate::shell::chrome::FIELD_WIDTH+px(30.))
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(border_col)
                .bg(popover_bg)
                .shadow_md()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                        state.line_dash_open = false;
                        state.hovered_dash = None;
                    }
                    cx.notify();
                }));
            for (idx, d) in DASHES.into_iter().enumerate() {
                let is_sel = dash == d;
                let is_hov = hovered_dash == Some(d);
                let mut item_row = h_flex()
                    .id(("line-dash-opt", idx))
                    .relative()
                    .w_full()
                    .h(px(26.))
                    .px_2()
                    .items_center()
                    .cursor_pointer()
                    .when(is_sel || is_hov, |el| el.bg(accent_bg))
                    .child(render_line_dash_graphic(d, 96.0, fg))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                            if *hovered {
                                state.hovered_dash = Some(d);
                            } else if state.hovered_dash == Some(d) {
                                state.hovered_dash = None;
                            }
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                            state.dash = d;
                            state.line_dash_open = false;
                            state.hovered_dash = None;
                        }
                        this.apply_text_box(cx);
                    }));
                if is_hov {
                    item_row = item_row.child(
                        div()
                            .absolute()
                            .right(px(128.))
                            .top(px(2.))
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(border_col)
                            .bg(popover_bg)
                            .shadow_sm()
                            .text_sm()
                            .whitespace_nowrap()
                            .child(crate::shell::i18n::text(dash_label(d))),
                    );
                }
                list_col = list_col.child(item_row);
            }
            line_style_wrap = line_style_wrap.child(
                deferred(
                    div()
                        .absolute()
                        .top(px(30.))
                        .left_0()
                        .child(list_col),
                )
                .with_priority(10),
            );
        }
        let line_style_el = line_style_wrap.into_any_element();

        let toggle = |id: &'static str, label: &'static str, on: bool, f: fn(&mut TextBoxState)| {
            crate::shell::chrome::checkbox_button(id,crate::shell::i18n::text(label),on,false,cx.listener(move |this, _, _, cx| {
                if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    f(state);
                }
                this.apply_text_box(cx);
            }))
        };

        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.)).p_0().child(tabs)
            .children((!is_line).then(|| v_flex().gap_1().child(div().text_sm().child(crate::shell::i18n::text("docx.text-box-panel.shape-styles"))).child(h_flex().gap_1().flex_wrap().children(SHAPE_STYLES.iter().enumerate().map(|(i, &(f, l, _))| {
                let hex = |c: &str| u32::from_str_radix(c, 16).unwrap_or(0);
                div().id(("shape-style", i)).w(px(26.)).h(px(20.)).rounded_sm().border_2().border_color(rgb(hex(l))).bg(rgb(hex(f))).cursor_pointer().on_click(cx.listener(move |this, _, window, cx| this.apply_shape_style(i, window, cx)))
            }))))
            )
            .child(row_el("docx.picture-panel.width-pt", w_el))
            .child(row_el("docx.picture-panel.height-pt", h_el))
            .children(floating.then(|| row_el("docx.text-box-panel.left-pt", x_el)))
            .children(floating.then(|| row_el("docx.text-box-panel.top-pt", y_el)))
            .children(floating.then(|| h_flex().flex_wrap().gap_1().children(WRAPS.iter().map(|&(id, label, wrap)| crate::shell::chrome::action_button(id).small().label(crate::shell::i18n::text(label)).on_click(cx.listener(move |this, _, _, cx| this.set_shape_wrap(wrap, cx)))))))
            .children((!is_line).then(|| row_el("docx.text-box-panel.fill-color", fill_el)))
            .child(row_el("docx.text-box-panel.outline-color", line_el))
            .child(row_el("docx.text-box-panel.line-width-pt", lw_el))
            .child(row_el("docx.text-box-panel.line-style", line_style_el))
            .children(is_line.then(|| h_flex().gap_1().child(toggle("line-head", "docx.text-box-panel.arrow-start", head, |s| s.head = !s.head)).child(toggle("line-tail", "docx.text-box-panel.arrow-end", tail, |s| s.tail = !s.tail))))
            .children(has_text.then(|| row_el("docx.text-box-panel.font-size", fs_el)))
            .children(has_text.then(|| row_el("docx.text-box-panel.text-color", color_el)));
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.text-box"), body: body.into_any_element() })
    }
}
