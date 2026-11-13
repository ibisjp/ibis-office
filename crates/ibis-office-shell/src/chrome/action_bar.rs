// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Screen code supplies data; this module creates every terminal command button.
use super::*;

pub(crate) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type BoundsObserver = Rc<dyn Fn(Bounds<Pixels>, &mut Window, &mut App)>;

/// No widget or styling escape hatch: size, padding and colors belong to the renderer.
pub struct ActionDefinition {
    id: ElementId,
    label: SharedString,
    kind: ActionButtonKind,
    disabled: bool,
    icon: Option<Icon>,
    click: Option<ClickHandler>,
    observers: Vec<BoundsObserver>,
    no_submit: bool,
    /// The keyboard is on this button (Tab moves it, Enter or Space presses it): drawn with a focus ring.
    focused: bool,
}
impl gpui_kit::prelude::FluentBuilder for ActionDefinition {}
impl ActionDefinition {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: ActionButtonKind) -> Self {
        Self { id: id.into(), label: label.into(), kind, disabled: false, icon: None, click: None, observers: Vec::new(), no_submit: false, focused: false }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self { self.label = label.into(); self }
    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.click = Some(Rc::new(handler)); self
    }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    /// Enter does not press this button even when it is the only plain one (it abandons a mode, such as range picking, instead of finishing the dialog).
    pub fn no_submit(mut self) -> Self { self.no_submit = true; self }
    /// The keyboard's button of the bar (a focus ring around it).
    pub fn focused(mut self, focused: bool) -> Self { self.focused = focused; self }
    #[allow(dead_code)] // Shared-control option; no action button has an icon yet.
    pub(crate) fn icon(mut self, icon: impl Into<Icon>) -> Self { self.icon = Some(icon.into()); self }
    pub fn on_bounds(mut self, observer: impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> Self {
        self.observers.push(Rc::new(observer)); self
    }
    fn render(self, cx: &App, access: &crate::access_key::Resolved) -> AnyElement {
        let mut button = styled_action_button(self.id, ActionButtonStyle { kind: self.kind }, Some(cx))
            .small().disabled(self.disabled);
        // The access letter is underlined by the bar (shell/access_key.rs); without one it is the plain label.
        button = if access.underline.is_some() {
            button.accessibility_label(access.text.clone()).child(crate::access_key::label_element(access))
        } else { button.label(access.text.clone()) };
        if let Some(icon) = self.icon { button = button.icon(icon); }
        if let Some(handler) = self.click { button = button.on_click(move |event,w,cx| handler(event,w,cx)); }
        // The keyboard's button: a ring outside it in the selection colour (macOS draws its focus ring so).
        let button: AnyElement = if self.focused {
            div().flex_none().rounded(px(8.)).border_2().border_color(rgb(crate::chrome::PRIMARY_BUTTON_BG)).p(px(1.)).child(button).into_any_element()
        } else { button.into_any_element() };
        if self.observers.is_empty() { return button; }
        let observers = self.observers;
        div().relative().flex_none().child(button).child(canvas(|_,_,_| (), move |bounds,_,w,cx| {
            for observer in &observers { observer(bounds,w,cx); }
        }).absolute().inset_0()).into_any_element()
    }
}

#[derive(IntoElement)]
pub struct ActionBar {
    definitions: Vec<ActionDefinition>,
    align_end: bool,
    nowrap: bool,
    scope: crate::access_key::Scope,
}
impl ActionBar {
    pub(super) fn new() -> Self { Self { definitions: Vec::new(), align_end: false, nowrap: false, scope: crate::access_key::Scope::Dialog } }
    /// Which layer owns the Alt access keys of this bar (default: a dialog). A message box says `MessageBox`,
    /// a pane footer `Off`. Nothing else about the keys is a screen's business (shell/access_key.rs).
    pub(crate) fn access_scope(mut self, scope: crate::access_key::Scope) -> Self { self.scope = scope; self }
    pub fn action(mut self, definition: ActionDefinition) -> Self { self.definitions.push(definition); self }
    pub fn actions(mut self, definitions: impl IntoIterator<Item=ActionDefinition>) -> Self { self.definitions.extend(definitions); self }
    pub(crate) fn submit_action(&self)->(Option<ClickHandler>,bool,bool){
        if let Some(primary)=self.definitions.iter().find(|d|matches!(d.kind,ActionButtonKind::Primary)){return (primary.click.clone().filter(|_|!primary.disabled),false,true);}
        if self.definitions.len()==1 && matches!(self.definitions[0].kind,ActionButtonKind::Normal) && !self.definitions[0].no_submit {let d=&self.definitions[0];return (d.click.clone().filter(|_|!d.disabled),true,true);}
        (None,false,false)
    }
    /// The buttons as the keyboard sees them (message box: shell/message_box.rs): kind, enabled, what a press runs.
    pub(crate) fn key_buttons(&self) -> Vec<(ActionButtonKind, bool, Option<ClickHandler>)> {
        self.definitions.iter().map(|d| (d.kind, !d.disabled, d.click.clone())).collect()
    }
    /// The keyboard is on button `index` (a ring around it); the others lose it.
    pub(crate) fn focus_button(mut self, index: usize) -> Self {
        for (i, d) in self.definitions.iter_mut().enumerate() { d.focused = i == index; }
        self
    }
    pub fn into_definitions(self) -> Vec<ActionDefinition> { self.definitions }
    pub fn justify_end(mut self) -> Self { self.align_end = true; self }
    pub fn flex_nowrap(mut self) -> Self { self.nowrap = true; self }
}
impl RenderOnce for ActionBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Alt access keys: resolved for the whole bar (no duplicates), registered for this draw, off where the OS has none.
        let on = crate::access_key::supported() && self.scope != crate::access_key::Scope::Off;
        let labels: Vec<&str> = self.definitions.iter().map(|d| d.label.as_ref()).collect();
        let resolved = crate::access_key::resolve(&labels, on);
        if on {
            for (d, r) in self.definitions.iter().zip(&resolved) {
                if let Some(key) = r.key { crate::access_key::register(window, self.scope, key, d.click.clone().filter(|_| !d.disabled)); }
            }
        }
        h_flex().w_full().flex_none().justify_center().items_center().flex_wrap().gap_2()
            .when(self.align_end, |row| row.justify_end())
            .when(self.nowrap, |row| row.flex_nowrap())
            .px(px(12.)).py_0().bg(rgb(0xffffff))
            .children(self.definitions.into_iter().zip(resolved).map(|(definition, access)| definition.render(cx, &access)))
    }
}
