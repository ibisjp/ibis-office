// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What is typed into stays in the part of the window the soft keyboard leaves (iOS and Android; design `tasks/doc/test-design/K-keyboard-scroll-into-view.md`).
//!
//! The soft keyboard takes the window's bottom and shortens the body (`editor.rs`, K0064). When that hides what is typed into (the caret, the cell, the end of the
//! composing text, the clause being chosen), or when what is typed into moves while the keyboard is up (a long entry running to the right), the body's scroll is moved
//! by the least that brings it back in view with [`MARGIN`] to spare, along both axes. A keyboard that goes does not bring the scroll back; one that takes no room (floating,
//! hardware: height 0) moves nothing. The same for the three screens: what is typed into is read with the input hooks (`bounds_for_range`, the input method's candidate window
//! uses them), the scroll is the body's `workspace_scroll`.
use gpui_kit::*;

use crate::editor::SlideEditor;
use crate::format::with_format;

/// The room kept between what must be seen and the edge of the part of the window that is left (points).
pub const MARGIN: f32 = 12.0;

/// How many frames after a change the place is looked at again (the layout of the shortened body may be a frame behind the keyboard's height).
const LOOKS: u8 = 3;

#[derive(Default)]
pub struct KeyboardScroll {
    /// What was seen at the last look: the keyboard's height, and where what is typed into is on the content (the window's place less the scroll).
    seen: Option<(i32, [i32; 4])>,
    looks_left: u8,
    /// Where the target was in the window just before the last scroll: when it is still there at the next look the scroll did nothing for it (the body cannot bring it
    /// in view), and looking stops.
    moved_from: Option<(i32, i32)>,
}

impl SlideEditor {
    /// Called as the window is painted: brings what is typed into in view when the keyboard (or the place) changed. See the module's docs.
    pub(crate) fn reveal_input_over_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keyboard = if self.keyboard_toolbar_shown() { self.soft_keyboard.height } else { 0.0 };
        if keyboard <= 0.0 {
            // No keyboard (or none that takes room): nothing to bring in view, and nothing is scrolled back when it goes.
            self.kb_scroll.seen = None;
            self.kb_scroll.looks_left = 0;
            return;
        }
        let Some(view) = self.workspace_bounds else { return };
        let Some((must, extra)) = self.input_focus(window, cx) else { return };
        let offset = self.workspace_scroll.offset();
        let on_content = [
            (f32::from(must.left()) - f32::from(offset.x)).round() as i32,
            (f32::from(must.top()) - f32::from(offset.y)).round() as i32,
            f32::from(must.size.width).round() as i32,
            f32::from(must.size.height).round() as i32,
        ];
        let seen = (keyboard.round() as i32, on_content);
        if self.kb_scroll.seen != Some(seen) {
            self.kb_scroll.seen = Some(seen);
            self.kb_scroll.looks_left = LOOKS;
            self.kb_scroll.moved_from = None;
        }
        if self.kb_scroll.looks_left == 0 {
            return;
        }
        self.kb_scroll.looks_left -= 1;
        let here = (f32::from(must.left()).round() as i32, f32::from(must.top()).round() as i32);
        if self.kb_scroll.moved_from == Some(here) {
            // The last scroll left the target where it was: no more.
            self.kb_scroll.looks_left = 0;
            self.kb_scroll.moved_from = None;
            return;
        }
        // The clause first; the end of the composing text too when both fit.
        let target = match extra {
            Some(end) => {
                let both = union(must, end);
                if fits(both, view) { both } else { must }
            }
            None => must,
        };
        if with_format!(self, reveal_input_by_itself(self, view)) {
            cx.notify();
            return;
        }
        let (dx, dy) = shift_into(target, view);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let scroll = self.workspace_scroll.clone();
        let max = scroll.max_offset();
        let offset = scroll.offset();
        let x = (f32::from(offset.x) + dx).clamp(-f32::from(max.x).max(0.0), 0.0);
        let y = (f32::from(offset.y) + dy).clamp(-f32::from(max.y).max(0.0), 0.0);
        if px(x) != offset.x || px(y) != offset.y {
            self.kb_scroll.moved_from = Some(here);
            scroll.set_offset(point(px(x), px(y)));
            cx.notify();
        }
    }

    /// What must be seen of what is typed into, and what should be seen too if it fits: the box of the clause being chosen (and the end of the composing text), the end of the composing
    /// text, or the caret's character; the worksheet's current cell before its editor opens.
    fn input_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<(Bounds<Pixels>, Option<Bounds<Pixels>>)> {
        // Typing in a field that is not in the body (the formula bar): the body is not moved for that field, which it could never bring in; the cell it edits is what is shown.
        if with_format!(self, typing_outside_the_body(self, window, cx)) {
            return with_format!(self, input_place(self)).map(|p| (p, None));
        }
        let empty = Bounds::default();
        let rect = |this: &mut Self, range: std::ops::Range<usize>, window: &mut Window, cx: &mut Context<Self>| -> Option<Bounds<Pixels>> {
            with_format!(this, bounds_for_range(this, range, empty, window, cx))
        };
        if let Some(marked) = with_format!(self, marked_text_range(self, window, cx)).filter(|m| m.end > m.start) {
            let end = rect(self, marked.end.saturating_sub(1)..marked.end, window, cx);
            if let Some(clause) = with_format!(self, ime_thick_clause(self)).filter(|c| c.end > c.start)
                && let Some(clause_box) = rect(self, marked.start + clause.start..marked.start + clause.end, window, cx)
            {
                return Some((clause_box, end));
            }
            return end.map(|e| (e, None));
        }
        let selection = with_format!(self, selected_text_range(self, false, window, cx));
        if let Some(selection) = selection {
            let at = selection.range.end;
            let caret = if at > 0 { rect(self, at - 1..at, window, cx) } else { rect(self, 0..1, window, cx) };
            if let Some(caret) = caret {
                return Some((caret, None));
            }
        }
        with_format!(self, input_place(self)).map(|p| (p, None))
    }
}

fn union(a: Bounds<Pixels>, b: Bounds<Pixels>) -> Bounds<Pixels> {
    let (l, t) = (a.left().min(b.left()), a.top().min(b.top()));
    let (r, bo) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
    Bounds::new(point(l, t), size(r - l, bo - t))
}

fn fits(inner: Bounds<Pixels>, view: Bounds<Pixels>) -> bool {
    f32::from(inner.size.width) <= f32::from(view.size.width) - 2.0 * MARGIN && f32::from(inner.size.height) <= f32::from(view.size.height) - 2.0 * MARGIN
}

/// How far the content must move (points; positive: right, down) to bring `inner` inside `view` with the margin: the least. What is larger than the view shows its
/// left (top) edge.
fn shift_into(inner: Bounds<Pixels>, view: Bounds<Pixels>) -> (f32, f32) {
    let axis = |lo: f32, hi: f32, view_lo: f32, view_hi: f32| -> f32 {
        let (min, max) = (view_lo + MARGIN, view_hi - MARGIN);
        if hi - lo > max - min {
            // Wider than the room: its start is what is kept.
            return min - lo;
        }
        if hi > max {
            max - hi
        } else if lo < min {
            min - lo
        } else {
            0.0
        }
    };
    (
        axis(f32::from(inner.left()), f32::from(inner.right()), f32::from(view.left()), f32::from(view.right())),
        axis(f32::from(inner.top()), f32::from(inner.bottom()), f32::from(view.top()), f32::from(view.bottom())),
    )
}
