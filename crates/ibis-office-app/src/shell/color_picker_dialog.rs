// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One arbitrary-colour dialog for every palette and colour field.
use super::{
    chrome,
    color_editor::{self, ColorAction, ColorEditorState},
    color_palette::PickColor,
    editor::SlideEditor,
    i18n::text,
};
use gpui_kit::{
    component::input::{InputEvent, InputState},
    *,
};
use ibis_doc_model::Rgb;
use std::{cell::RefCell, rc::Rc};
const KEY: &str = "color.picker-dialog";
type Shared = Rc<RefCell<ColorPickerDialogState>>;
struct ColorPickerDialogState {
    picker: ColorEditorState,
    hex: Entity<InputState>,
    pick: PickColor,
    sidebar: bool,
    error: bool,
    _subscription: Subscription,
}
impl SlideEditor {
    fn color_picker_state(&self) -> Option<Shared> {
        self.document_memory.get::<Option<Shared>>(KEY).flatten()
    }
    pub(crate) fn open_color_picker_dialog(
        &mut self,
        color: Rgb,
        pick: PickColor,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hex = cx.new(|cx| InputState::new(w, cx).default_value(format!("{:06X}", color.0)));
        let picker = ColorEditorState::new(color, w, cx, Self::change_color_picker);
        let subscription = cx.subscribe_in(&hex, w, |e, _, event: &InputEvent, w, cx| {
            if matches!(event, InputEvent::Change) {
                e.change_color_picker(ColorAction::Hex, w, cx);
            }
        });
        let state = Rc::new(RefCell::new(ColorPickerDialogState {
            picker,
            hex,
            pick,
            sidebar: self.sidebar_open,
            error: false,
            _subscription: subscription,
        }));
        self.document_memory.set(KEY, Some(state));
        self.sidebar_open = true;
        self.panel_dialog = Default::default();
        cx.notify();
    }
    fn change_color_picker(&mut self, action: ColorAction, w: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.color_picker_state() else {
            return;
        };
        let mut state = state.borrow_mut();
        let hex = state.hex.clone();
        if state.picker.change(action, &hex, w, cx) {
            state.error = false;
            cx.notify();
        }
    }
    pub(crate) fn close_color_picker_dialog(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.color_picker_state() {
            self.sidebar_open = state.borrow().sidebar;
        }
        self.document_memory.set(KEY, None::<Shared>);
        self.panel_dialog = Default::default();
        self.focus_handle.focus(w, cx);
        cx.notify();
    }
    fn commit_color_picker(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.color_picker_state() else {
            return;
        };
        let value = state
            .borrow()
            .hex
            .read(cx)
            .value()
            .trim()
            .trim_start_matches('#')
            .to_string();
        let Some(color) = (value.len() == 6)
            .then(|| u32::from_str_radix(&value, 16).ok())
            .flatten()
            .map(Rgb)
        else {
            state.borrow_mut().error = true;
            cx.notify();
            return;
        };
        let pick = state.borrow().pick.clone();
        self.close_color_picker_dialog(w, cx);
        pick(self, Some(color), w, cx);
        self.note_recent_color(color);
    }
    pub(crate) fn color_picker_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<chrome::SidebarPanel> {
        let state = self.color_picker_state()?;
        let state = state.borrow();
        let body = color_editor::color_editor(
            &state.picker,
            &state.hex,
            &self.recent_colors,
            Self::change_color_picker,
            cx,
        )
        .children(
            state
                .error
                .then(|| div().child(text("color.dialog.invalid"))),
        );
        let actions = chrome::action_bar()
            .action(
                chrome::ActionDefinition::new(
                    "color-picker-ok",
                    text("shell.action.ok"),
                    chrome::ActionButtonKind::Primary,
                )
                .on_click(cx.listener(|e, _, w, cx| e.commit_color_picker(w, cx))),
            )
            .action(
                chrome::ActionDefinition::new(
                    "color-picker-cancel",
                    text("_shell.cancel"),
                    chrome::ActionButtonKind::Normal,
                )
                .on_click(cx.listener(|e, _, w, cx| e.close_color_picker_dialog(w, cx))),
            );
        Some(chrome::SidebarPanel::with_footer(
            text("color.dialog.title"),
            body.into_any_element(),
            actions,
        ))
    }
}
