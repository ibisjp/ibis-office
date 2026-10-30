// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shapes, text boxes, and pictures on a worksheet: insertion, ordering, and manipulation.

use gpui_kit::*;
use ibis_doc_model::{
    CellValue, Color, Command, Emu, EmuRect, Fill, LineDash, Paragraph, Run, RunStyle,
    Shadow, Shape, ShapeId, ShapeKind, SheetCommand, Stroke, TextBody, TextStyle, TileAlign,
};
use crate::shell::editor::SlideEditor;
use crate::shell::editor::sheet_of;

/// A new text box's fill (a pale yellow, `RRGGBB`) and the width of its outline (0.75 pt, in EMU).
const NEW_TEXT_BOX_FILL: u32 = 0xFFFFE5;
const NEW_TEXT_BOX_LINE_WIDTH: Emu = 9_525;

/// Default preset shapes available in the worksheet Insert > Shapes menu:
/// (preset, item id, label, icon).
pub(crate) const SHEET_SHAPES: &[(&str, &str, &str, &str)] = &[
    ("rect", "xlsx.shape.rect", "", "rectangle"),
    ("roundRect", "xlsx.shape.round-rect", "", "rounded_corner"),
    ("ellipse", "xlsx.shape.ellipse", "", "circle"),
    ("triangle", "xlsx.shape.triangle", "", "change_history"),
    ("rightArrow", "xlsx.shape.right-arrow", "", "arrow_forward"),
    ("star5", "xlsx.shape.star5", "", "star"),
    ("wedgeRectCallout", "xlsx.shape.callout", "", "chat_bubble"),
    ("heart", "xlsx.shape.heart", "", "favorite"),
];

impl SlideEditor {
    /// Calculate insertion coordinates (x, y in EMU) on the worksheet.
    /// Prefers the top-left of the currently selected cell, or a default visible offset.
    pub(crate) fn sheet_insertion_origin(&self) -> (i64, i64) {
        if let Some(((col, row), (shape, sheet))) = self.current_cell().zip(self.current_sheet()) {
            let xs = sheet.column_offsets();
            let ys = sheet.row_offsets();
            let left = shape.frame.x + xs.get(col as usize).copied().unwrap_or(0);
            let top = shape.frame.y + ys.get(row as usize).copied().unwrap_or(0);
            (left, top)
        } else {
            (952_500, 952_500) // ~100px from top-left (100 * 9525 EMU)
        }
    }

