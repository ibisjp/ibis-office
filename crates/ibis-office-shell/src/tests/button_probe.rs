// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Opt-in measurement of actual rendered command buttons, never a direct handler shortcut.
use super::Scenario;
use gpui_kit::component::button::Button;
use gpui_kit::*;
use std::collections::BTreeMap;
thread_local! {
    static ENABLED:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};
    static COLLECT_INPUTS:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};
    static INPUTS:std::cell::RefCell<Vec<WeakEntity<gpui_kit::component::input::InputState>>>=const{std::cell::RefCell::new(Vec::new())};
    static SAVE_AS:std::cell::Cell<Option<u32>>=const{std::cell::Cell::new(None)};
    static FOCUS:std::cell::RefCell<BTreeMap<String,FocusHandle>>=const{std::cell::RefCell::new(BTreeMap::new())};
    static BOUNDS:std::cell::RefCell<BTreeMap<String,Bounds<Pixels>>>=const{std::cell::RefCell::new(BTreeMap::new())};
}
pub struct Probe { previous: bool }
impl Probe {
    pub fn new() -> Self {
        let previous=ENABLED.with(|x| x.replace(true));
        clear();
        Self { previous }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        ENABLED.with(|x| x.set(self.previous));
        clear();
    }
}
pub fn clear() {
    FOCUS.with(|m| m.borrow_mut().clear());
    BOUNDS.with(|m| m.borrow_mut().clear());
}
pub fn bounds(id: &str) -> Option<Bounds<Pixels>> {
    BOUNDS.with(|m| m.borrow().get(id).copied())
}
/// Measurement for canvas-based controls which do not contain a Div.
pub(crate) fn record_bounds(id: &str, bounds: Bounds<Pixels>) {
    if ENABLED.with(|enabled| enabled.get()) {
        BOUNDS.with(|all| { all.borrow_mut().insert(id.to_owned(), bounds); });
    }
}
pub fn all() -> Vec<String> {
    BOUNDS.with(|m| m.borrow().keys().cloned().collect())
}
pub(crate) fn observe(button: Button, id: ElementId) -> Button {
    if !ENABLED.with(|x| x.get()) {
        return button;
    }
    let key = match id {
        ElementId::Name(n) => n.to_string(),
        other => format!("{other:?}"),
    };
    button.relative().child(
        canvas(
            |_, _, _| (),
            move |b, _, _, _| {
                BOUNDS.with(|m| {
                    m.borrow_mut().insert(key.clone(), b);
                })
            },
        )
        .absolute()
        .inset_0(),
    )
}
pub fn observe_control(control:Stateful<Div>,id:ElementId)->Stateful<Div>{
 if !ENABLED.with(|x|x.get()){return control;}
 let key=match id{ElementId::Name(n)=>n.to_string(),other=>format!("{other:?}")};
 control.relative().child(canvas(|_,_,_|(),move|b,_,_,_|BOUNDS.with(|m|{m.borrow_mut().insert(key.clone(),b);})).absolute().inset_0())
}
pub fn click(s: &mut Scenario, id: &str) {
    for _ in 0..8 {
        let b = bounds(id).unwrap_or_else(|| panic!("button {id} not rendered: {:?}", all()));
        let dialog = s.with(|e, _| e.dialog.bounds);
        // A button outside the dialog (a confirmation box over it) is not scrolled with the dialog.
        let dialog = dialog.filter(|d| b.origin.x < d.origin.x + d.size.width && b.origin.x + b.size.width > d.origin.x);
        if let Some(d) = dialog {
            if b.origin.y + b.size.height > d.origin.y + d.size.height - px(5.) {
                s.wheel(
                    (
                        f32::from(d.origin.x + d.size.width - px(18.)),
                        f32::from(d.origin.y + px(70.)),
                    ),
                    -180.,
                );
                s.settle(); // Paint the scrolled dialog, so the next bounds are where the button is now.
                continue;
            }
            if b.origin.y < d.origin.y + px(44.) {
                s.wheel(
                    (
                        f32::from(d.origin.x + d.size.width - px(18.)),
                        f32::from(d.origin.y + px(70.)),
                    ),
                    180.,
                );
                s.settle();
                continue;
            }
        }
        s.click((
            f32::from(b.origin.x + b.size.width / 2.),
            f32::from(b.origin.y + b.size.height / 2.),
        ));
        return;
    }
    panic!("button {id} could not be brought into view");
}

pub fn input_scope(on: bool) {
    if ENABLED.with(|e| e.get()) {
        COLLECT_INPUTS.with(|v| v.set(on));
        if on {
            INPUTS.with(|v| v.borrow_mut().clear());
            clear();
        }
    }
}
pub(crate) fn record_input(input: &Entity<gpui_kit::component::input::InputState>) {
    if ENABLED.with(|v| v.get()) && COLLECT_INPUTS.with(|v| v.get()) {
        INPUTS.with(|v| {
            let mut v = v.borrow_mut();
            if !v.iter().any(|i| i.entity_id() == input.entity_id()) {
                v.push(input.downgrade());
            }
        });
    }
}
pub fn inputs(_cx: &App) -> Vec<Entity<gpui_kit::component::input::InputState>> {
    INPUTS.with(|v| v.borrow().iter().filter_map(|input| input.upgrade()).collect())
}

/// Start (`true`) or stop (`false`) catching 名前を付けて保存: while catching, the dialog is not opened and the asks are counted.
pub fn catch_save_as(on: bool) {
    SAVE_AS.with(|c| c.set(on.then_some(0)));
}
/// Called where the save-as dialog would open: true (and counted) while catching, so the dialog is left out of a scenario.
pub(crate) fn save_as_caught() -> bool {
    SAVE_AS.with(|c| match c.get() {
        Some(n) => {
            c.set(Some(n + 1));
            true
        }
        None => false,
    })
}
/// How many times save-as was asked while catching.
pub fn save_as_asks() -> u32 {
    SAVE_AS.with(|c| c.get().unwrap_or(0))
}

/// The focus handle of a control the production code registers (the segment buttons), so a scenario can put the keyboard focus there.
pub(crate) fn record_focus(id: &str, handle: FocusHandle) {
    if ENABLED.with(|enabled| enabled.get()) {
        FOCUS.with(|all| { all.borrow_mut().insert(id.to_owned(), handle); });
    }
}
/// Put the keyboard focus on the registered control `id` (what a Tab arriving there does).
pub fn focus(s: &mut Scenario, id: &str) {
    let handle = FOCUS.with(|m| m.borrow().get(id).cloned()).unwrap_or_else(|| panic!("no focus handle {id}"));
    s.update(|_, window, cx| handle.focus(window, cx));
    s.settle();
}
/// The registered control that has the keyboard focus now.
pub fn focused(s: &mut Scenario) -> Option<String> {
    let all: Vec<(String, FocusHandle)> = FOCUS.with(|m| m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    s.update(|_, window, _| all.into_iter().find(|(_, h)| h.is_focused(window)).map(|(k, _)| k))
}
