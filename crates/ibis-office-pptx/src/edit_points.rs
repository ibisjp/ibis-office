// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 頂点の編集 (配置 > 頂点の編集): the vertices of a shape's outline shown as
//! handles to drag; a press on an edge adds a vertex, Delete takes the
//! selected one away, Esc ends. The shape becomes a custom (freeform) shape,
//! unturned, its curves kept: a vertex has handles to drag (the curve through
//! it), and is made a smooth, straight or corner point.
use gpui_kit::*;
use ibis_doc_model::shape_geometry::Node;
use ibis_doc_model::shape_merge::{move_handle, move_node, node_kind, nodes_polylines, nearest_on_segment_curve, reshaped_nodes, set_node_kind, shape_nodes, shape_polygons, split_segment, NodeKind, Nodes, Polygons};
use ibis_doc_model::{Command, ShapeId};

use crate::interaction::HANDLE_HIT_PX;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The shape whose vertices are edited, and the vertex last pressed.
#[derive(Clone, Debug)]
pub(crate) struct EditPoints {
    pub(crate) shape: ShapeId,
    /// (figure, index) of the selected vertex.
    pub(crate) selected: Option<(usize, usize)>,
    /// The vertices made 角の点 by hand: their handles move on their own (a smooth vertex's turn together).
    pub(crate) corners: std::collections::HashSet<(usize, usize)>,
}

/// Which part of a vertex is pressed or dragged: the vertex, or one of its handles.
pub(crate) const PART_POINT: u8 = 0;
pub(crate) const PART_IN: u8 = 1;
pub(crate) const PART_OUT: u8 = 2;

/// The outline of `shape` as vertices with their handles: its curves where it has them, else its lines.
fn nodes_of(shape: &ibis_doc_model::Shape) -> Option<Nodes> {
    shape_nodes(shape).or_else(|| shape_polygons(shape).map(|polygons| polygons.into_iter().map(|p| p.into_iter().map(Node::corner).collect()).collect()))
}

#[ibis_office_macros::editor_ext(EditPointsExt)]
impl SlideEditor {
    /// The shape whose vertices are edited, if any.
    fn edit_points_shape(&self) -> Option<&ibis_doc_model::Shape> {
        let id = self.pptx()?.edit_points.as_ref()?.shape;
        self.slide().shape(id)
    }

    /// Whether the selection is one closed figure, which 頂点の編集 takes.
    pub(crate) fn can_edit_points(&self) -> bool {
        match self.selected_shapes().as_slice() {
            [shape] => !self.slide().placeholders.contains_key(&shape.id) && !self.slide().is_attached(shape.id) && shape_polygons(shape).is_some(),
            _ => false,
        }
    }

    pub(crate) fn is_editing_points(&self) -> bool {
        self.edit_points_shape().is_some()
    }

    /// 頂点の編集: the selected shape made a freeform of its outline (its curves kept), its vertices shown.
    pub(crate) fn begin_edit_points(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit_points() {
            return;
        }
        let Some(shape) = self.selected_shapes().first().map(|s| (*s).clone()) else { return };
        let Some(nodes) = nodes_of(&shape) else { return };
        let id = shape.id;
        // A freeform, unturned (a turned shape's outline is as drawn).
        let unchanged = matches!(&shape.kind, ibis_doc_model::ShapeKind::Auto(a) if matches!(&a.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none())) && shape.rotation == 0 && !shape.flip_h && !shape.flip_v;
        if !unchanged && !self.replace_nodes(&shape, &nodes, true, cx) {
            return;
        }
        if let Some(s) = self.pptx_mut() {
            s.edit_points = Some(EditPoints { shape: id, selected: None, corners: Default::default() });
        }
        cx.notify();
    }

