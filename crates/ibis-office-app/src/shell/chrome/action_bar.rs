// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Screen code supplies data; this module creates every terminal command button.
use super::*;

pub(crate) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type BoundsObserver = Rc<dyn Fn(Bounds<Pixels>, &mut Window, &mut App)>;

/// No widget or styling escape hatch: size, padding and colors belong to the renderer.
pub(crate) struct ActionDefinition {
    id: ElementId,
    label: SharedString,
    kind: ActionButtonKind,
    disabled: bool,
    icon: Option<Icon>,
    click: Option<ClickHandler>,
    observers: Vec<BoundsObserver>,
    no_submit: bool,
}
impl gpui_kit::prelude::FluentBuilder for ActionDefinition {}
impl ActionDefinition {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: ActionButtonKind) -> Self {
        Self { id: id.into(), label: label.into(), kind, disabled: false, icon: None, click: None, observers: Vec::new(), no_submit: false }
    }
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self { self.label = label.into(); self }
    pub(crate) fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.click = Some(Rc::new(handler)); self
    }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    /// Enter does not press this button even when it is the only plain one (it abandons a mode, such as range picking, instead of finishing the dialog).
    pub(crate) fn no_submit(mut self) -> Self { self.no_submit = true; self }
    pub(crate) fn icon(mut self, icon: impl Into<Icon>) -> Self { self.icon = Some(icon.into()); self }
    pub(crate) fn on_bounds(mut self, observer: impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> Self {
        self.observers.push(Rc::new(observer)); self
    }
    fn render(self, cx: &App) -> AnyElement {
        let mut button = styled_action_button(self.id, ActionButtonStyle { kind: self.kind }, Some(cx))
            .small().label(self.label).disabled(self.disabled);
        if let Some(icon) = self.icon { button = button.icon(icon); }
        if let Some(handler) = self.click { button = button.on_click(move |event,w,cx| handler(event,w,cx)); }
        if self.observers.is_empty() { return button.into_any_element(); }
        let observers = self.observers;
        div().relative().flex_none().child(button).child(canvas(|_,_,_| (), move |bounds,_,w,cx| {
            for observer in &observers { observer(bounds,w,cx); }
        }).absolute().inset_0()).into_any_element()
    }
}

#[derive(IntoElement)]
pub(crate) struct ActionBar {
    definitions: Vec<ActionDefinition>,
    align_end: bool,
    nowrap: bool,
}
impl ActionBar {
    pub(super) fn new() -> Self { Self { definitions: Vec::new(), align_end: false, nowrap: false } }
    pub(crate) fn action(mut self, definition: ActionDefinition) -> Self { self.definitions.push(definition); self }
    pub(crate) fn actions(mut self, definitions: impl IntoIterator<Item=ActionDefinition>) -> Self { self.definitions.extend(definitions); self }
    pub(crate) fn submit_action(&self)->(Option<ClickHandler>,bool,bool){
        if let Some(primary)=self.definitions.iter().find(|d|matches!(d.kind,ActionButtonKind::Primary)){return (primary.click.clone().filter(|_|!primary.disabled),false,true);}
        if self.definitions.len()==1 && matches!(self.definitions[0].kind,ActionButtonKind::Normal) && !self.definitions[0].no_submit {let d=&self.definitions[0];return (d.click.clone().filter(|_|!d.disabled),true,true);}
        (None,false,false)
    }
    pub(crate) fn into_definitions(self) -> Vec<ActionDefinition> { self.definitions }
    pub(crate) fn justify_end(mut self) -> Self { self.align_end = true; self }
    pub(crate) fn flex_nowrap(mut self) -> Self { self.nowrap = true; self }
}
impl RenderOnce for ActionBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex().w_full().flex_none().justify_center().items_center().flex_wrap().gap_2()
            .when(self.align_end, |row| row.justify_end())
            .when(self.nowrap, |row| row.flex_nowrap())
            .px(px(12.)).py_0().bg(rgb(0xffffff))
            .children(self.definitions.into_iter().map(|definition| definition.render(cx)))
    }
}
