// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! All eight dialog resize directions keep the opposite edge anchored.
use super::{DialogDrag, SlideEditor};
use gpui_kit::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ResizeEdge { North, NorthEast, East, SouthEast, South, SouthWest, West, NorthWest }

impl ResizeEdge {
    fn sides(self) -> (bool, bool, bool, bool) {
        use self::ResizeEdge::*;
        (matches!(self, West|NorthWest|SouthWest), matches!(self, North|NorthWest|NorthEast),
         matches!(self, East|NorthEast|SouthEast), matches!(self, South|SouthWest|SouthEast))
    }
    pub(crate) fn bounds(self, original: Bounds<Pixels>, delta: Point<Pixels>, viewport: Size<Pixels>) -> Bounds<Pixels> {
        let (west,north,east,south) = self.sides();
        let mut left = original.origin.x;
        let mut top = original.origin.y;
        let mut right = left + original.size.width;
        let mut bottom = top + original.size.height;
        let min_width = px(320.).min(viewport.width);
        let min_height = px(180.).min(viewport.height);
        if west { left = (left+delta.x).max(px(0.)).min((right-min_width).max(px(0.))); }
        if north { top = (top+delta.y).max(px(0.)).min((bottom-min_height).max(px(0.))); }
        if east { right = (right+delta.x).max(left+min_width).min(viewport.width.max(left+min_width)); }
        if south { bottom = (bottom+delta.y).max(top+min_height).min(viewport.height.max(top+min_height)); }
        Bounds::new(point(left,top), size(right-left,bottom-top))
    }
}

/// Resolve corners before edges. Called during window capture, before body scrollbars.
pub(crate) fn edge_at(bounds: Bounds<Pixels>, position: Point<Pixels>) -> Option<ResizeEdge> {
    use self::ResizeEdge::*;
    // Keep the inner hit strip narrower than a scrollbar; the outside halo
    // makes edges easy to grab without taking the scrollbar's thumb.
    let outer=Bounds::new(bounds.origin-point(px(3.),px(3.)),size(bounds.size.width+px(6.),bounds.size.height+px(6.)));
    if !outer.contains(&position) { return None; }
    let p=position-bounds.origin;
    let right=bounds.size.width-p.x;
    let bottom=bounds.size.height-p.y;
    if p.x<=px(11.) && p.y<=px(11.) { Some(NorthWest) }
    else if right<=px(11.) && p.y<=px(11.) { Some(NorthEast) }
    else if right<=px(11.) && bottom<=px(11.) { Some(SouthEast) }
    else if p.x<=px(11.) && bottom<=px(11.) { Some(SouthWest) }
    else if p.y<=px(3.) { Some(North) }
    else if right<=px(3.) { Some(East) }
    else if bottom<=px(3.) { Some(South) }
    else if p.x<=px(3.) { Some(West) }
    else { None }
}

pub(crate) fn handles(cx: &Context<SlideEditor>) -> Vec<AnyElement> {
    use self::ResizeEdge::*;
    [North,NorthEast,East,SouthEast,South,SouthWest,West,NorthWest].into_iter().map(|edge| {
        let (id,cursor) = match edge {
            North => ("settings-dialog-resize-n",CursorStyle::ResizeUpDown),
            NorthEast => ("settings-dialog-resize-ne",CursorStyle::ResizeUpRightDownLeft),
            East => ("settings-dialog-resize-e",CursorStyle::ResizeLeftRight),
            SouthEast => ("settings-dialog-resize",CursorStyle::ResizeUpLeftDownRight),
            South => ("settings-dialog-resize-s",CursorStyle::ResizeUpDown),
            SouthWest => ("settings-dialog-resize-sw",CursorStyle::ResizeUpRightDownLeft),
            West => ("settings-dialog-resize-w",CursorStyle::ResizeLeftRight),
            NorthWest => ("settings-dialog-resize-nw",CursorStyle::ResizeUpLeftDownRight),
        };
        let editor = cx.weak_entity();
        let surface = canvas(|bounds,w,_| w.insert_hitbox(bounds,HitboxBehavior::Normal),move|_,hitbox,w,_| {
            let editor=editor.clone();
            w.on_mouse_event(move|event:&MouseDownEvent,phase,w,cx| {
                if phase!=DispatchPhase::Bubble || event.button!=MouseButton::Left || !hitbox.is_hovered(w) { return; }
                let _=editor.update(cx,|e,cx| {
                    if let Some(bounds)=e.panel_dialog.bounds {
                        e.panel_dialog.drag=Some(DialogDrag::Resize{pointer:event.position,bounds,edge});
                        w.capture_pointer(hitbox.id);cx.notify();cx.stop_propagation();
                    }
                });
            });
        }).absolute().inset_0();
        let d=div().id(id).absolute().occlude().cursor(cursor).child(surface);
        match edge {
            North=>d.top(px(-3.)).left(px(11.)).right(px(11.)).h(px(6.)),
            NorthEast=>d.top(px(-3.)).right(px(-3.)).size(px(14.)),
            East=>d.right(px(-3.)).top(px(11.)).bottom(px(11.)).w(px(6.)),
            SouthEast=>d.bottom(px(-3.)).right(px(-3.)).size(px(14.)),
            South=>d.bottom(px(-3.)).left(px(11.)).right(px(11.)).h(px(6.)),
            SouthWest=>d.bottom(px(-3.)).left(px(-3.)).size(px(14.)),
            West=>d.left(px(-3.)).top(px(11.)).bottom(px(11.)).w(px(6.)),
            NorthWest=>d.top(px(-3.)).left(px(-3.)).size(px(14.)),
        }.into_any_element()
    }).collect()
}
