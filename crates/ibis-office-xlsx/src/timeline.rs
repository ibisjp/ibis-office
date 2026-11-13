// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pressing a timeline (タイムライン): a period selected (Shift: a run of them), the clear icon, the level changed, the
//! bar scrolled. The pivot tables it is on keep the items within the selection.

use gpui_kit::*;
use ibis_doc_model::pivot::PivotItem;
use ibis_doc_model::slicer::locate_pivot;
use ibis_doc_model::timeline::{Timeline, TimelineHit, first_shown, period_at_x, scroll_at_thumb_x};
use ibis_doc_model::{CellValue, Command, EmuRect, ShapeId, ShapeKind, SheetCommand, SlideId};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimelineDragKind {
    SelectionStart,
    SelectionEnd,
    PeriodRange(usize),
    ScrollThumb,
}

#[derive(Clone, Debug)]
pub(crate) struct TimelineDrag {
    pub(crate) slide: SlideId,
    pub(crate) shape: ShapeId,
    pub(crate) frame: EmuRect,
    pub(crate) kind: TimelineDragKind,
}

#[ibis_office_macros::editor_ext(TimelineExt)]
impl SlideEditor {
    /// A press on a timeline shape (`false`: not on any part of it, the press is for the shape).
    pub(crate) fn timeline_press(&mut self, shape: ShapeId, event: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(layout) = self.slide_layout() else { return false };
        let Some(sh) = self.slide().shape(shape) else { return false };
        let ShapeKind::Timeline(t) = &sh.kind else { return false };
        let t = (**t).clone();
        let frame = sh.frame;
        let p = layout.to_emu(event.position);
        let Some(hit) = ibis_doc_model::timeline::hit(&t, frame.width, frame.height, p.x - frame.x, p.y - frame.y) else { return false };
        let slide = self.current_slide;
        let mut next = t.clone();
        match hit {
            TimelineHit::Clear => next.selection = None,
            TimelineHit::Level => {
                next.level = (t.level + 1) % ibis_doc_model::timeline::LEVELS;
                next.selection_level = next.level;
            }
            TimelineHit::Period(i) => {
                let periods = t.periods(t.selection_level);
                next.selection = t.select_after_click(&periods, i, event.modifiers.shift);
                if let Some(state) = self.sheet_state_mut() {
                    state.timeline_drag = Some(TimelineDrag {
                        slide,
                        shape,
                        frame,
                        kind: TimelineDragKind::PeriodRange(i),
                    });
                }
            }
            TimelineHit::SelectionStart => {
                if let Some(state) = self.sheet_state_mut() {
                    state.timeline_drag = Some(TimelineDrag {
                        slide,
                        shape,
                        frame,
                        kind: TimelineDragKind::SelectionStart,
                    });
                }
            }
            TimelineHit::SelectionEnd => {
                if let Some(state) = self.sheet_state_mut() {
                    state.timeline_drag = Some(TimelineDrag {
                        slide,
                        shape,
                        frame,
                        kind: TimelineDragKind::SelectionEnd,
                    });
                }
            }
            TimelineHit::Scroll(d) => {
                let periods = t.periods(t.selection_level);
                let at = (first_shown(&periods, t.scroll) as i64 + d).clamp(0, periods.len().saturating_sub(1) as i64) as usize;
                next.scroll = periods.get(at).map_or(t.scroll, |p| p.start);
            }
            TimelineHit::ScrollThumb => {
                if let Some(state) = self.sheet_state_mut() {
                    state.timeline_drag = Some(TimelineDrag {
                        slide,
                        shape,
                        frame,
                        kind: TimelineDragKind::ScrollThumb,
                    });
                }
            }
        }
        if next != t {
            self.apply_timeline(slide, shape, next, cx);
        }
        true
    }

