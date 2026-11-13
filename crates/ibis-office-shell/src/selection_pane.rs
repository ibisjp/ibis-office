// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 選択ウィンドウ (配置 > 選択ウィンドウ): the slide's shapes, frontmost
//! first, by name; a click selects one, the eye shows or hides it (a hidden
//! shape stays on the slide, not drawn: `Slide::hidden_shapes`), the pencil
//! renames it, ↑ ↓ bring it forward or send it backward; すべて表示 and
//! すべて非表示 for them all. Each change can be undone.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::palette::rgb;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{Command, ShapeId};

use crate::chrome::SidebarPanel;
use crate::editor::SlideEditor;
use crate::icons::Material;

/// The pane, while open: the shape being renamed and its field.
#[derive(Default)]
pub struct SelectionPane {
    pub renaming: Option<(ShapeId, Entity<InputState>)>,
    _subscription: Option<Subscription>,
    pub reorder: crate::chrome::data_table::ListState,
}

impl SlideEditor {
    /// 選択ウィンドウ: shown or put away.
    pub fn toggle_selection_pane(&mut self, cx: &mut Context<Self>) {
        if self.uses_shape_edit() {
            self.shape_edit.selection_pane = match self.shape_edit.selection_pane.take() {
                Some(_) => None,
                None => Some(SelectionPane::default()),
            };
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// A shape's name as the pane shows it: its own, else what it is and its place.
    pub fn shape_label(&self, id: ShapeId) -> String {
        let slide = self.slide();
        if let Some(name) = slide.shape_names.get(&id) {
            return name.clone();
        }
        crate::i18n::format("pptx.selection-pane.shape-n", &[("n", &(slide.shape_index(id).map_or(0, |x| x + 1)))])
    }

    /// The eye: shape `id` shown or hidden (a hidden one is no longer selected).
    pub fn set_shape_hidden(&mut self, id: ShapeId, hidden: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if hidden {
            let rest: Vec<ShapeId> = self.selected_ids().into_iter().filter(|s| *s != id).collect();
            self.set_selection(rest);
        }
        self.run(vec![Command::SetShapeHidden { slide, shape: id, hidden }], cx);
    }

    /// すべて表示 / すべて非表示.
    pub fn set_all_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
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
    pub fn start_renaming(&mut self, id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.shape_label(id);
        let field = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(&field, window, move |e: &mut Self, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                e.finish_renaming(window, cx);
            }
        });
        field.update(cx, |f, cx| f.focus(window, cx));
        if let Some(p) = self.shape_edit.selection_pane.as_mut() {
            p.renaming = Some((id, field));
            p._subscription = Some(subscription);
        }
        cx.notify();
    }