    /// Insert a text box on the worksheet at the current selection.
    pub(crate) fn insert_sheet_textbox(&mut self, window: &mut Window, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 1_000_000);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        // A new text box has a pale yellow fill and a thin black outline (0.75 pt).
        let body = TextBody::plain("", TextStyle::default());
        let mut auto = ibis_doc_model::AutoShape::text_box(Fill::Solid(Color::opaque(ibis_doc_model::Rgb(NEW_TEXT_BOX_FILL))), body);
        auto.stroke = Some(Stroke::solid(Color::BLACK, NEW_TEXT_BOX_LINE_WIDTH));
        let shape = Shape::new(id, frame, ShapeKind::Auto(auto));
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        self.begin_edit(id, false, window, cx);
        cx.notify();
        id
    }

    /// Returns the IDs of all worksheet shapes currently being edited or selected that can hold text (`Auto` or `TextBox`).
    pub(crate) fn selected_sheet_text_shape_ids(&self) -> Vec<ShapeId> {
        let raw_ids: Vec<ShapeId> = if let Some(edit) = self.editing.as_ref() {
            vec![edit.shape]
        } else {
            let mut ids = self.selected_ids();
            if ids.is_empty()
                && let Some(id) = self.selected_sheet_shape()
            {
                ids.push(id);
            }
            ids
        };
        raw_ids
            .into_iter()
            .filter(|id| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect()
    }

    /// Whether one or more worksheet text shapes (`Auto` or `TextBox`) are currently selected or being edited in place.
    pub(crate) fn has_selected_sheet_text_shapes(&self) -> bool {
        !self.selected_sheet_text_shape_ids().is_empty()
    }

    /// Returns the active DrawingML underline kind of the edited or primary selected worksheet shape.
    pub(crate) fn selected_sheet_shape_underline(&self) -> Option<ibis_doc_model::Underline> {
        if let Some(edit) = self.editing.as_ref()
            && let Some(body) = self.slide().shape(edit.shape).and_then(|s| s.text_body())
        {
            let caret = edit.edit.range().start;
            let mut pos = 0usize;
            let mut found = None;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if caret <= pos + n {
                        found = Some(&r.style);
                        break;
                    }
                    pos += n;
                }
                if found.is_some() {
                    break;
                }
                if pos == caret {
                    found = p.runs.last().map(|r| &r.style).or(Some(&p.end_style));
                    break;
                }
                pos += 1;
            }
            let s = found.or_else(|| body.paragraphs.first().and_then(|p| p.runs.first().map(|r| &r.style)))?;
            return s.underline.then_some(s.underline_kind);
        }
        let ids = self.selected_sheet_text_shape_ids();
        let primary_id = self
            .selected_sheet_shape()
            .filter(|id| ids.contains(id))
            .or_else(|| ids.first().copied())?;
        let body = self.slide().shape(primary_id).and_then(|s| s.text_body())?;
        let s = body
            .paragraphs
            .iter()
            .flat_map(|p| &p.runs)
            .find(|r| !r.text.is_empty())
            .or_else(|| body.paragraphs.first().and_then(|p| p.runs.first()))
            .map(|r| &r.style)
            .or_else(|| body.paragraphs.first().and_then(|p| Some(&p.end_style)))?;
        s.underline.then_some(s.underline_kind)
    }

    /// Returns the active paragraph list/bullet kind of the edited or primary selected worksheet shape.
    pub(crate) fn selected_sheet_shape_list(&self) -> Option<ibis_doc_model::ListKind> {
        if let Some(edit) = self.editing.as_ref()
            && let Some(body) = self.slide().shape(edit.shape).and_then(|s| s.text_body())
        {
            let caret = edit.edit.range().start;
            let mut pos = 0usize;
            for p in &body.paragraphs {
                let len: usize = p.runs.iter().map(|r| r.text.chars().count()).sum();
                if caret <= pos + len {
                    return Some(ibis_doc_model::ListKind::of(p.bullet.as_ref()));
                }
                pos += len + 1;
            }
            return body.paragraphs.last().map(|p| ibis_doc_model::ListKind::of(p.bullet.as_ref()));
        }
        let ids = self.selected_sheet_text_shape_ids();
        let primary_id = self
            .selected_sheet_shape()
            .filter(|id| ids.contains(id))
            .or_else(|| ids.first().copied())?;
        let body = self.slide().shape(primary_id).and_then(|s| s.text_body())?;
        body.paragraphs.first().map(|p| ibis_doc_model::ListKind::of(p.bullet.as_ref()))
    }

    /// Mutate `TextBody` across all currently edited or selected worksheet text shapes (`Auto` or `TextBox`),
    /// initializing `TextBody` on `ShapeKind::Auto` shapes that do not yet have one.
    pub(crate) fn mutate_selected_shape_text(&mut self, mutator: impl Fn(&mut TextBody), cx: &mut Context<Self>) -> bool {
        let ids = self.selected_sheet_text_shape_ids();
        if ids.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            if let Some(body) = self.slide().shape(shape_id).and_then(|s| s.text_body()).cloned() {
                let mut next = body;
                mutator(&mut next);
                commands.push(Command::SetTextBody { slide, shape: shape_id, body: next });
            } else if let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned() {
                let mut next_auto = auto;
                let mut body = TextBody::plain("", TextStyle::default());
                mutator(&mut body);
                next_auto.text = Some(body);
                commands.push(Command::SetKind {
                    slide,
                    shape: shape_id,
                    kind: Box::new(ShapeKind::Auto(next_auto)),
                });
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        true
    }

    /// Apply a `CharacterFormat` to the edited range or to all text of all selected worksheet text shapes.
    pub(crate) fn apply_sheet_shape_character_format(&mut self, format: ibis_doc_model::CharacterFormat, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            self.selected_sheet_text_shape_ids().into_iter().map(|id| (id, 0..0)).collect()
        };
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = targets
            .into_iter()
            .filter(|(id, _)| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect();
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for (shape_id, range) in targets {
            let (from, to) = (range.start, range.end);
            if self.slide().shape(shape_id).and_then(|s| s.text_body()).is_none()
                && let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned()
            {
                let mut next_auto = auto;
                next_auto.text = Some(TextBody::plain("", TextStyle::default()));
                commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
            }
            let char_len = self
                .slide()
                .shape(shape_id)
                .and_then(|s| s.text_body())
                .map_or(0, |b| b.text().chars().count());
            let (start, end) = if from < to { (from, to) } else { (0, char_len) };
            if from == to {
                let mut style = self
                    .slide()
                    .shape(shape_id)
                    .and_then(|s| s.text_body())
                    .map(|b| b.style.clone())
                    .unwrap_or_default();
                let mut changed = false;
                if let Some(b) = format.bold {
                    style.bold = b;
                    changed = true;
                }
                if let Some(i) = format.italic {
                    style.italic = i;
                    changed = true;
                }
                if let Some(sz) = format.font_size {
                    style.font_size = sz;
                    changed = true;
                }
                if let Some(ref f) = format.font {
                    style.font = f.clone();
                    changed = true;
                }
                if let Some(c) = format.color {
                    style.color = c;
                    changed = true;
                }
                if changed {
                    commands.push(Command::SetTextStyle { slide, shape: shape_id, style });
                }
            }
            commands.push(Command::SetCharacterFormat {
                slide,
                shape: shape_id,
                start,
                end,
                format: format.clone(),
            });
        }
        self.run_sheet(commands, cx);
        true
    }

    /// Apply a `ParagraphFormat` to the edited range or to all paragraphs of all selected worksheet text shapes.
    pub(crate) fn apply_sheet_shape_paragraph_format(&mut self, format: ibis_doc_model::ParagraphFormat, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            self.selected_sheet_text_shape_ids().into_iter().map(|id| (id, 0..0)).collect()
        };
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = targets
            .into_iter()
            .filter(|(id, _)| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect();
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for (shape_id, range) in targets {
            let (from, to) = (range.start, range.end);
            if self.slide().shape(shape_id).and_then(|s| s.text_body()).is_none()
                && let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned()
            {
                let mut next_auto = auto;
                next_auto.text = Some(TextBody::plain("", TextStyle::default()));
                commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
            }
            let char_len = self
                .slide()
                .shape(shape_id)
                .and_then(|s| s.text_body())
                .map_or(0, |b| b.text().chars().count());
            let (start, end) = if from < to { (from, to) } else { (0, char_len) };
            if from == to
                && let Some(align) = format.align
            {
                let mut style = self
                    .slide()
                    .shape(shape_id)
                    .and_then(|s| s.text_body())
                    .map(|b| b.style.clone())
                    .unwrap_or_default();
                style.align = align;
                commands.push(Command::SetTextStyle { slide, shape: shape_id, style });
            }
            commands.push(Command::SetParagraphFormat {
                slide,
                shape: shape_id,
                start,
                end,
                format: format.clone(),
            });
        }
        self.run_sheet(commands, cx);
        true
    }

    /// Set or clear a DrawingML underline style (`ibis_doc_model::Underline`) on edited or selected worksheet shapes.
    pub(crate) fn set_sheet_shape_underline(&mut self, kind: Option<ibis_doc_model::Underline>, cx: &mut Context<Self>) -> bool {
        self.apply_sheet_shape_character_format(
            ibis_doc_model::CharacterFormat {
                underline: Some(kind.is_some()),
                underline_kind: kind,
                ..Default::default()
            },
            cx,
        )
    }

    /// Set or clear text highlight (background) color on edited or selected worksheet shapes.
    pub(crate) fn set_sheet_shape_highlight(&mut self, color: Option<ibis_doc_model::Rgb>, cx: &mut Context<Self>) -> bool {
        self.apply_sheet_shape_character_format(
            ibis_doc_model::CharacterFormat {
                highlight: Some(color),
                ..Default::default()
            },
            cx,
        )
    }

    /// Set a specific paragraph list/bullet kind on edited or selected worksheet shapes.
    pub(crate) fn set_sheet_shape_list(&mut self, list: ibis_doc_model::ListKind, cx: &mut Context<Self>) -> bool {
        self.apply_sheet_shape_paragraph_format(
            ibis_doc_model::ParagraphFormat {
                list: Some(list),
                ..Default::default()
            },
            cx,
        )
    }

    /// Toggle a paragraph list/bullet kind (bullet, numbered, or checklist) on edited or selected worksheet shapes.
    pub(crate) fn toggle_sheet_shape_list(&mut self, target: ibis_doc_model::ListKind, cx: &mut Context<Self>) -> bool {
        use ibis_doc_model::ListKind;
        let cur = self.selected_sheet_shape_list().unwrap_or(ListKind::None);
        let next = match target {
            ListKind::Bullet => {
                if matches!(cur, ListKind::Bullet) || matches!(cur, ListKind::Symbol(c) if c != '☐' && c != '☑') {
                    ListKind::None
                } else {
                    ListKind::Bullet
                }
            }
            ListKind::Numbered => {
                if matches!(cur, ListKind::Numbered | ListKind::Scheme(_)) {
                    ListKind::None
                } else {
                    ListKind::Numbered
                }
            }
            ListKind::Symbol('☐') => {
                if matches!(cur, ListKind::Symbol('☐' | '☑')) {
                    ListKind::None
                } else {
                    ListKind::Symbol('☐')
                }
            }
            other => {
                if cur == other {
                    ListKind::None
                } else {
                    other
                }
            }
        };
        self.set_sheet_shape_list(next, cx)
    }

    /// Format the selected text (or whole text if no selection) inside worksheet shape(s) being edited in place or selected.
    pub(crate) fn format_sheet_shape_text(&mut self, change: &ibis_doc_model::StyleChange, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            let mut ids = self.selected_ids();
            if ids.is_empty()
                && let Some(id) = self.selected_sheet_shape()
            {
                ids.push(id);
            }
            ids.into_iter().map(|id| (id, 0..0)).collect()
        };
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = targets
            .into_iter()
            .filter(|(id, _)| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect();
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;

        use ibis_doc_model::{
            CellAlign, CellUnderline, CellVerticalAlign, CharacterFormat, ParagraphFormat, Strike,
            StyleChange, TextAlign, Underline, VerticalAnchor,
        };

        match change {
            StyleChange::Fill(opt_rgb) => {
                let fill = match opt_rgb {
                    Some(c) => Fill::Solid(Color::opaque(*c)),
                    None => Fill::None,
                };
                self.fill_sheet_shape(fill, cx);
                return true;
            }
            StyleChange::FillTheme(rgb, _) => {
                self.fill_sheet_shape(Fill::Solid(Color::opaque(*rgb)), cx);
                return true;
            }
            StyleChange::Vertical(v) => {
                let anchor = match v {
                    CellVerticalAlign::Top => VerticalAnchor::Top,
                    CellVerticalAlign::Center => VerticalAnchor::Middle,
                    CellVerticalAlign::Bottom
                    | CellVerticalAlign::Justify
                    | CellVerticalAlign::Distributed => VerticalAnchor::Bottom,
                };
                let mut commands = Vec::new();
                for &(shape_id, _) in &targets {
                    if let Some(body) = self.slide().shape(shape_id).and_then(|s| s.text_body()).cloned() {
                        let mut next = body;
                        next.anchor = anchor;
                        commands.push(Command::SetTextBody { slide, shape: shape_id, body: next });
                    } else if let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned() {
                        let mut next_auto = auto;
                        let mut body = TextBody::plain("", TextStyle::default());
                        body.anchor = anchor;
                        next_auto.text = Some(body);
                        commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
                    }
                }
                if !commands.is_empty() {
                    self.run_sheet(commands, cx);
                }
                return true;
            }
            StyleChange::Wrap(wrap) => {
                let mut commands = Vec::new();
                for &(shape_id, _) in &targets {
                    if let Some(body) = self.slide().shape(shape_id).and_then(|s| s.text_body()).cloned() {
                        let mut next = body;
                        next.wrap = *wrap;
                        commands.push(Command::SetTextBody { slide, shape: shape_id, body: next });
                    } else if let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned() {
                        let mut next_auto = auto;
                        let mut body = TextBody::plain("", TextStyle::default());
                        body.wrap = *wrap;
                        next_auto.text = Some(body);
                        commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
                    }
                }
                if !commands.is_empty() {
                    self.run_sheet(commands, cx);
                }
                return true;
            }
            StyleChange::Align(cell_align) => {
                let align = match cell_align {
                    CellAlign::Left | CellAlign::General | CellAlign::Fill => TextAlign::Left,
                    CellAlign::Center | CellAlign::CenterAcross => TextAlign::Center,
                    CellAlign::Right => TextAlign::Right,
                    CellAlign::Justify => TextAlign::Justify,
                    CellAlign::Distributed => TextAlign::Distributed,
                };
                let mut commands = Vec::new();
                for (shape_id, range) in &targets {
                    let shape_id = *shape_id;
                    let (from, to) = (range.start, range.end);
                    if self.slide().shape(shape_id).and_then(|s| s.text_body()).is_none()
                        && let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned()
                    {
                        let mut next_auto = auto;
                        let mut style = TextStyle::default();
                        style.align = align;
                        next_auto.text = Some(TextBody::plain("", style));
                        commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
                    }
                    let char_len = self.slide().shape(shape_id).and_then(|s| s.text_body()).map_or(0, |b| b.text().chars().count());
                    let (start, end) = if from < to { (from, to) } else { (0, char_len) };
                    if from == to
                        && let Some(mut style) = self.slide().shape(shape_id).and_then(|s| s.text_body()).map(|b| b.style.clone())
                    {
                        style.align = align;
                        commands.push(Command::SetTextStyle { slide, shape: shape_id, style });
                    }
                    commands.push(Command::SetParagraphFormat {
                        slide,
                        shape: shape_id,
                        start,
                        end,
                        format: ParagraphFormat { align: Some(align), ..Default::default() },
                    });
                }
                self.run_sheet(commands, cx);
                return true;
            }
            StyleChange::Indent(_) => {
                return self.step_sheet_shape_indent(true, cx);
            }
            _ => {}
        }

        let (primary_id, primary_range) = targets
            .iter()
            .find(|(id, _)| Some(*id) == self.selected_sheet_shape())
            .cloned()
            .unwrap_or_else(|| targets[0].clone());
        let (p_from, p_to) = (primary_range.start, primary_range.end);
        let p_char_len = self.slide().shape(primary_id).and_then(|s| s.text_body()).map_or(0, |b| b.text().chars().count());
        let p_start = if p_from < p_to { p_from } else { 0.min(p_char_len) };
        let (cur_bold, cur_italic, cur_underline, cur_strike) = self
            .slide()
            .shape(primary_id)
            .and_then(|s| s.text_body())
            .map(|body| {
                let mut pos = 0;
                for p in &body.paragraphs {
                    for r in &p.runs {
                        let n = r.text.chars().count();
                        if p_start < pos + n || (n > 0 && p_start == pos && pos == 0) {
                            return (
                                r.style.bold.unwrap_or(body.style.bold),
                                r.style.italic.unwrap_or(body.style.italic),
                                r.style.underline,
                                r.style.strike != ibis_doc_model::Strike::None,
                            );
                        }
                        pos += n;
                    }
                    pos += 1;
                }
                (body.style.bold, body.style.italic, false, false)
            })
            .unwrap_or((false, false, false, false));

        let mut format = CharacterFormat::default();
        match change {
            StyleChange::Bold(_) => format.bold = Some(!cur_bold),
            StyleChange::Italic(_) => format.italic = Some(!cur_italic),
            StyleChange::Underline(_) => {
                let on = !cur_underline;
                format.underline = Some(on);
                if on {
                    format.underline_kind = Some(Underline::Single);
                }
            }
            StyleChange::UnderlineKind(u) => match u {
                CellUnderline::None => format.underline = Some(false),
                CellUnderline::Double | CellUnderline::DoubleAccounting => {
                    format.underline = Some(true);
                    format.underline_kind = Some(Underline::Double);
                }
                CellUnderline::Single | CellUnderline::SingleAccounting => {
                    format.underline = Some(true);
                    format.underline_kind = Some(Underline::Single);
                }
            },
            StyleChange::Strike(_) => {
                format.strike = Some(if !cur_strike { Strike::Single } else { Strike::None });
            }
            StyleChange::FontSize(s) => format.font_size = Some(*s),
            StyleChange::Font(f) => format.font = Some(f.clone()),
            StyleChange::Color(c) | StyleChange::ColorTheme(c, _) => format.color = Some(*c),
            _ => return false,
        }
        let mut commands = Vec::new();
        for (shape_id, range) in targets {
            let (from, to) = (range.start, range.end);
            if self.slide().shape(shape_id).and_then(|s| s.text_body()).is_none()
                && let Some(ShapeKind::Auto(auto)) = self.slide().shape(shape_id).map(|s| &s.kind).cloned()
            {
                let mut next_auto = auto;
                let mut style = TextStyle::default();
                if let Some(b) = format.bold {
                    style.bold = b;
                }
                if let Some(i) = format.italic {
                    style.italic = i;
                }
                if let Some(s) = format.font_size {
                    style.font_size = s;
                }
                if let Some(f) = format.font.clone() {
                    style.font = f;
                }
                if let Some(c) = format.color {
                    style.color = c;
                }
                next_auto.text = Some(TextBody::plain("", style));
                commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
            }
            let char_len = self.slide().shape(shape_id).and_then(|s| s.text_body()).map_or(0, |b| b.text().chars().count());
            let (start, end) = if from < to { (from, to) } else { (0, char_len) };
            if from == to
                && let Some(mut style) = self.slide().shape(shape_id).and_then(|s| s.text_body()).map(|b| b.style.clone())
            {
                if let Some(b) = format.bold {
                    style.bold = b;
                }
                if let Some(i) = format.italic {
                    style.italic = i;
                }
                if let Some(s) = format.font_size {
                    style.font_size = s;
                }
                if let Some(ref f) = format.font {
                    style.font = f.clone();
                }
                if let Some(c) = format.color {
                    style.color = c;
                }
                commands.push(Command::SetTextStyle { slide, shape: shape_id, style });
            }
            commands.push(Command::SetCharacterFormat { slide, shape: shape_id, start, end, format: format.clone() });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        true
    }

    /// Step each targeted run's font size individually inside worksheet shape(s) being edited or selected.
    pub(crate) fn step_sheet_shape_font_size(&mut self, larger: bool, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            let mut ids = self.selected_ids();
            if ids.is_empty()
                && let Some(id) = self.selected_sheet_shape()
            {
                ids.push(id);
            }
            ids.into_iter().map(|id| (id, 0..0)).collect()
        };
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = targets
            .into_iter()
            .filter(|(id, _)| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect();
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for (shape_id, range) in targets {
            let Some(shape) = self.slide().shape(shape_id).cloned() else {
                continue;
            };
            let Some(body) = shape.text_body().cloned() else {
                if let ShapeKind::Auto(auto) = shape.kind {
                    let mut next_auto = auto;
                    let mut style = TextStyle::default();
                    style.font_size = ibis_doc_model::step_font_size(style.font_size, larger);
                    next_auto.text = Some(TextBody::plain("", style));
                    commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
                }
                continue;
            };
            let (from, to) = (range.start, range.end);
            let char_len = body.text().chars().count();
            let (start, end) = if from < to { (from, to) } else { (0, char_len) };
            let default_size = body.style.font_size;
            if from == to {
                let mut style = body.style.clone();
                style.font_size = ibis_doc_model::step_font_size(style.font_size, larger);
                commands.push(Command::SetTextStyle { slide, shape: shape_id, style });
            }
            let mut pos = 0usize;
            let mut any_run = false;
            for p in &body.paragraphs {
                if p.runs.is_empty() {
                    if start <= pos && pos <= end {
                        let cur = p.end_style.font_size.unwrap_or(default_size);
                        let next = ibis_doc_model::step_font_size(cur, larger);
                        commands.push(Command::SetCharacterFormat {
                            slide,
                            shape: shape_id,
                            start: pos,
                            end: pos,
                            format: ibis_doc_model::CharacterFormat { font_size: Some(next), ..Default::default() },
                        });
                        any_run = true;
                    }
                } else {
                    for r in &p.runs {
                        let n = r.text.chars().count();
                        let r_start = pos;
                        let r_end = pos + n;
                        let seg_start = start.max(r_start);
                        let seg_end = end.min(r_end);
                        if seg_start < seg_end || (n == 0 && start <= r_start && r_start <= end) {
                            let cur = r.style.font_size.unwrap_or(default_size);
                            let next = ibis_doc_model::step_font_size(cur, larger);
                            commands.push(Command::SetCharacterFormat {
                                slide,
                                shape: shape_id,
                                start: seg_start,
                                end: seg_end,
                                format: ibis_doc_model::CharacterFormat { font_size: Some(next), ..Default::default() },
                            });
                            any_run = true;
                        }
                        pos += n;
                    }
                }
                pos += 1;
            }
            if !any_run {
                let next = ibis_doc_model::step_font_size(default_size, larger);
                commands.push(Command::SetCharacterFormat {
                    slide,
                    shape: shape_id,
                    start,
                    end,
                    format: ibis_doc_model::CharacterFormat { font_size: Some(next), ..Default::default() },
                });
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        true
    }

    /// Step paragraph indentation inside worksheet shape(s) being edited or selected.
    pub(crate) fn step_sheet_shape_indent(&mut self, more: bool, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            let mut ids = self.selected_ids();
            if ids.is_empty()
                && let Some(id) = self.selected_sheet_shape()
            {
                ids.push(id);
            }
            ids.into_iter().map(|id| (id, 0..0)).collect()
        };
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = targets
            .into_iter()
            .filter(|(id, _)| {
                self.slide()
                    .shape(*id)
                    .is_some_and(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            })
            .collect();
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for (shape_id, range) in targets {
            let Some(shape) = self.slide().shape(shape_id).cloned() else {
                continue;
            };
            if shape.text_body().is_none()
                && let ShapeKind::Auto(auto) = shape.kind.clone()
            {
                let mut next_auto = auto;
                next_auto.text = Some(TextBody::plain("", TextStyle::default()));
                commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(next_auto)) });
            }
            let char_len = shape.text_body().map_or(0, |b| b.text().chars().count());
            let (start, end) = if range.start < range.end { (range.start, range.end) } else { (0, char_len) };
            commands.push(Command::SetParagraphFormat {
                slide,
                shape: shape_id,
                start,
                end,
                format: ibis_doc_model::ParagraphFormat {
                    indent_by: Some(if more { 457_200 } else { -457_200 }),
                    ..Default::default()
                },
            });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        true
    }

    /// Insert an auto shape preset on the worksheet.
    pub(crate) fn insert_sheet_shape(&mut self, preset: &str, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 1_200_000);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = crate::pptx::editing::new_shape(id, preset, frame);
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        id
    }

    /// Insert a line or arrow line on the worksheet.
    pub(crate) fn insert_sheet_line(&mut self, arrow: bool, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 0);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = crate::pptx::editing::new_line(id, frame, false, false, arrow);
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        id
    }

    /// Insert an image file on the worksheet as a floating picture.
    pub(crate) fn insert_sheet_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ShapeId> {
        self.insert_sheet_picture_impl(name,data,false,cx)
    }
    pub(crate) fn paste_sheet_picture(&mut self,name:&str,data:Vec<u8>,cx:&mut Context<Self>)->Option<ShapeId>{self.insert_sheet_picture_impl(name,data,true,cx)}
    fn insert_sheet_picture_impl(&mut self,name:&str,data:Vec<u8>,paste:bool,cx:&mut Context<Self>)->Option<ShapeId>{
        let stamp=crate::shell::paste_placement::stamp(&data);
        let format = ibis_doc_model::MediaFormat::from_path(name);
        let (w, h) = crate::pptx::editing::validated_picture_size(format, &data)?;
        let (x, y) = self.sheet_insertion_origin();
        // 96 DPI: 1 px = 9,525 EMU. Cap initial dimensions to 4,000,000 EMU (~420px).
        let max_dim = 4_000_000i64;
        let mut w_emu = w as i64 * 9_525;
        let mut h_emu = h as i64 * 9_525;
        if w_emu > max_dim || h_emu > max_dim {
            let scale = (max_dim as f64 / w_emu as f64).min(max_dim as f64 / h_emu as f64);
            w_emu = (w_emu as f64 * scale) as i64;
            h_emu = (h_emu as f64 * scale) as i64;
        }
        let frame = EmuRect::new(x, y, w_emu, h_emu);
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let id = self.document.ids().new_shape_id();
        let picture = ibis_doc_model::Picture {
            media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: None,
            alt_text: Default::default(),
            decorative: false,
            name: String::new(),
            shadow: None,
            reflection: None,
            soft_edge: None, geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
        let mut shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        if paste{self.position_sheet_paste(std::slice::from_mut(&mut shape),stamp);}
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        Some(id)
    }

    /// Pick a picture file asynchronously and insert it onto the worksheet.
    pub(crate) fn pick_sheet_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| {
                    this.insert_sheet_picture(&name, data, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| {
                    this.insert_sheet_picture(&name, data, cx);
                })
                .ok();
            })
            .detach();
        }
    }

    /// Insert multiple images into cells sequentially starting at (start_col, start_row).
    pub(crate) fn insert_sheet_pictures_in_cells(
        &mut self,
        items: Vec<(String, Vec<u8>)>,
        start_col: u32,
        start_row: u32,
        cx: &mut Context<Self>,
    ) {
        if items.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for (i, (name, data)) in items.into_iter().enumerate() {
            let row = start_row + i as u32;
            let column = start_col;
            let format = ibis_doc_model::MediaFormat::from_path(&name);
            let Some((w, h)) = crate::pptx::editing::validated_picture_size(format, &data) else { continue };
            let media = self.document.ids().add_media(format, data.clone());
            self.resources.add_picture(media, format, &data);
            let id = self.document.ids().new_shape_id();
            let picture = ibis_doc_model::Picture {
                media,
                crop: Default::default(),
                stroke: None,
                corner: 0,
                duotone: None,
                clear: None,
                luminance: None,
                clip: None,
                alt_text: Default::default(),
                decorative: false,
                name: String::new(),
                shadow: None,
                reflection: None,
                soft_edge: None,
                geometry: None,
                effects: Default::default(),
                tile: None,
                fill_rect: None,
                artistic_effect: None,
            };
            let initial_frame = EmuRect::new(0, 0, (w as i64 * 9525).max(1), (h as i64 * 9525).max(1));
            let shape = Shape::new(id, initial_frame, ShapeKind::Picture(picture));
            let index = self.slide().shapes.len() + i;
            commands.push(Command::InsertShape { slide, index, shape });
            commands.push(Command::Sheet(SheetCommand::PlacePictureInCell { slide, shape: id, row, column }));
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    /// Pick multiple picture files asynchronously and insert them into cells sequentially.
    pub(crate) fn pick_sheet_pictures_in_cells(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (col, row) = self.current_cell().unwrap_or((0, 0));
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: Vec::new(),
                multiple: true,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let mut items = Vec::new();
                for path in paths {
                    if let Ok(data) = std::fs::read(&path) {
                        let name = path.to_string_lossy().into_owned();
                        items.push((name, data));
                    }
                }
                this.update(cx, |this, cx| {
                    this.insert_sheet_pictures_in_cells(items, col, row, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| {
                    this.insert_sheet_pictures_in_cells(vec![(name, data)], col, row, cx);
                })
                .ok();
            })
            .detach();
        }
    }

    /// Convert a floating picture shape into an in-cell image at the cell beneath it (or current cell).
    pub(crate) fn place_picture_in_cell(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(shape) = self.slide().shapes.iter().find(|s| s.id == shape_id) else { return };
        if !matches!(shape.kind, ShapeKind::Picture(_)) { return };
        let (col, row) = if let Some((_, sheet)) = self.current_sheet() {
            let x_pos = shape.frame.x + (shape.frame.width / 4).min(95_250).max(1);
            let y_pos = shape.frame.y + (shape.frame.height / 4).min(95_250).max(1);
            let xs = sheet.column_offsets();
            let ys = sheet.row_offsets();
            let c = xs.windows(2).position(|w| x_pos >= w[0] && x_pos < w[1]).map(|i| i as u32);
            let r = ys.windows(2).position(|w| y_pos >= w[0] && y_pos < w[1]).map(|i| i as u32);
            c.zip(r).or_else(|| self.current_cell()).unwrap_or((0, 0))
        } else {
            self.current_cell().unwrap_or((0, 0))
        };
        self.run_sheet(vec![Command::Sheet(SheetCommand::PlacePictureInCell { slide, shape: shape_id, row, column: col })], cx);
        cx.notify();
    }

    /// Convert an in-cell image into a floating picture over cells.
    pub(crate) fn place_picture_over_cells(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(shape) = self.slide().shapes.iter().find(|s| s.id == shape_id) else { return };
        let x_pos = shape.frame.x + (shape.frame.width / 4).min(95_250).max(1);
        let y_pos = shape.frame.y + (shape.frame.height / 4).min(95_250).max(1);
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();
        let col = xs.windows(2).position(|w| x_pos >= w[0] && x_pos < w[1]).map(|i| i as u32).unwrap_or(0);
        let row = ys.windows(2).position(|w| y_pos >= w[0] && y_pos < w[1]).map(|i| i as u32).unwrap_or(0);

        let mut commands = vec![Command::Sheet(SheetCommand::PlacePictureOverCells { slide, shape: shape_id, row, column: col })];
        if shape.frame.width < 1_428_750 || shape.frame.height < 1_428_750 {
            let mut enlarged = shape.frame;
            enlarged.width = enlarged.width.max(1_428_750);
            enlarged.height = enlarged.height.max(1_428_750);
            commands.push(Command::SetFrame { slide, shape: shape_id, frame: enlarged });
        }
        self.run_sheet(commands, cx);
        self.set_selection(vec![shape_id]);
        cx.notify();
    }

    /// Convert the in-cell picture in the active cell into a floating picture over cells.
    pub(crate) fn place_current_cell_picture_over_cells(&mut self, cx: &mut Context<Self>) {
        let Some((col, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        if !sheet.cell_images.iter().any(|i| i.column == col && i.row == row) { return };
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();
        let col_idx = col as usize;
        let row_idx = row as usize;
        if col_idx + 1 >= xs.len() || row_idx + 1 >= ys.len() { return };
        let (x0, y0) = (xs[col_idx], ys[row_idx]);
        let (x1, y1) = (xs[col_idx + 1], ys[row_idx + 1]);
        let shape_id = self.slide().shapes.iter().find_map(|s| {
            if matches!(s.kind, ShapeKind::Picture(_)) {
                let cx_pos = s.frame.x + s.frame.width / 2;
                let cy_pos = s.frame.y + s.frame.height / 2;
                if (x0..=x1).contains(&cx_pos) && (y0..=y1).contains(&cy_pos) {
                    Some(s.id)
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some(id) = shape_id {
            self.place_picture_over_cells(id, cx);
        }
    }

    /// Order a selected worksheet shape (front, back, forward, backward).
    pub(crate) fn order_sheet_shape(&mut self, order: crate::pptx::editing::Order, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let shapes = &self.slide().shapes;
        let Some(pos) = shapes.iter().position(|s| s.id == shape_id) else { return };
        // shapes[0] is the sheet itself. Drawing shapes occupy positions 1..shapes.len().
        if shapes.len() <= 2 {
            return; // At most one floating shape; ordering changes nothing.
        }
        let new_pos = match order {
            crate::pptx::editing::Order::Front => shapes.len() - 1,
            crate::pptx::editing::Order::Back => 1,
            crate::pptx::editing::Order::Forward => (pos + 1).min(shapes.len() - 1),
            crate::pptx::editing::Order::Backward => if pos > 1 { pos - 1 } else { 1 },
        };
        if new_pos != pos {
            self.run_sheet(vec![Command::MoveShape { slide, shape: shape_id, index: new_pos }], cx);
        }
    }

    /// Align a selected worksheet shape (Left, Center, Right, Top, Middle, Bottom).
    pub(crate) fn align_sheet_shape(&mut self, align: crate::pptx::editing::Align, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let shapes = &self.slide().shapes;
        let Some(target) = shapes.iter().find(|s| s.id == shape_id) else { return };

        // Floating shapes are shapes[1..]
        let floating: Vec<&Shape> = shapes.iter().skip(1).collect();
        if floating.is_empty() {
            return;
        }

        let bounds = if floating.len() > 1 {
            let min_x = floating.iter().map(|s| s.frame.x).min().unwrap_or(0);
            let min_y = floating.iter().map(|s| s.frame.y).min().unwrap_or(0);
            let max_r = floating.iter().map(|s| s.frame.right()).max().unwrap_or(0);
            let max_b = floating.iter().map(|s| s.frame.bottom()).max().unwrap_or(0);
            EmuRect::new(min_x, min_y, (max_r - min_x).max(0), (max_b - min_y).max(0))
        } else {
            let (origin_x, origin_y) = self.sheet_insertion_origin();
            EmuRect::new(origin_x, origin_y, target.frame.width, target.frame.height)
        };

        let mut new_frame = target.frame;
        match align {
            crate::pptx::editing::Align::Left => new_frame.x = bounds.x,
            crate::pptx::editing::Align::Center => new_frame.x = bounds.x + (bounds.width - target.frame.width) / 2,
            crate::pptx::editing::Align::Right => new_frame.x = bounds.right() - target.frame.width,
            crate::pptx::editing::Align::Top => new_frame.y = bounds.y,
            crate::pptx::editing::Align::Middle => new_frame.y = bounds.y + (bounds.height - target.frame.height) / 2,
            crate::pptx::editing::Align::Bottom => new_frame.y = bounds.bottom() - target.frame.height,
        }

        if new_frame != target.frame {
            self.run_sheet(vec![Command::SetFrame { slide, shape: shape_id, frame: new_frame }], cx);
        }
    }

    /// Distribute worksheet shapes evenly (horizontal or vertical) across 3 or more shapes.
    pub(crate) fn distribute_sheet_shapes(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut floating: Vec<Shape> = self.slide().shapes.iter().skip(1).cloned().collect();
        if floating.len() < 3 {
            return;
        }

        if horizontal {
            floating.sort_by_key(|s| s.frame.x);
            let first_x = floating[0].frame.x;
            let last_r = floating.last().map(|s| s.frame.right()).unwrap_or(first_x);
            let total_w: i64 = floating.iter().map(|s| s.frame.width).sum();
            let gap = (last_r - first_x - total_w) as f64 / (floating.len() - 1) as f64;

            let mut at = first_x as f64;
            let mut commands = Vec::new();
            for shape in floating {
                let target_x = at.round() as i64;
                if shape.frame.x != target_x {
                    let mut frame = shape.frame;
                    frame.x = target_x;
                    commands.push(Command::SetFrame { slide, shape: shape.id, frame });
                }
                at += shape.frame.width as f64 + gap;
            }
            if !commands.is_empty() {
                self.run_sheet(commands, cx);
            }
        } else {
            floating.sort_by_key(|s| s.frame.y);
            let first_y = floating[0].frame.y;
            let last_b = floating.last().map(|s| s.frame.bottom()).unwrap_or(first_y);
            let total_h: i64 = floating.iter().map(|s| s.frame.height).sum();
            let gap = (last_b - first_y - total_h) as f64 / (floating.len() - 1) as f64;

            let mut at = first_y as f64;
            let mut commands = Vec::new();
            for shape in floating {
                let target_y = at.round() as i64;
                if shape.frame.y != target_y {
                    let mut frame = shape.frame;
                    frame.y = target_y;
                    commands.push(Command::SetFrame { slide, shape: shape.id, frame });
                }
                at += shape.frame.height as f64 + gap;
            }
            if !commands.is_empty() {
                self.run_sheet(commands, cx);
            }
        }
    }

    /// 選択中の図形の塗りつぶしを変更.
    pub(crate) fn fill_sheet_shape(&mut self, fill: Fill, cx: &mut Context<Self>) {
        let mut ids = self.selected_ids();
        if ids.is_empty()
            && let Some(id) = self.selected_sheet_shape()
        {
            ids.push(id);
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            if !matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)) {
                continue;
            }
            commands.push(Command::SetFill { slide, shape: shape_id, fill: fill.clone() });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形の枠線の色を変更（None で枠線なし）.
    pub(crate) fn stroke_sheet_shape_color(&mut self, color: Option<Color>, cx: &mut Context<Self>) {
        let mut ids = self.selected_ids();
        if ids.is_empty()
            && let Some(id) = self.selected_sheet_shape()
        {
            ids.push(id);
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            let current_stroke = match &shape.kind {
                ShapeKind::Auto(a) => a.stroke.clone(),
                ShapeKind::Picture(p) => p.stroke.clone(),
                ShapeKind::Line(l) => Some(l.clone()),
                _ => None,
            };
            let new_stroke = match (current_stroke, color) {
                (_, None) => None,
                (Some(mut s), Some(c)) => {
                    s.color = c;
                    Some(s)
                }
                (None, Some(c)) => Some(Stroke::solid(c, Stroke::DEFAULT_WIDTH)),
            };
            commands.push(Command::SetStroke { slide, shape: shape_id, stroke: new_stroke });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形の枠線の太さを変更.
    pub(crate) fn stroke_sheet_shape_width(&mut self, width: Emu, cx: &mut Context<Self>) {
        let mut ids = self.selected_ids();
        if ids.is_empty()
            && let Some(id) = self.selected_sheet_shape()
        {
            ids.push(id);
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            let current_stroke = match &shape.kind {
                ShapeKind::Auto(a) => a.stroke.clone(),
                ShapeKind::Picture(p) => p.stroke.clone(),
                ShapeKind::Line(l) => Some(l.clone()),
                _ => None,
            };
            let mut s = current_stroke.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
            s.width = width;
            commands.push(Command::SetStroke { slide, shape: shape_id, stroke: Some(s) });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形の枠線のダッシュパターン（実線・破線・点線）を変更.
    pub(crate) fn stroke_sheet_shape_dash(&mut self, dash: LineDash, cx: &mut Context<Self>) {
        let mut ids = self.selected_ids();
        if ids.is_empty()
            && let Some(id) = self.selected_sheet_shape()
        {
            ids.push(id);
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            let current_stroke = match &shape.kind {
                ShapeKind::Auto(a) => a.stroke.clone(),
                ShapeKind::Picture(p) => p.stroke.clone(),
                ShapeKind::Line(l) => Some(l.clone()),
                _ => None,
            };
            let s = current_stroke.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH)).with_dash(dash);
            commands.push(Command::SetStroke { slide, shape: shape_id, stroke: Some(s) });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形の影（Shadow）効果を設定.
    pub(crate) fn shadow_sheet_shape(&mut self, shadow: Option<Shadow>, cx: &mut Context<Self>) {
        let mut ids = self.selected_ids();
        if ids.is_empty()
            && let Some(id) = self.selected_sheet_shape()
        {
            ids.push(id);
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        for shape_id in ids {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            let ShapeKind::Auto(auto) = &shape.kind else { continue };
            if auto.shadow == shadow {
                continue;
            }
            let mut new_auto = auto.clone();
            new_auto.shadow = shadow;
            commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(new_auto)) });
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 単一の図形を選択.
    pub(crate) fn select_sheet_shape(&mut self, id: ShapeId) {
        self.set_selection(vec![id]);
    }

    /// 複数の図形を一括選択.
    #[allow(dead_code)]
    pub(crate) fn select_sheet_shapes(&mut self, ids: &[ShapeId]) {
        self.set_selection(ids.to_vec());
    }

    /// 図形の選択状態をトグル（Shift+クリック等用）.
    pub(crate) fn toggle_select_sheet_shape(&mut self, id: ShapeId) {
        let mut ids = self.selected_ids();
        if ids.contains(&id) {
            ids.retain(|&x| x != id);
        } else {
            ids.push(id);
        }
        self.set_selection(ids);
    }

    /// 図形選択を全解除.
    #[allow(dead_code)]
    pub(crate) fn clear_sheet_shape_selection(&mut self) {
        self.set_selection(Vec::new());
    }

    /// テキストボックス・図形内の文字の太字をトグル.
    pub(crate) fn format_sheet_text_bold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Bold(true), cx);
    }

    /// テキストボックス・図形内の文字の斜体をトグル.
    pub(crate) fn format_sheet_text_italic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Italic(true), cx);
    }

    /// テキストボックス・図形内の文字の下線をトグル.
    pub(crate) fn format_sheet_text_underline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Underline(true), cx);
    }

    /// テキストボックス・図形内の文字の取り消し線をトグル.
    pub(crate) fn format_sheet_text_strikethrough(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Strike(true), cx);
    }

    /// テキストボックス・図形内の文字色を変更.
    pub(crate) fn format_sheet_text_color(&mut self, color: ibis_doc_model::Rgb, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Color(color), cx);
    }

    /// テキストボックス・図形内のフォントサイズを変更 (pt 単位).
    pub(crate) fn format_sheet_text_size(&mut self, size_pt: u32, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::FontSize(size_pt * 100), cx);
    }

    /// テキストボックス・図形内の段落配置を変更.
    pub(crate) fn format_sheet_text_align(&mut self, align: ibis_doc_model::TextAlign, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let cell_align = match align {
            ibis_doc_model::TextAlign::Left => ibis_doc_model::CellAlign::Left,
            ibis_doc_model::TextAlign::Center => ibis_doc_model::CellAlign::Center,
            ibis_doc_model::TextAlign::Right => ibis_doc_model::CellAlign::Right,
            ibis_doc_model::TextAlign::Justify => ibis_doc_model::CellAlign::Justify,
            ibis_doc_model::TextAlign::Distributed => ibis_doc_model::CellAlign::Distributed,
        };
        self.format_sheet_shape_text(&ibis_doc_model::StyleChange::Align(cell_align), cx);
    }

    /// テキストボックス・図形の上下方向（垂直）の配置を変更.
    pub(crate) fn format_sheet_text_vertical(&mut self, anchor: ibis_doc_model::VerticalAnchor, cx: &mut Context<Self>) {
        self.mutate_selected_shape_text(|b| b.anchor = anchor, cx);
        self.sync_workbook_peers(cx);
    }

    /// テキストボックス・図形の文字列の方向を変更.
    pub(crate) fn format_sheet_text_direction(&mut self, vertical: bool, sideways: i32, cx: &mut Context<Self>) {
        self.mutate_selected_shape_text(|b| {
            b.vertical = vertical;
            if vertical {
                b.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
            }
            b.sideways = sideways;
        }, cx);
        self.sync_workbook_peers(cx);
    }

    /// テキストボックス・図形の内余白を変更.
    pub(crate) fn format_sheet_text_insets(&mut self, insets: ibis_doc_model::Insets, cx: &mut Context<Self>) {
        self.mutate_selected_shape_text(|b| b.insets = insets, cx);
        self.sync_workbook_peers(cx);
    }

    /// テキストボックス・図形内のテキスト折り返しをトグル.
    pub(crate) fn format_sheet_text_wrap(&mut self, cx: &mut Context<Self>) {
        self.mutate_selected_shape_text(|b| b.wrap = !b.wrap, cx);
        self.sync_workbook_peers(cx);
    }

    /// 選択中の図形・画像・グラフのセルに対する配置方式を変更.
    pub(crate) fn set_sheet_shape_placement(&mut self, placement: ibis_doc_model::ObjectPlacement, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePlacement { slide, shape, placement }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形・画像・グラフの印刷指定を変更.
    pub(crate) fn set_sheet_shape_print(&mut self, print: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePrint { slide, shape, print }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形・画像・グラフの印刷指定をトグル.
    pub(crate) fn toggle_sheet_shape_print(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let cur_print = self.slide().shape_prints.get(&ids[0]).copied().unwrap_or(true);
        let new_print = !cur_print;
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePrint { slide, shape, print: new_print }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形の形状（プリセット geometry）を変更する (XLSX-OBJECT-002).
    pub(crate) fn change_sheet_shape(&mut self, preset: &str, cx: &mut Context<Self>) {
        let Some(geometry) = ibis_pptx::preset_geometry(preset) else { return };
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        let mut commands = Vec::new();
        for id in ids {
            let Some(shape) = self.slide().shape(id) else { continue };
            match &shape.kind {
                ShapeKind::Auto(a) if a.geometry != geometry => {
                    let mut next = a.clone();
                    next.geometry = geometry.clone();
                    commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(next)) });
                }
                ShapeKind::TextBox(tb) => {
                    let auto = ibis_doc_model::AutoShape {
                        geometry: geometry.clone(),
                        text: Some(tb.clone()),
                        fill: Fill::None,
                        stroke: None,
                        shadow: None,
                        tilt: None,
                        glow: None,
                        inner_shadow: None,
                        soft_edge: None,
                        reflection: None,
                        bevel: None,
                    };
                    commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(auto)) });
                }
                _ => {}
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形・テキストボックスにセル参照数式（textlink）を設定（または解除）する。
    pub(crate) fn set_selected_shape_formula(&mut self, formula: Option<&str>, cx: &mut Context<Self>) {
        let selected_ids = self.selected_ids();
        let Some(&shape_id) = selected_ids.first() else { return };
        let slide_id = self.current_slide;
        let formula_string = formula.map(|f| {
            let trimmed = f.trim();
            if trimmed.starts_with('=') { trimmed[1..].trim().to_string() } else { trimmed.to_string() }
        }).filter(|s| !s.is_empty());

        let mut commands = vec![Command::SetShapeFormula {
            slide: slide_id,
            shape: shape_id,
            formula: formula_string.clone(),
        }];

        if let Some(ref f) = formula_string {
            if let Some((sheet_name_opt, range)) = parse_shape_formula_range(f) {
                let target_info = {
                    let p = self.presentation();
                    let target_slide = match sheet_name_opt {
                        Some(ref name) => p.slides.iter().find(|s| s.name.as_deref().unwrap_or("").eq_ignore_ascii_case(name)),
                        None => p.slide(slide_id),
                    };
                    let target_sheet = target_slide.and_then(|ts| sheet_of(ts).map(|(_, s)| s.clone()));
                    let cur_slide = p.slide(slide_id);
                    let shape = cur_slide.and_then(|cs| cs.shape(shape_id));
                    let kind = shape.map(|s| s.kind.clone());
                    let frame = shape.map(|s| s.frame);
                    target_sheet.zip(kind).zip(frame).map(|((s, k), f)| (s, k, f))
                };
                if let Some((sheet, kind, frame)) = target_info {
                    match kind {
                        ShapeKind::Picture(mut pic) => {
                            if let Some(png) = super::copy_picture::render_sheet_range_to_png(&sheet, range) {
                                let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
                                self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
                                pic.media = media;
                                commands.push(Command::SetKind {
                                    slide: slide_id,
                                    shape: shape_id,
                                    kind: Box::new(ShapeKind::Picture(pic)),
                                });
                                let (w_emu, h_emu) = super::copy_picture::sheet_range_emu_size(&sheet, range);
                                let new_frame = EmuRect::new(frame.x, frame.y, w_emu, h_emu);
                                if new_frame != frame {
                                    commands.push(Command::SetFrame {
                                        slide: slide_id,
                                        shape: shape_id,
                                        frame: new_frame,
                                    });
                                }
                            }
                        }
                        _ => {
                            let text = cell_display_text(sheet.cell(range.first_row, range.first_column));
                            let body = ibis_doc_model::TextBody::plain(&text, ibis_doc_model::TextStyle::default());
                            commands.push(Command::SetTextBody {
                                slide: slide_id,
                                shape: shape_id,
                                body,
                            });
                        }
                    }
                }
            }
        }

        self.run_sheet(commands, cx);
    }

    /// 選択中の図形を現在選択されているセルと連動させる。
    pub(crate) fn link_selected_shape_to_current_cell(&mut self, cx: &mut Context<Self>) {
        let Some((col, row)) = self.current_cell() else { return };
        let ref_name = ibis_doc_model::cell_name(col, row);
        self.set_selected_shape_formula(Some(&ref_name), cx);
    }

    /// 選択中の図形のセル連動を解除する。
    pub(crate) fn clear_selected_shape_formula(&mut self, cx: &mut Context<Self>) {
        self.set_selected_shape_formula(None, cx);
    }

    /// ワークシート上の全図形の連動式（shape_formulas）を最新のセル値と同期する。
    pub(crate) fn sync_shape_formulas(&mut self, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let (text_updates, picture_updates) = {
            let p = self.presentation();
            let Some(slide) = p.slide(slide_id) else { return };
            if slide.shape_formulas.is_empty() {
                return;
            }

            let mut text_updates: Vec<(ibis_doc_model::ShapeId, String)> = Vec::new();
            let mut picture_updates: Vec<(ibis_doc_model::ShapeId, ibis_doc_model::Picture, EmuRect, Vec<u8>, (i64, i64))> = Vec::new();

            for (&shape_id, formula) in &slide.shape_formulas {
                let Some(shape) = slide.shape(shape_id) else { continue };
                let Some((sheet_name_opt, range)) = parse_shape_formula_range(formula) else { continue };
                let target_slide = match sheet_name_opt {
                    Some(ref name) => p.slides.iter().find(|s| s.name.as_deref().unwrap_or("").eq_ignore_ascii_case(name)),
                    None => Some(slide),
                };
                let Some(target_slide) = target_slide else { continue };
                let Some((_, sheet)) = sheet_of(target_slide) else { continue };

                match &shape.kind {
                    ShapeKind::Picture(pic) => {
                        if let Some(png) = super::copy_picture::render_sheet_range_to_png(sheet, range) {
                            let current_data = p.media(pic.media).map(|m| &*m.data);
                            if current_data != Some(&png) {
                                let (w_emu, h_emu) = super::copy_picture::sheet_range_emu_size(sheet, range);
                                picture_updates.push((shape_id, pic.clone(), shape.frame, png, (w_emu, h_emu)));
                            }
                        }
                    }
                    _ => {
                        let text = cell_display_text(sheet.cell(range.first_row, range.first_column));
                        let current_text = shape.text_body().map(|b| b.text()).unwrap_or_default();
                        if current_text != text {
                            text_updates.push((shape_id, text));
                        }
                    }
                }
            }
            (text_updates, picture_updates)
        };

        if text_updates.is_empty() && picture_updates.is_empty() {
            return;
        }

        let mut commands = Vec::new();
        for (shape_id, text) in text_updates {
            let body = ibis_doc_model::TextBody::plain(&text, ibis_doc_model::TextStyle::default());
            commands.push(Command::SetTextBody {
                slide: slide_id,
                shape: shape_id,
                body,
            });
        }

        for (shape_id, mut pic, frame, png, (w_emu, h_emu)) in picture_updates {
            let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
            self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
            pic.media = media;
            commands.push(Command::SetKind {
                slide: slide_id,
                shape: shape_id,
                kind: Box::new(ShapeKind::Picture(pic)),
            });
            let new_frame = EmuRect::new(frame.x, frame.y, w_emu, h_emu);
            if new_frame != frame {
                commands.push(Command::SetFrame {
                    slide: slide_id,
                    shape: shape_id,
                    frame: new_frame,
                });
            }
        }

        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// ワードアート (WordArt) をワークシートに挿入する。
    pub(crate) fn insert_sheet_wordart(&mut self, style_idx: usize, text: Option<&str>, cx: &mut Context<Self>) -> ShapeId {
        let preset = crate::pptx::editing::WORDART_PRESETS.get(style_idx).copied().unwrap_or(crate::pptx::editing::WORDART_PRESETS[0]);
        let id = self.document.ids().new_shape_id();
        let (x, y) = self.sheet_insertion_origin();
        let width = 3_810_000i64;
        let height = 952_500i64;
        let frame = EmuRect::new(x, y, width, height);

        let default_text = crate::shell::i18n::text("_xlsx.wordart-default-text");
        let initial_text = text.unwrap_or(&default_text);
        let mut style = TextStyle::default().with_font_size(3600);
        style.color = preset.color;
        style.bold = preset.bold;

        let shadow = preset.shadow.map(|(dir, dist, blur)| Shadow {
            color: Color { rgb: ibis_doc_model::Rgb(0), alpha: 102 },
            blur,
            distance: dist,
            direction: dir,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: TileAlign::TopLeft,
            rot_with_shape: true,
        });

        let run_style = RunStyle {
            font_size: Some(3600),
            bold: Some(preset.bold),
            color: Some(preset.color),
            outline: preset.outline,
            shadow,
            ..Default::default()
        };

        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![Paragraph {
            runs: vec![Run {
                text: initial_text.to_string(),
                style: run_style,
            }],
            ..Default::default()
        }];

        let shape = Shape::new(id, frame, ShapeKind::TextBox(body));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        id
    }

    /// 選択中またはインプレース編集中のワークシート図形・テキストボックスの文字列書式をクリアする。
    /// 対象となる図形が存在して処理した場合は `true` を返す。
    pub(crate) fn clear_sheet_shape_formats(&mut self, cx: &mut Context<Self>) -> bool {
        let targets: Vec<(ShapeId, std::ops::Range<usize>)> = if let Some(edit) = self.editing.as_ref() {
            vec![(edit.shape, edit.edit.range())]
        } else {
            let mut ids = self.selected_ids();
            if ids.is_empty()
                && let Some(id) = self.selected_sheet_shape()
            {
                ids.push(id);
            }
            ids.into_iter().map(|id| (id, 0..0)).collect()
        };
        if targets.is_empty() {
            return false;
        }
        let slide = self.current_slide;
        let mut commands = Vec::new();
        let default_style = TextStyle::default();
        for (shape_id, range) in targets {
            let Some(shape) = self.slide().shape(shape_id) else { continue };
            if !matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)) {
                continue;
            }
            let Some(mut body) = shape.text_body().cloned() else { continue };
            if range.start < range.end {
                if body.style != default_style {
                    let base = body.style.clone();
                    for p in &mut body.paragraphs {
                        for r in &mut p.runs {
                            if r.style.bold.is_none() && base.bold != default_style.bold {
                                r.style.bold = Some(base.bold);
                            }
                            if r.style.italic.is_none() && base.italic != default_style.italic {
                                r.style.italic = Some(base.italic);
                            }
                            if r.style.font_size.is_none() && base.font_size != default_style.font_size {
                                r.style.font_size = Some(base.font_size);
                            }
                            if r.style.font.is_none() && base.font != default_style.font {
                                r.style.font = base.font.clone();
                            }
                            if r.style.color.is_none() && base.color != default_style.color {
                                r.style.color = Some(base.color);
                            }
                        }
                    }
                    body.style = default_style.clone();
                }
                body.clear_run_formats(range.start, range.end);
            } else {
                body.style = default_style.clone();
                body.warp = None;
                for p in &mut body.paragraphs {
                    for r in &mut p.runs {
                        r.style = RunStyle {
                            field: r.style.field.clone(),
                            ..RunStyle::default()
                        };
                    }
                    p.end_style = RunStyle::default();
                    p.align = None;
                    p.bullet = None;
                    p.level = 0;
                    p.margin_left = 0;
                    p.indent = 0;
                    p.space_before = 0;
                    p.space_after = 0;
                    p.line_spacing = None;
                }
            }
            commands.push(Command::SetTextBody { slide, shape: shape_id, body });
        }
        if commands.is_empty() {
            return false;
        }
        self.run_sheet(commands, cx);
        cx.notify();
        true
    }

    /// 選択中のテキストまたは図形にワードアートスタイルを適用する。
    pub(crate) fn set_selected_sheet_wordart_style(&mut self, style_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selected_text_wordart_style(style_idx, window, cx);
        self.sync_workbook_peers(cx);
    }

    /// 文字の変形 (`a:prstTxWarp`) を設定・解除する。
    pub(crate) fn set_sheet_text_warp(&mut self, warp: Option<&str>, cx: &mut Context<Self>) {
        self.set_text_warp(warp, cx);
        self.sync_workbook_peers(cx);
    }

    /// 選択中のテキストボックス・図形の文字変形プリセット名を取得する。
    pub(crate) fn selected_sheet_text_warp(&self) -> Option<String> {
        self.selected_text_warp()
    }

    /// 図の圧縮 (xlsx.picture.compress):
    /// 選択中の図（またはシート上のすべての図）を指定の解像度で圧縮し、
    /// 必要に応じてトリミング部分を切り落とす。
    pub(crate) fn compress_sheet_pictures(
        &mut self,
        resolution: crate::pptx::compress::Resolution,
        delete_crop: bool,
        selected_only: bool,
        cx: &mut Context<Self>,
    ) -> (usize, usize, usize) {
        let (count, before, after) = self.compress_pictures(resolution, delete_crop, selected_only, cx);
        self.sync_workbook_peers(cx);
        let message = if count == 0 {
            crate::shell::i18n::text("pptx.compress.no-pictures-can-made-smaller").to_string()
        } else {
            let before_kb = before / 1024;
            let after_kb = after / 1024;
            crate::shell::i18n::format("pptx.compress.compressed-n-picture-s-before", &[
                ("n", &count),
                ("before", &before_kb),
                ("after", &after_kb),
            ])
        };
        self.status = Some(message.into());
        cx.notify();
        (count, before, after)
    }
}

