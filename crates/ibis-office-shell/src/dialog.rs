// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Movable, resizable settings dialogs. Modal dialogs freeze edits, not the document viewport.
mod resize;
use resize::ResizeEdge;
use gpui_kit::*;
use crate::format::with_format;
use std::rc::Rc;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::{h_flex, v_flex};
use super::{chrome::{SidebarPanel, BORDER, TEXT, icon}, editor::SlideEditor, palette::rgb};

pub(crate) mod form;
pub use form::SubmitType;
actions!(dialog,[SubmitEnter,SubmitModifiedEnter]);
pub fn init(cx:&mut App){cx.bind_keys([
    KeyBinding::new("enter",SubmitEnter,Some("Dialog")),
    KeyBinding::new(if cfg!(target_os="macos"){"cmd-enter"}else{"ctrl-enter"},SubmitModifiedEnter,Some("Dialog")),
]);}

/// A question waiting for its answer (`SlideEditor::ask`).
pub struct PendingAsk {
    pub title: SharedString,
    pub message: SharedString,
    pub(crate) on_ok: std::rc::Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>,
    /// The buttons are はい and いいえ (a yes-or-no question such as deleting a sheet), not OK and キャンセル.
    pub yes_no: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogMode {
    #[default]
    Sidebar,
    Modal,
    Modeless,
}

/// One authority for input ownership. A confirmation always supersedes a settings/range session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionLayer { Document, Settings, SheetRange, Message, Zoom, Save, External, FileChanged, CommentDelete, Alert, Ask, About }
impl InteractionLayer {
    pub fn allows_range_pick(self) -> bool { self == Self::SheetRange }
    pub fn is_confirmation(self) -> bool { matches!(self,Self::Message|Self::Zoom|Self::Save|Self::External|Self::FileChanged|Self::CommentDelete|Self::Alert|Self::Ask|Self::About) }
}

#[derive(Clone, Copy)]
pub enum DialogDrag {
    Move { pointer: Point<Pixels>, origin: Point<Pixels> },
    Resize { pointer: Point<Pixels>, bounds: Bounds<Pixels>, edge: ResizeEdge },
}

#[derive(Default)]
pub struct DialogState {
    pub mode: DialogMode,
    pub key: Option<&'static str>,
    pub active: bool,
    pub title: SharedString,
    pub bounds: Option<Bounds<Pixels>>,
    pub drag: Option<DialogDrag>,
    pub user_resized: bool,
    pub user_moved: bool,
    /// Center once using the measured content, then preserve the top across content changes.
    pub(crate) content_measured: bool,
    pub footer_height: Pixels,
    /// The attribute `esc_closes` turned OFF (`SidebarPanel::esc_closes(false)`): Escape does not close this modal dialog.
    pub(crate) esc_blocked: bool,
    pub focus: Option<FocusHandle>,
    pub(crate) focus_initialized: bool,
    pub form_controls:Vec<gpui_kit::component::form_controls::FormControl>,
    pub submit:form::SubmitType,
    pub(crate) submit_handler:Option<super::chrome::action_bar::ClickHandler>,
    pub(crate) single_normal:bool,
    pub(crate) has_submit_action:bool,
    pub(crate) confirmation_restore: Option<FocusHandle>,
    pub scroll: ScrollHandle,
    pub overlay: Option<AnyElement>,
    /// The dialog this one was opened from, which stays open behind it (a parent dialog; the child is modal over it).
    /// The parent is drawn as it was (`inert`: it neither measures itself nor takes the pointer) until the child closes
    /// and gives it back as `SlideEditor::dialog`.
    pub parent: Option<Box<DialogState>>,
    /// Set only while the parent of a child dialog is drawn.
    pub inert: bool,
}

impl SlideEditor {
    pub fn interaction_layer(&self) -> InteractionLayer {
        if self.office_path_confirm_dialog_open {InteractionLayer::External}
        else if self.save_confirm_dialog_open {InteractionLayer::Save}
        else if self.external_update_dialog_open() {InteractionLayer::FileChanged}
        else if self.comment_delete.is_some() {InteractionLayer::CommentDelete}
        else if self.alert.is_some() {InteractionLayer::Alert}
        else if self.ask.is_some() {InteractionLayer::Ask}
        else if self.about_open {InteractionLayer::About}
        else if self.zoom_dialog_open {InteractionLayer::Zoom}
        else if self.panel_message.is_some() {InteractionLayer::Message}
        else if with_format!(self, range_pick_active(self)) {InteractionLayer::SheetRange}
        else if self.sidebar_open && self.dialog.active {InteractionLayer::Settings}
        else {InteractionLayer::Document}
    }
    /// Render and route from the same discriminant. Callers cannot place a confirmation below a modeless panel.
    pub(crate) fn render_dialog_stack(&mut self, d: Stateful<Div>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let layer=self.interaction_layer();
        if layer.is_confirmation() && layer != InteractionLayer::Zoom {
            // Native text input and widget actions must not keep targeting a covered field.
            if self.dialog.confirmation_restore.is_none() {
                self.dialog.confirmation_restore=window.focused(cx).or_else(||Some(self.focus_handle.clone()));
                self.focus_handle.focus(window,cx);
            }
        } else if let Some(previous)=self.dialog.confirmation_restore.take() {
            if self.has_document {previous.focus(window,cx);} else {self.focus_handle.focus(window,cx);}
        }
        if !matches!(layer, InteractionLayer::Save|InteractionLayer::FileChanged|InteractionLayer::CommentDelete|InteractionLayer::Alert|InteractionLayer::Ask) { self.msgbox.clear(); }
        let d=d.children(self.dialog.overlay.take());
        match layer {
            InteractionLayer::External=>d.child(self.render_office_path_confirm_dialog(window,cx)),
            InteractionLayer::Save=>d.child(self.render_save_confirm_dialog(window,cx)),
            #[cfg(not(target_family = "wasm"))]
            InteractionLayer::FileChanged=>d.child(self.render_external_update_dialog(cx)),
            InteractionLayer::CommentDelete=>d.child(self.render_delete_confirm(cx)),
            InteractionLayer::Alert=>d.child(self.render_alert(cx)),
            InteractionLayer::Ask=>d.child(self.render_ask(cx)),
            InteractionLayer::About=>d.child(self.render_about(cx)),
            InteractionLayer::Zoom=>d.child(self.render_zoom_dialog(window,cx)),
            InteractionLayer::Message=>d.child(self.render_panel_message(cx)),
            _=>d.children(self.render_hyperlink_context()),
        }
    }
    /// Whether the question about the file changed by another app is showing (shell/file_watch.rs; never in the browser).
    fn external_update_dialog_open(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        { self.watch.dialog }
        #[cfg(target_family = "wasm")]
        { false }
    }
    /// Show `message` under `title` in a message box with OK, over the dimmed window (the backdrop).
    pub fn show_alert(&mut self, title: SharedString, message: SharedString, cx: &mut Context<Self>) {
        self.alert = Some((title, message));
        self.dialog.drag = None;
        cx.notify();
    }
    pub(crate) fn dismiss_alert(&mut self, cx: &mut Context<Self>) {
        self.alert = None;
        cx.notify();
    }
    /// The message box of `show_alert`: OK (or Enter, Escape) closes it.
    pub(crate) fn render_alert(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use super::chrome::{ActionButtonKind, ActionDefinition, action_bar};
        let (title, message) = self.alert.clone().unwrap_or_default();
        let ok = ActionDefinition::new("alert-ok", super::i18n::text("shell.action.ok"), ActionButtonKind::Primary)
            .on_click(cx.listener(|this, _, _, cx| this.dismiss_alert(cx)));
        super::message_box::message_box("alert-dialog", title, message, action_bar().action(ok), &self.msgbox, true, cx)
    }
    /// Ask `message` under `title` in a message box with OK and キャンセル; OK runs `on_ok`, キャンセル (or Escape) does nothing.
    pub fn ask(&mut self, title: SharedString, message: SharedString, on_ok: Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>, cx: &mut Context<Self>) {
        self.ask = Some(PendingAsk { title, message, on_ok, yes_no: false });
        self.dialog.drag = None;
        cx.notify();
    }
    /// `ask` with the buttons はい and いいえ (the user's K0019: deleting a sheet asks so).
    pub fn ask_yes_no(&mut self, title: SharedString, message: SharedString, on_yes: Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>, cx: &mut Context<Self>) {
        self.ask = Some(PendingAsk { title, message, on_ok: on_yes, yes_no: true });
        self.dialog.drag = None;
        cx.notify();
    }
    /// The answer of `ask`: OK runs what was asked, キャンセル drops it.
    pub fn answer_ask(&mut self, ok: bool, cx: &mut Context<Self>) {
        if let Some(pending) = self.ask.take() {
            if ok {
                (pending.on_ok)(self, cx);
            }
        }
        cx.notify();
    }
    /// The message box of `ask`: OK and キャンセル.
    pub(crate) fn render_ask(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use super::chrome::{ActionButtonKind, ActionDefinition, action_bar};
        let (title, message) = self.ask.as_ref().map(|a| (a.title.clone(), a.message.clone())).unwrap_or_default();
        let yes_no = self.ask.as_ref().is_some_and(|a| a.yes_no);
        let (ok_key, cancel_key) = if yes_no { ("shell.action.yes", "shell.action.no") } else { ("shell.action.ok", "shell.action.cancel") };
        let ok = ActionDefinition::new("ask-ok", super::i18n::text(ok_key), ActionButtonKind::Primary)
            .on_click(cx.listener(|this, _, _, cx| this.answer_ask(true, cx)));
        let cancel = ActionDefinition::new("ask-cancel", super::i18n::text(cancel_key), ActionButtonKind::Normal)
            .on_click(cx.listener(|this, _, _, cx| this.answer_ask(false, cx)));
        super::message_box::message_box("ask-dialog", title, message, action_bar().action(ok).action(cancel), &self.msgbox, false, cx)
    }
    /// Show the About screen as a modal dialog inside the window (every platform: shell/about.rs).
    pub(crate) fn show_about(&mut self, cx: &mut Context<Self>) {
        self.about_open = true;
        self.dialog.drag = None;
        cx.notify();
    }
    pub(crate) fn dismiss_about(&mut self, cx: &mut Context<Self>) {
        self.about_open = false;
        cx.notify();
    }
    pub(crate) fn render_about(&self, cx: &mut Context<Self>) -> impl IntoElement {
        super::about::render_modal(cx.listener(|this, _, _, cx| this.dismiss_about(cx)), cx)
    }
    pub fn show_panel_message(&mut self, text: SharedString, cx: &mut Context<Self>) {
        with_format!(self, panel_message_shown(self));
        self.panel_message=Some(text);
        self.dialog.drag=None;
        if let Some(dnd)=&self.active_row_dnd {dnd.cancel();}
        cx.notify();
    }
    pub fn dismiss_panel_message(&mut self, cx: &mut Context<Self>) {
        self.panel_message = None;
        with_format!(self, panel_message_dismissed(self, cx));
        cx.notify();
    }
    pub(crate) fn render_panel_message(&self, cx: &Context<Self>) -> impl IntoElement {
        let editor_measure = cx.entity().downgrade();
        // The same occluding shell is used by every blocking confirmation.
        confirmation_surface("panel-message-backdrop", false)
            .when(self.sidebar_open && !with_format!(self, panel_message_anchored(self)),|d|d.justify_end())
            .child(v_flex().id("panel-message").relative()
                .when_some(with_format!(self, panel_message_origin(self)),|d,p|d.absolute().left(p.x).top(p.y))
                .w(px(380.)).max_w_full().p_5().gap_4()
                .child(canvas(|_,_,_|(),move |bounds,_,_,cx| {
                    let editor=editor_measure.clone();
                    cx.defer(move |cx| { let _=editor.update(cx,|e,cx| { with_format!(e, panel_message_measured(e, bounds, cx)); }); });
                }).absolute().inset_0())
                .bg(rgb(0xffffff)).rounded_lg().shadow_lg()
                .child(self.panel_message.clone().unwrap_or_default())
                .child(h_flex().justify_center().child(super::chrome::primary_button("panel-message-ok")
                    .label("OK")
                    .on_click(cx.listener(|e,_,_,cx|{e.dismiss_panel_message(cx);})))))
    }
    pub fn dialog_is_modal(&self) -> bool {
        self.sidebar_open && self.dialog.active && self.dialog.mode == DialogMode::Modal
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.key==Some("color_picker_dialog"){self.close_color_picker_dialog(window,cx);return;}
        if self.theme_colors_ui.editing && matches!(self.dialog.key,Some("docx_theme_panel"|"xlsx_theme_panel"|"theme_panel")) {self.set_theme_colors_editor(false,window,cx);return;}
        if with_format!(self, close_dialog(self, window, cx)) { return; }
        if self.dialog.key.is_some_and(|key|key.contains("autocorrect")) && self.cancel_autocorrect_entry() { self.dialog.bounds=None;cx.notify();return; }
        let keep_chart_pane = with_format!(self, keeps_sidebar_after_dialog(self));
        self.discard_current_dialog();
        self.sidebar_open = keep_chart_pane;
        self.dialog.active = false;
        self.dialog.drag = None;
        window.release_pointer();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Discard only the current command's UI draft. Applied document edits and unrelated live panes stay intact.
    fn discard_current_dialog(&mut self) {
        let Some(key) = self.dialog.key.take() else { return; };
        match key {
            "symbol_panel" => {
                self.symbol = None;
                with_format!(self, discard_dialog(self, key));
                return;
            }
            "autorecover_settings_panel" => { self.autorecover_settings = None; return; }
            "recover_unsaved_panel" => { self.recover_unsaved = None; return; }
            "replace_fonts_panel" => { self.replace_fonts = None; return; }
            #[cfg(not(target_family = "wasm"))]
            "rename_file_panel" => { self.rename_file = None; return; }
            "font_setup_dialog" => { self.font_setup_discard(); return; }
            "settings_dialog" => { self.settings = None; self.toolbar_search_open = None; return; }
            _ => {}
        }
        with_format!(self, discard_dialog(self, key));
    }

    /// Modal settings block background selection and commands, but retain the scrollbars and zoom controls.
    pub(crate) fn dialog_allows_pointer(&self, position: Point<Pixels>, _window: &Window) -> bool {
        // The save question is a higher-priority overlay and supplies its own backdrop.
        if self.interaction_layer().is_confirmation() { return true; }
        if !self.dialog_is_modal() { return true; }
        if self.dialog.bounds.is_some_and(|b| b.contains(&position)) { return true; }
        // Native title row: window dragging and window controls remain available, and so does the logo, which opens the start window.
        if position.y < px(36.) { return true; }
        if self.logo_bounds.get().is_some_and(|b| b.contains(&position)) { return true; }
        if self.workspace_bounds.is_some_and(|b| b.contains(&position)) { return true; }
        self.zoom_controls_bounds.get().is_some_and(|b| b.contains(&position))
    }

    /// Run before keymap resolution, so format-specific superscript bindings cannot consume viewport zoom.
    pub(crate) fn modal_viewport_keystroke(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dialog_is_modal() || self.save_confirm_dialog_open { return; }
        // Some formats bind Cmd++ to character formatting. Modal viewport zoom takes precedence.
        if (key.modifiers.platform || key.modifiers.control) && matches!(key.key.as_str(), "+" | "=" | "-" | "0") {
            match key.key.as_str() {
                "+" | "=" => self.step_zoom(true, window, cx),
                "-" => self.step_zoom(false, window, cx),
                "0" if key.modifiers.alt => self.set_zoom(None, window, cx),
                _ => self.set_zoom(Some(1.), window, cx),
            }
            cx.stop_propagation();
            return;
        }
    }

    /// Whether a menu or a dropdown list (one that is closed by an outside click, not a hover tip) is open: it takes Escape
    /// before the modal dialog under it does. Popovers, selects and the dropdown menus register with `GlobalState`; the menu bar's menu is the editor's.
    pub(crate) fn popup_takes_escape(&self, cx: &App) -> bool {
        gpui_kit::component::global_state::GlobalState::is_in_deferred_context(cx) || self.open_menu.is_some()
    }

    pub fn cancel_table_reorder(&mut self,window:&mut Window,cx:&mut Context<Self>)->bool{
        if self.active_row_dnd.as_ref().is_some_and(|d|d.dragging().is_some()&&d.cancel()){window.release_pointer();cx.notify();return true;}
        false
    }
    /// Alt+letter on the topmost layer's action bar (shell/access_key.rs). True when the press was an access key (handled or swallowed).
    fn access_key_pressed(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use super::access_key::{Pressed, Scope, dispatch};
        let scope = match self.interaction_layer() {
            InteractionLayer::Document | InteractionLayer::SheetRange => Scope::Off,
            // A modal dialog owns the keys; a modeless one only while the focus is in it.
            InteractionLayer::Settings => if self.dialog_is_modal() || self.dialog.focus.as_ref().is_some_and(|f| f.contains_focused(window, cx)) {Scope::Dialog} else {Scope::Off},
            InteractionLayer::Zoom => Scope::Dialog,
            _ => Scope::MessageBox,
        };
        match dispatch(window, key, scope) {
            Pressed::No => false,
            Pressed::Disabled => true,
            Pressed::Click(handler) => { window.defer(cx, move |w, cx| handler(&ClickEvent::default(), w, cx)); true }
        }
    }
    pub(crate) fn dialog_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key=="escape"&&self.cancel_table_reorder(window,cx){cx.stop_propagation();return;}
        let key = &event.keystroke;
        if self.access_key_pressed(key, window, cx) {window.prevent_default();cx.stop_propagation();return;}
        // The menu bar's keys: Alt+letter, Alt+J, Alt+D, F10 (Windows and Linux).
        if self.bar_key_down(key, window, cx) {window.prevent_default();cx.stop_propagation();return;}
        if self.interaction_layer() == InteractionLayer::Zoom {return;}
        if self.interaction_layer().is_confirmation() {
            if self.panel_message.is_some() && matches!(key.key.as_str(),"escape"|"enter") {self.dismiss_panel_message(cx);}
            else if self.interaction_layer()==InteractionLayer::External && key.key == "escape" {self.close_office_path_confirm_dialog(cx);}
            else if self.save_confirm_dialog_open && key.key == "escape" {self.close_save_confirm_dialog(cx);}
            // Enter presses the primary button, 保存.
            else if self.save_confirm_dialog_open && key.key == "enter" {self.confirm_save_and_close(window, cx);}
            else if self.comment_delete.is_some() && key.key == "escape" {self.comment_delete = None; cx.notify();}
            else if self.interaction_layer()==InteractionLayer::Alert && matches!(key.key.as_str(),"escape"|"enter") {self.dismiss_alert(cx);}
            else if self.interaction_layer()==InteractionLayer::Ask && key.key == "escape" {self.answer_ask(false, cx);}
            else if self.interaction_layer()==InteractionLayer::Ask && key.key == "enter" {self.answer_ask(true, cx);}
            else if self.interaction_layer()==InteractionLayer::About && matches!(key.key.as_str(),"escape"|"enter") {self.dismiss_about(cx);}
            else if self.interaction_layer()==InteractionLayer::FileChanged {
                // Escape is いいえ.
                if key.key == "escape" {self.dismiss_external_update_if_open(cx);}
            }
            cx.stop_propagation();
            return;
        }
        if key.key=="enter" && !key.modifiers.alt && self.dialog_enter(if cfg!(target_os="macos"){key.modifiers.platform}else{key.modifiers.control},key.modifiers.shift,window,cx) {window.prevent_default();cx.stop_propagation();return;}
        // Up and Down choose among the options of the insert or delete cells dialog.
        if matches!(key.key.as_str(),"up"|"down") && !key.modifiers.platform && !key.modifiers.control && !key.modifiers.alt && !key.modifiers.shift && with_format!(self, dialog_arrow_step(self, if key.key=="down"{1}else{-1}, cx)) {cx.stop_propagation();return;}
        if !self.dialog_is_modal() { return; }
        if self.save_confirm_dialog_open {
            if key.key == "escape" { self.close_save_confirm_dialog(cx); }
            cx.stop_propagation();
            return;
        }
        if key.key == "escape" {
            if self.dismiss_toolbar_dropdown(window,cx) {cx.stop_propagation();return;}
            if self.cancel_table_reorder(window,cx){cx.stop_propagation();return;}
            // A drag in the dialog is cancelled by Escape; the dialog stays.
            if cx.stop_active_drag(window){cx.stop_propagation();return;}
            // A menu or dropdown list that is open takes Escape (it closes itself); the dialog stays.
            if self.popup_takes_escape(cx){return;}
            // An input method that is composing takes Escape (it cancels the composition); the dialog stays.
            if self.dialog.form_controls.iter().any(|c|c.focus.as_ref().is_some_and(|f|f.is_focused(window))&&c.composing.clone().is_some_and(|f|f(window,cx))){return;}
            // The dialog attribute `esc_closes` (default ON).
            if !self.dialog.esc_blocked {self.close_dialog(window, cx);}
            cx.stop_propagation();
            return;
        }
        // A focused field still edits its own text; keyboard navigation must never reach the document.
        let searching = self.toolbar_search_open.and_then(|id| self.toolbar_search_inputs.get(id))
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window));
        if !searching && !self.dialog.focus.as_ref().is_some_and(|f| f.contains_focused(window, cx)) {
            cx.stop_propagation();
        }
    }

    fn dialog_enter(&mut self,modified:bool,shift:bool,window:&mut Window,cx:&mut Context<Self>)->bool {
        if shift || !self.sidebar_open || !self.dialog.active || self.font_size_menu_open || self.toolbar_search_open.is_some()
            || !(self.dialog_is_modal() || self.dialog.focus.as_ref().is_some_and(|f|f.contains_focused(window,cx))) {return false;}
        let composing=self.dialog.form_controls.iter().find(|c|c.focus.as_ref().is_some_and(|f|f.is_focused(window))).and_then(|c|c.composing.clone());
        if composing.is_some_and(|f|f(window,cx)){return false;}
        if self.dialog.has_submit_action && form::shortcut(self.dialog.submit,&self.dialog.form_controls,self.dialog.single_normal,modified) {if let Some(handler)=self.dialog.submit_handler.clone(){window.defer(cx,move|w,cx|handler(&ClickEvent::default(),w,cx));}return true;}
        // Do not let old single-field Enter callbacks bypass a multi-field form's policy.
        // A field with its own Enter (the import dialog's file path loads the file; it does not submit the form) keeps it.
        if with_format!(self, dialog_field_owns_enter(self, window, cx)){return false;}
        self.dialog.has_submit_action && self.dialog.form_controls.iter().any(|c|c.kind==gpui_kit::component::form_controls::ControlKind::Text && c.focus.as_ref().is_some_and(|f|f.is_focused(window)))
    }

    /// Actions dispatch before ordinary key handlers. Block document actions in capture,
    /// while leaving the input widget's own copy/paste/undo actions available.
    pub(crate) fn guard_panel_actions(&self, mut element: Stateful<Div>, cx: &Context<Self>) -> Stateful<Div> {
        element=element.capture_action(cx.listener(|e,action:&gpui_kit::component::input::Enter,w,cx|{if e.dialog_enter(action.secondary,action.shift,w,cx){cx.stop_propagation();}}));
        let names: Vec<_> = cx.all_action_names().iter().copied().filter(|name| {
            name.starts_with("ibis_office::") || name.starts_with("docx::") || name.starts_with("pptx::")
        }).collect();
        // The actions Enter and Escape are bound to (CancelTool, a sheet's SelectCellDown, ...): they reach this guard
        // before the key handlers, so the message box of `show_alert` closes on them here.
        let closes_alert: Vec<std::any::TypeId> = ["enter", "escape"].iter()
            .filter_map(|key| Keystroke::parse(key).ok())
            .flat_map(|key| cx.all_bindings_for_input(&[key]))
            .map(|binding| binding.action().as_any().type_id())
            .collect();
        for name in names {
            if ["::OpenStartWindow", "::Quit"]
                .iter().any(|suffix| name.ends_with(suffix)) { continue; }
            let Ok(action) = cx.build_action(name, None) else { continue; };
            let editor = cx.weak_entity();
            let closes_alert = closes_alert.contains(&action.as_any().type_id());
            element.interactivity().capture_action_type(action.as_any().type_id(), move |_, window, app| {
                editor.update(app, |this, cx| {
                    // The grid inside the chart's data pane is a sheet editor of its own: its keys (Enter, the arrows, Tab, copy, ...) go
                    // to it, as they do in a sheet (Esc closes the dialog, unless a cell is being typed in).
                    if this.chart_grid_takes_keys(name.ends_with("::CancelTool"), window, cx) {return;}
                    if name.ends_with("::CancelTool")&&this.dismiss_toolbar_dropdown(window,cx){cx.stop_propagation();return;}
                    if name.ends_with("::CancelTool")&&this.cancel_table_reorder(window,cx){cx.stop_propagation();return;}
                    if this.dialog_is_modal() || this.interaction_layer().is_confirmation() {
                        if closes_alert && this.interaction_layer()==InteractionLayer::Alert {this.dismiss_alert(cx);}
                        else if closes_alert && this.interaction_layer()==InteractionLayer::About {this.dismiss_about(cx);}
                        else if closes_alert && this.interaction_layer()==InteractionLayer::Ask {this.answer_ask(name.ends_with("::CancelTool") == false, cx);}
                        else if name.ends_with("::CancelTool") {
                            if this.office_path_confirm_dialog_open { this.close_office_path_confirm_dialog(cx); }
                            else if this.save_confirm_dialog_open { this.close_save_confirm_dialog(cx); }
                            else if this.dismiss_external_update_if_open(cx) {}
                            else if this.comment_delete.is_some() {this.comment_delete = None; cx.notify();}
                            else if this.panel_message.is_some() {this.dismiss_panel_message(cx);}
                            else if this.interaction_layer()==InteractionLayer::Zoom {this.close_zoom_dialog(cx);}
                            else if !this.interaction_layer().is_confirmation() && !this.dialog.esc_blocked && !this.popup_takes_escape(cx) && !cx.has_active_drag() { this.close_dialog(window, cx); }
                        }
                        cx.stop_propagation();
                    }
                }).ok();
            });
        }
        element
    }

    pub(crate) fn move_dialog(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.dialog.drag else { return false; };
        let Some(mut bounds) = self.dialog.bounds else { return false; };
        let viewport = window.viewport_size();
        match drag {
            DialogDrag::Move { pointer, origin } => {
                let offset = event.position - pointer;
                self.dialog.user_moved = true;
                bounds.origin.x = origin.x + offset.x;
                bounds.origin.y = origin.y + offset.y;
            }
            DialogDrag::Resize { pointer, bounds: original, edge } => {
                self.dialog.user_resized = true;
                bounds = edge.bounds(original, event.position-pointer, viewport);
            }
        }
        self.dialog.bounds = Some(bounds);
        cx.notify();
        true
    }

    pub(crate) fn render_dialog(&mut self, panel: SidebarPanel, footer: Option<crate::chrome::ActionBar>, mode: DialogMode,
        window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let backdrop = super::chrome::take_panel_backdrop() && mode == DialogMode::Modal;
        self.dialog.esc_blocked = !super::chrome::take_panel_esc_closes();
        let (panel,footer,mode)=if with_format!(self, range_pick_active(self)) && self.dialog.key!=Some("chart_series_editor") {
            let owner_title=panel.title.clone();
            let mut picker=with_format!(self, range_pick_panel(self, cx)).unwrap();picker.title=owner_title;
            let footer=super::chrome::take_sidebar_footer();
            (picker,footer,DialogMode::Modeless)
        } else {(panel,footer,mode)};
        let footer=footer.or_else(||Some(super::chrome::action_bar().action(super::chrome::ActionDefinition::new("dialog-default-close",super::i18n::text("_shell.close"),super::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_dialog(w,cx))))));
        let options=form::OPTIONS.with(|o|o.get());
        let header=super::chrome::take_panel_header();
        let header_height=if header.is_some(){px(super::chrome::panel::PANEL_HEADER_HEIGHT)}else{px(0.)};
        let (submit_handler,single_normal,has_submit_action)=footer.as_ref().map(|f|f.submit_action()).unwrap_or_default();
        self.dialog.submit=options.submit;self.dialog.submit_handler=submit_handler;self.dialog.single_normal=single_normal;self.dialog.has_submit_action=has_submit_action;
        let viewport = window.viewport_size();
        if self.dialog.title != panel.title || self.dialog.bounds.is_none() {
            let preferred = match self.dialog.key {
                Some("presentation_properties_panel") | Some("sheet_doc_properties_panel") => 520. * 0.7,
                Some("font_panel") => 520. * 0.6,
                Some("docx_compress_panel") | Some("compress_panel") => 520. * 0.55,
                Some("fit_text_panel") | Some("tate_chu_yoko_panel") => 320.,
                Some("ruby_panel") => 520. * 0.4,
                Some("style_panel") | Some("doc_properties_panel") => 520. * 0.8,
                Some("cells_dialog_panel") | Some("cell_size_panel") => 520. * 0.7,
                Some("tab_stop_editor") => 320.,
                Some("names_dialog") => 400.,
                Some("pivot_value_dialog") => 400.,
                Some("pivot_field_dialog") => 360.,
                Some("autofilter_condition_dialog") => 400.,
                Some("autofilter_top_dialog") => 340.,
                Some("data_table_panel") => 364.,
                Some("conditional_panel") => 650.,
                Some("conditional_rule_editor") => 480.,
                Some("chart_series_editor") => if with_format!(self, range_pick_active(self)) {320.} else {520.},
                Some("merge_recipients") | Some("merge_import") => 820.,
                Some("scenario_manager_panel") => 760.,
                Some("paragraph_panel") => 520. * 0.8,
                Some("animation_options_panel") => 360.,
                Some("tab_panel") => 520. * 0.7,
                // The page setup dialog is 30 % wider than the default (its segment rows hold many items).
                Some("page_setup_panel") => 520. * 1.3,
                Some("settings_dialog") => crate::settings_dialog::DIALOG_WIDTH,
                _ => 520.,
            };
            let width = px(preferred).min(viewport.width).max(px(320.));
            self.dialog.user_resized = false;
            self.dialog.user_moved = false;
            self.dialog.content_measured = false;
            self.dialog.focus_initialized = false;
            self.dialog.footer_height = if footer.is_some() { px(super::chrome::BUTTON_HEIGHT + 2. * super::chrome::ACTION_BAR_PADDING_Y + 1.) } else { px(0.) };
            let height = px(560.).min((viewport.height - px(80.)).max(px(180.)));
            self.dialog.bounds = Some(Bounds::new(
                point(if matches!(self.dialog.key,Some("tab_stop_editor" | "chart_series_editor" | "names_dialog")) {viewport.width-width} else if self.dialog.key==Some("animation_options_panel") {
                    // At the main window's right edge, next to the animation pane (the list stays in sight).
                    let pane=if self.right_pane.shown {px(self.sidebar_width.max(super::chrome::SIDEBAR_WIDTH))} else {px(0.)};
                    (viewport.width-pane-width).max(px(0.))
                } else {(viewport.width - width) / 2.}, (viewport.height - height) / 2.), size(width, height)));
            self.dialog.title = panel.title.clone();
            self.dialog.scroll = ScrollHandle::new();
        }
        let focus = self.dialog.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        self.dialog.mode = mode;
        self.dialog.active = true;
        let bounds = self.dialog.bounds.unwrap();
        let scroll = self.dialog.scroll.clone();
        let manual = self.dialog.user_resized;
        let inert = self.dialog.inert;
        let autocorrect_parent = with_format!(self, autocorrect_dialog_parent_open(self));
        let fill_height = !with_format!(self, range_pick_active(self)) && (matches!(self.dialog.key,Some("field_panel"|"function_panel"|"dedupe_panel"|"settings_dialog")) || (autocorrect_parent && matches!(self.dialog.key,Some("doc_autocorrect_panel"|"autocorrect_panel"|"pptx_autocorrect_panel"))));
        let max_height = (viewport.height - px(80.)).max(px(100.));
        // The settings dialog is as high as its content wants (the table fills it), not as high as the window allows.
        let fixed_height = (self.dialog.key==Some("settings_dialog")).then(||px(crate::settings_dialog::DIALOG_HEIGHT).min(max_height));
        let max_body = (max_height - px(46.) - self.dialog.footer_height - header_height).max(px(0.));
        let editor_measure = cx.entity().downgrade();
        let footer_measure = cx.entity().downgrade();
        // Track the actual content as a direct scroll child so its minimum width
        // contributes to horizontal overflow, while auto-width content still reflows.
        let body=form::Content{body:panel.body,editor:cx.weak_entity(),key:self.dialog.key,options}.into_any_element();
        let body = if fill_height {super::chrome::panel_body(body).h_full().into_any_element()} else {body};
        let dialog = v_flex().id("settings-dialog").occlude().absolute().left(bounds.origin.x).top(bounds.origin.y)
            .w(bounds.size.width).min_w(px(320.))
            .when(manual || fill_height, |d| d.h(if fill_height && !manual { fixed_height.unwrap_or(max_height) } else { bounds.size.height }).min_h(if fill_height {px(260.)} else {px(180.)}))
            .when(!manual && !fill_height, |d| d.h_auto().max_h(max_height))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .track_focus(&focus).key_context("Dialog")
            .on_action(cx.listener(|e,_:&SubmitEnter,w,cx|{if e.dialog_enter(false,false,w,cx){cx.stop_propagation();}else{cx.propagate();}}))
            .on_action(cx.listener(|e,_:&SubmitModifiedEnter,w,cx|{if e.dialog_enter(true,false,w,cx){cx.stop_propagation();}else{cx.propagate();}}))
            .bg(rgb(0xffffff)).border_1().border_color(rgb(BORDER)).rounded(px(8.)).shadow_lg()
            .child(h_flex().id("settings-dialog-title").relative().h(px(44.)).flex_none().px(px(12.))
                .items_center().justify_between().border_b_1().border_color(rgb(BORDER))
                .child(if inert { div().absolute().left_0().top_0().bottom_0().right(px(44.)).into_any_element() } else { dialog_drag_surface(cx).into_any_element() })
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT))
                    .child(super::chrome::panel::panel_title(&panel.title)))
                .child(div().id("settings-dialog-close").size(px(28.)).flex().items_center().justify_center().cursor_pointer()
                    .child(icon("close", 18., rgb(TEXT).into()))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(!inert, |d| d.on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))))))
            .children(header.map(super::chrome::panel::panel_header_host))
            .child(div().relative().min_h_0().w_full()
                .when(manual || fill_height, |d| d.flex_1())
                .child(div().id("settings-dialog-body").w_full()
                    .when(manual || fill_height, |d| d.h_full())
                    .when(!manual && !fill_height, |d| d.max_h(max_body))
                    .when(!fill_height, |d|d.p(px(super::chrome::FORM_PADDING)).overflow_scroll().track_scroll(&scroll))
                    .when(fill_height, |d|d.overflow_hidden()).child(body))
                .when(!fill_height, |d|d.child(super::chrome::always_visible_scrollbar_overlay("settings-dialog-scrollbar", scroll.clone(), None))
                    .child(super::chrome::horizontal_scrollbar_overlay("settings-dialog-horizontal-scrollbar", scroll))))
            .when_some(footer, |d, footer| d.child(super::chrome::action_bar_host(footer.access_scope(if inert {super::access_key::Scope::Off} else {super::access_key::Scope::Dialog}))
                .when(!inert, |d| d.child(canvas(|_, _, _| (), move |measured, _, _, cx| {
                    let editor = footer_measure.clone();
                    cx.defer(move |cx| { let _ = editor.update(cx, |this, cx| {
                        if (this.dialog.footer_height - measured.size.height).abs() > px(0.5) {
                            this.dialog.footer_height = measured.size.height; cx.notify();
                        }
                    }); });
                }).absolute().inset_0()))))
            .when(!inert, |d| d.child(canvas(|_, _, _| (), move |measured, _, _, cx| {
                let editor = editor_measure.clone();
                cx.defer(move |cx| { let _ = editor.update(cx, |this, cx| {
                    let Some(mut actual) = this.dialog.bounds else { return; };
                    if !this.dialog.active { return; }
                    // The absolute measurement canvas covers the padding box; include both 1px borders.
                    let outer_height = measured.size.height + px(2.);
                    let first_measurement = !this.dialog.content_measured;
                    this.dialog.content_measured = true;
                    if first_measurement || (actual.size.height - outer_height).abs() > px(0.5) {
                        actual.size.height = outer_height;
                        if !this.dialog.user_resized &&
                            ((first_measurement && !this.dialog.user_moved) || actual.bottom() > viewport.height) {
                            actual.origin.y = (viewport.height - actual.size.height) / 2.;
                        }
                        this.dialog.bounds = Some(actual); cx.notify();
                    }
                }); });
            }).absolute().inset_0()))
            .children(if inert { Vec::new() } else { resize::handles(cx) })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.dialog.drag = None;
                cx.stop_propagation();
            }))
            .into_any_element();
        // The attribute `backdrop` (default OFF): a dim layer behind the modal dialog, the same as `confirmation_surface(_, true)`.
        if backdrop { div().absolute().inset_0().bg(gpui_kit::rgba(BACKDROP_COLOR)).child(dialog).into_any_element() } else { dialog }
    }
}

