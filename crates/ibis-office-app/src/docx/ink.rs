// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word の手描き注釈・インク描画・インク エディター（DOCX-IMAGE-019, DOCX-INK-001〜007, DOCX-INK-010）。

use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{
    Block, Color, EmuSize, FlowCommand, FlowPos, FlowRunKind, FormulaGeometry, Geometry,
    GeometryDef, GeometryPath, Glue, PathFill, PathOp, Rgb, RunChange, Shape, ShapeId, ShapeKind,
    Stroke,
};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
pub(crate) use crate::xlsx::ink::{
    COLORS as INK_COLORS, InkEffect, PenKind, Recognized, WIDTHS as INK_WIDTHS,
    distance_to_stroke, ink_stroke_with_effect, recognize, stroke_points,
};

const EMU_PER_PT: f32 = 12_700.0;

fn effect_primary_color(effect: InkEffect) -> Rgb {
    effect
        .stops()
        .first()
        .map_or(Rgb(0xC00000), |s| s.color.rgb)
}

/// インク エディター（DOCX-INK-010）で認識されたジェスチャ操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocxInkGesture {
    /// 取り消し線・塗りつぶしジェスチャによるテキスト範囲の削除。
    DeleteRange(FlowPos, FlowPos),
    /// 丸囲みジェスチャによるテキスト範囲の選択。
    SelectRange(FlowPos, FlowPos),
    /// 蛍光ペンジェスチャによるテキスト範囲の蛍光ペン（ハイライト）適用。
    HighlightRange(FlowPos, FlowPos),
    /// 縦線・挿入ジェスチャによる空白の挿入。
    InsertSpace(FlowPos),
}

/// Word の描画（インク）タブ・サイドバーの状態。
#[derive(Clone, Debug)]
pub(crate) struct DocxInkState {
    /// 選択中のペン種別（ペン・鉛筆・蛍光ペン・消しゴム・選択）。
    pub(crate) pen: PenKind,
    /// ペンの色。
    pub(crate) color: Rgb,
    /// インク効果（虹・銀河・溶岩・海・ローズゴールド・金・銀・銅）。
    pub(crate) effect: Option<InkEffect>,
    /// ペンの太さ（pt）。
    pub(crate) width: f32,
    /// マウス・トラックパッドでの描画を許可するか（DOCX-INK-006: タッチして描画 / マウスで描画の切替）。
    pub(crate) mouse: bool,
    /// なげなわ（範囲）選択モード（DOCX-INK-004）。
    pub(crate) lasso_mode: bool,
    /// インクを図形に自動変換するモード（DOCX-INK-007）。
    pub(crate) ink_to_shape_mode: bool,
    /// インク エディター（ジェスチャで削除・選択・ハイライト・空白挿入）モード（DOCX-INK-010）。
    pub(crate) ink_editor_mode: bool,
    /// ドラッグ中のストローク（ページ番号, ページ内 pt 座標列）。
    pub(crate) active_stroke: Option<(usize, Vec<(f32, f32)>)>,
    /// ドラッグ中の範囲選択矩形（ページ番号, 始点 pt, 終点 pt）。
    pub(crate) lasso_rect: Option<(usize, (f32, f32), (f32, f32))>,
    /// 範囲選択されたインク図形の位置一覧（DOCX-INK-004）。
    pub(crate) selected_ink: Vec<FlowPos>,
    /// ステータスメッセージ。
    pub(crate) message: String,
}

impl Default for DocxInkState {
    fn default() -> Self {
        Self {
            pen: PenKind::Pen,
            color: Rgb(0xC00000),
            effect: None,
            width: 2.25,
            mouse: true,
            lasso_mode: false,
            ink_to_shape_mode: false,
            ink_editor_mode: false,
            active_stroke: None,
            lasso_rect: None,
            selected_ink: Vec::new(),
            message: String::new(),
        }
    }
}

/// Shape がインク ストロークかどうかを判定する。
pub(crate) fn is_docx_ink_shape(shape: &Shape) -> bool {
    stroke_points(shape).is_some()
}

/// インク図形のページ絶対 EMU 座標列を取得する。
pub(crate) fn docx_stroke_points(shape: &Shape) -> Option<Vec<(i64, i64)>> {
    let mut pts = stroke_points(shape)?;
    if shape.frame.x == 0
        && shape.frame.y == 0
        && let Some(f) = &shape.floating
        && (f.offset_x != 0 || f.offset_y != 0)
    {
        for p in &mut pts {
            p.0 += f.offset_x;
            p.1 += f.offset_y;
        }
    }
    Some(pts)
}

