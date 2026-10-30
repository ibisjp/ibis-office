// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! トレース矢印（参照元・参照先のトレース、矢印の削除）。
//!
//! Excel の「数式」>「ワークシート分析」にある「参照元のトレース」(Trace Precedents)、
//! 「参照先のトレース」(Trace Dependents)、「トレース矢印の削除」(Remove Arrows) を
//! 提供する。

use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_calc::formula::{Expr, parse};
use crate::render::shape_view::polygon;
use crate::shell::editor::SlideEditor;

/// トレース矢印の種類（参照元か参照先か）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceArrowKind {
    /// 選択セルの数式が参照しているセル（Precedents）。
    Precedent,
    /// 選択セルを参照している他の数式セル（Dependents）。
    Dependent,
}

/// ワークシート上に描画するトレース矢印。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceArrow {
    pub sheet: ibis_doc_model::SlideId,
    /// 参照元範囲 (min_col, min_row, max_col, max_row)。
    pub from_range: (u32, u32, u32, u32),
    /// 参照先セル (col, row)。
    pub to: (u32, u32),
    /// 別シート参照の場合のシート名（例: "Sheet2"）。
    pub external_sheet: Option<String>,
    /// 種類。
    pub kind: TraceArrowKind,
}

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub(crate) sheet: Option<String>,
    pub(crate) min_col: u32,
    pub(crate) min_row: u32,
    pub(crate) max_col: u32,
    pub(crate) max_row: u32,
}

/// 数式表現（Expr）からすべてのセル参照（AreaRef）を再帰的に抽出する。
pub(crate) fn collect_references(expr: &Expr, out: &mut Vec<Reference>) {
    collect_references_with_names(expr, &[], None, &[], 0, out);
}

/// 名前定義（DefinedName）を展開しながら数式表現（Expr）からセル参照を再帰的に抽出する。
pub(crate) fn collect_references_with_names(
    expr: &Expr,
    names: &[ibis_doc_model::DefinedName],
    current_sheet_idx: Option<usize>,
    sheet_names: &[String],
    depth: u8,
    out: &mut Vec<Reference>,
) {
    if depth > 8 {
        return;
    }
    match expr {
        Expr::Ref(r) => {
            let min_col = r.start.column.min(r.end.column);
            let min_row = r.start.row.min(r.end.row);
            let max_col = r.start.column.max(r.end.column);
            let max_row = r.start.row.max(r.end.row);
            let sheet = r.sheet.as_ref().map(|s| s.first.clone());
            out.push(Reference { sheet, min_col, min_row, max_col, max_row });
        }
        Expr::Name { sheet: name_sheet, name } => {
            let target_sheet_idx = match name_sheet {
                Some(sref) => sheet_names.iter().position(|sn| sn.eq_ignore_ascii_case(&sref.first)),
                None => current_sheet_idx,
            };
            let resolved = names
                .iter()
                .find(|dn| dn.sheet.is_some() && dn.sheet == target_sheet_idx && dn.name.eq_ignore_ascii_case(name))
                .or_else(|| {
                    if name_sheet.is_none() {
                        names.iter().find(|dn| dn.sheet.is_none() && dn.name.eq_ignore_ascii_case(name))
                    } else {
                        None
                    }
                });
            if let Some(dn) = resolved {
                let f = dn.formula.trim().trim_start_matches('=');
                if let Ok(sub_expr) = parse(f) {
                    collect_references_with_names(&sub_expr, names, dn.sheet.or(target_sheet_idx), sheet_names, depth + 1, out);
                }
            }
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_references_with_names(arg, names, current_sheet_idx, sheet_names, depth, out);
            }
        }
        Expr::Invoke { callee, args } => {
            collect_references_with_names(callee, names, current_sheet_idx, sheet_names, depth, out);
            for arg in args {
                collect_references_with_names(arg, names, current_sheet_idx, sheet_names, depth, out);
            }
        }
        Expr::Array(rows) => {
            for row in rows {
                for cell in row {
                    collect_references_with_names(cell, names, current_sheet_idx, sheet_names, depth, out);
                }
            }
        }
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => {
            collect_references_with_names(a, names, current_sheet_idx, sheet_names, depth, out);
            collect_references_with_names(b, names, current_sheet_idx, sheet_names, depth, out);
        }
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) | Expr::Spill(x) => {
            collect_references_with_names(x, names, current_sheet_idx, sheet_names, depth, out);
        }
        Expr::Union(items) => {
            for item in items {
                collect_references_with_names(item, names, current_sheet_idx, sheet_names, depth, out);
            }
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing | Expr::Structured { .. } => {}
    }
}

