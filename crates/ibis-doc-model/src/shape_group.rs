// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Grouping shapes (配置 > グループ化) and copying groups with their shapes: the calculation the window and the MCP tools share.

use std::collections::{HashMap, HashSet};

use crate::command::Command;
use crate::model::{GroupMember, ShapeGroup, Slide};
use crate::ShapeId;

/// The shapes of a group, at any depth.
pub fn leaves(group: &ShapeGroup, out: &mut Vec<ShapeId>) {
    for child in &group.children {
        match child {
            GroupMember::Shape(id) => out.push(*id),
            GroupMember::Group(inner) => leaves(inner, out),
        }
    }
}

/// The ids (`native_id`) the groups and the groups inside them use.
pub fn native_ids(groups: &[ShapeGroup]) -> HashSet<u32> {
    fn walk(groups: &[ShapeGroup], out: &mut HashSet<u32>) {
        for g in groups {
            if let Ok(id) = g.native_id.parse::<u32>() {
                out.insert(id);
            }
            for c in &g.children {
                if let GroupMember::Group(inner) = c {
                    walk(std::slice::from_ref(inner.as_ref()), out);
                }
            }
        }
    }
    let mut out = HashSet::new();
    walk(groups, &mut out);
    out
}

/// The first id from 10000 that `used` does not have.
fn unused_native_id(used: &HashSet<u32>) -> u32 {
    (10_000u32..).find(|n| !used.contains(n)).unwrap_or(u32::MAX)
}

/// The biggest n of the groups named "グループ n" or "Group n" (the names of both languages are read), at any depth.
fn max_group_number(groups: &[ShapeGroup]) -> usize {
    let mut max = 0;
    for g in groups {
        if let Some(n) = g.name.strip_prefix("グループ ").or_else(|| g.name.strip_prefix("Group ")) { // i18n-ok（保存されたグループ名の接頭辞（日本語・英語の両方を読む））
            if let Ok(n) = n.parse::<usize>() {
                max = max.max(n);
            }
        }
        for c in &g.children {
            if let GroupMember::Group(inner) = c {
                max = max.max(max_group_number(std::slice::from_ref(inner.as_ref())));
            }
        }
    }
    max
}

/// How many things (groups and shapes outside groups) the selection holds at the top level: two or more can be grouped.
pub fn top_level_count(slide: &Slide, selected: &[ShapeId]) -> usize {
    let mut counted: HashSet<ShapeId> = HashSet::new();
    let mut count = 0;
    for g in &slide.groups {
        let mut l = Vec::new();
        leaves(g, &mut l);
        if l.iter().any(|id| selected.contains(id)) {
            count += 1;
            counted.extend(l);
        }
    }
    count + selected.iter().filter(|id| !counted.contains(id)).count()
}

/// The result of `group`.
pub struct Grouping {
    pub commands: Vec<Command>,
    /// The shapes of the new group, in stacking order.
    pub leaves: Vec<ShapeId>,
    pub native_id: String,
}

/// Group the selected shapes: a group that has a selected shape joins whole (a group inside the new one), the members then stand together in the
/// stacking order. `name_of(n)` is the name of the new group, n one more than the biggest number in the names of the slide's groups.
/// None when the selection holds fewer than two things.
pub fn group(slide: &Slide, selected: &[ShapeId], name_of: impl Fn(usize) -> String) -> Option<Grouping> {
    if selected.len() < 2 {
        return None;
    }
    let mut children = Vec::new();
    let mut remaining = Vec::new();
    let mut consumed: HashSet<ShapeId> = HashSet::new();
    for g in &slide.groups {
        let mut l = Vec::new();
        leaves(g, &mut l);
        if l.iter().any(|id| selected.contains(id)) {
            children.push(GroupMember::Group(Box::new(g.clone())));
            consumed.extend(l);
        } else {
            remaining.push(g.clone());
        }
    }
    for shape in &slide.shapes {
        if selected.contains(&shape.id) && !consumed.contains(&shape.id) {
            children.push(GroupMember::Shape(shape.id));
            consumed.insert(shape.id);
        }
    }
    if children.len() < 2 {
        return None;
    }
    let first_index = |m: &GroupMember| match m {
        GroupMember::Shape(id) => slide.shape_index(*id).unwrap_or(usize::MAX),
        GroupMember::Group(g) => {
            let mut l = Vec::new();
            leaves(g, &mut l);
            l.iter().filter_map(|id| slide.shape_index(*id)).min().unwrap_or(usize::MAX)
        }
    };
    children.sort_by_key(first_index);
    let native_id = unused_native_id(&native_ids(&slide.groups));
    let new_group = ShapeGroup { has_unmodelled_children: false, soft_edge: None, native_id: native_id.to_string(), name: name_of(max_group_number(&slide.groups) + 1), children };
    let mut target = Vec::new();
    leaves(&new_group, &mut target);
    target.sort_by_key(|id| slide.shape_index(*id).unwrap_or(usize::MAX));
    let mut stack: Vec<ShapeId> = slide.shapes.iter().map(|s| s.id).collect();
    let first = stack.iter().position(|id| target.contains(id))?;
    let mut commands = Vec::new();
    for (offset, &leaf) in target.iter().enumerate() {
        let at = stack.iter().position(|&id| id == leaf)?;
        if at != first + offset {
            commands.push(Command::MoveShape { slide: slide.id, shape: leaf, index: first + offset });
            stack.remove(at);
            stack.insert(first + offset, leaf);
        }
    }
    remaining.push(new_group);
    commands.push(Command::SetShapeGroups { slide: slide.id, groups: remaining });
    Some(Grouping { commands, leaves: target, native_id: native_id.to_string() })
}

/// The groups whose shapes are all among `shape_ids` (the groups a copy of those shapes takes along): the outermost ones, whole; where an
/// outer group is only partly chosen, the groups inside it that are chosen whole (a chosen group in a group is copied as a group).
pub fn groups_of(groups: &[ShapeGroup], shape_ids: &[ShapeId]) -> Vec<ShapeGroup> {
    fn collect(groups: &[ShapeGroup], set: &HashSet<ShapeId>, out: &mut Vec<ShapeGroup>) {
        for g in groups {
            let mut l = Vec::new();
            leaves(g, &mut l);
            if !l.is_empty() && l.iter().all(|id| set.contains(id)) {
                out.push(g.clone());
            } else {
                let inner: Vec<ShapeGroup> = g.children.iter().filter_map(|c| if let GroupMember::Group(i) = c { Some((**i).clone()) } else { None }).collect();
                collect(&inner, set, out);
            }
        }
    }
    let set: HashSet<ShapeId> = shape_ids.iter().copied().collect();
    let mut out = Vec::new();
    collect(groups, &set, &mut out);
    out
}

/// A copy of a group (and the groups in it) over the copied shapes: `id_map` is from the original shapes to their copies, `used` the
/// `native_id`s taken (the ones of the new groups are added).
pub fn copy_group(group: &ShapeGroup, id_map: &HashMap<ShapeId, ShapeId>, used: &mut HashSet<u32>) -> ShapeGroup {
    let native = unused_native_id(used);
    used.insert(native);
    let children = group
        .children
        .iter()
        .map(|c| match c {
            GroupMember::Shape(id) => GroupMember::Shape(*id_map.get(id).unwrap_or(id)),
            GroupMember::Group(inner) => GroupMember::Group(Box::new(copy_group(inner, id_map, used))),
        })
        .collect();
    ShapeGroup { has_unmodelled_children: false, soft_edge: None, native_id: native.to_string(), name: group.name.clone(), children }
}