fn cell_display_text(cell: Option<&ibis_doc_model::SheetCell>) -> String {
    cell.map_or(String::new(), |c| {
        if !c.text.is_empty() {
            c.text.clone()
        } else {
            match c.value {
                CellValue::Text(ref t) => t.clone(),
                CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                CellValue::Error(e) => e.as_str().to_string(),
                _ => String::new(),
            }
        }
    })
}

/// Presets for changing shapes in the worksheet Format > Change Shape menu:
/// (preset, item id, label, icon).
pub(crate) const CHANGE_SHAPES: &[(&str, &str, &str, &str)] = &[
    ("rect", "xlsx.change-shape.rect", "", "rectangle"),
    ("roundRect", "xlsx.change-shape.round-rect", "", "rounded_corner"),
    ("ellipse", "xlsx.change-shape.ellipse", "", "circle"),
    ("triangle", "xlsx.change-shape.triangle", "", "change_history"),
    ("rtTriangle", "xlsx.change-shape.rt-triangle", "", "change_history"),
    ("diamond", "xlsx.change-shape.diamond", "", "diamond"),
    ("pentagon", "xlsx.change-shape.pentagon", "", "pentagon"),
    ("hexagon", "xlsx.change-shape.hexagon", "", "hexagon"),
    ("rightArrow", "xlsx.change-shape.right-arrow", "", "arrow_forward"),
    ("leftArrow", "xlsx.change-shape.left-arrow", "", "arrow_back"),
    ("upArrow", "xlsx.change-shape.up-arrow", "", "arrow_upward"),
    ("downArrow", "xlsx.change-shape.down-arrow", "", "arrow_downward"),
    ("star5", "xlsx.change-shape.star5", "", "star"),
    ("wedgeRectCallout", "xlsx.change-shape.callout", "", "chat_bubble"),
    ("cloud", "xlsx.change-shape.cloud", "", "cloud"),
    ("heart", "xlsx.change-shape.heart", "", "favorite"),
];