    /// The name typed given to its shape.
    pub fn finish_renaming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.shape_edit.selection_pane.as_mut().and_then(|p| p.renaming.take()) else { return };
        let name = field.read(cx).value().trim().to_string();
        let slide = self.current_slide;
        if !name.is_empty() && self.slide().shape_names.get(&id) != Some(&name) && self.slide().shape(id).is_some() {
            self.run(vec![Command::SetShapeName { slide, shape: id, name }], cx);
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// ↑ (forward) or ↓ (backward) for shape `id`.
    pub fn order_one(&mut self, id: ShapeId, forward: bool, cx: &mut Context<Self>) {
        self.set_selection(vec![id]);
        self.order_selected(if forward { crate::shape_items::Order::Forward } else { crate::shape_items::Order::Backward }, cx);
    }

    /// Context menu entries for a shape row in the Selection Pane.
    pub fn selection_pane_context_menu_entries(&self, id: ShapeId) -> Vec<crate::chrome::MenuEntry> {
        use crate::chrome::{MenuEntry, MenuItem};
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
                e.order_selected(crate::shape_items::Order::Front, cx);
            }).into(),
            MenuItem::tr("pptx.order.forward").icon("arrow_upward").on(move |e, _, cx| e.order_one(id, true, cx)).into(),
            MenuItem::tr("pptx.order.backward").icon("arrow_downward").on(move |e, _, cx| e.order_one(id, false, cx)).into(),
            MenuItem::tr("pptx.order.back").icon("flip_to_back").on(move |e, _, cx| {
                e.set_selection(vec![id]);
                e.order_selected(crate::shape_items::Order::Back, cx);
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

    pub fn selection_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let pane = self.shape_edit.selection_pane.as_ref()?;
        let t = crate::i18n::text;
        let renaming = pane.renaming.clone();
        let slide = self.slide().clone();
        let selected = self.selected_ids();
        let shapes: Vec<ShapeId> = slide.shapes.iter().rev().filter(|s| !slide.is_attached(s.id)).map(|s| s.id).collect();
        let reorder = pane.reorder.clone();
        if shapes.is_empty() {
            let body = v_flex().gap_1().child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.selection-pane.no-shapes-slide")));
            return Some(SidebarPanel { title: t("pptx.selection-pane.selection-pane"), body: body.into_any_element() });
        }
        use crate::chrome::data_table::{DataTable, no_drag};
        use crate::chrome::virtual_table::{TableColumn, TableRow};
        let all_hidden = shapes.iter().all(|id| slide.hidden_shapes.contains(id));
        let editor = cx.entity().downgrade();
        let all_editor = editor.clone();
        let header = vec![
            crate::chrome::action_button("sel-all-eye").small().w(px(28.)).icon(Icon::new(Material(if all_hidden { "visibility_off" } else { "visibility" }))).tooltip(if all_hidden { t("pptx.selection-pane.show-all") } else { t("pptx.selection-pane.hide-all") })
                .on_click(move |_, _, cx| { let _ = all_editor.update(cx, |e, cx| e.set_all_hidden(!all_hidden, cx)); }).into_any_element(),
            div().text_sm().child(t("pptx.selection-pane.selection-pane")).into_any_element(),
        ];
        let labels: std::rc::Rc<Vec<String>> = std::rc::Rc::new(shapes.iter().map(|id| self.shape_label(*id)).collect());
        let hidden_of: std::rc::Rc<Vec<bool>> = std::rc::Rc::new(shapes.iter().map(|id| slide.hidden_shapes.contains(id)).collect());
        let selected_of: std::rc::Rc<Vec<bool>> = std::rc::Rc::new(shapes.iter().map(|id| selected.contains(id)).collect());
        let shapes = std::rc::Rc::new(shapes);
        let count = shapes.len();
        let build = {
            let (shapes, labels, hidden_of, selected_of, editor, renaming) = (shapes.clone(), labels.clone(), hidden_of.clone(), selected_of.clone(), editor.clone(), renaming.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range.filter_map(|i| shapes.get(i).copied().map(|id| (i, id))).map(|(i, id)| {
                    let (hidden, n) = (hidden_of[i], id.0 as usize);
                    let t = crate::i18n::text;
                    let toggle = editor.clone();
                    let eye = no_drag(crate::chrome::action_button(("sel-eye", n)).small().w(px(28.))
                        .icon(Icon::new(Material(if hidden { "visibility_off" } else { "visibility" })))
                        .tooltip(if hidden { t("_xlsx.show") } else { t("pptx.selection-pane.hide") })
                        .on_click(move |_, _, cx| { cx.stop_propagation(); let _ = toggle.update(cx, |e, cx| e.set_shape_hidden(id, !hidden, cx)); })).into_any_element();
                    let name: AnyElement = match &renaming {
                        Some((r, field)) if *r == id => {
                            let ok = editor.clone();
                            no_drag(h_flex().w_full().min_w_0().gap_1()
                                .child(div().flex_1().min_w_0().child(crate::chrome::text_field(field).full_width()))
                                .child(crate::chrome::primary_button(("sel-rename-ok", n)).small().label("OK").on_click(move |_, w, cx| { let _ = ok.update(cx, |e, cx| e.finish_renaming(w, cx)); }))).w_full().into_any_element()
                        }
                        _ => div().w_full().min_w_0().overflow_hidden().whitespace_nowrap().px_1().text_sm()
                            .when(hidden, |d| d.text_color(rgb(0x9aa0a6))).child(SharedString::from(labels[i].clone())).into_any_element(),
                    };
                    let (select, rename, menu) = (editor.clone(), editor.clone(), editor.clone());
                    let row = TableRow::new(vec![eye, name])
                        .selected(selected_of[i])
                        .on_click(move |_, _, cx| {
                            let _ = select.update(cx, |e, cx| {
                                if !e.slide().hidden_shapes.contains(&id) { e.set_selection(vec![id]); }
                                cx.notify();
                            });
                        })
                        .draggable(id.0, SharedString::from(labels[i].clone()))
                        .drop_between(id.0)
                        .action("sel-rename", "edit", t("pptx.selection-pane.rename"), move |_, w, cx| { let _ = rename.update(cx, |e, cx| e.start_renaming(id, w, cx)); });
                    let _ = &menu;
                    row
                }).collect()
            }
        };
        let table = DataTable::reorder_list("pptx-selection-table", vec![TableColumn::fixed("eye", 28.), TableColumn::flex("name")], header, &reorder, count, build, |e, from, to, _, cx| {
            let shape = ShapeId(from as u64); let target = ShapeId(to as u64);
            if let Some(index) = e.slide().shape_index(target) {
                let slide = e.current_slide;
                e.set_selection(vec![shape]);
                e.run(vec![Command::MoveShape {slide, shape, index}], cx);
            }
        }, cx).row_actions(1);
        // The right click of a row opens its menu.
        let right = editor.clone();
        let dnd = reorder.clone();
        let host = div().id("sel-table-host").w_full().flex_1().min_h_0().child(table)
            .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, window, cx| {
                if let Some(id) = (0..count).find(|i| dnd.row_bounds(*i).is_some_and(|b| b.contains(&ev.position))).map(|i| shapes[i]) {
                    cx.stop_propagation();
                    let _ = right.update(cx, |e, cx| e.open_selection_pane_context_menu(id, ev.position, window, cx));
                }
            });
        Some(SidebarPanel { title: t("pptx.selection-pane.selection-pane"), body: v_flex().size_full().child(host).into_any_element() }.fill())
    }
}
