// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared dropdown content, including a focus-preserving cell-completion host.
use super::*;

pub(crate) const MAX_WIDTH: f32 = 600.;
/// Rank cheaply by Unicode character count, and shape at most the longest 100 entries.
pub(crate) fn content_width<T>(items:&[T], label:impl Fn(&T)->String, measure:impl Fn(&T)->f32, minimum:f32)->f32 {
    let mut ranked:Vec<_>=items.iter().enumerate().map(|(i,item)|(i,label(item).chars().count())).collect();
    ranked.sort_by(|a,b|b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().take(100).fold(minimum,|w,(i,_)|w.max(measure(&items[i]))).min(MAX_WIDTH)
}
pub(crate) fn label_width(window:&Window,label:&str,size:f32)->f32 {
    let run=TextRun {len:label.len(),font:window.text_style().font(),color:rgb(TEXT).into(),background_color:None,underline:None,strikethrough:None};
    f32::from(window.text_system().shape_line(SharedString::from(label.to_owned()),px(size),&[run],None).width)
}

pub(crate) const ROW_HEIGHT: f32 = 28.;
const PADDING: f32 = 6.;

pub(crate) fn surface() -> Div {
    v_flex().p(px(PADDING)).gap(px(PADDING)).bg(rgb(0xffffff)).text_size(px(14.)).text_color(rgb(TEXT)).on_mouse_down_stop()
}

pub(crate) fn list(id: impl Into<ElementId>, scroll: &ScrollHandle) -> Stateful<Div> {
    div().id(id).size_full().min_h_0().overflow_y_scroll().track_scroll(scroll).flex().flex_col()
}

pub(crate) fn option(id: impl Into<ElementId>, selected: bool, highlighted: bool, enabled: bool) -> Stateful<Div> {
    div().id(id).w_full().min_w_0().h(px(ROW_HEIGHT)).px(px(6.)).rounded(px(4.))
        .flex().flex_none().items_center().gap(px(6.))
        .when(selected, |d| d.bg(rgb(SELECTED_ROW_BG)))
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(if selected { Hsla::from(rgb(SELECTED_ROW_BG)) } else { crate::shell::palette::shade(0.,0.08) })))
        .when(highlighted && !selected, |d| d.bg(crate::shell::palette::shade(0.,0.08)))
}

pub(crate) fn checkmark(checked:bool,show:bool,color:Hsla)->Option<Icon> {
    (checked && show).then(||icon("check",16.,color))
}

#[derive(IntoElement)]
pub(crate) struct AnchoredDropdown {
    pub trigger: Bounds<Pixels>,
    pub width: Pixels,
    pub labels: Vec<String>,
    pub rows: Vec<AnyElement>,
    pub scroll: ScrollHandle,
    pub measured: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}
impl RenderOnce for AnchoredDropdown {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let viewport = window.viewport_size();
        let desired_height = px(self.rows.len() as f32 * ROW_HEIGHT + PADDING * 2. + 2.);
        let below = (viewport.height-self.trigger.bottom()).max(px(0.));
        let above = self.trigger.top().max(px(0.));
        let upward = desired_height.min(px(320.)) > below;
        let height = desired_height.min(px(320.)).min(if upward { above } else { below });
        let width = px(content_width(&self.labels,|s|s.clone(),|s|label_width(window,s,14.)+PADDING*2.+12.+10.+2.,f32::from(self.width))).min(viewport.width);
        let position = point(self.trigger.left().clamp(px(0.),(viewport.width-width).max(px(0.))),
            if upward { self.trigger.top()-height } else { self.trigger.bottom() });
        let scroll = self.scroll;
        let content = surface().relative().w(width).min_w(width).max_w(width).h(height)
            .border_1().border_color(rgb(BORDER)).rounded(px(6.)).shadow_md()
            .child(div().relative().flex_1().min_h_0().w_full()
                .child(list("validation-choices-list", &scroll).children(self.rows))
                .child(always_visible_scrollbar_overlay("validation-choices-scrollbar", scroll, None)))
            .child(canvas(|_,_,_|(),move |b,_,_,_|self.measured.set(Some(b))).absolute().inset_0());
        // Explicitly choose the edge of the whole trigger rectangle; switching a point anchor
        // would align an upward popup to the trigger's bottom instead of its top.
        deferred(anchored().position(position).snap_to_window().child(content)).with_priority(2)
    }
}
