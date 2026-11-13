// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drag and drop between the rows of the shared scrolling tables (`DataTable::virtualized`).
//!
//! The screen gives each row an id (`TableRow::drag`, `TableRow::drop_between`, `TableRow::drop_into`) and one
//! `RowDnd` that its tables share; the table does the rest: the press and the 4 px threshold, the floating label
//! under the pointer, the insertion line between two rows or the tint of a group row that takes the drop, the
//! release, and Escape. The screen only learns the result: `RowDropEvent` (which row of which table came to which
//! row of which table, and before it, after it or into it).
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// How far the pointer must move after the press before the press becomes a drag.
const START_DISTANCE: f32 = 4.;

/// Where in the target row the dragged row was dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlace {
    /// Above the target row (the row takes the target's place).
    Before,
    /// Below the target row.
    After,
    /// Onto the target (a group row, or the placeholder of an empty group).
    Into,
}

/// What a row does when something is dragged over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    /// Takes the drop above or below itself, by the half of the row the pointer is in.
    Between,
    /// Takes the drop as a whole (its tint shows it).
    Into,
}

/// A drop, as the screen receives it. `from` and `target` are the ids the screen gave the rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowDropEvent {
    pub from_table: &'static str,
    pub from: u64,
    pub target_table: &'static str,
    pub target: u64,
    pub place: DropPlace,
}

pub(crate) type DropHandler = Rc<dyn Fn(&mut SlideEditor, RowDropEvent, &mut Window, &mut Context<SlideEditor>)>;

#[derive(Clone)]
struct Dragged {
    table: &'static str,
    id: u64,
    /// The row's index in its table when it was pressed.
    index: usize,
    label: SharedString,
    /// How far below the row's top the pointer was when it was pressed (the row is carried by that point).
    grab: f32,
    /// Which rows it may change places with (`TableRow::drag_scope`): a row with a scope stays among the rows of that scope.
    scope: u64,
    /// The top of the row's own place (follows the table's scroll; set when the row is painted).
    top: f32,
}

#[derive(Clone)]
struct Slot {
    id: u64,
    kind: DropKind,
    scope: u64,
    bounds: Bounds<Pixels>,
}

/// A table that reorders live (`DataTable::live_reorder`): its rows are all one height and the dragged row is carried.
struct Live {
    count: usize,
    height: f32,
    /// Where the dragged row stands now among the rows (its place once taken out of the list).
    at: usize,
}