    /// Update timeline selection or scroll position while dragging a selection grip, period tile, or scrollbar thumb.
    pub(crate) fn on_timeline_drag_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.sheet_state().and_then(|s| s.timeline_drag.clone()) else { return };
        let Some(layout) = self.slide_layout() else { return };
        let Some(sh) = self.presentation().slide(drag.slide).and_then(|sl| sl.shape(drag.shape)) else { return };
        let ShapeKind::Timeline(t) = &sh.kind else { return };
        let t = (**t).clone();
        let p = layout.to_emu(position);
        let rx = p.x - drag.frame.x;
        let periods = t.periods(t.selection_level);
        if periods.is_empty() {
            return;
        }
        let mut next = t.clone();
        match drag.kind {
            TimelineDragKind::ScrollThumb => {
                next.scroll = scroll_at_thumb_x(&t, &periods, drag.frame.width, rx);
            }
            TimelineDragKind::PeriodRange(anchor) => {
                if let Some(idx) = period_at_x(&t, &periods, drag.frame.width, rx) {
                    let (a, b) = (anchor.min(idx), anchor.max(idx));
                    next.selection = Some((periods[a].start, periods[b].end));
                }
            }
            TimelineDragKind::SelectionStart => {
                if let (Some((_, cur_end)), Some(idx)) = (t.selection, period_at_x(&t, &periods, drag.frame.width, rx)) {
                    let new_start = periods[idx].start;
                    if new_start < cur_end {
                        next.selection = Some((new_start, cur_end));
                    } else {
                        next.selection = Some((periods[idx].start, periods[idx].end));
                    }
                }
            }
            TimelineDragKind::SelectionEnd => {
                if let (Some((cur_start, _)), Some(idx)) = (t.selection, period_at_x(&t, &periods, drag.frame.width, rx)) {
                    let new_end = periods[idx].end;
                    if new_end > cur_start {
                        next.selection = Some((cur_start, new_end));
                    } else {
                        next.selection = Some((periods[idx].start, periods[idx].end));
                    }
                }
            }
        }
        if next != t {
            self.apply_timeline(drag.slide, drag.shape, next, cx);
        }
    }

    pub(crate) fn on_timeline_drag_up(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(state) = self.sheet_state_mut()
            && state.timeline_drag.take().is_some()
        {
            cx.notify();
            return true;
        }
        false
    }

    /// The timeline `shape` made `next`, and the items of its pivot tables kept as it says.
    pub(crate) fn apply_timeline(&mut self, slide: SlideId, shape: ShapeId, next: Timeline, cx: &mut Context<Self>) {
        let edit = Command::Sheet(SheetCommand::EditTimeline { slide, shape, timeline: next.clone() });
        let mut filtered = false;
        for name in &next.pivots {
            let Some(target) = locate_pivot(self.presentation(), name) else { continue };
            let (source, keeper) = (next.source.clone(), next.clone());
            self.change_pivot(target, move |p| {
                let Some(f) = p.fields.iter_mut().find(|f| f.name == source) else { return };
                if f.items.iter().filter(|it| it.kind.is_none()).count() != f.shared.len() && !f.shared.is_empty() {
                    let subtotals: Vec<PivotItem> = f.items.iter().filter(|it| it.kind.is_some()).cloned().collect();
                    f.items = (0..f.shared.len() as u32)
                        .map(|x| PivotItem { x: Some(x), hidden: false, kind: None, collapsed: false })
                        .chain(subtotals)
                        .collect();
                }
                let shared = f.shared.clone();
                for item in f.items.iter_mut().filter(|it| it.kind.is_none()) {
                    if let Some(CellValue::Number(n)) = item.x.and_then(|x| shared.get(x as usize)) {
                        item.hidden = !keeper.keeps(*n);
                    }
                }
            }, cx);
            filtered = true;
        }
        if filtered {
            if let Err(err) = self.document.amend_last(vec![edit]) {
                log::warn!("timeline refused: {err}");
            }
            cx.notify();
        } else {
            self.run_sheet(vec![edit], cx);
        }
    }
}
