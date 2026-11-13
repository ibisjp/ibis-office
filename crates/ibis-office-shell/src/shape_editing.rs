// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The edits of shapes that the three formats share, as the menus, the toolbar, the keys and the pointer ask for them: the
//! selection, running commands as one undo step, delete, nudge, duplicate and paste, order, rotate, flip, align, group, the
//! pictures' crop and colours, and what the panels of `shell` and the worksheet's shapes call. A slide, a worksheet's drawing
//! layer and a document's page hold the same `ibis_doc_model::Shape`s, so these are here; what only a presentation has (slides,
//! layouts, notes, the tools' drags) stays in `pptx/editing.rs`. A format's own part of an edit is asked through the hooks of
//! `shell/format.rs` (`commands_ran`, `delete_own_selection`).

use gpui_kit::*;
use ibis_doc_model::{Color, Command, EmuRect, Rgb, Shape, ShapeId, ShapeKind, SlideId, TextBody, TextStyle};

use crate::editor::SlideEditor;
use crate::interaction;
use crate::shape_items::*;

/// How far an arrow key moves the selection, in EMU (0.2 cm, PowerPoint's
/// grid in Japanese); with ⌥, a point.
const NUDGE: i64 = 72_000;

const FINE_NUDGE: i64 = 12_700;

/// The commands that crop a picture to `crop`: the crop, and the frame cut by what is cropped (the part that stays is not
/// moved or stretched; see `crop_mode::recropped_frame`).
pub fn crop_commands(slide: SlideId, shape: &ibis_doc_model::Shape, picture: &ibis_doc_model::Picture, crop: ibis_doc_model::Crop) -> Vec<Command> {
    if picture.crop == crop { return Vec::new(); }
    let mut commands = Vec::new();
    let mut picture = picture.clone();
    picture.crop = crop;
    let frame = crate::crop_mode::recropped_frame(shape.frame, shape.rotation, shape.flip_h, shape.flip_v, match &shape.kind { ShapeKind::Picture(p) => p.crop, _ => crop }, crop);
    commands.push(Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) });
    if let Some(frame) = frame && frame != shape.frame { commands.push(Command::SetFrame { slide, shape: shape.id, frame }); }
    commands
}

impl SlideEditor {
    /// The selected shapes of the current slide, back to front: those of `shape_edit.selection`; in a screen that keeps its own
    /// editing state (a workbook, a document) with none there, the one shape the shell's selection names (not a sheet's own shape).
    pub fn selected_ids(&self) -> Vec<ShapeId> {
        if self.uses_shape_edit() || !self.shape_edit.selection.is_empty() {
            self.slide().shapes.iter().map(|s| s.id).filter(|id| self.shape_edit.selection.contains(id)).collect()
        } else if let Some(id) = self.selected_sheet_shape() {
            vec![id]
        } else {
            Vec::new()
        }
    }

    pub fn selected_shapes(&self) -> Vec<&Shape> {
        let ids = self.selected_ids();
        self.slide().shapes.iter().filter(|s| ids.contains(&s.id)).collect()
    }

    /// Select `ids` (the last is the one the shell's single selection names). A screen where one shape at a time is selected (a
    /// document) keeps only the shell's one.
    pub fn set_selection(&mut self, ids: Vec<ShapeId>) {
        self.selection = ids.last().copied();
        if self.selects_several_shapes() {
            self.shape_edit.selection = ids;
        }
    }

    /// Keep the selection to shapes on the current slide, and in step with
    /// the shell's (which a slide change, a load or text editing may set).
    pub fn sync_selection(&mut self) {
        if !self.selects_several_shapes() {
            return;
        }
        let on_slide: Vec<ShapeId> = self.slide().shapes.iter().map(|s| s.id).collect();
        let shell = self.selection;
        self.shape_edit.selection.retain(|id| on_slide.contains(id));
        if shell != self.shape_edit.selection.last().copied() {
            self.shape_edit.selection = shell.into_iter().filter(|id| on_slide.contains(id)).collect();
        }
    }