/// ページ内 pt 座標列からカスタムジオメトリと外接矩形（EMU）を生成する。
pub(crate) fn build_ink_geometry(points_pt: &[(f32, f32)]) -> Option<((i64, i64), EmuSize, Geometry)> {
    if points_pt.len() < 2 {
        return None;
    }
    let pts_emu: Vec<(i64, i64)> = points_pt
        .iter()
        .map(|&(x, y)| ((x * EMU_PER_PT).round() as i64, (y * EMU_PER_PT).round() as i64))
        .collect();
    let min_x = pts_emu.iter().map(|p| p.0).min()?;
    let max_x = pts_emu.iter().map(|p| p.0).max()?;
    let min_y = pts_emu.iter().map(|p| p.1).min()?;
    let max_y = pts_emu.iter().map(|p| p.1).max()?;
    let w = (max_x - min_x).max(12_700);
    let h = (max_y - min_y).max(12_700);
    let mut ops = Vec::with_capacity(pts_emu.len());
    ops.push(PathOp::MoveTo(
        (pts_emu[0].0 - min_x).to_string(),
        (pts_emu[0].1 - min_y).to_string(),
    ));
    for &(px, py) in &pts_emu[1..] {
        ops.push(PathOp::LineTo(
            (px - min_x).to_string(),
            (py - min_y).to_string(),
        ));
    }
    let path = GeometryPath {
        width: w,
        height: h,
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
    let geom = Geometry::Formula(FormulaGeometry {
        preset: None,
        adjust: Vec::new(),
        def: std::sync::Arc::new(def),
    });
    Some(((min_x, min_y), EmuSize::new(w, h), geom))
}

impl SlideEditor {
    /// 描画（インク）サイドバーパネルを開く。
    pub(crate) fn open_docx_ink_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(s) = self.doc_state_mut() {
            if s.docx_ink.is_none() {
                s.docx_ink = Some(DocxInkState::default());
            }
            s.master_doc_panel = None;
            s.theme_panel = None;
            s.compatibility_panel = None;
            s.inspect_panel = None;
            s.docx_math = None;
            s.quick_parts = None;
            s.table_formula = None;
            s.table_size = None;
            s.picture_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
            s.list_style = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// ペン種別を切り替える（DOCX-INK-001, DOCX-INK-003, DOCX-INK-004）。
    pub(crate) fn set_docx_ink_pen(&mut self, pen: PenKind, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.pen = pen;
            if pen == PenKind::Select {
                ink.lasso_mode = true;
            } else {
                ink.lasso_mode = false;
            }
            ink.width = match pen {
                PenKind::Pen => 2.25,
                PenKind::Pencil => 1.0,
                PenKind::Highlighter => 6.0,
                PenKind::Eraser | PenKind::Select => ink.width,
            };
            if pen == PenKind::Highlighter && ink.effect.is_none() {
                ink.color = Rgb(0xFFFF00);
            }
        }
        cx.notify();
    }

    /// ペンの色を設定し、選択中のインクがあればその色も更新する（DOCX-INK-002）。
    pub(crate) fn set_docx_ink_color(&mut self, color: Rgb, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.color = color;
            ink.effect = None;
        }
        self.apply_docx_ink_style_to_selected(cx);
        cx.notify();
    }

    /// ペンの太さ（pt）を設定し、選択中のインクがあればその太さも更新する（DOCX-INK-002）。
    pub(crate) fn set_docx_ink_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let clamped = width.clamp(0.25, 24.0);
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.width = clamped;
        }
        self.apply_docx_ink_style_to_selected(cx);
        cx.notify();
    }

    /// インク効果（グラデーション・メタリック等）を設定し、選択中のインクにも適用する（DOCX-INK-002）。
    pub(crate) fn set_docx_ink_effect(&mut self, effect: Option<InkEffect>, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.effect = effect;
            if let Some(eff) = effect {
                ink.color = effect_primary_color(eff);
            }
        }
        self.apply_docx_ink_style_to_selected(cx);
        cx.notify();
    }

    /// マウス・タッチ描画モードを切り替える（DOCX-INK-006）。
    pub(crate) fn toggle_docx_ink_mouse(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.mouse = !ink.mouse;
            ink.active_stroke = None;
            ink.lasso_rect = None;
        }
        cx.notify();
    }

    /// なげなわ（範囲）選択モードを切り替える（DOCX-INK-004）。
    #[allow(dead_code)]
    pub(crate) fn toggle_docx_ink_lasso_mode(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.lasso_mode = !ink.lasso_mode;
            if ink.lasso_mode {
                ink.pen = PenKind::Select;
            } else if ink.pen == PenKind::Select {
                ink.pen = PenKind::Pen;
            }
        }
        cx.notify();
    }

    /// インクを図形に変換モードを切り替える（DOCX-INK-007）。
    pub(crate) fn toggle_docx_ink_to_shape_mode(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.ink_to_shape_mode = !ink.ink_to_shape_mode;
        }
        cx.notify();
    }

    /// インク エディター（ジェスチャ編集）モードを切り替える（DOCX-INK-010）。
    pub(crate) fn toggle_docx_ink_editor_mode(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.ink_editor_mode = !ink.ink_editor_mode;
        }
        cx.notify();
    }

    /// 現在選択中のインク図形の位置一覧を返す（範囲選択またはキャレット位置のインク図形）。
    pub(crate) fn target_docx_ink_positions(&self) -> Vec<FlowPos> {
        let Some(doc) = self.flow() else { return Vec::new() };
        if let Some(ink) = self.doc_state().and_then(|s| s.docx_ink.as_ref())
            && !ink.selected_ink.is_empty()
        {
            let valid: Vec<FlowPos> = ink
                .selected_ink
                .iter()
                .copied()
                .filter(|pos| Self::ink_shape_at_pos(doc, *pos).is_some())
                .collect();
            if !valid.is_empty() {
                return valid;
            }
        }
        if let Some(info) = self.text_box_at_caret()
            && Self::ink_shape_at_pos(doc, info.at).is_some()
        {
            return vec![info.at];
        }
        Vec::new()
    }

    fn ink_shape_at_pos(doc: &ibis_doc_model::FlowDocument, pos: FlowPos) -> Option<&Shape> {
        let p = ibis_doc_model::flow_command::paragraph(doc, pos.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == pos.offset {
                if let FlowRunKind::Shape(shape) = &run.kind
                    && is_docx_ink_shape(shape)
                {
                    return Some(shape);
                }
                return None;
            }
            offset += run.len();
        }
        None
    }

    /// 選択中のインク図形に現在のペン色・太さ・インク効果を適用する（1 ステップ Undo/Redo、DOCX-INK-002）。
    pub(crate) fn apply_docx_ink_style_to_selected(&mut self, cx: &mut Context<Self>) -> usize {
        let targets = self.target_docx_ink_positions();
        if targets.is_empty() {
            return 0;
        }
        let Some(ink) = self.doc_state().and_then(|s| s.docx_ink.clone()) else {
            return 0;
        };
        let kind = match ink.pen {
            PenKind::Pencil => PenKind::Pencil,
            PenKind::Highlighter => PenKind::Highlighter,
            _ => PenKind::Pen,
        };
        let stroke = ink_stroke_with_effect(kind, ink.color, ink.effect, ink.width);
        let commands: Vec<FlowCommand> = targets
            .iter()
            .map(|&at| FlowCommand::SetShapeStroke {
                at,
                stroke: Some(stroke.clone()),
            })
            .collect();
        let count = commands.len();
        let keep = self.text_selection();
        if self.run_flow(commands, cx)
            && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut())
        {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
        count
    }

    /// ページ `page` 上にフリーハンドのインク ストローク（または自動図形・ジェスチャ）を追加する（DOCX-IMAGE-019, DOCX-INK-001, DOCX-INK-007, DOCX-INK-010）。
    pub(crate) fn insert_docx_ink_stroke(
        &mut self,
        page: usize,
        points_pt: &[(f32, f32)],
        cx: &mut Context<Self>,
    ) -> bool {
        if points_pt.len() < 2 {
            return false;
        }
        let ink = self
            .doc_state()
            .and_then(|s| s.docx_ink.clone())
            .unwrap_or_default();

        // インク エディター モードの場合はジェスチャとして解釈を試みる（DOCX-INK-010）
        if ink.ink_editor_mode && self.apply_docx_ink_gesture(page, points_pt, cx).is_some() {
            return true;
        }

        let Some(((min_x, min_y), size, geom)) = build_ink_geometry(points_pt) else {
            return false;
        };
        let pts_emu: Vec<(i64, i64)> = points_pt
            .iter()
            .map(|&(x, y)| ((x * EMU_PER_PT).round() as i64, (y * EMU_PER_PT).round() as i64))
            .collect();

        // 挿入先段落を決定（ページの先頭段落または現在のキャレット段落）
        let at = self
            .position_at(page, points_pt[0].0, points_pt[0].1)
            .or_else(|| self.text_selection().map(|(_, c)| c))
            .or_else(|| {
                self.flow()?.body.iter().find_map(|b| {
                    if let Block::Paragraph(p) = b {
                        Some(FlowPos::new(p.id, 0))
                    } else {
                        None
                    }
                })
            });
        let Some(at) = at else { return false };

        let pen_kind = match ink.pen {
            PenKind::Pencil => PenKind::Pencil,
            PenKind::Highlighter => PenKind::Highlighter,
            _ => PenKind::Pen,
        };
        let stroke = ink_stroke_with_effect(pen_kind, ink.color, ink.effect, ink.width);

        // インクを図形に自動変換モード（DOCX-INK-007）
        if ink.ink_to_shape_mode
            && let Some(rec) = recognize(&pts_emu)
        {
            return self.insert_recognized_ink_shape(
                at,
                rec,
                &pts_emu,
                (min_x, min_y),
                size,
                stroke,
                cx,
            );
        }

        let commands = vec![
            FlowCommand::InsertConnector {
                at,
                size,
                stroke,
                geometry: Some(geom),
                flip_h: false,
                flip_v: false,
                glue: Glue::default(),
            },
            FlowCommand::PlaceShapeOnPage {
                at,
                size,
                x: min_x,
                y: min_y,
            },
        ];
        if self.run_flow(commands, cx) {
            self.set_caret(at, false, cx);
            if let Some(state) = self.doc_state_mut() {
                let ink_state = state.docx_ink.get_or_insert_with(DocxInkState::default);
                ink_state.selected_ink = vec![at];
                ink_state.message = crate::shell::i18n::text("docx.ink.status.drawn").to_string();
            }
            cx.notify();
            return true;
        }
        false
    }

    fn insert_recognized_ink_shape(
        &mut self,
        at: FlowPos,
        rec: Recognized,
        pts_emu: &[(i64, i64)],
        (min_x, min_y): (i64, i64),
        bbox_size: EmuSize,
        stroke: Stroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let commands = match rec {
            Recognized::Line => {
                let start = pts_emu[0];
                let end = pts_emu[pts_emu.len() - 1];
                let x = start.0.min(end.0);
                let y = start.1.min(end.1);
                let w = (end.0 - start.0).abs().max(12_700);
                let h = (end.1 - start.1).abs();
                let size = EmuSize::new(w, h);
                vec![
                    FlowCommand::InsertConnector {
                        at,
                        size,
                        stroke,
                        geometry: None,
                        flip_h: end.0 < start.0,
                        flip_v: end.1 < start.1,
                        glue: Glue::default(),
                    },
                    FlowCommand::PlaceShapeOnPage { at, size, x, y },
                ]
            }
            Recognized::Rectangle | Recognized::Ellipse | Recognized::Triangle => {
                let preset = match rec {
                    Recognized::Rectangle => "rect",
                    Recognized::Ellipse => "ellipse",
                    Recognized::Triangle => "triangle",
                    Recognized::Line => unreachable!(),
                };
                let Some(geometry) = ibis_pptx::preset_geometry(preset) else {
                    return false;
                };
                let size = EmuSize::new(bbox_size.width.max(12_700), bbox_size.height.max(12_700));
                vec![
                    FlowCommand::InsertShape {
                        at,
                        size,
                        geometry,
                        fill: None,
                        line: Some((stroke.color.rgb, stroke.width)),
                    },
                    FlowCommand::PlaceShapeOnPage {
                        at,
                        size,
                        x: min_x,
                        y: min_y,
                    },
                ]
            }
        };
        if self.run_flow(commands, cx) {
            self.set_caret(at, false, cx);
            if let Some(state) = self.doc_state_mut() {
                let ink_state = state.docx_ink.get_or_insert_with(DocxInkState::default);
                ink_state.selected_ink = vec![at];
                ink_state.message =
                    crate::shell::i18n::text("docx.ink.status.converted-shape").to_string();
            }
            cx.notify();
            return true;
        }
        false
    }

    /// ページ `page` の座標 `(x_pt, y_pt)` 付近にあるインク ストロークを消去する（DOCX-INK-003）。
    pub(crate) fn erase_docx_ink_at(
        &mut self,
        page: usize,
        x_pt: f32,
        y_pt: f32,
        cx: &mut Context<Self>,
    ) -> usize {
        let Some(doc) = self.flow() else { return 0 };
        let laid = super::flow_layout::pages(doc, &self.resources);
        let Some(pg) = laid.pages.get(page) else { return 0 };
        let px_emu = (x_pt * EMU_PER_PT).round() as i64;
        let py_emu = (y_pt * EMU_PER_PT).round() as i64;
        let mut hit_ids = Vec::new();
        for item in &pg.items {
            let super::flow_layout::Item::Shape { shape, x, y, w, h } = item else { continue };
            if !is_docx_ink_shape(shape) {
                continue;
            }
            if let Some(pts) = docx_stroke_points(shape) {
                if distance_to_stroke(&pts, (px_emu, py_emu)) <= 76_200.0 {
                    hit_ids.push(shape.id);
                }
            } else if x_pt >= *x - 6.0 && x_pt <= x + w + 6.0 && y_pt >= *y - 6.0 && y_pt <= y + h + 6.0 {
                hit_ids.push(shape.id);
            }
        }
        if hit_ids.is_empty() {
            return 0;
        }
        let positions = Self::find_shape_positions(doc, &hit_ids);
        self.delete_ink_at_positions(positions, cx)
    }

    /// 選択中のインク ストロークを消去する（DOCX-INK-003）。
    pub(crate) fn erase_selected_docx_ink(&mut self, cx: &mut Context<Self>) -> usize {
        let positions = self.target_docx_ink_positions();
        self.delete_ink_at_positions(positions, cx)
    }

    /// 文書内のすべてのインク ストロークを消去する（DOCX-INK-003）。
    pub(crate) fn erase_all_docx_ink(&mut self, cx: &mut Context<Self>) -> usize {
        let Some(doc) = self.flow() else { return 0 };
        let mut positions = Vec::new();
        for block in &doc.body {
            let Block::Paragraph(p) = block else { continue };
            let mut offset = 0;
            for run in &p.runs {
                if let FlowRunKind::Shape(shape) = &run.kind
                    && is_docx_ink_shape(shape)
                {
                    positions.push(FlowPos::new(p.id, offset));
                }
                offset += run.len();
            }
        }
        self.delete_ink_at_positions(positions, cx)
    }

    fn find_shape_positions(doc: &ibis_doc_model::FlowDocument, ids: &[ShapeId]) -> Vec<FlowPos> {
        let mut positions = Vec::new();
        for block in &doc.body {
            let Block::Paragraph(p) = block else { continue };
            let mut offset = 0;
            for run in &p.runs {
                if let FlowRunKind::Shape(shape) = &run.kind
                    && ids.contains(&shape.id)
                {
                    positions.push(FlowPos::new(p.id, offset));
                }
                offset += run.len();
            }
        }
        positions
    }

    fn delete_ink_at_positions(
        &mut self,
        mut positions: Vec<FlowPos>,
        cx: &mut Context<Self>,
    ) -> usize {
        if positions.is_empty() {
            return 0;
        }
        // 同一段落内で後ろのオフセットから削除することでオフセットずれを防ぐ
        positions.sort_by(|a, b| a.para.cmp(&b.para).then(b.offset.cmp(&a.offset)));
        positions.dedup();
        let count = positions.len();
        let commands: Vec<FlowCommand> = positions
            .into_iter()
            .map(|at| FlowCommand::Delete {
                from: at,
                to: FlowPos::new(at.para, at.offset + 1),
            })
            .collect();
        if self.run_flow(commands, cx) {
            if let Some(state) = self.doc_state_mut() {
                if let Some(ink) = state.docx_ink.as_mut() {
                    ink.selected_ink.clear();
                    ink.message = crate::shell::i18n::text("docx.ink.status.erased").to_string();
                }
            }
            cx.notify();
            return count;
        }
        0
    }

    /// ページ `page` の矩形範囲 `[x0_pt..x1_pt, y0_pt..y1_pt]` にあるインク ストロークを選択する（DOCX-INK-004）。
    pub(crate) fn select_docx_ink_in_rect(
        &mut self,
        page: usize,
        x0_pt: f32,
        y0_pt: f32,
        x1_pt: f32,
        y1_pt: f32,
        cx: &mut Context<Self>,
    ) -> usize {
        let Some(doc) = self.flow() else { return 0 };
        let laid = super::flow_layout::pages(doc, &self.resources);
        let Some(pg) = laid.pages.get(page) else { return 0 };
        let (min_x, max_x) = (x0_pt.min(x1_pt), x0_pt.max(x1_pt));
        let (min_y, max_y) = (y0_pt.min(y1_pt), y0_pt.max(y1_pt));
        let mut hit_ids = Vec::new();
        for item in &pg.items {
            let super::flow_layout::Item::Shape { shape, x, y, w, h } = item else { continue };
            if !is_docx_ink_shape(shape) {
                continue;
            }
            let intersects = *x <= max_x && x + w >= min_x && *y <= max_y && y + h >= min_y;
            if intersects {
                hit_ids.push(shape.id);
            }
        }
        let positions = Self::find_shape_positions(doc, &hit_ids);
        let count = positions.len();
        if let Some(&first) = positions.first() {
            self.set_caret(first, false, cx);
        }
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.selected_ink = positions;
            ink.message = crate::shell::i18n::format(
                "docx.ink.status.selected-count",
                &[("count", &count.to_string())],
            );
        }
        cx.notify();
        count
    }

    /// 文書内のすべてのインク ストロークを選択する（DOCX-INK-004）。
    pub(crate) fn select_all_docx_ink(&mut self, cx: &mut Context<Self>) -> usize {
        let Some(doc) = self.flow() else { return 0 };
        let mut positions = Vec::new();
        for block in &doc.body {
            let Block::Paragraph(p) = block else { continue };
            let mut offset = 0;
            for run in &p.runs {
                if let FlowRunKind::Shape(shape) = &run.kind
                    && is_docx_ink_shape(shape)
                {
                    positions.push(FlowPos::new(p.id, offset));
                }
                offset += run.len();
            }
        }
        let count = positions.len();
        if let Some(&first) = positions.first() {
            self.set_caret(first, false, cx);
        }
        if let Some(state) = self.doc_state_mut() {
            let ink = state.docx_ink.get_or_insert_with(DocxInkState::default);
            ink.selected_ink = positions;
            ink.message = crate::shell::i18n::format(
                "docx.ink.status.selected-count",
                &[("count", &count.to_string())],
            );
        }
        cx.notify();
        count
    }

    /// 選択中のインク ストロークを `(dx_pt, dy_pt)` だけ移動する（1 ステップ Undo/Redo、DOCX-INK-005）。
    pub(crate) fn move_selected_docx_ink(
        &mut self,
        dx_pt: f32,
        dy_pt: f32,
        cx: &mut Context<Self>,
    ) -> usize {
        let targets = self.target_docx_ink_positions();
        if targets.is_empty() {
            return 0;
        }
        let Some(doc) = self.flow() else { return 0 };
        let dx_emu = (dx_pt * EMU_PER_PT).round() as i64;
        let dy_emu = (dy_pt * EMU_PER_PT).round() as i64;
        let mut commands = Vec::new();
        for &at in &targets {
            if let Some(shape) = Self::ink_shape_at_pos(doc, at) {
                let (ox, oy) = shape
                    .floating
                    .as_ref()
                    .map_or((shape.frame.x, shape.frame.y), |f| (f.offset_x, f.offset_y));
                commands.push(FlowCommand::SetShapeGeometry {
                    at,
                    size: None,
                    offset: Some((ox + dx_emu, oy + dy_emu)),
                });
            }
        }
        let count = commands.len();
        let keep = self.text_selection();
        if self.run_flow(commands, cx)
            && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut())
        {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
        cx.notify();
        count
    }

    /// 選択中のインク ストロークのサイズを `(scale_x, scale_y)` 倍に変更する（1 ステップ Undo/Redo、DOCX-INK-005）。
    pub(crate) fn resize_selected_docx_ink(
        &mut self,
        scale_x: f32,
        scale_y: f32,
        cx: &mut Context<Self>,
    ) -> usize {
        let targets = self.target_docx_ink_positions();
        if targets.is_empty() {
            return 0;
        }
        let Some(doc) = self.flow() else { return 0 };
        let sx = scale_x.clamp(0.1, 10.0);
        let sy = scale_y.clamp(0.1, 10.0);
        let mut commands = Vec::new();
        for &at in &targets {
            if let Some(shape) = Self::ink_shape_at_pos(doc, at) {
                let nw = ((shape.frame.width as f32 * sx).round() as i64).max(12_700);
                let nh = ((shape.frame.height as f32 * sy).round() as i64).max(12_700);
                commands.push(FlowCommand::SetShapeGeometry {
                    at,
                    size: Some(EmuSize::new(nw, nh)),
                    offset: None,
                });
            }
        }
        let count = commands.len();
        let keep = self.text_selection();
        if self.run_flow(commands, cx)
            && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut())
        {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
        cx.notify();
        count
    }

    /// 選択中のインク ストロークを認識して幾何学図形（直線・四角形・楕円・三角形）に変換する（1 ステップ Undo/Redo、DOCX-INK-007）。
    pub(crate) fn docx_ink_to_shape(&mut self, cx: &mut Context<Self>) -> usize {
        let mut targets = self.target_docx_ink_positions();
        if targets.is_empty() {
            return 0;
        }
        let Some(doc) = self.flow() else { return 0 };
        // 後ろのオフセットから置換することでオフセットずれを防ぐ
        targets.sort_by(|a, b| a.para.cmp(&b.para).then(b.offset.cmp(&a.offset)));
        targets.dedup();

        let mut commands = Vec::new();
        let mut converted = 0usize;
        for &at in &targets {
            let Some(shape) = Self::ink_shape_at_pos(doc, at) else { continue };
            let Some(pts) = docx_stroke_points(shape) else { continue };
            let Some(rec) = recognize(&pts) else { continue };
            let (ox, oy) = shape
                .floating
                .as_ref()
                .map_or((shape.frame.x, shape.frame.y), |f| (f.offset_x, f.offset_y));
            let stroke = match &shape.kind {
                ShapeKind::Auto(a) => a
                    .stroke
                    .clone()
                    .unwrap_or_else(|| Stroke::solid(Color::opaque(Rgb(0x2F528F)), 19_050)),
                ShapeKind::Line(s) => s.clone(),
                _ => Stroke::solid(Color::opaque(Rgb(0x2F528F)), 19_050),
            };
            commands.push(FlowCommand::Delete {
                from: at,
                to: FlowPos::new(at.para, at.offset + 1),
            });
            match rec {
                Recognized::Line => {
                    let start = pts[0];
                    let end = pts[pts.len() - 1];
                    let x = start.0.min(end.0);
                    let y = start.1.min(end.1);
                    let w = (end.0 - start.0).abs().max(12_700);
                    let h = (end.1 - start.1).abs();
                    let size = EmuSize::new(w, h);
                    commands.push(FlowCommand::InsertConnector {
                        at,
                        size,
                        stroke,
                        geometry: None,
                        flip_h: end.0 < start.0,
                        flip_v: end.1 < start.1,
                        glue: Glue::default(),
                    });
                    commands.push(FlowCommand::PlaceShapeOnPage { at, size, x, y });
                }
                Recognized::Rectangle | Recognized::Ellipse | Recognized::Triangle => {
                    let preset = match rec {
                        Recognized::Rectangle => "rect",
                        Recognized::Ellipse => "ellipse",
                        Recognized::Triangle => "triangle",
                        Recognized::Line => unreachable!(),
                    };
                    let Some(geometry) = ibis_pptx::preset_geometry(preset) else {
                        continue;
                    };
                    let size = EmuSize::new(
                        shape.frame.width.max(12_700),
                        shape.frame.height.max(12_700),
                    );
                    commands.push(FlowCommand::InsertShape {
                        at,
                        size,
                        geometry,
                        fill: None,
                        line: Some((stroke.color.rgb, stroke.width)),
                    });
                    commands.push(FlowCommand::PlaceShapeOnPage {
                        at,
                        size,
                        x: ox,
                        y: oy,
                    });
                }
            }
            converted += 1;
        }
        if converted == 0 {
            return 0;
        }
        if self.run_flow(commands, cx) {
            if let Some(&first) = targets.last() {
                self.set_caret(first, false, cx);
            }
            if let Some(state) = self.doc_state_mut()
                && let Some(ink) = state.docx_ink.as_mut()
            {
                ink.message =
                    crate::shell::i18n::text("docx.ink.status.converted-shape").to_string();
            }
            cx.notify();
        }
        converted
    }

    /// インク エディターのジェスチャ（取り消し線で削除・丸囲みで選択・蛍光ペンでハイライト・縦線で空白挿入）を解釈して実行する（DOCX-INK-010）。
    pub(crate) fn apply_docx_ink_gesture(
        &mut self,
        page: usize,
        points_pt: &[(f32, f32)],
        cx: &mut Context<Self>,
    ) -> Option<DocxInkGesture> {
        if points_pt.len() < 2 {
            return None;
        }
        let min_x = points_pt.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points_pt
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = points_pt.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points_pt
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let w = (max_x - min_x).max(0.0);
        let h = (max_y - min_y).max(0.0);
        let mid_x = (min_x + max_x) * 0.5;
        let mid_y = (min_y + max_y) * 0.5;

        let ink = self
            .doc_state()
            .and_then(|s| s.docx_ink.clone())
            .unwrap_or_default();

        // 1. 蛍光ペンジェスチャ：横方向に引いた範囲のテキストに蛍光ペン（ハイライト）を適用
        if ink.pen == PenKind::Highlighter && w >= 8.0 {
            let from = self.position_at(page, min_x, mid_y)?;
            let to = self.position_at(page, max_x, mid_y)?;
            let (start, end) = self.order_flow_pos(from, to);
            if start != end {
                self.run_flow(
                    vec![FlowCommand::SetRunFormat {
                        from: start,
                        to: end,
                        change: RunChange::Highlight(Some(ink.color)),
                    }],
                    cx,
                );
                self.set_caret(start, false, cx);
                self.set_caret(end, true, cx);
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.message =
                        crate::shell::i18n::text("docx.ink.status.gesture-highlight").to_string();
                }
                cx.notify();
                return Some(DocxInkGesture::HighlightRange(start, end));
            }
        }

        // 2. 丸囲み（閉ループ）ジェスチャ：囲んだ範囲のテキストを選択
        let first = points_pt[0];
        let last = *points_pt.last()?;
        let close_dist = ((last.0 - first.0).powi(2) + (last.1 - first.1).powi(2)).sqrt();
        let diag = (w * w + h * h).sqrt();
        if points_pt.len() >= 4 && w >= 8.0 && h >= 6.0 && close_dist <= (diag * 0.45).max(18.0) {
            let from = self.position_at(page, min_x, mid_y)?;
            let to = self.position_at(page, max_x, mid_y)?;
            let (start, end) = self.order_flow_pos(from, to);
            if start != end {
                self.set_caret(start, false, cx);
                self.set_caret(end, true, cx);
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.message =
                        crate::shell::i18n::text("docx.ink.status.gesture-select").to_string();
                }
                cx.notify();
                return Some(DocxInkGesture::SelectRange(start, end));
            }
        }

        // 3. 縦線ジェスチャ：単語間に空白を挿入
        if w <= 10.0 && h >= 8.0 {
            let pos = self.position_at(page, mid_x, mid_y)?;
            if self.run_flow(
                vec![FlowCommand::InsertText {
                    at: pos,
                    text: " ".to_string(),
                    format: None,
                }],
                cx,
            ) {
                self.set_caret(FlowPos::new(pos.para, pos.offset + 1), false, cx);
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.message =
                        crate::shell::i18n::text("docx.ink.status.gesture-space").to_string();
                }
                cx.notify();
                return Some(DocxInkGesture::InsertSpace(pos));
            }
        }

        // 4. 取り消し線 / 塗りつぶしジェスチャ：横切ったテキスト範囲を削除
        if w >= 10.0 && h <= 28.0 {
            let from = self.position_at(page, min_x, mid_y)?;
            let to = self.position_at(page, max_x, mid_y)?;
            let (start, end) = self.order_flow_pos(from, to);
            if start != end
                && self.run_flow(
                    vec![FlowCommand::Delete {
                        from: start,
                        to: end,
                    }],
                    cx,
                )
            {
                self.set_caret(start, false, cx);
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.message =
                        crate::shell::i18n::text("docx.ink.status.gesture-delete").to_string();
                }
                cx.notify();
                return Some(DocxInkGesture::DeleteRange(start, end));
            }
        }

        None
    }

    fn order_flow_pos(&self, a: FlowPos, b: FlowPos) -> (FlowPos, FlowPos) {
        if a.para == b.para {
            if a.offset <= b.offset {
                (a, b)
            } else {
                (b, a)
            }
        } else {
            self.set_caret_order(a, b)
        }
    }

    fn set_caret_order(&self, a: FlowPos, b: FlowPos) -> (FlowPos, FlowPos) {
        let Some(doc) = self.flow() else {
            return (a, b);
        };
        for block in &doc.body {
            if let Block::Paragraph(p) = block {
                if p.id == a.para {
                    return (a, b);
                }
                if p.id == b.para {
                    return (b, a);
                }
            }
        }
        (a, b)
    }

    /// ページ上でのマウス押下時にインク操作（描画・消しゴム・なげなわ選択）を開始するか判定・処理する。
    pub(crate) fn begin_docx_ink(
        &mut self,
        page: usize,
        x_pt: f32,
        y_pt: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(ink) = self.doc_state().and_then(|s| s.docx_ink.clone()) else {
            return false;
        };
        if !ink.mouse || !self.sidebar_open {
            return false;
        }
        match ink.pen {
            PenKind::Eraser => {
                self.erase_docx_ink_at(page, x_pt, y_pt, cx);
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.active_stroke = Some((page, vec![(x_pt, y_pt)]));
                }
                true
            }
            PenKind::Select => {
                if ink.lasso_mode {
                    if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                        s.lasso_rect = Some((page, (x_pt, y_pt), (x_pt, y_pt)));
                    }
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            PenKind::Pen | PenKind::Pencil | PenKind::Highlighter => {
                if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) {
                    s.active_stroke = Some((page, vec![(x_pt, y_pt)]));
                }
                cx.notify();
                true
            }
        }
    }

    /// ページ上でのマウス移動時にインク操作を更新する。
    pub(crate) fn drag_docx_ink(
        &mut self,
        page: usize,
        x_pt: f32,
        y_pt: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(ink) = self.doc_state().and_then(|s| s.docx_ink.clone()) else {
            return false;
        };
        if let Some((p, _, _)) = ink.lasso_rect
            && p == page
        {
            if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut())
                && let Some((_, _, end)) = s.lasso_rect.as_mut()
            {
                *end = (x_pt, y_pt);
            }
            cx.notify();
            return true;
        }
        if let Some((p, _)) = ink.active_stroke
            && p == page
        {
            if ink.pen == PenKind::Eraser {
                self.erase_docx_ink_at(page, x_pt, y_pt, cx);
                return true;
            }
            if let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut())
                && let Some((_, pts)) = s.active_stroke.as_mut()
            {
                if pts
                    .last()
                    .is_none_or(|&(lx, ly)| (x_pt - lx).hypot(y_pt - ly) >= 1.0)
                {
                    pts.push((x_pt, y_pt));
                }
            }
            cx.notify();
            return true;
        }
        false
    }

    /// ページ上でのマウス解放時にインク操作を確定する。
    pub(crate) fn end_docx_ink(&mut self, cx: &mut Context<Self>) {
        let (stroke, lasso, pen) = {
            let Some(s) = self.doc_state_mut().and_then(|s| s.docx_ink.as_mut()) else {
                return;
            };
            (s.active_stroke.take(), s.lasso_rect.take(), s.pen)
        };
        if let Some((page, p0, p1)) = lasso {
            self.select_docx_ink_in_rect(page, p0.0, p0.1, p1.0, p1.1, cx);
        }
        if let Some((page, pts)) = stroke
            && pen != PenKind::Eraser
            && pts.len() >= 2
        {
            self.insert_docx_ink_stroke(page, &pts, cx);
        }
    }

    /// ドラッグ中のインク ストローク・範囲選択枠・複数選択中のインク枠をページ上にオーバーレイ描画する。
    pub(crate) fn docx_ink_overlay(&self, page: usize, k: f32) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let Some(ink) = self.doc_state().and_then(|s| s.docx_ink.as_ref()) else {
            return out;
        };

        // 範囲選択矩形（なげなわ選択中）
        if let Some((p, (x0, y0), (x1, y1))) = ink.lasso_rect
            && p == page
        {
            let (lx, rx) = (x0.min(x1), x0.max(x1));
            let (ty, by) = (y0.min(y1), y0.max(y1));
            out.push(
                div()
                    .absolute()
                    .left(px(lx * k))
                    .top(px(ty * k))
                    .w(px((rx - lx).max(1.0) * k))
                    .h(px((by - ty).max(1.0) * k))
                    .border_1()
                    .border_color(rgb(0x1a73e8))
                    .bg(hsla(0.58, 0.85, 0.5, 0.14))
                    .into_any_element(),
            );
        }

        // 複数選択されているインク図形の破線枠
        if !ink.selected_ink.is_empty()
            && let Some(doc) = self.flow()
        {
            let laid = super::flow_layout::pages(doc, &self.resources);
            if let Some(pg) = laid.pages.get(page) {
                for &at in &ink.selected_ink {
                    if let Some(shape) = Self::ink_shape_at_pos(doc, at) {
                        for item in &pg.items {
                            if let super::flow_layout::Item::Shape {
                                shape: s,
                                x,
                                y,
                                w,
                                h,
                            } = item
                                && s.id == shape.id
                            {
                                out.push(
                                    div()
                                        .absolute()
                                        .left(px(x * k - 2.0))
                                        .top(px(y * k - 2.0))
                                        .w(px(w * k + 4.0))
                                        .h(px(h * k + 4.0))
                                        .border_1()
                                        .border_color(rgb(0x1a73e8))
                                        .into_any_element(),
                                );
                            }
                        }
                    }
                }
            }
        }

        // ドラッグ中のストロークのライブプレビュー
        if let Some((p, pts)) = &ink.active_stroke
            && *p == page
            && ink.pen != PenKind::Eraser
            && pts.len() >= 2
        {
            let color = rgb(ink.effect.map_or(ink.color.0, |e| effect_primary_color(e).0));
            let dot = (ink.width * k).max(2.0);
            for &(x, y) in pts {
                out.push(
                    div()
                        .absolute()
                        .left(px(x * k - dot * 0.5))
                        .top(px(y * k - dot * 0.5))
                        .w(px(dot))
                        .h(px(dot))
                        .rounded_full()
                        .bg(color)
                        .into_any_element(),
                );
            }
        }

        out
    }

    /// 描画（インク）サイドバーパネルの UI。
    pub(crate) fn docx_ink_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.docx_ink.as_ref()?;
        self.render_docx_ink_panel(cx)
    }

    #[inline(never)]
    fn render_docx_ink_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel>{
        use crate::shell::ink_panel::{InkChoice,InkToggle,InkCommand,InkRow,InkSection,InkPanelSpec};
        let ink=self.doc_state()?.docx_ink.clone()?;
        let selected_count=self.target_docx_ink_positions().len();
        let tr=crate::shell::i18n::text;
        let tools=[("docx-ink-pen","docx.ink.pen",PenKind::Pen),("docx-ink-pencil","docx.ink.pencil",PenKind::Pencil),("docx-ink-highlighter","docx.ink.highlighter",PenKind::Highlighter),("docx-ink-eraser","docx.ink.eraser",PenKind::Eraser),("docx-ink-lasso","docx.ink.lasso",PenKind::Select)].into_iter().map(|(id,label,kind)|InkChoice::new(id,tr(label),ink.pen==kind,move|e,_,cx|e.set_docx_ink_pen(kind,cx))).collect();
        let widths=INK_WIDTHS.into_iter().enumerate().map(|(i,width)|InkChoice::new(("docx-ink-width",i),format!("{width} pt"),(ink.width-width).abs()<0.01,move|e,_,cx|e.set_docx_ink_width(width,cx))).collect();
        let mut effects=vec![InkChoice::new("docx-ink-eff-none",tr("docx.ink.effect.none"),ink.effect.is_none(),|e,_,cx|e.set_docx_ink_effect(None,cx))];
        effects.extend(InkEffect::ALL.into_iter().map(|effect|InkChoice::new(effect.id(),tr(effect.label_key()),ink.effect==Some(effect),move|e,_,cx|e.set_docx_ink_effect(Some(effect),cx))));
        let options=InkSection{title:None,rows:vec![
            InkRow::Toggle(InkToggle::new("docx-ink-to-shape-mode",tr("docx.ink.ink-to-shape"),ink.ink_to_shape_mode,|e,_,cx|e.toggle_docx_ink_to_shape_mode(cx))),
            InkRow::Toggle(InkToggle::new("docx-ink-editor-mode",tr("docx.ink.ink-editor"),ink.ink_editor_mode,|e,_,cx|e.toggle_docx_ink_editor_mode(cx))),
        ]};
        let actions=InkSection{title:Some(format!("{} ({selected_count})",tr("docx.ink.actions")).into()),rows:vec![
            InkRow::Commands(vec![
                InkCommand::new("docx-ink-select-all",tr("docx.ink.select-all"),true,|e,_,cx|{e.select_all_docx_ink(cx);}),
                InkCommand::new("docx-ink-convert-selected",tr("docx.ink.convert-now"),selected_count>0,|e,_,cx|{e.docx_ink_to_shape(cx);}),
                InkCommand::new("docx-ink-erase-selected",tr("docx.ink.erase-selected"),selected_count>0,|e,_,cx|{e.erase_selected_docx_ink(cx);}).destructive(),
                InkCommand::new("docx-ink-erase-all",tr("docx.ink.erase-all"),true,|e,_,cx|{e.erase_all_docx_ink(cx);}).destructive(),
            ]),
            InkRow::Commands(vec![
                InkCommand::new("docx-ink-move-left","←",selected_count>0,|e,_,cx|{e.move_selected_docx_ink(-12.,0.,cx);}),
                InkCommand::new("docx-ink-move-right","→",selected_count>0,|e,_,cx|{e.move_selected_docx_ink(12.,0.,cx);}),
                InkCommand::new("docx-ink-move-up","↑",selected_count>0,|e,_,cx|{e.move_selected_docx_ink(0.,-12.,cx);}),
                InkCommand::new("docx-ink-move-down","↓",selected_count>0,|e,_,cx|{e.move_selected_docx_ink(0.,12.,cx);}),
                InkCommand::new("docx-ink-scale-up",tr("docx.ink.scale-up"),selected_count>0,|e,_,cx|{e.resize_selected_docx_ink(1.25,1.25,cx);}),
                InkCommand::new("docx-ink-scale-down",tr("docx.ink.scale-down"),selected_count>0,|e,_,cx|{e.resize_selected_docx_ink(0.8,0.8,cx);}),
            ]),
        ]};
        Some(crate::shell::ink_panel::panel(InkPanelSpec{
            color_id:"docx-ink-color-field",tools,widths,drawing:matches!(ink.pen,PenKind::Pen|PenKind::Pencil|PenKind::Highlighter),color:ink.color,
            change_color:std::rc::Rc::new(|e,color,_,cx|e.set_docx_ink_color(color,cx)),effects,
            mouse:InkToggle::new("docx-ink-mouse-toggle",tr("ink.panel.mouse"),ink.mouse,|e,_,cx|e.toggle_docx_ink_mouse(cx)),
            sections:vec![options,actions],message:ink.message,context_menu:None,
        },cx))
    }
}