/// Cover the document's hitboxes, underneath its actual scrollbars. Wheel events pass through.
pub fn viewport_shield() -> Div {
    div().absolute().inset_0().block_mouse_except_scroll()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Capture the title grab so fields cannot become hovered during a drag.
fn dialog_drag_surface(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let down = editor.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = down.update(cx, |this, cx| {
                        if this.toolbar_search_open.is_some() || this.interaction_layer().is_confirmation() { return; }
                        if let Some(bounds)=this.dialog.bounds {
                            if let Some(edge)=resize::edge_at(bounds,event.position) {
                                this.dialog.drag=Some(DialogDrag::Resize{pointer:event.position,bounds,edge});
                                window.capture_pointer(hitbox.id);cx.notify();cx.stop_propagation();
                            }
                        }
                    });
                }
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && hitbox.is_hovered(window) {
                    let _ = down.update(cx, |this, cx| {
                        if let Some(bounds) = this.dialog.bounds {
                            this.dialog.drag = Some(DialogDrag::Move { pointer: event.position, origin: bounds.origin });
                            window.capture_pointer(hitbox.id);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    });
                }
            });
            let released = editor.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = released.update(cx, |this, cx| {
                        if this.dialog.drag.take().is_some() {
                            window.release_pointer(); cx.notify(); cx.stop_propagation();
                        }
                    });
                }
            });
            let moving = editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    let _ = moving.update(cx, |this, cx| {
                        if this.move_dialog(event, window, cx) { cx.stop_propagation(); }
                    });
                }
            });
        },
    ).absolute().left_0().top_0().bottom_0().right(px(44.))
}

/// The dim colour behind a dialog whose `backdrop` attribute is ON.
pub(crate) const BACKDROP_COLOR: u32 = 0x00000066;

/// A blocking overlay must always occlude lower hitboxes and consume all pointer phases.
/// `backdrop` is the dialog attribute (default OFF): ON dims the window behind (`BACKDROP_COLOR`), OFF stays transparent.
pub(crate) fn confirmation_surface(id: &'static str, backdrop: bool) -> Stateful<Div> {
    div().id(id).absolute().inset_0().occlude().flex().items_center().justify_center()
        .when(backdrop, |d| d.bg(gpui_kit::rgba(BACKDROP_COLOR)))
        .on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
        .on_mouse_down(MouseButton::Right,|_,_,cx|cx.stop_propagation())
        .on_mouse_up(MouseButton::Left,|_,_,cx|cx.stop_propagation())
        .on_scroll_wheel(|_,_,cx|cx.stop_propagation())
}
