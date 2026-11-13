// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 図形の結合 (配置 > 図形の結合): the selected shapes made one shape, or
//! cut into pieces (see `ibis_doc_model::shape_merge`). The first selected
//! shape gives the look; the shapes it replaces are taken off.
use gpui_kit::*;
use ibis_doc_model::shape_merge::{merge, shape_polygons, MergeOp};
use ibis_doc_model::{Command, ShapeId};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(ShapeMerge)]
impl SlideEditor {
    /// The selected shapes in the order they were chosen, when two or more
    /// are closed figures that are not placeholders or in a group.
    fn mergeable_selection(&self) -> Option<Vec<ShapeId>> {
        self.pptx()?;
        let slide = self.slide();
        let chosen: Vec<ShapeId> = self.shape_edit.selection.iter().copied().filter(|id| slide.shape_index(*id).is_some()).collect();
        if chosen.len() < 2 {
            return None;
        }
        let mut in_group = Vec::new();
        for group in &slide.groups {
            Self::collect_group_leaves(group, &mut in_group);
        }
        let all_figures = chosen.iter().all(|id| {
            !in_group.contains(id) && !slide.placeholders.contains_key(id) && slide.shapes.iter().find(|s| s.id == *id).and_then(shape_polygons).is_some()
        });
        all_figures.then_some(chosen)
    }

    pub(crate) fn can_merge_shapes(&self) -> bool {
        self.mergeable_selection().is_some()
    }

    pub(crate) fn merge_selected_shapes(&mut self, op: MergeOp, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some(chosen) = self.mergeable_selection() else { return };
        let slide = self.slide().clone();
        let shapes: Vec<&ibis_doc_model::Shape> = chosen.iter().filter_map(|id| slide.shapes.iter().find(|s| s.id == *id)).collect();
        let mut next = Vec::new();
        for _ in 0..shapes.len().max(3) * 2 {
            next.push(self.document.ids().new_shape_id());
        }
        let mut ids = next.into_iter();
        let Some(made) = merge(&shapes, op, || ids.next().unwrap_or(ShapeId(u64::MAX))) else { return };
        let index = chosen.iter().filter_map(|id| slide.shape_index(*id)).min().unwrap_or(0);
        let mut commands = vec![Command::RemoveShapes { slide: slide.id, shapes: chosen.clone() }];
        let new_ids: Vec<ShapeId> = made.iter().map(|s| s.id).collect();
        for (k, shape) in made.into_iter().enumerate() {
            commands.push(Command::InsertShape { slide: slide.id, index: index + k, shape });
        }
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(new_ids);
        }
    }
}
