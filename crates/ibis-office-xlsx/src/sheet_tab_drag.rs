// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Dragging a sheet tab along the tab strip (表計算画面（XLSX）の下端のシートタブ).
//!
//! The tab that is dragged follows the pointer sideways only (its height never changes) and wears a drop shadow.
//! The other tabs make room: when the pointer passes the middle of a tab, that tab slides to the other side of the
//! gap (the screen animates it from `starts`). Nothing is changed in the workbook until the button goes up; Escape
//! gives the drag up and the tabs are in the order they had before. The geometry here is plain numbers (pixels of
//! the window) so that it is tested without a window.
use ibis_doc_model::SlideId;
use std::collections::HashMap;

/// How far the pointer must move after the press before the press becomes a drag (the same as the rows of tables).
pub(super) const START_DISTANCE: f32 = 4.;
/// Within this distance of the strip's end the strip scrolls while the tab is dragged.
pub(super) const EDGE: f32 = 16.;
/// How much the strip scrolls at each movement of the pointer in the edge zone.
pub(super) const EDGE_STEP: f32 = 12.;

pub(crate) struct TabDrag {
    pub id: SlideId,
    /// The window position of the press.
    pub press: (f32, f32),
    /// Whether the press has become a drag.
    pub active: bool,
    /// Whether the workbook's structure is protected (the tabs cannot be moved).
    pub locked: bool,
    pub(crate) pointer_x: f32,
    /// Where on the tab the press was (the pointer's x minus the tab's left edge).
    pub grab: f32,
    /// The strip's visible part, in the window (left, right).
    pub view: (f32, f32),
    /// The strip's scroll (zero or negative) as of the last move; the "+" button, when it follows the last tab inside
    /// the strip, is where the tabs end (`float_left`).
    offset: f32,
    original: Vec<SlideId>,
    /// The tabs in the order they are shown now (the dragged tab is where it would be dropped).
    pub order: Vec<SlideId>,
    widths: HashMap<SlideId, f32>,
    /// Counts the moves of tabs, so each move is a new animation.
    pub generation: usize,
    /// Where each tab was (from the strip's left, scroll not included) before the last move.
    pub starts: HashMap<SlideId, f32>,
}

impl TabDrag {
    /// A press on tab `id`. `tabs` are the tabs in order with their widths; `tab_left` is the pressed tab's left edge
    /// in the window and `view` the strip's visible part.
    pub(crate) fn press(id: SlideId, press: (f32, f32), tabs: &[(SlideId, f32)], tab_left: f32, view: (f32, f32), locked: bool) -> Self {
        let order: Vec<SlideId> = tabs.iter().map(|t| t.0).collect();
        Self {
            id,
            press,
            active: false,
            locked,
            pointer_x: press.0,
            grab: press.0 - tab_left,
            view,
            offset: 0.,
            original: order.clone(),
            order,
            widths: tabs.iter().copied().collect(),
            generation: 0,
            starts: HashMap::new(),
        }
    }

    pub(crate) fn width(&self, id: SlideId) -> f32 {
        self.widths.get(&id).copied().unwrap_or(0.)
    }

    /// The left edge of every tab in `order`, from the strip's left (the scroll not included).
    pub(crate) fn positions(&self) -> HashMap<SlideId, f32> {
        let mut at = 0.;
        let mut out = HashMap::new();
        for id in &self.order {
            out.insert(*id, at);
            at += self.width(*id);
        }
        out
    }

    /// The dragged tab's left edge in the window: it follows the pointer and stays within the strip (its right edge
    /// never passes the "+" button's left edge), and only moves sideways.
    pub(crate) fn dragged_left(&self) -> f32 {
        let total: f32 = self.order.iter().map(|id| self.width(*id)).sum();
        float_left(self.pointer_x, self.grab, self.width(self.id), self.view, self.offset, total)
    }

    /// Whether the tabs are in another order than they were.
    pub(crate) fn changed(&self) -> bool {
        self.order != self.original
    }

    /// The pointer is at window x `x` and the strip is scrolled by `offset` (zero or negative). The dragged tab swaps
    /// places with the neighbour whose middle the pointer has passed (as shown now, the gap of the dragged tab
    /// included), as many times as the pointer has passed. Whether the tabs changed their places.
    pub(crate) fn moved(&mut self, x: f32, offset: f32) -> bool {
        self.pointer_x = x;
        self.offset = offset;
        let at = x - self.view.0 - offset;
        let before = self.positions();
        let mut swapped = false;
        loop {
            let k = self.order.iter().position(|i| *i == self.id).unwrap_or(0);
            let places = self.positions();
            let passed = |id: SlideId, forward: bool| {
                let middle = places[&id] + self.width(id) / 2.;
                if forward { at > middle } else { at < middle }
            };
            if k + 1 < self.order.len() && passed(self.order[k + 1], true) {
                self.order.swap(k, k + 1);
            } else if k > 0 && passed(self.order[k - 1], false) {
                self.order.swap(k, k - 1);
            } else {
                break;
            }
            swapped = true;
        }
        if swapped {
            self.generation += 1;
            self.starts = before;
        }
        swapped
    }