    /// Run `commands` as one undo step.
    pub fn run(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
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
        // The sections or the slides' order changed: the summary zooms follow (`refresh_summary_zooms`).
        let structural = commands.iter().any(|c| matches!(c, Command::SetSections { .. } | Command::InsertSlide { .. } | Command::RemoveSlide { .. } | Command::MoveSlide { .. } | Command::SlideStructure { .. }));
        let flows = !commands.iter().all(|c| matches!(c, Command::SetTextChains { .. }));
        if let Err(err) = self.document.execute(commands) {
            log::warn!("edit rejected: {err}");
        } else if flows && !self.slide().text_chains.is_empty() && self.text_editing().is_none() {
            // Linked text boxes: the text flows through them again (a box deleted, moved or resized, a size or format changed, a paste).
            self.reflow_all_chains();
        }
        crate::format::with_format!(self, commands_ran(self, structural, cx));
        if let Some(edit) = self.shape_edit.edit.as_mut() {
            edit.is_new = false;
            edit.typing = false;
        }
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.is_new = false;
            edit.edit.typing = false;
        }
        self.sync_selection();
        if self.format_info().several_windows.as_ref().is_some_and(|w| w.follow_shape_edits) {
            self.sync_peers(cx);
        }
        cx.notify();
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        // A selection of the format's own inside the shape (a vertex of 頂点の編集) is what Delete takes.
        if crate::format::with_format!(self, delete_own_selection(self, cx)) {
            return;
        }
        let slide = self.current_slide;
        let selected = self.selected_ids();
        let mut commands: Vec<Command> = selected.iter().map(|&shape| Command::RemoveShape { slide, shape }).collect();
        // Boxes of a chain deleted: the text they hold flows on through the boxes that are left (it is not deleted with them).
        for chain in self.slide().text_chains.clone() {
            if !chain.boxes.iter().any(|b| selected.contains(b)) {
                continue;
            }
            let Some(all) = chain.boxes.first().and_then(|&b| self.chain_boxes_of(b)) else { continue };
            let remaining: Vec<ShapeId> = all.iter().copied().filter(|b| !selected.contains(b)).collect();
            if remaining.is_empty() {
                continue;
            }
            let full = self.chain_text(&all, Some(&chain));
            if let Some(flow) = self.flow_command(&remaining, full) {
                commands.insert(0, flow);
            }
        }
        self.set_selection(Vec::new());
        self.run(commands, cx);
    }

    pub fn nudge_selected(&mut self, dx: i64, dy: i64, fine: bool, cx: &mut Context<Self>) {
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
    pub fn empty_content_placeholder_for(&self, specific_kind: &str) -> Option<(ShapeId, EmuRect)> {
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
    pub fn insert_shape(&mut self, shape: Shape, cx: &mut Context<Self>) -> ShapeId {
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

    pub fn can_copy_shapes(shapes: &[Shape]) -> bool {
        !shapes.is_empty() && shapes.iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok())
    }

    pub fn extract_groups_for_shapes(groups: &[ibis_doc_model::ShapeGroup], shape_ids: &[ShapeId]) -> Vec<ibis_doc_model::ShapeGroup> {
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
            soft_edge: None,
            native_id: next_native_id.to_string(),
            name: group.name.clone(),
            children: new_children,
        }
    }

    /// All selected shapes have been checked before allocating ids or editing.
    pub fn copies_with_groups(
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

        // Linked text boxes (as PowerPoint's own would be copied together or not at all): a copy of a whole chain is a chain of its own;
        // a copy of some of its boxes has no link, only the text its box holds.
        let chain_copies: Vec<ibis_doc_model::TextChain> = self.presentation().slides.iter().flat_map(|sl| sl.text_chains.iter())
            .filter(|c| c.boxes.iter().all(|b| id_map.contains_key(b)))
            .map(|c| ibis_doc_model::TextChain { boxes: c.boxes.iter().map(|b| id_map[b]).collect(), joined: c.joined.clone(), padding: c.padding.clone() })
            .collect();
        if self.uses_shape_edit() {
            self.shape_edit.pending_chain_copies = chain_copies;
        }

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

    pub fn insert_copies(
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
        let chain_copies = std::mem::take(&mut self.shape_edit.pending_chain_copies);
        if !chain_copies.is_empty() {
            let mut chains = self.slide().text_chains.clone();
            chains.extend(chain_copies);
            commands.push(Command::SetTextChains { slide, chains, bodies: Vec::new() });
        }
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(ids);
        }
    }

    /// 複製（⌘D）: copies of the selection just below and to the right.
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        if !Self::can_copy_shapes(&shapes) { return; }
        let shape_ids: Vec<ShapeId> = shapes.iter().map(|s| s.id).collect();
        let groups = Self::extract_groups_for_shapes(&self.slide().groups, &shape_ids);
        let (copies, _, copied_groups) = self.copies_with_groups(&shapes, &groups, PASTE_OFFSET);
        self.insert_copies(copies, copied_groups, cx);
    }

    /// Paste shapes: at their place on another slide, moved down and right
    /// on the slide they were copied from (further each time).
    pub fn paste_shapes(&mut self, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        if !self.uses_shape_edit() || !Self::can_copy_shapes(&self.shape_edit.clipboard) {
            return false;
        }
        let same = self.shape_edit.clipboard_slide == Some(slide);
        if same {
            self.shape_edit.pastes += 1;
        }
        let dx = if same { PASTE_OFFSET * self.shape_edit.pastes as i64 } else { 0 };
        let shapes = self.shape_edit.clipboard.clone();
        let groups = self.shape_edit.clipboard_groups.clone();
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
    pub fn order_selected(&mut self, order: Order, cx: &mut Context<Self>) {
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
    pub fn rotate_selected(&mut self, degrees: i32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let commands = self
            .selected_shapes()
            .into_iter()
            .map(|s| Command::SetTransform { slide, shape: s.id, rotation: s.rotation + degrees * 60_000, flip_h: s.flip_h, flip_v: s.flip_v })
            .collect();
        self.run(commands, cx);
    }

    pub fn flip_selected(&mut self, horizontal: bool, cx: &mut Context<Self>) {
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
    pub fn align_selected(&mut self, align: Align, cx: &mut Context<Self>) {
        let shapes: Vec<(ShapeId, EmuRect, EmuRect)> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, interaction::bounds(s))).collect();
        if shapes.is_empty() {
            return;
        }
        let area = if shapes.len() == 1 || self.shape_edit.align_to_slide {
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
    pub fn center_selected(&mut self, horizontal: bool, cx: &mut Context<Self>) {
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

    pub fn collect_group_leaves(group: &ibis_doc_model::ShapeGroup, out: &mut Vec<ShapeId>) {
        for child in &group.children {
            match child {
                ibis_doc_model::GroupMember::Shape(id) => out.push(*id),
                ibis_doc_model::GroupMember::Group(inner) => Self::collect_group_leaves(inner, out),
            }
        }
    }

    pub fn can_group(&self) -> bool {
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

    pub fn can_ungroup(&self) -> bool {
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
    pub fn group_selected(&mut self, cx: &mut Context<Self>) {
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
        let new_name = crate::i18n::format("_pptx.group-n", &[("n", &(max_group_num + 1))]);

        let new_group = ibis_doc_model::ShapeGroup {
            has_unmodelled_children: false,
            soft_edge: None,
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
    pub fn ungroup_selected(&mut self, cx: &mut Context<Self>) {
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

    /// The characters a text format goes to: the text selected in the
    /// shape being edited (the edit goes on), else all of each selected
    /// shape's text. (shape, start, end) in characters.
    pub fn text_targets(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Vec<(ShapeId, usize, usize)> {
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

    pub fn format_characters(&mut self, window: &mut Window, cx: &mut Context<Self>, format: ibis_doc_model::CharacterFormat) {
        let slide = self.current_slide;
        let commands = self
            .text_targets(window, cx)
            .into_iter()
            .map(|(shape, start, end)| Command::SetCharacterFormat { slide, shape, start, end, format: format.clone() })
            .collect();
        let commands = self.in_edited_cell(commands);
        self.run(commands, cx);
    }

    pub fn format_text_body(&mut self, update: impl Fn(&mut TextBody), cx: &mut Context<Self>) {
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
    pub fn set_text_warp(&mut self, warp: Option<&str>, cx: &mut Context<Self>) {
        let warp_owned = warp.map(str::to_string);
        self.format_text_body(|body| {
            body.warp = warp_owned.clone();
        }, cx);
    }

    /// 選択中のテキストボックス・図形の文字変形プリセット名を取得する。
    pub fn selected_text_warp(&self) -> Option<String> {
        let targets = if let Some(edit) = self.text_editing() { vec![edit.shape] } else { self.selected_ids().to_vec() };
        let shape = *targets.first()?;
        let body = if self.text_editing().is_some_and(|e| e.shape == shape) { self.edited_body()? } else { self.slide().shape(shape)?.text_body()? };
        body.warp.clone()
    }

    /// 選択中のテキストにワードアートスタイルを適用する。
    pub fn set_selected_text_wordart_style(&mut self, style_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
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
            outline: Some(preset.outline.map(|(c, w)| (Color::opaque(c), w))),
            shadow: Some(shadow),
            ..Default::default()
        });
    }

    /// The first selected character, or the caret's paragraph and local offset.
    pub fn selected_text_position(&self) -> Option<(&TextBody, usize, usize)> {
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

    /// The list of the active logical paragraph (soft breaks share its list).
    pub fn selected_list(&self) -> Option<ibis_doc_model::ListKind> {
        let (body, mut index, _) = self.selected_text_position()?;
        while index > 0 && body.paragraphs[index].line_break { index -= 1; }
        Some(ibis_doc_model::ListKind::of(body.paragraphs[index].bullet.as_ref()))
    }

    /// Put picture file `data` (named `name`) on the current slide: at its
    /// size at 96 dpi, made smaller to fit the slide, in the middle.
    pub fn insert_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ShapeId> {
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

    /// 行の挿入・削除、列の挿入・削除 for the selected table, at the cell
    /// being edited (else its last row and column): `change` gives the new
    /// table, how much the frame grows (width, height) and the cell to edit
    /// after. The new cells are as the table's style makes them (the table
    /// read back through the file, when there is one).
    pub fn change_table(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl Fn(&ibis_doc_model::Table, (usize, usize)) -> Option<(ibis_doc_model::Table, (i64, i64), (usize, usize))>) {
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

    /// A table of `rows` by `columns`, its cells holding `contents` (by row) when given.
    pub fn insert_slide_table_with(&mut self, rows: usize, columns: usize, contents: Option<&[Vec<String>]>, window: &mut Window, cx: &mut Context<Self>) {
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

    pub fn table_insert_row(&mut self, below: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.change_table(window, cx, |t, (r, c)| {
            let at = if below { r + 1 } else { r };
            let height = t.rows.get(r).map_or(0, |row| row.height);
            Some((t.with_row_inserted(at)?, (0, height), (at, c)))
        });
    }

    /// Change the canvas without scaling or moving its contents.
    pub fn set_slide_size(&mut self, size: ibis_doc_model::EmuSize, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.presentation().slide_size != size {
            self.run(vec![Command::SetSlideSize { size }], cx);
        }
    }

    /// Change the canvas and scale what is on it by `mode` (最大化・サイズに
    /// 合わせて調整: all of one scale, centred): the slides' shapes and
    /// backdrops, the layouts (and, saved, their parts and the masters').
    pub fn set_slide_size_scaled(&mut self, size: ibis_doc_model::EmuSize, mode: super::size_panel::ScaleMode, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn crop_selected_pictures(&mut self, crop: ibis_doc_model::Crop, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            Some(crop_commands(self.current_slide, &shape, picture, crop))
        }).flatten().collect();
        self.run(commands, cx);
    }

    /// Crop selected pictures to a rounded-corner shape (corner in 100,000ths of shorter side; 0 for square).
    pub fn crop_selected_pictures_corner(&mut self, corner: u32, cx: &mut Context<Self>) {
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
    pub fn set_selected_pictures_duotone(&mut self, duotone: Option<(ibis_doc_model::Rgb, ibis_doc_model::Rgb)>, cx: &mut Context<Self>) {
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
    pub fn set_selected_pictures_luminance(&mut self, luminance: Option<(i32, i32)>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.luminance == luminance { return None; }
            let mut picture = picture.clone();
            picture.luminance = luminance;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    pub fn apply_pptx_commands_checked(&mut self,commands:Vec<Command>,cx:&mut Context<Self>) -> bool {
        if commands.is_empty() {return true;}
        if !self.format_info().shapes.try_first {
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

    /// Reset picture formatting (stroke, corner, duotone, clear, luminance, shadow, reflection, soft_edge, artistic_effect), keeping crop and frame.
    pub fn reset_picture_effects(&mut self, cx: &mut Context<Self>) {
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

    /// Set or clear transparent color for selected pictures.
    #[allow(dead_code)]
    pub fn set_selected_pictures_clear(&mut self, clear: Option<ibis_doc_model::Rgb>, cx: &mut Context<Self>) {
        let commands = self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Picture(picture) = &shape.kind else { return None };
            if picture.clear == clear { return None; }
            let mut picture = picture.clone();
            picture.clear = clear;
            Some(Command::SetKind { slide: self.current_slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) })
        }).collect();
        self.run(commands, cx);
    }

    pub fn is_pptx_gridlines_enabled(&self) -> bool {
        self.shape_edit.gridlines
    }

    pub fn toggle_pptx_gridlines(&mut self, cx: &mut Context<Self>) {
        if self.uses_shape_edit() {
            self.shape_edit.gridlines = !self.shape_edit.gridlines;
            cx.notify();
        }
    }

    pub fn is_pptx_guides_enabled(&self) -> bool {
        self.shape_edit.guides
    }

    pub fn is_pptx_snap_to_grid_enabled(&self) -> bool {
        self.shape_edit.snap_to_grid
    }

    pub fn toggle_pptx_snap_to_grid(&mut self, cx: &mut Context<Self>) {
        if self.uses_shape_edit() {
            self.shape_edit.snap_to_grid = !self.shape_edit.snap_to_grid;
            cx.notify();
        }
    }

    pub fn is_pptx_snap_to_shape_enabled(&self) -> bool {
        self.shape_edit.snap_to_shape
    }

    pub fn toggle_pptx_snap_to_shape(&mut self, cx: &mut Context<Self>) {
        if self.uses_shape_edit() {
            self.shape_edit.snap_to_shape = !self.shape_edit.snap_to_shape;
            cx.notify();
        }
    }

    /// プレゼンテーションが最終版としてマークされているか（PowerPoint の「最終版にする」）。
    pub fn is_pptx_mark_as_final(&self) -> bool {
        self.presentation().mark_as_final
    }

    /// 読み取り専用を推奨する設定になっているか（PowerPoint の「読み取り専用を推奨する」）。
    pub fn is_pptx_read_only_recommended(&self) -> bool {
        self.presentation().read_only_recommended
    }

    /// 読み取り専用推奨の情報バーで「編集を有効にする」を押したか。
    pub fn is_pptx_read_only_dismissed(&self) -> bool {
        self.uses_shape_edit() && self.shape_edit.read_only_dismissed
    }

    /// Whether ibisOffice's own features PowerPoint does not have are on for this file (`SetIbisExtensions`).
    pub fn ibis_extensions_on(&self) -> bool {
        self.presentation().ibis_extensions
    }

    /// Runs `then` for a feature PowerPoint does not have. In a file that has not turned them on yet, asks first ("この機能は
    /// PowerPoint にない機能です。有効にしますか？"): OK turns them on for the whole file (saved in it) and runs `then`;
    /// キャンセル does nothing. Once on, the file is not asked again.
    pub fn require_ibis_extensions(&mut self, then: impl Fn(&mut Self, &mut Context<Self>) + 'static, cx: &mut Context<Self>) {
        self.require_ibis_extensions_asking("shell.ibis-ext.message", then, cx);
    }

    /// `require_ibis_extensions` with the question's text `message_key` (a feature of the spreadsheet screen says what Excel does
    /// with it, not what PowerPoint does). The answer is the same one flag of the file.
    pub fn require_ibis_extensions_asking(&mut self, message_key: &str, then: impl Fn(&mut Self, &mut Context<Self>) + 'static, cx: &mut Context<Self>) {
        if self.ibis_extensions_on() {
            then(self, cx);
            return;
        }
        let then = std::rc::Rc::new(then);
        self.ask(
            crate::i18n::text("shell.ibis-ext.title"),
            crate::i18n::text(message_key),
            std::rc::Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                this.run(vec![ibis_doc_model::Command::SetIbisExtensions { enabled: true }], cx);
                if this.ibis_extensions_on() {
                    (*then)(this, cx);
                }
            }),
            cx,
        );
    }

    /// The shape (shape, picture, chart) selected on the current worksheet.
    pub fn selected_sheet_shape(&self) -> Option<ShapeId> {
        if let Some(id) = self.selection {
            if let Some(shape) = self.slide().shapes.iter().find(|s| s.id == id) {
                if !matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_)) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// 太字 and the like as toggles: on unless the targeted text already
    /// starts with it on (as PowerPoint toggles).
    pub fn toggle_characters(&mut self, window: &mut Window, cx: &mut Context<Self>, is_on: fn(&TextStyle, &ibis_doc_model::RunStyle) -> bool, format: fn(bool) -> ibis_doc_model::CharacterFormat) {
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
}
