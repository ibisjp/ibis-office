// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Dialog policy based on controls actually rendered in its body.
use super::*;
use gpui_kit::component::form_controls::{ControlKind, FormControl};
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum SubmitType {
    #[default]
    Auto,
    None,
    Enter,
    CtrlEnter,
}
#[derive(Clone, Copy)]
pub(crate) struct Options {
    pub submit: SubmitType,
    pub auto_focus: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            submit: SubmitType::Auto,
            auto_focus: true,
        }
    }
}
thread_local! {pub(crate) static OPTIONS:std::cell::Cell<Options>=std::cell::Cell::new(Options::default());}
pub(crate) struct Content {
    pub body: AnyElement,
    pub editor: WeakEntity<SlideEditor>,
    pub key: Option<&'static str>,
    pub options: Options,
}
impl IntoElement for Content {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Content {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let (layout, controls) =
            gpui_kit::component::form_controls::collect(|| self.body.request_layout(w, cx));
        let editor = self.editor.clone();
        let key = self.key;
        let options = self.options;
        w.defer(cx, move |w, cx| {
            let _ = editor.update(cx, |e, cx| {
                if e.panel_dialog.active && e.panel_dialog.key == key {
                    if !e.panel_dialog.focus_initialized {
                        e.panel_dialog.focus_initialized = true;
                        // A control the panel focused itself when it opened (the field a press on the page header
                        // meant, the value of a rule) keeps the focus; the host only gives it where nothing has it.
                        let focused_already = controls.iter().any(|c| c.focus.as_ref().is_some_and(|f| f.is_focused(w)));
                        if focused_already {
                        } else if options.auto_focus
                            && let Some(control) = controls.iter().find(|c| !c.disabled)
                            && control.kind == ControlKind::Text
                            && let Some(focus) = &control.focus
                        {
                            focus.focus(w, cx);
                        } else if e.panel_dialog.mode == PanelMode::Modal
                            && let Some(focus) = &e.panel_dialog.focus
                        {
                            focus.focus(w, cx);
                        }
                    }
                    e.panel_dialog.form_controls = controls;
                }
            });
        });
        (layout, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.body.prepaint(w, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.body.paint(w, cx);
    }
}
pub(crate) fn shortcut(
    policy: SubmitType,
    controls: &[FormControl],
    single_normal: bool,
    modified: bool,
) -> bool {
    if controls.iter().any(|c| c.open) {
        return false;
    }
    match policy {
        SubmitType::None => false,
        SubmitType::Enter => !modified,
        SubmitType::CtrlEnter => modified,
        SubmitType::Auto => {
            let simple = single_normal
                || controls
                    .iter()
                    .filter(|c| c.kind == ControlKind::Text)
                    .count()
                    <= 1
                    && !controls.iter().any(|c| c.kind == ControlKind::Multiline);
            if simple { !modified } else { modified }
        }
    }
}