#[derive(Default)]
struct Inner {
    press: Option<(Dragged, Point<Pixels>)>,
    active: Option<Dragged>,
    pointer: Point<Pixels>,
    slots: BTreeMap<(&'static str, usize), Slot>,
    /// The slots the table forgot when it began to make its rows again (they are painted again after the rows are made, so
    /// while the rows are made, this is where the rows stood): what the carried row is placed by.
    prev: BTreeMap<(&'static str, usize), Slot>,
    live: BTreeMap<&'static str, Live>,
    hover: Option<(&'static str, usize, DropPlace)>,
    handlers: BTreeMap<&'static str, DropHandler>,
    /// For the live reorder of a table (`DataTable::live_reorder`): each row's last shift (in rows), the number of its
    /// moves so far and the shift it moved from, so a row that shifts again slides from where it was.
    shifts: BTreeMap<(&'static str, usize), (i32, usize, i32)>,
}

/// The drag and drop of one screen's tables. Cloning shares it, so the tables that take part get the same one.
#[derive(Clone, Default)]
pub struct RowDnd(Rc<RefCell<Inner>>);

impl RowDnd {
    /// The row of `table` that is being dragged (its id).
    pub fn dragging(&self) -> Option<(&'static str, u64)> {
        self.0.borrow().active.as_ref().map(|d| (d.table, d.id))
    }

    /// Where the drag would drop now: the table, the row index in the table and the place.
    pub fn hover(&self) -> Option<(&'static str, usize, DropPlace)> {
        self.0.borrow().hover
    }

    /// The bounds the table's row `index` was last painted at, if it takes drops (for tests and for the screen's own checks).
    pub fn row_bounds(&self, table: &'static str, index: usize) -> Option<Bounds<Pixels>> {
        self.0.borrow().slots.get(&(table, index)).map(|s| s.bounds)
    }

    /// Give up a drag in progress (Escape). Whether there was one.
    pub(crate) fn cancel(&self) -> bool {
        let mut d = self.0.borrow_mut();
        let (dragged, pressed) = (d.active.take().is_some(), d.press.take().is_some());
        let was = dragged || pressed;
        d.hover = None;
        d.shifts.clear();
        was
    }

    pub(crate) fn press(&self, table: &'static str, id: u64, index: usize, scope: u64, label: SharedString, at: Point<Pixels>) {
        let mut d = self.0.borrow_mut();
        let top = d.slots.get(&(table, index)).map_or(0., |s| f32::from(s.bounds.origin.y));
        d.press = Some((Dragged { table, id, index, label, grab: f32::from(at.y) - top, scope, top }, at));
        d.pointer = at;
        d.hover = None;
    }

    /// The row of `table` that is held: pressed (the button is down, the pointer may not have moved) or dragged (its id). A live
    /// table lifts it from the press on.
    pub fn carried(&self) -> Option<(&'static str, u64)> {
        let d = self.0.borrow();
        d.active.as_ref().or(d.press.as_ref().map(|(p, _)| p)).map(|c| (c.table, c.id))
    }

    /// The held row of `table` is row `index` now (the rows were made again and it is somewhere else): its place is that one. The
    /// row stays where the pointer holds it (the offset from the pointer is kept).
    ///
    /// The rows of the table are all one height, so the place the row now stands in is the old one moved by the rows between
    /// (until the row is painted and `register` tells the exact place): the held row is carried from where it stood on the
    /// screen, not from its new place, so it does not jump when the rows around it fold.
    pub(crate) fn rebase(&self, table: &'static str, index: usize) {
        let mut d = self.0.borrow_mut();
        let height = d.live.get(table).map_or(0., |l| l.height);
        let Inner { active, press, .. } = &mut *d;
        for c in [active.as_mut(), press.as_mut().map(|(p, _)| p)].into_iter().flatten() {
            if c.table == table && c.index != index {
                c.top += (index as f32 - c.index as f32) * height;
                c.index = index;
            }
        }
    }

    /// A table that reorders live tells how many rows it has and how high each is (every frame).
    pub(crate) fn set_live(&self, table: &'static str, count: usize, height: f32) {
        let mut d = self.0.borrow_mut();
        let at = d.live.get(table).map_or(0, |l| l.at);
        d.live.insert(table, Live { count, height, at });
    }

    /// The slot of row `index` of `table`: as painted last, or where it stood before the table made its rows again.
    fn slot_of(d: &Inner, table: &'static str, index: usize) -> Option<Slot> {
        d.slots.get(&(table, index)).or_else(|| d.prev.get(&(table, index))).cloned()
    }

    /// The rows the dragged row may take the place of: those of its scope that adjoin it (a row with a scope stays among
    /// the rows of that scope), or every row when it has no scope.
    fn extent(d: &Inner, from: &Dragged) -> (usize, usize) {
        let count = d.live.get(from.table).map_or(from.index + 1, |l| l.count);
        if from.scope == 0 {
            return (0, count.saturating_sub(1));
        }
        let same = |k: usize| Self::slot_of(d, from.table, k).is_some_and(|s| s.scope == from.scope);
        let (mut lo, mut hi) = (from.index, from.index);
        while lo > 0 && same(lo - 1) {
            lo -= 1;
        }
        while hi + 1 < count && same(hi + 1) {
            hi += 1;
        }
        (lo, hi)
    }

    /// The top of the place of row `k` of the live table (rows are one height; the dragged row's own place is the reference).
    fn place_top(from: &Dragged, height: f32, k: usize) -> f32 {
        from.top + (k as f32 - from.index as f32) * height
    }

    /// Where the row at `index` of `table` stands while a row of the same table is dragged over the others (the live reorder:
    /// the rows between the dragged row's place and the place it would take move aside, the dragged row goes to that place).
    /// The shift in rows (a row moves by whole rows) from where it stood to where it stands now, and the number of moves it has
    /// made (the key of its slide). `None` while no row of `table` is dragged.
    pub fn live_shift(&self, table: &'static str, index: usize) -> Option<(i32, i32, usize)> {
        let mut d = self.0.borrow_mut();
        let Some(active) = d.active.clone().filter(|a| a.table == table) else {
            d.shifts.retain(|(t, _), _| *t != table);
            return None;
        };
        let to = match d.hover.filter(|(t, _, _)| *t == table) {
            None => 0,
            Some((_, target, place)) => {
                let from = active.index;
                let slot = match place {
                    DropPlace::Before => target,
                    DropPlace::After | DropPlace::Into => target + 1,
                };
                // The place the dragged row ends in once it is taken out of the list.
                let end = if from < slot { slot - 1 } else { slot };
                if index == from {
                    end as i32 - from as i32
                } else if end > from && index > from && index <= end {
                    -1
                } else if end < from && index >= end && index < from {
                    1
                } else {
                    0
                }
            }
        };
        let (last, moves, was) = d.shifts.get(&(table, index)).copied().unwrap_or((0, 0, 0));
        if last == to {
            return Some((was, to, moves));
        }
        d.shifts.insert((table, index), (to, moves + 1, last));
        Some((last, to, moves + 1))
    }

    pub(crate) fn register(&self, table: &'static str, index: usize, id: u64, kind: DropKind, scope: u64, bounds: Bounds<Pixels>) {
        let mut d = self.0.borrow_mut();
        let top = f32::from(bounds.origin.y);
        let Inner { active, press, .. } = &mut *d;
        for a in [active.as_mut(), press.as_mut().map(|(p, _)| p)].into_iter().flatten() {
            if a.table == table && a.index == index {
                a.top = top;
            }
        }
        d.slots.insert((table, index), Slot { id, kind, scope, bounds });
    }

    /// The table's rows are about to be made again: forget where the old ones were (they are kept until the new ones are
    /// painted, for what is placed while the rows are made).
    pub(crate) fn forget(&self, table: &'static str) {
        let mut d = self.0.borrow_mut();
        let old: Vec<_> = d.slots.iter().filter(|((t, _), _)| *t == table).map(|(k, v)| (*k, v.clone())).collect();
        // The rows may be made twice before they are painted: the second time there is nothing new to keep, and what was kept stays.
        if !old.is_empty() {
            d.prev.retain(|(t, _), _| *t != table);
            d.slots.retain(|(t, _), _| *t != table);
            d.prev.extend(old);
        }
    }

    pub(crate) fn handle(&self, table: &'static str, handler: DropHandler) {
        self.0.borrow_mut().handlers.insert(table, handler);
    }

    /// Where the dragged row of a live-reordering table is carried: how far below the top of its own place (row `index` of
    /// `table`) the pointer holds it, kept inside the rows it may move among (the table's top and bottom, or its scope's).
    /// `None` unless that row is the one being dragged.
    pub fn lift_offset(&self, table: &'static str, index: usize) -> Option<f32> {
        let d = self.0.borrow();
        let a = d.active.as_ref().or(d.press.as_ref().map(|(p, _)| p)).filter(|a| a.table == table && a.index == index)?;
        let raw = f32::from(d.pointer.y) - a.top - a.grab;
        let Some(live) = d.live.get(table) else { return Some(raw) };
        let (lo, hi) = Self::extent(&d, a);
        let (min, max) = (Self::place_top(a, live.height, lo) - a.top, Self::place_top(a, live.height, hi) - a.top);
        Some(raw.clamp(min, max.max(min)))
    }

    /// The label under the pointer, when a row of `table` is dragged: its text and the pointer.
    pub fn ghost(&self, table: &'static str) -> Option<(SharedString, Point<Pixels>)> {
        let d = self.0.borrow();
        d.active.as_ref().filter(|a| a.table == table).map(|a| (a.label.clone(), d.pointer))
    }

    /// The pointer moved. Whether the screen must be drawn again.
    pub(crate) fn moved(&self, position: Point<Pixels>) -> bool {
        let mut d = self.0.borrow_mut();
        if d.active.is_none() {
            let Some((dragged, from)) = d.press.clone() else { return false };
            // The row held by a live table follows the pointer from the press, before the drag has begun.
            d.pointer = position;
            let live = d.live.contains_key(dragged.table);
            let (dx, dy) = (f32::from(position.x - from.x), f32::from(position.y - from.y));
            if (dx * dx + dy * dy).sqrt() < START_DISTANCE {
                return live;
            }
            if let Some(l) = d.live.get_mut(dragged.table) {
                l.at = dragged.index;
            }
            d.active = Some(dragged);
        }
        d.pointer = position;
        let from = d.active.clone().expect("a drag");
        // A table that reorders live follows the carried row.
        if d.live.contains_key(from.table) {
            d.hover = Self::live_hover(&mut d, &from);
            return true;
        }
        let next = d.slots.iter().find(|(_, s)| s.bounds.contains(&position)).and_then(|((table, index), s)| {
            if *table == from.table && s.id == from.id {
                return None;
            }
            if s.scope != from.scope {
                return None;
            }
            let place = match s.kind {
                DropKind::Into => DropPlace::Into,
                DropKind::Between => {
                    if position.y < s.bounds.origin.y + s.bounds.size.height / 2. { DropPlace::Before } else { DropPlace::After }
                }
            };
            Some((*table, *index, place))
        });
        d.hover = next;
        true
    }

    /// The place the carried row of a live table has reached. It passes a row when its centre
    /// passes that row's centre (`reorder_step`), and it stays among the rows it may move among. A row without a scope does
    /// not stop among the rows of another scope (a group of rows that move as one): it goes past the group.
    fn live_hover(d: &mut Inner, from: &Dragged) -> Option<(&'static str, usize, DropPlace)> {
        let (height, was) = d.live.get(from.table).map(|l| (l.height, l.at))?;
        let (lo, hi) = Self::extent(d, from);
        let top = (f32::from(d.pointer.y) - from.grab).clamp(Self::place_top(from, height, lo), Self::place_top(from, height, hi).max(Self::place_top(from, height, lo)));
        let center = top + height / 2.;
        let mut at = super::data_table::reorder_step(was.clamp(lo, hi), lo, hi, center, height, |k| Self::place_top(from, height, k));
        if from.scope == 0 && at != from.index {
            // Past a group of another scope, not inside it.
            let foreign = |k: usize| Self::slot_of(d, from.table, k).is_some_and(|s| s.scope != 0);
            if foreign(at) {
                if at > was {
                    while at < hi && foreign(at) {
                        at += 1;
                    }
                } else {
                    while at > lo && foreign(at) {
                        at -= 1;
                    }
                }
                if foreign(at) {
                    at = was;
                }
            }
        }
        let slot = Self::slot_of(d, from.table, at).or_else(|| Self::slot_of(d, from.table, from.index));
        let at = if slot.is_some() { at } else { from.index };
        if let Some(l) = d.live.get_mut(from.table) {
            l.at = at;
        }
        match at.cmp(&from.index) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some((from.table, at, DropPlace::After)),
            std::cmp::Ordering::Less => Some((from.table, at, DropPlace::Before)),
        }
    }

    /// The button went up: the drop to hand to the screen (the handler of the table it fell on), if the drag was one.
    pub(crate) fn released(&self) -> Option<(DropHandler, RowDropEvent)> {
        let mut d = self.0.borrow_mut();
        d.press = None;
        d.shifts.clear();
        let from = d.active.take()?;
        let hover = d.hover.take()?;
        let (table, index, place) = hover;
        let target = d.slots.get(&(table, index))?.id;
        let handler = d.handlers.get(table)?.clone();
        Some((handler, RowDropEvent { from_table: from.table, from: from.id, target_table: table, target, place }))
    }

    /// Whether a press is waiting to become a drag (the button is down on a draggable row).
    pub(crate) fn pressed(&self) -> bool {
        let d = self.0.borrow();
        d.press.is_some() || d.active.is_some()
    }
}

/// The label that follows the pointer.
pub(crate) fn ghost_element(label: SharedString, pointer: Point<Pixels>) -> impl IntoElement {
    deferred(
        anchored()
            .position(point(pointer.x + px(12.), pointer.y + px(10.)))
            .snap_to_window()
            .child(
                h_flex()
                    .h(px(28.))
                    .px(px(8.))
                    .gap(px(6.))
                    .items_center()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(PRIMARY_BUTTON_BG))
                    .rounded(px(4.))
                    .shadow_md()
                    .text_size(px(13.))
                    .text_color(rgb(TEXT))
                    .child(icon("drag_indicator", 16., rgb(0x9aa0a6).into()))
                    .child(label),
            ),
    )
    .with_priority(3)
}
