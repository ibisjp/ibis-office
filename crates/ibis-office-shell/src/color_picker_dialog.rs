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
    component::{v_flex, input::{InputEvent, InputState}},
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
    /// The colour the open picker holds now and whether it shows colours as printed (`None` when no picker is open).
    pub fn picker_state_summary(&self) -> Option<(ibis_doc_model::Rgb, bool)> {
        self.color_picker_state().map(|s| { let s = s.borrow(); (s.picker.color, s.picker.gamut) })
    }
    pub fn open_color_picker_dialog(
        &mut self,
        color: Rgb,
        pick: PickColor,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A file with the CMYK mode on shows and keeps the colour as the press prints it.
        let gamut = self.cmyk_on();
        let shown = self.chosen_color(color);
        let hex = cx.new(|cx| InputState::new(w, cx).default_value(format!("{:06X}", shown.0)));
        let mut picker = ColorEditorState::new(color, w, cx, Self::change_color_picker);
        picker.gamut = gamut;
        if gamut {
            picker.color = shown;
            picker.hsv = super::color_editor::rgb_to_hsv(shown);
            picker.sync(None, w, cx);
        }
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
        self.dialog = Default::default();
        cx.notify();
    }
    pub fn change_color_picker(&mut self, action: ColorAction, w: &mut Window, cx: &mut Context<Self>) {
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
        self.dialog = Default::default();
        self.focus_handle.focus(w, cx);
        cx.notify();
    }
    pub fn commit_color_picker(&mut self, w: &mut Window, cx: &mut Context<Self>) {
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
        // (A file with the CMYK mode on saves the colour as the press prints it.)
        let color = self.chosen_color(color);
        self.close_color_picker_dialog(w, cx);
        pick(self, Some(color), w, cx);
        self.note_recent_color(color);
    }
    /// The checkbox for the CMYK mode: turning it on asks first (a feature PowerPoint does not have); the picker then shows
    /// the colours as printed. Turning it off shows them as they are.
    fn toggle_cmyk_mode_from_picker(&mut self, cx: &mut Context<Self>) {
        if self.cmyk_on() {
            let mut mode = self.presentation().cmyk.clone();
            mode.enabled = false;
            self.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: mode }], cx);
            super::cmyk::set_display_gamut(false);
            if let Some(state) = self.color_picker_state() {
                state.borrow_mut().picker.gamut = false;
            }
            cx.notify();
        } else {
            self.require_cmyk(
                Rc::new(|e, cx| {
                    e.cmyk_picker_refresh = true;
                    cx.notify();
                }),
                cx,
            );
        }
    }

    /// 「プロファイル…」: the picker is closed (nothing is chosen) and the window to choose the ICC profile opens.
    fn open_profile_from_picker(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.close_color_picker_dialog(w, cx);
        self.open_cmyk_profile_chooser(w, cx);
    }

    pub(crate) fn color_picker_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<chrome::SidebarPanel> {
        let state = self.color_picker_state()?;
        // The question about CMYK was answered with OK: the picker shows the colours as printed from now on.
        if std::mem::take(&mut self.cmyk_picker_refresh) {
            super::cmyk::set_display_gamut(true);
            let hex = state.borrow().hex.clone();
            let mut s = state.borrow_mut();
            s.picker.gamut = true;
            let now = s.picker.color;
            s.picker.change(ColorAction::Pick(now), &hex, window, cx);
        }
        let state = state.borrow();
        let cmyk = self.has_cmyk().then(|| {
            v_flex()
                .gap_1()
                .child(chrome::checkbox_button("color-cmyk-mode", text("color.dialog.cmyk-mode"), self.cmyk_on(), false, cx.listener(|e, _, _, cx| e.toggle_cmyk_mode_from_picker(cx))))
                .children(self.cmyk_on().then(|| {
                    chrome::button_group([chrome::ButtonGroupItem::new("color-cmyk-profile", text("shell.cmyk.profile.title")).on_click(cx.listener(|e, _, w, cx| e.open_profile_from_picker(w, cx)))])
                }))
                .into_any_element()
        });
        let body = color_editor::color_editor(
            &state.picker,
            &state.hex,
            &self.recent_colors,
            Self::change_color_picker,
            cmyk,
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
