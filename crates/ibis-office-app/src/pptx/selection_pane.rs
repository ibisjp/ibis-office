// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 選択ウィンドウ (配置 > 選択ウィンドウ): the slide's shapes, frontmost
//! first, by name; a click selects one, the eye shows or hides it (a hidden
//! shape stays on the slide, not drawn: `Slide::hidden_shapes`), the pencil
//! renames it, ↑ ↓ bring it forward or send it backward; すべて表示 and
//! すべて非表示 for them all. Each change can be undone.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{Command, ShapeId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// The pane, while open: the shape being renamed and its field.
#[derive(Default)]
pub(crate) struct SelectionPane {
    pub(crate) renaming: Option<(ShapeId, Entity<InputState>)>,
    _subscription: Option<Subscription>,
    pub(crate) reorder: std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
}

impl SlideEditor {
    /// 選択ウィンドウ: shown or put away.
    pub(crate) fn toggle_selection_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.selection_pane = match s.selection_pane.take() {
                Some(_) => None,
                None => Some(SelectionPane::default()),
            };
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// A shape's name as the pane shows it: its own, else what it is and its place.
    pub(crate) fn shape_label(&self, id: ShapeId) -> String {
        let slide = self.slide();
        if let Some(name) = slide.shape_names.get(&id) {
            return name.clone();
        }
        crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &(slide.shape_index(id).map_or(0, |x| x + 1)))])
    }

    /// The eye: shape `id` shown or hidden (a hidden one is no longer selected).
    pub(crate) fn set_shape_hidden(&mut self, id: ShapeId, hidden: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if hidden {
            let rest: Vec<ShapeId> = self.selected_ids().into_iter().filter(|s| *s != id).collect();
            self.set_selection(rest);
        }
        self.run(vec![Command::SetShapeHidden { slide, shape: id, hidden }], cx);
    }

    /// すべて表示 / すべて非表示.
    pub(crate) fn set_all_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let s = self.slide();
        let commands: Vec<Command> = s
            .shapes
            .iter()
            .filter(|sh| !s.is_attached(sh.id) && s.hidden_shapes.contains(&sh.id) != hidden)
            .map(|sh| Command::SetShapeHidden { slide, shape: sh.id, hidden })
            .collect();
        if hidden {
            self.set_selection(Vec::new());
        }
        self.run(commands, cx);
    }

    /// The pencil: a field to rename shape `id`.
    pub(crate) fn start_renaming(&mut self, id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.shape_label(id);
        let field = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(&field, window, move |e: &mut Self, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                e.finish_renaming(window, cx);
            }
        });
        field.update(cx, |f, cx| f.focus(window, cx));
        if let Some(p) = self.pptx_mut().and_then(|s| s.selection_pane.as_mut()) {
            p.renaming = Some((id, field));
            p._subscription = Some(subscription);
        }
        cx.notify();
    }

    /// The name typed given to its shape.
    pub(crate) fn finish_renaming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.pptx_mut().and_then(|s| s.selection_pane.as_mut()).and_then(|p| p.renaming.take()) else { return };
        let name = field.read(cx).value().trim().to_string();
        let slide = self.current_slide;
        if !name.is_empty() && self.slide().shape_names.get(&id) != Some(&name) && self.slide().shape(id).is_some() {
            self.run(vec![Command::SetShapeName { slide, shape: id, name }], cx);
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// ↑ (forward) or ↓ (backward) for shape `id`.
    pub(crate) fn order_one(&mut self, id: ShapeId, forward: bool, cx: &mut Context<Self>) {
        self.set_selection(vec![id]);
        self.order_selected(if forward { super::editing::Order::Forward } else { super::editing::Order::Backward }, cx);
    }

    /// Context menu entries for a shape row in the Selection Pane.
    pub(crate) fn selection_pane_context_menu_entries(&self, id: ShapeId) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let hidden = self.slide().hidden_shapes.contains(&id);
        vec![
            MenuItem::tr("pptx.selection-pane.rename").icon("edit").on(move |e, w, cx| e.start_renaming(id, w, cx)).into(),
            MenuItem::tr(if hidden { "_xlsx.show" } else { "pptx.selection-pane.hide" })
                .icon(if hidden { "visibility" } else { "visibility_off" })
                .on(move |e, _, cx| e.set_shape_hidden(id, !hidden, cx))
                .into(),
            MenuItem::tr("pptx.selection-pane.show-all").icon("visibility").on(|e, _, cx| e.set_all_hidden(false, cx)).into(),
            MenuItem::tr("pptx.selection-pane.hide-all").icon("visibility_off").on(|e, _, cx| e.set_all_hidden(true, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.order.front").icon("flip_to_front").on(move |e, _, cx| {
                e.set_selection(vec![id]);
                e.order_selected(super::editing::Order::Front, cx);
            }).into(),
            MenuItem::tr("pptx.order.forward").icon("arrow_upward").on(move |e, _, cx| e.order_one(id, true, cx)).into(),
            MenuItem::tr("pptx.order.backward").icon("arrow_downward").on(move |e, _, cx| e.order_one(id, false, cx)).into(),
            MenuItem::tr("pptx.order.back").icon("flip_to_back").on(move |e, _, cx| {
                e.set_selection(vec![id]);
                e.order_selected(super::editing::Order::Back, cx);
            }).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.group")
                .icon("group_work")
                .shortcut("⌥⌘G")
                .disabled(!self.can_group())
                .on(|e, _, cx| e.group_selected(cx))
                .into(),
            MenuItem::tr("pptx.ungroup")
                .icon("workspaces")
                .shortcut("⌥⇧⌘G")
                .disabled(!self.can_ungroup())
                .on(|e, _, cx| e.ungroup_selected(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.frame.settings").icon("straighten").on(move |e, w, cx| {
                e.set_selection(vec![id]);
                e.open_frame_panel(w, cx);
            }).into(),
            MenuItem::tr("pptx.alt-text").icon("accessibility").on(move |e, w, cx| {
                e.set_selection(vec![id]);
                e.open_alt_text_panel(w, cx);
            }).into(),
            MenuEntry::Separator,
            MenuItem::tr("edit.duplicate").icon("control_point_duplicate").shortcut("⌘D").on(move |e, _, cx| {
                e.set_selection(vec![id]);
                e.duplicate_selected(cx);
            }).into(),
            MenuItem::tr("edit.delete").icon("delete").shortcut("⌫").on(move |e, _, cx| {
                e.set_selection(vec![id]);
                e.delete_selected(cx);
            }).into(),
        ]
    }

    /// Open context menu for shape `id` in the Selection Pane at pointer `position`.
    pub(crate) fn open_selection_pane_context_menu(&mut self, id: ShapeId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.slide().hidden_shapes.contains(&id) && !self.selected_ids().contains(&id) {
            self.set_selection(vec![id]);
            cx.notify();
        }
        let entries = self.selection_pane_context_menu_entries(id);
        self.open_popup_entries(entries, position, window, cx);
    }

    pub(crate) fn selection_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let pane = self.pptx()?.selection_pane.as_ref()?;
        let t = crate::shell::i18n::text;
        let renaming = pane.renaming.clone();
        let slide = self.slide().clone();
        let selected = self.selected_ids();
        let shapes: Vec<ShapeId> = slide.shapes.iter().rev().filter(|s| !slide.is_attached(s.id)).map(|s| s.id).collect();
        let reorder = pane.reorder.clone();
        let mut body = v_flex().gap_1();
        if shapes.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.selection-pane.no-shapes-slide")));
        }
        let all_hidden = !shapes.is_empty() && shapes.iter().all(|id| slide.hidden_shapes.contains(id));
        let header = h_flex().w_full().h_full().items_center().gap_1()
            .child(crate::shell::chrome::action_button("sel-all-eye").small().w(px(28.)).icon(Icon::new(Material(if all_hidden { "visibility_off" } else { "visibility" }))).tooltip(if all_hidden { t("pptx.selection-pane.show-all") } else { t("pptx.selection-pane.hide-all") }).on_click(cx.listener(move |e, _, _, cx| e.set_all_hidden(!all_hidden, cx))))
            .child(div().flex_1().child(t("pptx.selection-pane.selection-pane"))).into_any_element();
        let mut rows = Vec::new();
        for id in shapes {
            let hidden = slide.hidden_shapes.contains(&id);
            let n = id.0 as usize;
            let eye = crate::shell::chrome::action_button(("sel-eye", n)).small().w(px(28.))
                .icon(Icon::new(Material(if hidden { "visibility_off" } else { "visibility" })))
                .tooltip(if hidden { t("_xlsx.show") } else { t("pptx.selection-pane.hide") })
                .on_click(cx.listener(move |e, _, _, cx| { cx.stop_propagation(); e.set_shape_hidden(id, !hidden, cx); }));
            let name: AnyElement = match &renaming {
                Some((r, field)) if *r == id => crate::shell::chrome::data_table::row_controls(reorder.clone(), n, 2,
                    h_flex().flex_1().min_w_0().gap_1()
                    .child(div().flex_1().min_w_0().child(crate::shell::chrome::text_field(field).full_width()))
                    .child(crate::shell::chrome::primary_button(("sel-rename-ok", n)).small().label("OK").on_click(cx.listener(|e, _, w, cx| e.finish_renaming(w, cx)))))
                    .into_any_element(),
                _ => div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().px_1().text_sm()
                    .when(hidden, |d| d.text_color(rgb(0x9aa0a6))).child(self.shape_label(id)).into_any_element(),
            };
            let row = h_flex().id(("sel-row", n)).w_full().h_full().min_w_0().items_center().gap_1().px_1()
                .on_click(cx.listener(move |e, _, _, cx| {
                    if !e.slide().hidden_shapes.contains(&id) { e.set_selection(vec![id]); }
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation(); e.open_selection_pane_context_menu(id, event.position, window, cx);
                }))
                .child(crate::shell::chrome::data_table::row_controls(reorder.clone(), n, 0, eye))
                .child(name)
                .child(crate::shell::chrome::data_table::row_controls(reorder.clone(), n, 1,
                    crate::shell::chrome::action_button(("sel-rename", n)).small().w(px(28.)).icon(Icon::new(Material("edit"))).tooltip(t("pptx.selection-pane.rename")).on_click(cx.listener(move |e, _, w, cx| {cx.stop_propagation(); e.start_renaming(id, w, cx);}))))
                .child(crate::shell::chrome::icon("menu", 18., rgb(0x5f6368).into()).cursor_grab());
            rows.push((n, row.into_any_element(), selected.contains(&id)));
        }
        let table = crate::shell::chrome::data_table::DataTable::new("pptx-selection-table", Some(header), rows, cx)
            .reorderable(reorder, |e, from, to, _, cx| {
                let shape = ShapeId(from as u64); let target = ShapeId(to as u64);
                if let Some(index) = e.slide().shape_index(target) {
                    let slide = e.current_slide;
                    e.set_selection(vec![shape]);
                    e.run(vec![Command::MoveShape {slide, shape, index}], cx);
                }
            });
        body = body.child(table);
        Some(SidebarPanel { title: t("pptx.selection-pane.selection-pane"), body: body.into_any_element() })
    }
}
