// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation's editing operations, as the menus, the toolbar, the keys
//! and the pointer ask for them: selection, tools, and edits turned into
//! `ibis_doc_model::Command`s (one undo step per operation). The views call
//! these; the commands and saving know nothing of the UI.

use gpui_kit::*;
use ibis_doc_model::{
    Caps, Color, Command, EmuRect, Fill, LayoutId, ObjectDefaults, Rgb, Shape, ShapeDefault, ShapeId, ShapeKind, SlideId, Stroke, TextBody, TextStyle,
};

use super::State;
use super::interaction;
use crate::shell::editor::SlideEditor;

// The shape and picture items that every format uses are in `shell/shape_items.rs`; the old names stay here.
pub(crate) use crate::shell::shape_items::*;
pub(crate) use crate::shell::shape_editing::*;
pub(crate) use crate::shell::shape_state::Tool;
use crate::ext::*;

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

#[ibis_office_macros::editor_ext(Editing)]
impl SlideEditor {
    pub fn pptx(&self) -> Option<&State> {
        self.format_state::<State>()
    }

    pub fn pptx_mut(&mut self) -> Option<&mut State> {
        self.format_state_mut::<State>()
    }

    pub(crate) fn cancel_pptx_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let mut cancelled = false;
        if self.is_presentation() {
            if let Some(drag) = self.shape_edit.drag.take() {
                match drag {
                    super::slide_view::Drag::Band { before, .. } => {
                        self.shape_edit.selection = before;
                    }
                    super::slide_view::Drag::Text { .. } => {
                        if let Some(edit) = self.shape_edit.edit.as_mut() {
                            edit.caret = edit.anchor;
                        }
                    }
                    _ => {}
                }
                cancelled = true;
            }
        }
        if let Some(s) = self.pptx_mut() {
            if let Some(pane_drag) = s.pane_drag.take() {
                match pane_drag {
                    super::notes::PaneDrag::List { w0, .. } => {
                        s.pane_width = Some(w0);
                    }
                    super::notes::PaneDrag::Notes { h0, .. } => {
                        s.notes_height = h0;
                        s.notes_touched.to_bottom = false;
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

    pub(crate) fn pptx_tool(&self) -> Tool {
        self.shape_edit.tool
    }

    pub(crate) fn set_pptx_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.is_presentation() {
            self.shape_edit.tool = tool;
            self.shape_edit.drag = None;
        }
        if tool != Tool::Select {
            self.set_selection(Vec::new());
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

    pub(crate) fn copy_selected(&mut self) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        let slide = self.current_slide;
        let shape_ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let groups = Self::extract_groups_for_shapes(&self.slide().groups, &shape_ids);
        if self.is_presentation() && !shapes.is_empty() {
            self.shape_edit.clipboard = shapes;
            self.shape_edit.clipboard_groups = groups;
            if self.is_presentation() {
                self.shape_edit.clipboard_slide = Some(slide);
                self.shape_edit.pastes = 0;
            }
        }
    }

    pub(crate) fn cut_selected(&mut self, cx: &mut Context<Self>) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        self.copy_selected();
        self.write_shapes_to_system_clipboard(cx);
        self.stamp_shape_clipboard(cx);
        if self.is_presentation() {
            self.shape_edit.clipboard_slide = None;
        }
        self.delete_selected(cx);
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
        let to_slide = self.shape_edit.align_to_slide;
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

    /// 太字・斜体・下線・文字の色 and the like.
    /// フォント サイズの拡大・縮小 (A▲・A▼, ⌘⇧> and ⌘⇧<): the next size of the PowerPoint's list from
    /// that of the selected text (the caret's), for the selected text, or all the text of the shapes selected.
    pub fn step_font_size_of_text(&mut self, larger: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(style) = self.selected_text_style() else { return };
        let size = ibis_doc_model::step_font_size(style.font_size, larger);
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat { font_size: Some(size), ..Default::default() });
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
            outline: Some(Some((ibis_doc_model::Color::opaque(color), width))),
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
                        return Some(ibis_doc_model::Color::opaque(r.style.color.unwrap_or(body.style.color)));
                    }
                    at += n;
                }
                at += 1;
            }
            None
        }).unwrap_or(ibis_doc_model::Color::BLACK);
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
            outline: preset.outline.map(|(c, w)| (Color::opaque(c), w)),
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
            strike: Some(run.strike), caps: Some(run.caps), shadow: Some(run.shadow), glow: Some(run.glow), link: None, link_tooltip: None,
            lang: Some(run.lang.clone()), tate_chu_yoko: None, char_scale: None,
        };
        if self.is_presentation() { self.shape_edit.character_format_copy = Some(format); }
        cx.notify();
    }

    pub(crate) fn paste_character_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(format) = self.shape_edit.character_format_copy.clone().filter(|_| self.is_presentation()) {
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
        if self.is_presentation() { self.shape_edit.paragraph_format_copy = Some(format); }
        cx.notify();
    }

    pub(crate) fn paste_paragraph_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let Some(format) = self.shape_edit.paragraph_format_copy.clone().filter(|_| self.is_presentation()) else { return };
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
    pub fn clear_formats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        if self.is_presentation() {
            self.shape_edit.defaults = merged.clone();
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

    /// The active character's resolved style and paragraph alignment.
    pub fn selected_text_style(&self) -> Option<TextStyle> {
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
            let Ok(Some((name, data))) = crate::shell::web::pick_file().await else { return };
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

    /// Insert a bundled Material icon as an SVG picture onto the slide.
    pub fn insert_icon(&mut self, icon_name: &str, cx: &mut Context<Self>) -> Option<ShapeId> {
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
        let item = cx.read_from_clipboard();
        if let Some(item) = &item {
            for entry in item.entries() {
                match entry {
                    ClipboardEntry::ExternalPaths(paths) => {
                        for path in paths.paths() {
                            let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "emf" | "wmf"));
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
        // The picture itself: a drawing (SVG, a metafile) or a bitmap, named by what its bytes are.
        if let Some((name, bytes)) = crate::shell::native_shape_clipboard::clipboard_picture(item.as_ref()) {
            // A screenshot is named "Screenshot" with the extension of what its bytes are.
            let extension = name.rsplit_once('.').map_or("png", |(_, e)| e);
            return self.insert_picture(&format!("Screenshot.{extension}"), bytes, cx);
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
            let Ok(Some((name, data))) = crate::shell::web::pick_file().await else { return };
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
            let Ok(Some((name, data))) = crate::shell::web::pick_file().await else { return };
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
    pub fn insert_slide_table(&mut self, rows: usize, columns: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.insert_slide_table_with(rows, columns, None, window, cx);
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
            Some(crop_commands(self.current_slide, &shape, picture, crop))
        }).flatten().collect();
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
        let new = {
            // Ids for the placeholders the layout may add (at most one for each of its own).
            let count = self.presentation().layout(layout).map_or(0, |l| l.placeholders.len());
            let mut spare: std::collections::VecDeque<_> = (0..count).map(|_| self.document.ids().new_shape_id()).collect();
            let mut next = || spare.pop_front().expect("an id for each placeholder of the layout");
            match &self.source {
                Some(source) if !unsaved => source.with_layout_ids(self.presentation(), slide, layout, &mut next).map_err(|e| e.to_string()),
                _ => self.presentation().with_layout_ids(slide, layout, &mut next).ok_or_else(|| "no such layout".to_string()),
            }
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

    pub(crate) fn toggle_pptx_guides(&mut self, cx: &mut Context<Self>) {
        let on = self.is_presentation().then(|| {
            self.shape_edit.guides = !self.shape_edit.guides;
            self.shape_edit.guides
        });
        // Shown with none in the file: PowerPoint's two through the middle (ガイド線).
        if on == Some(true) {
            self.seed_guides(cx);
        }
        cx.notify();
    }

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

#[ibis_office_macros::editor_ext(Editing2)]
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

    /// 読み取り専用推奨の情報バーを閉じて編集を有効にする。
    pub(crate) fn dismiss_pptx_read_only(&mut self, cx: &mut Context<Self>) {
        if self.is_presentation() {
            self.shape_edit.read_only_dismissed = true;
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