/// セル参照数式文字列（`=A1`, `Sheet1!$B$3`, `A1`, `=$A$1:$C$5`, `Sheet1!$A$1:$D$5` 等）を解析する。
/// 返り値: (Option<シート名>, CellRange)
pub(crate) fn parse_shape_formula_range(raw: &str) -> Option<(Option<String>, ibis_doc_model::CellRange)> {
    let s = raw.strip_prefix('=').unwrap_or(raw).trim();
    if s.is_empty() {
        return None;
    }
    let (sheet_name, cell_part) = if let Some((sheet_part, rest)) = s.split_once('!') {
        let sheet_name = sheet_part.trim().trim_matches('\'').trim().to_string();
        (Some(sheet_name), rest.trim())
    } else {
        (None, s)
    };

    if let Some((start_str, end_str)) = cell_part.split_once(':') {
        let start = ibis_calc::formula::parser::parse_cell(start_str.trim())?;
        let end = ibis_calc::formula::parser::parse_cell(end_str.trim())?;
        let range = ibis_doc_model::CellRange {
            first_row: (start.row as u32).min(end.row as u32),
            last_row: (start.row as u32).max(end.row as u32),
            first_column: (start.column as u32).min(end.column as u32),
            last_column: (start.column as u32).max(end.column as u32),
        };
        Some((sheet_name, range))
    } else {
        let corner = ibis_calc::formula::parser::parse_cell(cell_part)?;
        let range = ibis_doc_model::CellRange {
            first_row: corner.row as u32,
            last_row: corner.row as u32,
            first_column: corner.column as u32,
            last_column: corner.column as u32,
        };
        Some((sheet_name, range))
    }
}

