// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::*;
#[derive(IntoElement)]
pub(crate) struct ScrollbarOverlay {
    pub id: ElementId,
    pub handle: ScrollHandle,
    pub known_heights: Option<(f32,f32)>,
    pub horizontal: bool,
}
impl RenderOnce for ScrollbarOverlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state=window.use_keyed_state(self.id.clone(),cx,|_,_|Rc::new(std::cell::Cell::new(None::<Pixels>)));
        let drag=state.read(cx).clone();
        let handle=self.handle;
        let horizontal=self.horizontal;
        let coordinate=move |p:Point<Pixels>|if horizontal {p.x} else {p.y};
        div().id(self.id).absolute()
            .when(horizontal,|d|d.left_0().right_0().bottom_0().h(px(10.)))
            .when(!horizontal,|d|d.top_0().bottom_0().right_0().w(px(10.))).child(
            canvas(|b,w,_|w.insert_hitbox(b,HitboxBehavior::Normal),move |bounds,hitbox,w,_| {
                let vh=f32::from(if horizontal {bounds.size.width} else {bounds.size.height}).max(1.);
                let max=f32::from(coordinate(handle.max_offset())).max(0.);
                let (vh_content,ch,max)=if max>0.5 {(vh,vh+max,max)} else if let Some((v,c))=self.known_heights.filter(|(v,c)|*c>*v+0.5){(v,c,c-v)}else{return};
                let thumb_h=(vh*(vh_content/ch).clamp(0.05,1.)).max(16.).min(vh);
                let travel=(vh-thumb_h).max(1.);
                let start=coordinate(bounds.origin);
                let thumb_top=start+px(travel*(-f32::from(coordinate(handle.offset()))/max).clamp(0.,1.));
                let thumb=if horizontal {
                    Bounds::new(point(thumb_top,bounds.top()+px(1.5)),size(px(thumb_h),px(7.)))
                } else {Bounds::new(point(bounds.left()+px(1.5),thumb_top),size(px(7.),px(thumb_h)))};
                w.paint_quad(gpui::fill(thumb,crate::shell::palette::shade(0.,0.28)).corner_radii(px(3.5)));
                let apply={let h=handle.clone();move |y:Pixels,grab:Pixels,w:&mut Window| {
                    let ratio=(f32::from(y-start-grab)/travel).clamp(0.,1.);
                    let offset=px(-ratio*max);
                    h.set_offset(if horizontal {point(offset,h.offset().y)} else {point(h.offset().x,offset)});w.refresh();
                }};
                let down=drag.clone();let scroll=apply.clone();let capture=hitbox.id;
                w.on_mouse_event(move|e:&MouseDownEvent,phase,w,cx|{
                    if phase==DispatchPhase::Bubble && e.button==MouseButton::Left && hitbox.is_hovered(w){
                        let grab=if thumb.contains(&e.position){coordinate(e.position)-thumb_top}else{px(thumb_h/2.)};
                        down.set(Some(grab));w.capture_pointer(capture);scroll(coordinate(e.position),grab,w);cx.stop_propagation();
                    }
                });
                let moving=drag.clone();
                w.on_mouse_event(move|e:&MouseMoveEvent,phase,w,cx|{
                    if phase==DispatchPhase::Capture && let Some(grab)=moving.get(){apply(coordinate(e.position),grab,w);cx.stop_propagation();}
                });
                let up=drag.clone();
                w.on_mouse_event(move|e:&MouseUpEvent,phase,w,cx|{
                    if phase==DispatchPhase::Capture && e.button==MouseButton::Left && up.take().is_some(){w.release_pointer();cx.stop_propagation();}
                });
            }).absolute().inset_0())
    }
}
