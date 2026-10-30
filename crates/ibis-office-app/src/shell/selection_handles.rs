// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared selection frame/handle appearance for all document formats.
use gpui_kit::*;
use ibis_doc_model::ResizeHandle;
pub(crate) const OUTLINE_WIDTH: f32 = 1.;
pub(crate) const HANDLE_SIZE: f32 = 6.;
pub(crate) const ROTATION_SIZE: f32 = (HANDLE_SIZE + 2.) * 1.3;
#[derive(Clone, Copy, Default)]
pub(crate) struct Options {
    pub rotation: bool,
    pub angle: i32,
}
pub(crate) fn additive(m: Modifiers) -> bool {
    m.shift || m.control || m.platform
}
pub(crate) fn square(
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    handle: ResizeHandle,
    blue: Hsla,
    fill: Hsla,
) -> Stateful<Div> {
    let id = id.into();
    let cursor = match handle {
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        ResizeHandle::Top | ResizeHandle::Bottom => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeLeftRight,
    };
    let control = div()
        .id(id.clone())
        .absolute()
        .left(at.x - px(HANDLE_SIZE / 2.))
        .top(at.y - px(HANDLE_SIZE / 2.))
        .size(px(HANDLE_SIZE))
        .border(px(OUTLINE_WIDTH))
        .border_color(blue)
        .bg(fill)
        .cursor(cursor);
    measured(control, id)
}
pub(crate) fn group(
    id: SharedString,
    points: impl IntoIterator<Item = (ResizeHandle, Point<Pixels>)>,
    rotation: Option<Point<Pixels>>,
    options: Options,
    blue: Hsla,
    fill: Hsla,
) -> Vec<(Option<ResizeHandle>, Stateful<Div>)> {
    let points = points.into_iter().collect::<Vec<_>>();
    let top = points
        .iter()
        .find(|(h, _)| *h == ResizeHandle::Top)
        .map(|(_, p)| *p);
    let mut out = points
        .into_iter()
        .map(|(handle, p)| {
            (
                Some(handle),
                rotated_square(
                    (id.clone(), handle as usize),
                    p,
                    handle,
                    blue,
                    fill,
                    options.angle,
                ),
            )
        })
        .collect::<Vec<_>>();
    if options.rotation
        && let Some(p) = rotation
    {
        if let Some(top) = top {
            out.insert(
                0,
                (
                    None,
                    div().id((id.clone(), 9_usize)).absolute().inset_0().child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, w, _| {
                                let mut path = PathBuilder::stroke(px(OUTLINE_WIDTH));
                                path.move_to(bounds.origin + top);
                                path.line_to(bounds.origin + p);
                                if let Ok(path) = path.build() {
                                    w.paint_path(path, blue);
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
                ),
            );
        }
        out.push((
            None,
            div()
                .id((id, 8_usize))
                .absolute()
                .left(p.x - px(ROTATION_SIZE / 2.))
                .top(p.y - px(ROTATION_SIZE / 2.))
                .size(px(ROTATION_SIZE))
                .rounded_full()
                .bg(gpui_kit::white())
                .border(px(OUTLINE_WIDTH))
                .border_color(blue)
                .cursor(CursorStyle::OpenHand),
        ));
    }
    out
}

pub(crate) fn outline(bounds: Bounds<Pixels>, blue: Hsla) -> Div {
    div()
        .absolute()
        .left(bounds.origin.x)
        .top(bounds.origin.y)
        .w(bounds.size.width)
        .h(bounds.size.height)
        .border(px(OUTLINE_WIDTH))
        .border_color(blue)
}

/// Cursor axes repeat every 180 degrees; quantize to the nearest 45-degree axis.
pub(crate) fn rotated_cursor(handle: ResizeHandle, angle: i32) -> CursorStyle {
    let base = match handle {
        ResizeHandle::Left | ResizeHandle::Right => 0.,
        ResizeHandle::Top | ResizeHandle::Bottom => 90.,
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => 45.,
        _ => 135.,
    };
    match ((base + angle as f64 / 60000.) / 45.).round() as i32 % 4 {
        0 => CursorStyle::ResizeLeftRight,
        1 | -3 => CursorStyle::ResizeUpLeftDownRight,
        2 | -2 => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn rotated_square(
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    handle: ResizeHandle,
    blue: Hsla,
    fill: Hsla,
    angle: i32,
) -> Stateful<Div> {
    let id = id.into();
    if angle.rem_euclid(21600000) == 0 {
        return square(id, at, handle, blue, fill);
    }
    let radians = (angle as f32 / 60000.).to_radians();
    let (sin, cos) = radians.sin_cos();
    let extent = HANDLE_SIZE * (sin.abs() + cos.abs());
    let control = div()
        .id(id.clone())
        .absolute()
        .left(at.x - px(extent / 2.))
        .top(at.y - px(extent / 2.))
        .size(px(extent))
        .cursor(rotated_cursor(handle, angle))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, w, _| {
                    let center = bounds.center();
                    let half = HANDLE_SIZE / 2.;
                    let points = [(-half, -half), (half, -half), (half, half), (-half, half)]
                        .map(|(x, y)| center + point(px(x * cos - y * sin), px(x * sin + y * cos)));
                    let mut surface = PathBuilder::fill();
                    surface.move_to(points[0]);
                    for p in &points[1..] {
                        surface.line_to(*p);
                    }
                    surface.close();
                    if let Ok(p) = surface.build() {
                        w.paint_path(p, fill);
                    }
                    let mut stroke = PathBuilder::stroke(px(OUTLINE_WIDTH));
                    stroke.move_to(points[0]);
                    for p in &points[1..] {
                        stroke.line_to(*p);
                    }
                    stroke.close();
                    if let Ok(p) = stroke.build() {
                        w.paint_path(p, blue);
                    }
                },
            )
            .absolute()
            .inset_0(),
        );
    measured(control, id)
}

fn measured(control: Stateful<Div>, id: ElementId) -> Stateful<Div> {
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    let control = crate::shell::scenario::button_probe::observe_control(control, id).absolute();
    #[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
    let _ = id;
    control
}

/// A frame hit, tested in the shape's unrotated coordinates.
pub(crate) fn on_border(frame:(f32,f32,f32,f32),angle:i32,p:(f32,f32),slop:f32)->bool {
    let (x,y,w,h)=frame;let (s,c)=(-(angle as f32/60000.).to_radians()).sin_cos();let (cx,cy)=(x+w/2.,y+h/2.);let (dx,dy)=(p.0-cx,p.1-cy);let (px,py)=(dx*c-dy*s+cx,dx*s+dy*c+cy);
    px>=x-slop&&px<=x+w+slop&&py>=y-slop&&py<=y+h+slop&&!(px>x+slop&&px<x+w-slop&&py>y+slop&&py<y+h-slop)
}
/// Transparent frame-only interaction, leaving text and resize handles reachable.
pub(crate) fn border_region(id:impl Into<ElementId>,frame:Bounds<Pixels>,angle:i32,down:impl Fn(&MouseDownEvent,&mut Window,&mut App)+'static)->Stateful<Div>{
    let f=(f32::from(frame.left()),f32::from(frame.top()),f32::from(frame.size.width),f32::from(frame.size.height));
    let (s,c)=(angle as f32/60000.).to_radians().sin_cos();let (cx,cy)=(f.0+f.2/2.,f.1+f.3/2.);
    let rotate=|x:f32,y:f32|{let (dx,dy)=(x-cx,y-cy);point(px(cx+dx*c-dy*s),px(cy+dx*s+dy*c))};
    let points=ResizeHandle::ALL.map(|h|{let(a,b)=h.anchor();rotate(f.0+f.2*a as f32/2.,f.1+f.3*b as f32/2.)});
    let min_x=points.iter().map(|p|f32::from(p.x)).fold(f32::INFINITY,f32::min)-4.;let min_y=points.iter().map(|p|f32::from(p.y)).fold(f32::INFINITY,f32::min)-4.;
    let max_x=points.iter().map(|p|f32::from(p.x)).fold(f32::NEG_INFINITY,f32::max)+4.;let max_y=points.iter().map(|p|f32::from(p.y)).fold(f32::NEG_INFINITY,f32::max)+4.;
    let down=std::rc::Rc::new(down);
    div().id(id).absolute().left(px(min_x)).top(px(min_y)).w(px(max_x-min_x)).h(px(max_y-min_y)).child(canvas(|b,w,_|w.insert_hitbox(b,HitboxBehavior::Normal),move|bounds,hit,w,_|{
        let delta=bounds.origin-point(px(min_x),px(min_y));
        let test=move|p:Point<Pixels>|{let p=p-delta;on_border(f,angle,(f32::from(p.x),f32::from(p.y)),3.)&&!points.iter().any(|h|(p.x-h.x).abs()<=px(5.)&&(p.y-h.y).abs()<=px(5.))};
        if test(w.mouse_position()){w.set_cursor_style(CursorStyle::OpenHand,&hit);}
        let hover_hit=hit.clone();
        w.on_mouse_event(move|ev:&MouseMoveEvent,phase,w,_|{if phase==DispatchPhase::Bubble&&hover_hit.is_hovered(w)&&test(ev.position){w.set_window_cursor_style(CursorStyle::OpenHand);}});
        let down=down.clone();
        w.on_mouse_event(move|ev:&MouseDownEvent,phase,w,cx|{if phase==DispatchPhase::Capture&&ev.button==MouseButton::Left&&hit.is_hovered(w)&&test(ev.position){down(ev,w,cx);cx.stop_propagation();}});
    }).absolute().inset_0())
}