impl SlideEditor {
    /// 選択中のセルが参照しているセル（参照元）へ向かうトレース矢印を追加する。
    /// 既に直近の参照元矢印が存在する場合は、さらに次の段階（多段）の参照元をトレースする。
    pub(crate) fn trace_precedents(&mut self, cx: &mut Context<Self>) {
        let Some((target_col, target_row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide_id = self.slide().id;
        let (current_sheet_idx, current_sheet_title, sheet_names, names) = self
            .presentation()
            .workbook
            .as_ref()
            .map(|wb| {
                let idx = wb.sheets.iter().position(|s| s.slide == Some(slide_id));
                let title = idx.and_then(|i| wb.sheets.get(i)).map(|s| s.name.clone()).unwrap_or_default();
                let s_names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
                (idx, title, s_names, wb.names.clone())
            })
            .unwrap_or((None, String::new(), Vec::new(), Vec::new()));

        let arrows_for_cell = |col: u32, row: u32| -> Vec<TraceArrow> {
            let Some(cell) = sheet.cell(row, col) else { return Vec::new() };
            let Some(formula) = &cell.formula else { return Vec::new() };
            let Ok(expr) = parse(formula.trim_start_matches('=')) else { return Vec::new() };
            let mut refs = Vec::new();
            collect_references_with_names(&expr, &names, current_sheet_idx, &sheet_names, 0, &mut refs);
            refs.into_iter()
                .map(|r| {
                    let is_same_sheet = r.sheet.is_none() || r.sheet.as_deref() == Some(&current_sheet_title);
                    TraceArrow {
                        sheet: self.current_slide,
                        from_range: (r.min_col, r.min_row, r.max_col, r.max_row),
                        to: (col, row),
                        external_sheet: if is_same_sheet { None } else { r.sheet },
                        kind: TraceArrowKind::Precedent,
                    }
                })
                .collect()
        };

        let level1 = arrows_for_cell(target_col, target_row);
        if level1.is_empty() {
            return;
        }

        let existing = self.sheet_state().map(|s| s.trace_arrows.iter().filter(|a|a.sheet == self.current_slide).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let all_level1_present = level1.iter().all(|a| existing.contains(a));

        let mut to_add = Vec::new();
        if !all_level1_present {
            to_add = level1;
        } else {
            let mut frontier_cells = Vec::new();
            for arrow in &existing {
                if arrow.kind == TraceArrowKind::Precedent && arrow.external_sheet.is_none() {
                    let (c0, r0, c1, r1) = arrow.from_range;
                    let count = ((c1 - c0 + 1) as u64) * ((r1 - r0 + 1) as u64);
                    if count <= 64 {
                        for r in r0..=r1 {
                            for c in c0..=c1 {
                                if !frontier_cells.contains(&(c, r)) {
                                    frontier_cells.push((c, r));
                                }
                            }
                        }
                    }
                }
            }
            for (c, r) in frontier_cells {
                for next_arrow in arrows_for_cell(c, r) {
                    if !existing.contains(&next_arrow) && !to_add.contains(&next_arrow) {
                        to_add.push(next_arrow);
                    }
                }
            }
        }

        if to_add.is_empty() {
            return;
        }

        let Some(state) = self.sheet_state_mut() else { return };
        for arrow in to_add {
            if !state.trace_arrows.contains(&arrow) {
                state.trace_arrows.push(arrow);
            }
        }
        cx.notify();
    }

    /// 選択中のセルを参照している数式セル（参照先）へ向かうトレース矢印を追加する。
    /// 既に直近の参照先矢印が存在する場合は、さらに次の段階（多段）の参照先をトレースする。
    pub(crate) fn trace_dependents(&mut self, cx: &mut Context<Self>) {
        let Some((target_col, target_row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide_id = self.slide().id;
        let (current_sheet_idx, current_sheet_title, sheet_names, names) = self
            .presentation()
            .workbook
            .as_ref()
            .map(|wb| {
                let idx = wb.sheets.iter().position(|s| s.slide == Some(slide_id));
                let title = idx.and_then(|i| wb.sheets.get(i)).map(|s| s.name.clone()).unwrap_or_default();
                let s_names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
                (idx, title, s_names, wb.names.clone())
            })
            .unwrap_or((None, String::new(), Vec::new(), Vec::new()));

        let dependents_of = |col: u32, row: u32| -> Vec<TraceArrow> {
            let mut arrows = Vec::new();
            for cell in &sheet.cells {
                let Some(formula) = &cell.formula else { continue };
                let Ok(expr) = parse(formula.trim_start_matches('=')) else { continue };
                let mut refs = Vec::new();
                collect_references_with_names(&expr, &names, current_sheet_idx, &sheet_names, 0, &mut refs);
                let refers_to_target = refs.into_iter().any(|r| {
                    let is_same_sheet = r.sheet.is_none() || r.sheet.as_deref() == Some(&current_sheet_title);
                    is_same_sheet && r.min_col <= col && col <= r.max_col && r.min_row <= row && row <= r.max_row
                });
                if refers_to_target {
                    arrows.push(TraceArrow {
                        sheet: self.current_slide,
                        from_range: (col, row, col, row),
                        to: (cell.column, cell.row),
                        external_sheet: None,
                        kind: TraceArrowKind::Dependent,
                    });
                }
            }
            arrows
        };

        let level1 = dependents_of(target_col, target_row);
        if level1.is_empty() {
            return;
        }

        let existing = self.sheet_state().map(|s| s.trace_arrows.iter().filter(|a|a.sheet == self.current_slide).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let all_level1_present = level1.iter().all(|a| existing.contains(a));

        let mut new_arrows = Vec::new();
        if !all_level1_present {
            new_arrows = level1;
        } else {
            let mut frontier_cells = Vec::new();
            for arrow in &existing {
                if arrow.kind == TraceArrowKind::Dependent && arrow.external_sheet.is_none() {
                    if !frontier_cells.contains(&arrow.to) {
                        frontier_cells.push(arrow.to);
                    }
                }
            }
            for (c, r) in frontier_cells {
                for next_arrow in dependents_of(c, r) {
                    if !existing.contains(&next_arrow) && !new_arrows.contains(&next_arrow) {
                        new_arrows.push(next_arrow);
                    }
                }
            }
        }

        if new_arrows.is_empty() {
            return;
        }

        let Some(state) = self.sheet_state_mut() else { return };
        for arrow in new_arrows {
            if !state.trace_arrows.contains(&arrow) {
                state.trace_arrows.push(arrow);
            }
        }
        cx.notify();
    }

    /// すべてのトレース矢印を削除する。
    pub(crate) fn remove_all_arrows(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.trace_arrows.clear();
            cx.notify();
        }
    }

    /// 参照元トレース矢印を削除する。
    pub(crate) fn remove_precedent_arrows(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.trace_arrows.retain(|a| a.kind != TraceArrowKind::Precedent);
            cx.notify();
        }
    }

    /// 参照先トレース矢印を削除する。
    pub(crate) fn remove_dependent_arrows(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.trace_arrows.retain(|a| a.kind != TraceArrowKind::Dependent);
            cx.notify();
        }
    }

    /// 現在のトレース矢印の一覧を取得する。
    pub(crate) fn trace_arrows(&self) -> &[TraceArrow] {
        self.sheet_state().map(|s| s.trace_arrows.as_slice()).unwrap_or(&[])
    }

    /// ワークスペース上にトレース矢印を描画する。
    pub(crate) fn render_trace_arrows(&self, scale: f32) -> Option<Div> {
        let state = self.sheet_state()?;
        if !state.trace_arrows.iter().any(|a|a.sheet == self.current_slide) {
            return None;
        }

        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |offsets: &[i64], i: u32| offsets[(i as usize).min(offsets.len() - 1)];
        let x = |c: u32| (shape.frame.x + at(&xs, c)) as f32 * scale;
        let y = |r: u32| (shape.frame.y + at(&ys, r)) as f32 * scale;

        let arrow_color = rgb(0x0070C0); // Excel の標準トレース矢印青色
        let mut overlay = div().absolute().top_0().left_0().size_full();

        // 直線と矢じり、塗りつぶし丸の描画用データ
        let mut lines: Vec<[Point<Pixels>; 2]> = Vec::new();
        let mut arrowheads: Vec<[Point<Pixels>; 3]> = Vec::new();
        let mut dots: Vec<[Point<Pixels>; 8]> = Vec::new();

        for arrow in state.trace_arrows.iter().filter(|a|a.sheet == self.current_slide) {
            let (to_c, to_r) = arrow.to;
            let (tx0, tx1, ty0, ty1) = (x(to_c), x(to_c + 1), y(to_r), y(to_r + 1));
            let to_center = point(px((tx0 + tx1) / 2.0), px((ty0 + ty1) / 2.0));

            if let Some(ref ext_sheet) = arrow.external_sheet {
                // 別シート参照: ターゲットセルの左側にバッジを表示し、そこからターゲットセルへ矢印
                let badge_w = 70.0 * scale.max(1.0);
                let badge_h = 20.0 * scale.max(1.0);
                let badge_x = tx0 - badge_w - 12.0 * scale.max(1.0);
                let badge_y = (ty0 + ty1) / 2.0 - badge_h / 2.0;

                overlay = overlay.child(
                    div()
                        .absolute()
                        .left(px(badge_x))
                        .top(px(badge_y))
                        .w(px(badge_w))
                        .h(px(badge_h))
                        .bg(arrow_color)
                        .text_color(crate::shell::palette::doc(0xffffff))
                        .rounded(px(3.0 * scale.max(1.0)))
                        .text_xs()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ext_sheet.clone()),
                );

                let from_center = point(px(badge_x + badge_w), px(badge_y + badge_h / 2.0));
                let dx = f32::from(to_center.x - from_center.x);
                let dy = f32::from(to_center.y - from_center.y);
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 2.0 {
                    let (ux, uy) = (dx / dist, dy / dist);
                    let (nx, ny) = (-uy, ux);
                    let arrow_len = (8.0 * scale.max(1.0)).min(dist * 0.5);
                    let arrow_w = 4.0 * scale.max(1.0);
                    let tip = to_center;
                    let base = point(px(f32::from(tip.x) - ux * arrow_len), px(f32::from(tip.y) - uy * arrow_len));
                    let p1 = point(px(f32::from(base.x) + nx * arrow_w), px(f32::from(base.y) + ny * arrow_w));
                    let p2 = point(px(f32::from(base.x) - nx * arrow_w), px(f32::from(base.y) - ny * arrow_w));

                    lines.push([from_center, base]);
                    arrowheads.push([tip, p1, p2]);
                }
                continue;
            }

            let (c0, r0, c1, r1) = arrow.from_range;
            let (bx0, bx1, by0, by1) = (x(c0), x(c1 + 1), y(r0), y(r1 + 1));
            let from_center = point(px((bx0 + bx1) / 2.0), px((by0 + by1) / 2.0));

            if c0 == c1 && r0 == r1 {
                // 単一セル: 始点に青い丸（Dot）
                let r = 3.5 * scale.max(1.0);
                let cx = f32::from(from_center.x);
                let cy = f32::from(from_center.y);
                let mut oct = [point(px(0.0), px(0.0)); 8];
                for i in 0..8 {
                    let angle = std::f32::consts::TAU * (i as f32) / 8.0;
                    oct[i] = point(px(cx + r * angle.cos()), px(cy + r * angle.sin()));
                }
                dots.push(oct);
            } else {
                // 範囲参照: 範囲を青い枠線で囲む
                let pad = 1.0 * scale.max(1.0);
                overlay = overlay.child(
                    div()
                        .absolute()
                        .left(px(bx0 - pad))
                        .top(px(by0 - pad))
                        .w(px((bx1 - bx0) + pad * 2.0))
                        .h(px((by1 - by0) + pad * 2.0))
                        .border_2()
                        .border_color(arrow_color)
                        .rounded(px(3.0 * scale.max(1.0))),
                );
            }

            // 始点中心から終点中心への矢印
            let dx = f32::from(to_center.x - from_center.x);
            let dy = f32::from(to_center.y - from_center.y);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 2.0 {
                let (ux, uy) = (dx / dist, dy / dist);
                let (nx, ny) = (-uy, ux);
                let arrow_len = (8.0 * scale.max(1.0)).min(dist * 0.5);
                let arrow_w = 4.0 * scale.max(1.0);
                let tip = to_center;
                let base = point(px(f32::from(tip.x) - ux * arrow_len), px(f32::from(tip.y) - uy * arrow_len));
                let p1 = point(px(f32::from(base.x) + nx * arrow_w), px(f32::from(base.y) + ny * arrow_w));
                let p2 = point(px(f32::from(base.x) - nx * arrow_w), px(f32::from(base.y) - ny * arrow_w));

                lines.push([from_center, base]);
                arrowheads.push([tip, p1, p2]);
            }
        }

        let line_w = 2.0 * scale.max(1.0);
        let arrows_canvas = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let o = bounds.origin;
                // 1. 直線
                for ends in &lines {
                    let pts = [point(o.x + ends[0].x, o.y + ends[0].y), point(o.x + ends[1].x, o.y + ends[1].y)];
                    if let Some(path) = polygon(&pts, Some(px(line_w)), false) {
                        window.paint_path(path, arrow_color);
                    }
                }
                // 2. 矢じり（塗りつぶし三角形）
                for tri in &arrowheads {
                    let pts = [
                        point(o.x + tri[0].x, o.y + tri[0].y),
                        point(o.x + tri[1].x, o.y + tri[1].y),
                        point(o.x + tri[2].x, o.y + tri[2].y),
                    ];
                    if let Some(path) = polygon(&pts, None, true) {
                        window.paint_path(path, arrow_color);
                    }
                }
                // 3. 始点丸（塗りつぶしポリゴン）
                for oct in &dots {
                    let pts: Vec<_> = oct.iter().map(|p| point(o.x + p.x, o.y + p.y)).collect();
                    if let Some(path) = polygon(&pts, None, true) {
                        window.paint_path(path, arrow_color);
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        overlay = overlay.child(arrows_canvas);
        Some(overlay)
    }
}
