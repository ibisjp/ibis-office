// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pressing a form control (フォーム コントロール): a check box turns, an option button chooses itself in its group, the
//! arrows of a scroll bar and a spin button and the scroll bar's track change the value by a step or a page, a list
//! box chooses the item pressed, a drop-down opens its items. What the control is linked to (the cell of its
//! `fmlaLink`) is written in the same step.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;
use ibis_doc_model::form_control::{ControlHit, radio_group, resolve};
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, ControlKind, FormControl, ShapeId, ShapeKind, SheetCommand, SlideId};

use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ControlThumbDrag {
    pub(crate) shape: ShapeId,
    pub(crate) grab_offset: i64,
}

#[ibis_office_macros::editor_ext(ControlExt)]
impl SlideEditor {
    /// A press on a control shape: operates it unless design mode or a selection modifier is active.
    pub(crate) fn control_press(&mut self, shape: ShapeId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.is_design_mode_active() || event.modifiers.alt || event.modifiers.platform || event.modifiers.control {
            return false;
        }
        let Some(layout) = self.slide_layout() else { return false };
        let Some(sh) = self.slide().shape(shape) else { return false };
        let ShapeKind::Control(control) = &sh.kind else { return false };
        let control = (**control).clone();
        let frame = sh.frame;
        let p = layout.to_emu(event.position);
        let Some(hit) = ibis_doc_model::form_control::hit(&control, frame.width, frame.height, p.x - frame.x, p.y - frame.y) else { return true };
        let slide = self.current_slide;
        let mut next = control.clone();
        match (control.kind, hit) {
            (ControlKind::CheckBox, _) => next.checked = u8::from(control.checked != 1),
            (ControlKind::Radio, _) => next.checked = 1,
            (ControlKind::Scroll | ControlKind::Spin, ControlHit::Arrow(d)) => next.value = (control.value + d * control.step).clamp(control.min, control.max),
            (ControlKind::Scroll, ControlHit::Track(d)) => next.value = (control.value + d * control.page).clamp(control.min, control.max),
            (ControlKind::Scroll, ControlHit::Whole) => {
                let along = if control.horizontal { p.x - frame.x } else { p.y - frame.y };
                let (length, across) = if control.horizontal { (frame.width, frame.height) } else { (frame.height, frame.width) };
                let square = across.min(length / 2).max(1);
                let (at, _size) = ibis_doc_model::form_control::thumb(&control, length, square);
                if let Some(state) = self.sheet_state_mut() {
                    state.control_drag = Some(ControlThumbDrag {
                        shape,
                        grab_offset: along - at,
                    });
                }
                return true;
            }
            (ControlKind::List, ControlHit::Item(i)) => next.value = i as i64 + 1,
            (ControlKind::Drop, _) => {
                // The ▼ button is the square at the right end of the control: the list hangs from its lower left corner.
                let button = (frame.height).min(16 * ibis_doc_model::EMU_PER_POINT);
                let origin = |x: i64, y: i64| point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(y));
                let bounds = Bounds::from_corners(origin(frame.x + frame.width - button, frame.y), origin(frame.x + frame.width, frame.y + frame.height));
                // The list is as wide as the control (a list that does not fit the window's right side hangs from the button's right edge).
                let list_width = layout.to_px(frame.width).max(bounds.size.width);
                self.open_control_list(shape, &control, crate::shell::chrome::dropdown_list::below_button(bounds, list_width, window.viewport_size()), window, cx);
                return true;
            }
            _ => return true,
        }
        if next != control {
            self.apply_control(slide, shape, next, cx);
        }
        true
    }

    pub(crate) fn on_control_drag_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.sheet_state().and_then(|s| s.control_drag) else { return };
        let Some(layout) = self.slide_layout() else { return };
        let Some(sh) = self.slide().shape(drag.shape) else { return };
        let ShapeKind::Control(control) = &sh.kind else { return };
        let control = (**control).clone();
        let frame = sh.frame;
        let p = layout.to_emu(position);
        let along = if control.horizontal { p.x - frame.x } else { p.y - frame.y };
        let (length, across) = if control.horizontal { (frame.width, frame.height) } else { (frame.height, frame.width) };
        let square = across.min(length / 2).max(1);
        let value = ibis_doc_model::form_control::thumb_value(&control, length, square, along, drag.grab_offset);
        if value != control.value {
            let mut next = control;
            next.value = value;
            self.apply_control(self.current_slide, drag.shape, next, cx);
        }
    }

    pub(crate) fn on_control_drag_up(&mut self, cx: &mut Context<Self>) -> bool {
        if self.sheet_state_mut().and_then(|s| s.control_drag.take()).is_some() {
            cx.notify();
            true
        } else {
            false
        }
    }

    /// The control `shape` made `next`, and its link written (an option button chooses itself among its group).
    pub(crate) fn apply_control(&mut self, slide: SlideId, shape: ShapeId, next: FormControl, cx: &mut Context<Self>) {
        let mut commands: Vec<Command> = Vec::new();
        let mut link: Option<(String, CellValue)> = None;
        match next.kind {
            ControlKind::CheckBox => link = next.link.clone().map(|l| (l, CellValue::Bool(next.checked == 1))),
            ControlKind::Scroll | ControlKind::Spin | ControlKind::Drop | ControlKind::List => link = next.link.clone().map(|l| (l, CellValue::Number(next.value as f64))),
            ControlKind::Radio => {
                let Some(index) = self.slide().shapes.iter().position(|s| s.id == shape) else { return };
                let group = radio_group(self.slide(), index);
                for (position, &i) in group.iter().enumerate() {
                    let other = &self.slide().shapes[i];
                    let ShapeKind::Control(c) = &other.kind else { continue };
                    if i == index {
                        link = c.link.clone().or_else(|| next.link.clone()).map(|l| (l, CellValue::Number(position as f64 + 1.0)));
                        // The group's number is kept by the link of the first button, if it has one.
                        if let Some(ShapeKind::Control(first)) = group.first().map(|&f| &self.slide().shapes[f].kind) {
                            if let Some(l) = &first.link {
                                link = Some((l.clone(), CellValue::Number(position as f64 + 1.0)));
                            }
                        }
                        continue;
                    }
                    if c.checked != 0 {
                        let mut off = (**c).clone();
                        off.checked = 0;
                        commands.push(Command::Sheet(SheetCommand::EditControl { slide, shape: other.id, control: off }));
                    }
                }
            }
            _ => {}
        }
        commands.push(Command::Sheet(SheetCommand::EditControl { slide, shape, control: next }));
        if let Some((text, value)) = link {
            if let Some((link_slide, range)) = resolve(self.presentation(), slide, &text) {
                commands.push(Command::Sheet(SheetCommand::SetCells { slide: link_slide, edits: vec![CellEdit { row: range.first_row, column: range.first_column, input: CellInput::Value(value) }] }));
            }
        }
        self.run_sheet(commands, cx);
    }

    /// A drop-down's items, open by it.
    fn open_control_list(&mut self, shape: ShapeId, control: &FormControl, (position, anchor): (Point<Pixels>, Anchor), window: &mut Window, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let entries: std::rc::Rc<Vec<MenuEntry>> = std::rc::Rc::new(
            control
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    MenuItem::new("xlsx.validation-item", item.clone())
                        .on(move |this, _, cx| {
                            if let Some(ShapeKind::Control(c)) = this.slide().shape(shape).map(|s| s.kind.clone()) {
                                let mut next = (*c).clone();
                                next.value = i as i64 + 1;
                                if next != *c {
                                    this.apply_control(slide, shape, next, cx);
                                }
                            }
                            if let Some(s) = this.sheet_state_mut() {
                                s.list_popup = None;
                            }
                            cx.notify();
                        })
                        .into()
                })
                .collect(),
        );
        if entries.is_empty() {
            return;
        }
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(160.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, anchor, _dismiss: dismiss });
        cx.notify();
    }
}
