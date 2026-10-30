// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド レイアウトの名前の変更と複製 (レイアウト メニューから): the
//! slide's layout is renamed in a side panel, or copied under a new name.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, Layout, LayoutId, Master, MasterId};

use crate::shell::{chrome::SidebarPanel, editor::SlideEditor};

#[derive(Clone, Copy)]
enum Target {
    Layout(LayoutId),
    Master(MasterId),
}

/// The name of a layout or a master being edited.
pub(crate) struct LayoutNamePanel {
    target: Target,
    name: Entity<InputState>,
    error: Option<SharedString>,
}

impl SlideEditor {
    /// The layout of the current slide.
    fn current_layout(&self) -> Option<&Layout> {
        let id = self.slide().layout?;
        self.presentation().layout(id)
    }

    pub(crate) fn open_layout_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some((layout, value)) = self.current_layout().map(|l| (l.id, l.name.clone())) else { return };
        let name = cx.new(|cx| InputState::new(window, cx).default_value(value));
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.layout_panel = Some(LayoutNamePanel { target: Target::Layout(layout), name: name.clone(), error: None });
        }
        self.sidebar_open = true;
        name.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn open_master_rename(&mut self, master: MasterId, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some(value) = self.presentation().master(master).map(|m| m.name.clone()) else { return };
        let name = cx.new(|cx| InputState::new(window, cx).default_value(value));
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.layout_panel = Some(LayoutNamePanel { target: Target::Master(master), name: name.clone(), error: None });
        }
        self.sidebar_open = true;
        name.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_layout_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, name)) = self.pptx().and_then(|s| s.layout_panel.as_ref()).map(|p| (p.target, p.name.read(cx).value().trim().to_string())) else { return };
        let (found, old) = match target {
            Target::Layout(id) => (self.presentation().layout(id).is_some(), self.presentation().layout(id).map(|l| l.name.clone())),
            Target::Master(id) => (self.presentation().master(id).is_some(), self.presentation().master(id).map(|m| m.name.clone())),
        };
        let error = if name.is_empty() {
            Some(crate::shell::i18n::text("pptx.section-panel.enter-name"))
        } else if !found {
            Some(crate::shell::i18n::text("pptx.layout-panel.layout-not-found"))
        } else if old.as_deref() != Some(name.as_str()) && {
            let command = match target {
                Target::Layout(layout) => Command::RenameLayout { layout, name },
                Target::Master(master) => Command::RenameMaster { master, name },
            };
            !self.apply_pptx_commands_checked(vec![command], cx)
        } {
            Some(crate::shell::i18n::text("pptx.section-panel.cannot-apply-because-original"))
        } else {
            None
        };
        match error {
            Some(error) => {
                if let Some(p) = self.pptx_mut().and_then(|s| s.layout_panel.as_mut()) {
                    p.error = Some(error.into());
                }
                cx.notify();
            }
            None => self.close_layout_rename(window, cx),
        }
    }

    /// Copy master `source` with its layouts (named "<name> のコピー").
    pub(crate) fn duplicate_master(&mut self, source: MasterId, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let p = self.presentation();
        let Some(master) = p.master(source) else { return };
        let top = p.layouts.iter().map(|l| l.id.0).chain(p.masters.iter().map(|m| m.id.0)).max().unwrap_or(0);
        let base = crate::shell::i18n::format("pptx.layout-panel.copy-name", &[("name", &master.name)]);
        let name = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") }).find(|n| !p.masters.iter().any(|m| &m.name == n)).unwrap_or(base);
        let id = MasterId(top + 1);
        let mut commands = vec![Command::InsertMaster { index: p.masters.len(), master: Master { id, name, copy_of: Some(source) } }];
        for (i, l) in p.layouts.iter().filter(|l| l.master == Some(source)).enumerate() {
            commands.push(Command::InsertLayout { index: p.layouts.len() + i, layout: Layout { id: LayoutId(top + 2 + i as u64), copy_of: Some(l.id), master: Some(id), ..l.clone() } });
        }
        self.apply_pptx_commands_checked(commands, cx);
    }

    /// Whether a slide follows a layout of master `master`.
    pub(crate) fn master_in_use(&self, master: MasterId) -> bool {
        let p = self.presentation();
        p.slides.iter().any(|s| s.layout.and_then(|l| p.layout(l)).is_some_and(|l| l.master == Some(master)))
    }

    /// Remove master `master` with its layouts (when no slide follows them).
    pub(crate) fn remove_master(&mut self, master: MasterId, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let p = self.presentation();
        if p.masters.len() < 2 || self.master_in_use(master) || p.master(master).is_none() {
            return;
        }
        let mut commands: Vec<Command> = p.layouts.iter().filter(|l| l.master == Some(master)).map(|l| Command::RemoveLayout { layout: l.id }).collect();
        commands.push(Command::RemoveMaster { master });
        self.apply_pptx_commands_checked(commands, cx);
    }

    pub(crate) fn close_layout_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.layout_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Copy the current slide's layout (named "<name> のコピー"), after the
    /// last layout; the slide keeps its own.
    pub(crate) fn duplicate_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some(source) = self.current_layout().cloned() else { return };
        let layouts = &self.presentation().layouts;
        let id = LayoutId(layouts.iter().map(|l| l.id.0).max().unwrap_or(0) + 1);
        let base = crate::shell::i18n::format("pptx.layout-panel.copy-name", &[("name", &source.name)]);
        let name = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") }).find(|n| !layouts.iter().any(|l| &l.name == n)).unwrap_or(base);
        let copy = Layout { id, name, copy_of: Some(source.id), ..source };
        let index = layouts.len();
        self.apply_pptx_commands_checked(vec![Command::InsertLayout { index, layout: copy }], cx);
    }

    pub(crate) fn layout_name_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.layout_panel.as_ref()?;
        let (name, error) = (p.name.clone(), p.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().child(div().child(crate::shell::i18n::text("_shell.name"))).child(crate::shell::chrome::text_field(&name).full_width());
        if let Some(error) = error {
            body = body.child(div().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("pptx-layout-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_layout_rename(w, cx))))
                .action(crate::shell::chrome::ActionDefinition::new("pptx-layout-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_layout_rename(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.selection-pane.rename"), body.into_any_element(), panel_actions))
    }
}