/// セル参照数式文字列（`=A1`, `Sheet1!$B$3`, `A1` 等）を解析する。
/// 返り値: (Option<シート名>, column: u32, row: u32)
#[allow(dead_code)]
pub(crate) fn parse_shape_formula(raw: &str) -> Option<(Option<String>, u32, u32)> {
    let (sheet_name, range) = parse_shape_formula_range(raw)?;
    Some((sheet_name, range.first_column, range.first_row))
}

impl SlideEditor {
    /// Insert a screenshot onto the worksheet (prefers clipboard, falls back to window snapshot).
    pub(crate) fn insert_sheet_screenshot(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(id) = self.insert_sheet_screenshot_from_clipboard(cx) {
            return Some(id);
        }
        self.insert_sheet_screenshot_window(cx)
    }

    /// Insert a screenshot specifically from the clipboard (image or copied image files).
    pub(crate) fn insert_sheet_screenshot_from_clipboard(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(item) = cx.read_from_clipboard() {
            for entry in item.entries() {
                match entry {
                    gpui::ClipboardEntry::Image(image) => {
                        let ext = match image.format {
                            gpui::ImageFormat::Png => "png",
                            gpui::ImageFormat::Jpeg => "jpg",
                            gpui::ImageFormat::Gif => "gif",
                            gpui::ImageFormat::Svg => "svg",
                            gpui::ImageFormat::Bmp => "bmp",
                            _ => "png",
                        };
                        let name = format!("Screenshot.{}", ext);
                        return self.insert_sheet_picture(&name, image.bytes.clone(), cx);
                    }
                    gpui::ClipboardEntry::ExternalPaths(paths) => {
                        for path in paths.paths() {
                            let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg"));
                            if let (true, Ok(data)) = (ok, std::fs::read(path)) {
                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Screenshot.png".into());
                                return self.insert_sheet_picture(&name, data, cx);
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
    pub(crate) fn insert_sheet_screenshot_window(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        let (data, _, _) = ibis_pptx::screenshot_snapshot(640, 360);
        self.insert_sheet_picture("Screenshot_Window.png", data, cx)
    }

    /// Insert a screen clipping screenshot (e.g. cropped screen area).
    pub(crate) fn insert_sheet_screenshot_clipping(&mut self, cx: &mut Context<Self>) -> Option<ShapeId> {
        if let Some(id) = self.insert_sheet_screenshot_from_clipboard(cx) {
            return Some(id);
        }
        let (data, _, _) = ibis_pptx::screenshot_snapshot(480, 270);
        self.insert_sheet_picture("Screen_Clipping.png", data, cx)
    }

    /// 挿入 > 3D モデル…: choose a 3D model file and place a placeholder on the worksheet.
    pub(crate) fn pick_sheet_3d_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    this.insert_sheet_3d_model(&path, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("pptx.3d-model.not-available-here"));
            cx.notify();
        }
    }

    /// Place a 3D model placeholder shape on the sheet at current selection.
    pub(crate) fn insert_sheet_3d_model(&mut self, path: &std::path::Path, cx: &mut Context<Self>) -> ShapeId {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("3D Model");
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 3_000_000, 1_500_000);
        let id = self.document.ids().new_shape_id();
        let shape = Shape::text_box(id, frame, format!("📦 {name}"));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        id
    }

    /// 挿入 > 署名欄…: place a signature line placeholder on the worksheet.
    pub(crate) fn insert_sheet_signature_line(&mut self, signer: &str, title: &str, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 3_000_000, 1_200_000);
        let id = self.document.ids().new_shape_id();
        let label = crate::shell::i18n::text("xlsx.signature-line");
        let text = if signer.is_empty() {
            format!("✕ _______________________________\n{label}") // i18n-ok: signature placeholder
        } else if title.is_empty() {
            format!("✕ _______________________________\n{signer}") // i18n-ok: signature line
        } else {
            format!("✕ _______________________________\n{signer}\n{title}") // i18n-ok: signature line
        };
        let shape = Shape::text_box(id, frame, text);
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.set_selection(vec![id]);
        cx.notify();
        id
    }
}