    pub(crate) fn end_edit_points(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.edit_points = None;
        }
        cx.notify();
    }

    /// `shape` with `nodes` as its outline (`unturn`: its rotation and flips taken off).
    fn replace_nodes(&mut self, shape: &ibis_doc_model::Shape, nodes: &Nodes, unturn: bool, cx: &mut Context<Self>) -> bool {
        let Some(new) = reshaped_nodes(shape, nodes) else { return false };
        let slide = self.current_slide;
        let mut commands = vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(new.kind.clone()) }, Command::SetFrame { slide, shape: shape.id, frame: new.frame }];
        if unturn && (shape.rotation != 0 || shape.flip_h || shape.flip_v) {
            commands.push(Command::SetTransform { slide, shape: shape.id, rotation: 0, flip_h: false, flip_v: false });
        }
        self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx)
    }

    /// The vertices with their handles, in slide EMU.
    pub(crate) fn edit_point_nodes(&self) -> Nodes {
        self.edit_points_shape().and_then(nodes_of).unwrap_or_default()
    }

    /// The vertices, as the figures (the one being dragged — `drag`: figure, vertex, part, where — at the pointer).
    pub(crate) fn edit_point_nodes_dragged(&self, drag: Option<(usize, usize, u8, (f64, f64))>) -> Nodes {
        let mut nodes = self.edit_point_nodes();
        if let Some((l, i, part, to)) = drag {
            self.apply_vertex_move(&mut nodes, l, i, part, to);
        }
        nodes
    }

    fn apply_vertex_move(&self, nodes: &mut Nodes, l: usize, i: usize, part: u8, to: (f64, f64)) {
        let corner = self.pptx().and_then(|s| s.edit_points.as_ref()).is_some_and(|e| e.corners.contains(&(l, i)));
        let Some(figure) = nodes.get_mut(l).filter(|f| i < f.len()) else { return };
        match part {
            PART_POINT => move_node(figure, i, to),
            handle => {
                let mirror = !corner && node_kind(figure, i) == NodeKind::Smooth;
                move_handle(figure, i, handle == PART_OUT, to, mirror);
            }
        }
    }

    /// The vertices as (figure, index, point in slide EMU).
    pub(crate) fn edit_point_vertices(&self) -> Vec<(usize, usize, (f64, f64))> {
        self.edit_point_nodes().iter().enumerate().flat_map(|(l, figure)| figure.iter().enumerate().map(move |(i, n)| (l, i, n.p))).collect()
    }

    /// The outline as lines in slide EMU (the curves cut short).
    pub(crate) fn edit_point_polygons(&self) -> Polygons {
        nodes_polylines(&self.edit_point_nodes())
    }

    /// The vertex selected.
    pub(crate) fn edit_point_selected(&self) -> Option<(usize, usize)> {
        self.pptx()?.edit_points.as_ref()?.selected
    }

    /// The kind of the vertex selected.
    pub(crate) fn edit_point_selected_kind(&self) -> Option<NodeKind> {
        let (l, i) = self.edit_point_selected()?;
        let (e, nodes) = (self.pptx()?.edit_points.as_ref()?, self.edit_point_nodes());
        let figure = nodes.get(l).filter(|f| i < f.len())?;
        Some(match node_kind(figure, i) {
            NodeKind::Smooth if e.corners.contains(&(l, i)) => NodeKind::Corner,
            kind => kind,
        })
    }

    /// The handles of the vertex selected: (which, point in slide EMU), with `drag` as `edit_point_nodes_dragged`.
    pub(crate) fn edit_point_handles(&self, drag: Option<(usize, usize, u8, (f64, f64))>) -> Vec<(u8, (f64, f64))> {
        let Some((l, i)) = self.edit_point_selected() else { return Vec::new() };
        let nodes = self.edit_point_nodes_dragged(drag);
        let Some(n) = nodes.get(l).and_then(|f| f.get(i)) else { return Vec::new() };
        [(PART_IN, n.cin), (PART_OUT, n.cout)].into_iter().filter_map(|(part, p)| p.map(|p| (part, p))).collect()
    }

    /// A press while vertices are edited: on a handle or a vertex, it is taken (and can be dragged); on an edge, a
    /// vertex is made there (on a curve, one that keeps the curve); elsewhere the editing ends.
    /// The (figure, vertex, part) pressed.
    pub(crate) fn edit_points_down(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> Option<(usize, usize, u8)> {
        let layout = self.slide_layout()?;
        let id = self.edit_points_shape()?.id;
        let nodes = self.edit_point_nodes();
        let emu = layout.to_emu(position);
        let p = (emu.x as f64, emu.y as f64);
        let slop = HANDLE_HIT_PX as f64 / layout.scale as f64;
        let near = |q: (f64, f64)| (q.0 - p.0).hypot(q.1 - p.1) <= slop;
        // The handles of the selected vertex.
        if let Some((l, i)) = self.edit_point_selected() {
            if let Some(n) = nodes.get(l).and_then(|f| f.get(i)) {
                for (part, q) in [(PART_IN, n.cin), (PART_OUT, n.cout)] {
                    if q.is_some_and(near) {
                        cx.notify();
                        return Some((l, i, part));
                    }
                }
            }
        }
        // A vertex.
        for (l, figure) in nodes.iter().enumerate() {
            for (i, n) in figure.iter().enumerate() {
                if near(n.p) {
                    if let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut()) {
                        e.selected = Some((l, i));
                    }
                    cx.notify();
                    return Some((l, i, PART_POINT));
                }
            }
        }
        // An edge: a vertex made on it.
        let mut best: Option<(f64, usize, usize, f64)> = None;
        for (l, figure) in nodes.iter().enumerate() {
            let n = figure.len();
            for i in 0..n {
                let (d, t, _) = nearest_on_segment_curve(&figure[i], &figure[(i + 1) % n], p);
                if d <= slop && best.is_none_or(|b| d < b.0) {
                    best = Some((d, l, i, t));
                }
            }
        }
        if let Some((_, l, i, t)) = best {
            let Some(shape) = self.slide().shape(id).cloned() else { return None };
            let mut new = nodes.clone();
            split_segment(&mut new[l], i, t.clamp(0.02, 0.98));
            if self.replace_nodes(&shape, &new, false, cx) {
                if let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut()) {
                    e.selected = Some((l, i + 1));
                    e.corners.clear();
                }
                return Some((l, i + 1, PART_POINT));
            }
            return None;
        }
        self.end_edit_points(cx);
        None
    }

    /// Vertex (`l`, `i`), or its handle `part`, moved to `to` (slide EMU).
    pub(crate) fn move_vertex(&mut self, l: usize, i: usize, part: u8, to: (f64, f64), cx: &mut Context<Self>) {
        let Some(shape) = self.edit_points_shape().cloned() else { return };
        let mut nodes = self.edit_point_nodes();
        let before = nodes.clone();
        self.apply_vertex_move(&mut nodes, l, i, part, to);
        if nodes == before {
            return;
        }
        self.replace_nodes(&shape, &nodes, false, cx);
    }

    /// Delete: the selected vertex taken away (a figure keeps three). `true` if there was one.
    pub(crate) fn delete_selected_vertex(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((l, i)) = self.edit_point_selected() else { return false };
        let Some(shape) = self.edit_points_shape().cloned() else { return false };
        let mut nodes = self.edit_point_nodes();
        if nodes.get(l).is_none_or(|f| f.len() <= 3 || i >= f.len()) {
            return true;
        }
        nodes[l].remove(i);
        if self.replace_nodes(&shape, &nodes, false, cx)
            && let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut())
        {
            e.selected = None;
            e.corners.clear();
        }
        true
    }

    /// 滑らかな点・直線の点・角の点: the selected vertex made of `kind`.
    pub(crate) fn set_selected_vertex_kind(&mut self, kind: NodeKind, cx: &mut Context<Self>) {
        let Some((l, i)) = self.edit_point_selected() else { return };
        let Some(shape) = self.edit_points_shape().cloned() else { return };
        let mut nodes = self.edit_point_nodes();
        let Some(figure) = nodes.get_mut(l).filter(|f| i < f.len()) else { return };
        set_node_kind(figure, i, kind);
        if self.replace_nodes(&shape, &nodes, false, cx)
            && let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut())
        {
            if kind == NodeKind::Corner {
                e.corners.insert((l, i));
            } else {
                e.corners.remove(&(l, i));
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use ibis_doc_model::shape_geometry::Node;
    use ibis_doc_model::shape_merge::nearest_on_segment_curve;

    #[test]
    fn nearest_points() {
        let (a, b) = (Node::corner((0.0, 0.0)), Node::corner((10.0, 0.0)));
        let (d, t, q) = nearest_on_segment_curve(&a, &b, (5.0, 3.0));
        assert_eq!((d, t, q), (3.0, 0.5, (5.0, 0.0)));
        // Past the end: the end.
        let (d, _, q) = nearest_on_segment_curve(&a, &b, (13.0, 4.0));
        assert_eq!((d, q), (5.0, (10.0, 0.0)));
        let c = Node::corner((2.0, 2.0));
        assert_eq!(nearest_on_segment_curve(&c, &c, (1.0, 1.0)).0, 2f64.sqrt());
    }
}