    /// Where the drop puts the dragged tab: the tab it comes after (`None`: it comes first) and the tab it comes before.
    pub(crate) fn neighbours(&self) -> (Option<SlideId>, Option<SlideId>) {
        let k = self.order.iter().position(|i| *i == self.id).unwrap_or(0);
        (k.checked_sub(1).map(|j| self.order[j]), self.order.get(k + 1).copied())
    }
}

/// The left edge (window x) of the dragged tab of `width` that the pointer at `pointer_x` holds at `grab` from the
/// tab's left edge. The tab stays inside the strip `view` (left, right); its right edge does not pass the left edge of
/// the "+" button either. The button is at the strip's right end when the tabs overflow (then it is outside `view`, and
/// `view.1` is its left edge), or right after the last tab when they fit: `total` is the width of all the tabs and
/// `offset` the scroll, so that button's left edge is `view.0 + offset + total`.
pub(super) fn float_left(pointer_x: f32, grab: f32, width: f32, view: (f32, f32), offset: f32, total: f32) -> f32 {
    let limit = view.1.min(view.0 + offset + total);
    let most = (limit - width).max(view.0);
    (pointer_x - grab).clamp(view.0, most)
}

/// How far the strip scrolls (to the left: a positive number moves the content right) when the pointer is at window
/// x `x` over the strip `view`: toward the end the pointer is near, none elsewhere.
pub(super) fn edge_scroll(x: f32, view: (f32, f32)) -> f32 {
    if x < view.0 + EDGE {
        EDGE_STEP
    } else if x > view.1 - EDGE {
        -EDGE_STEP
    } else {
        0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<(SlideId, f32)> {
        (1..=4).map(|i| (SlideId(i), 100.)).collect()
    }
    fn drag(id: u64) -> TabDrag {
        let tabs = tabs();
        let left = (id - 1) as f32 * 100.;
        TabDrag::press(SlideId(id), (left + 40., 10.), &tabs, left, (0., 400.), false)
    }

    #[test]
    fn a_tab_moves_when_the_pointer_passes_its_middle() {
        let mut d = drag(1);
        assert!(!d.moved(60., 0.), "the middle of the next tab is not passed");
        assert!(d.moved(160., 0.));
        assert_eq!(d.order, vec![SlideId(2), SlideId(1), SlideId(3), SlideId(4)]);
        assert_eq!(d.starts[&SlideId(2)], 100., "tab 2 slides from where it was");
        assert_eq!(d.positions()[&SlideId(2)], 0.);
        assert!(d.moved(30., 0.));
        assert!(!d.changed(), "back in the first place");
    }

    #[test]
    fn the_dragged_tab_stays_in_the_strip_and_keeps_its_row() {
        let mut d = drag(2);
        d.moved(-500., 0.);
        assert_eq!(d.dragged_left(), 0.);
        d.moved(900., 0.);
        assert_eq!(d.dragged_left(), 300.);
        d.moved(150., 0.);
        assert_eq!(d.dragged_left(), 110., "the grab point keeps its place on the tab");
    }

    #[test]
    fn scrolling_the_strip_moves_the_drop_place() {
        let mut d = drag(1);
        // The strip is scrolled left by 100: window x 160 is 260 from the first tab's edge.
        assert!(d.moved(160., -100.));
        assert_eq!(d.order, vec![SlideId(2), SlideId(3), SlideId(1), SlideId(4)]);
        assert_eq!(d.neighbours(), (Some(SlideId(3)), Some(SlideId(4))));
    }

    #[test]
    fn the_dragged_tab_stops_at_the_plus_button() {
        // Four tabs of 100 fit in a strip of 800: the "+" button follows the last tab, at x = 400.
        assert_eq!(float_left(900., 40., 100., (0., 800.), 0., 400.), 300., "the right edge is at the button's left edge");
        assert_eq!(float_left(-900., 40., 100., (0., 800.), 0., 400.), 0.);
        assert_eq!(float_left(150., 40., 100., (0., 800.), 0., 400.), 110.);
        // The strip starts at window x 50 (the buttons at the left) and is scrolled: the tabs end at 50 - 50 + 400.
        assert_eq!(float_left(900., 40., 100., (50., 300.), -50., 400.), 200., "overflowing: the strip's right end is the button's left edge");
        assert_eq!(float_left(900., 40., 100., (50., 300.), 0., 200.), 150., "fits with room: the button follows the tabs");
        // A tab wider than the room keeps to the left edge.
        assert_eq!(float_left(900., 0., 500., (0., 300.), 0., 200.), 0.);
    }

    #[test]
    fn the_edges_scroll_the_strip() {
        assert!(edge_scroll(5., (0., 400.)) > 0.);
        assert!(edge_scroll(395., (0., 400.)) < 0.);
        assert_eq!(edge_scroll(200., (0., 400.)), 0.);
    }
}
