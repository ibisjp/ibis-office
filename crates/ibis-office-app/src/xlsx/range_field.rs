// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared sheet-reference field and a reversible GUI range-picking session.
use crate::shell::{
    chrome::{self, TextField},
    editor::SlideEditor,
};
use gpui_kit::component::input::{InputState,InputEvent};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CellRange, ShapeId, SlideId};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RangeEdge {
    Top,
    Bottom,
    Left,
    Right,
}
impl RangeEdge {
    pub(crate) fn cursor(self) -> CursorStyle {
        match self {
            Self::Top | Self::Bottom => CursorStyle::ResizeUpDown,
            Self::Left | Self::Right => CursorStyle::ResizeLeftRight,
        }
    }
}

pub(crate) struct RangePick {
    input: Entity<InputState>,
    single_cell: bool,
    formula: bool,
    qualified: bool,
    anchor: Option<(u32, u32)>,
    original: String,
    slide: SlideId,
    ranges: Vec<CellRange>,
    selection: Option<ShapeId>,
    zoom: Option<f32>,
    scroll: Point<Pixels>,
    dialog: Option<Bounds<Pixels>>,
    dragging: bool,
    resizing: Option<(RangeEdge, CellRange)>,
    handles: Rc<RefCell<BTreeMap<RangeEdge, Bounds<Pixels>>>>,
    captures: Rc<RefCell<BTreeMap<RangeEdge, HitboxId>>>,
    cancel: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// A range input whose trailing table icon starts selection on the actual sheet.
pub(crate) fn range_field(
    input: &Entity<InputState>,
    id: &'static str,
    cx: &Context<SlideEditor>,
) -> TextField {
    let target = input.clone();
    chrome::text_field(input).full_width().trailing_icon(
        id,
        "table_chart",
        crate::shell::i18n::text("xlsx.range-field.pick"),
        cx,
        move |this, window, cx| this.start_sheet_range_pick(target.clone(), window, cx),
    )
}

/// Local addresses are accepted by sheet tools; a single-cell field confirms on mouse-up.
pub(crate) fn local_range_field(input:&Entity<InputState>,id:&'static str,single_cell:bool,cx:&Context<SlideEditor>)->TextField {
 let target=input.clone();
 chrome::text_field(input).full_width().trailing_icon(id,"table_chart",crate::shell::i18n::text("xlsx.range-field.pick"),cx,move |e,w,cx|e.start_sheet_range_pick_with_options(target.clone(),single_cell,false,false,w,cx))
}

impl SlideEditor {
    pub(crate) fn generic_range_pick_panel(&self,cx:&Context<Self>)->Option<chrome::SidebarPanel> {
        let pick=self.sheet_state()?.range_pick.as_ref()?;
        let body=gpui_kit::component::v_flex().p_3().gap_2().p_0().child(chrome::text_field(&pick.input).full_width()).child(div().text_color(chrome::text_secondary()).child(crate::shell::i18n::text("xlsx.range-field.pick")));
        let footer=chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("range-pick-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|e,_,w,cx|e.finish_sheet_range_pick(false,w,cx))))
            .action(cancel_action(crate::shell::chrome::ActionDefinition::new("range-pick-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e,_,w,cx|e.finish_sheet_range_pick(true,w,cx))), self));
        Some(chrome::SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.range-field.pick"),body.into_any_element(),footer))
    }
    pub(crate) fn suspend_sheet_range_drag(&mut self, window: &mut Window) {
        if let Some(p)=self.sheet_state_mut().and_then(|s|s.range_pick.as_mut()) {
            if p.dragging { p.dragging=false;p.resizing=None;window.release_pointer(); }
        }
    }
    pub(crate) fn clear_sheet_range_handle_bounds(&self) {
        if let Some(pick) = self.sheet_state().and_then(|s| s.range_pick.as_ref()) {
            pick.handles.borrow_mut().clear();
            pick.captures.borrow_mut().clear();
        }
    }
    pub(crate) fn sheet_range_handle_bounds(&self, edge: RangeEdge) -> Option<Bounds<Pixels>> {
        self.sheet_state()?
            .range_pick
            .as_ref()?
            .handles
            .borrow()
            .get(&edge)
            .copied()
    }
    pub(crate) fn sheet_range_pick_handles(&self, width: f32, height: f32) -> Vec<Stateful<Div>> {
        let Some(pick) = self.sheet_state().and_then(|s| s.range_pick.as_ref()) else {
            return vec![];
        };
        if pick.single_cell {return vec![];}
        [
            (RangeEdge::Top, width / 2. - 7., -7.),
            (RangeEdge::Bottom, width / 2. - 7., height - 7.),
            (RangeEdge::Left, -7., height / 2. - 7.),
            (RangeEdge::Right, width - 7., height / 2. - 7.),
        ]
        .into_iter()
        .map(|(edge, x, y)| {
            let bounds = pick.handles.clone();
            let captures = pick.captures.clone();
            div()
                .id(("sheet-range-resize", edge as usize))
                .absolute()
                .left(px(x))
                .top(px(y))
                .size(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .cursor(edge.cursor())
                .child(
                    div()
                        .size(px(8.))
                        .bg(rgb(0xffffff))
                        .border_2()
                        .border_color(rgb(super::sheet_ui::SELECTION_COLOR)),
                )
                .child(
                    canvas(
                        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                        move |measured, hitbox, window, _| {
                            let visible = measured.intersect(&window.content_mask().bounds);
                            if visible.size.width > px(0.) && visible.size.height > px(0.) {
                                bounds.borrow_mut().insert(edge, visible);
                                captures.borrow_mut().insert(edge, hitbox.id);
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
        })
        .collect()
    }

    /// Keep the live source range in sync with document history without replacing a series draft.
    pub(crate) fn refresh_chart_source_field_after_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet_range_pick_active() {
            return;
        }
        let Some(panel) = self
            .sheet_state()
            .and_then(|s| s.chart_edit.as_ref())
            .filter(|p| p.series_editor.is_none())
        else {
            return;
        };
        let (shape, input) = (panel.shape, panel.range.clone());
        let Some(chart) = self
            .presentation()
            .slides
            .iter()
            .flat_map(|s| &s.shapes)
            .find_map(|s| {
                if s.id != shape {
                    return None;
                }
                if let ibis_doc_model::ShapeKind::Chart(chart) = &s.kind {
                    Some(chart.as_ref())
                } else {
                    None
                }
            })
        else {
            return;
        };
        let text = self
            .source
            .as_ref()
            .and_then(|source| source.sheet_chart_refs(shape, chart))
            .and_then(|refs| ibis_pptx::refs_range(&refs))
            .map(|(sheet, range, _)| ibis_pptx::format_ref(&sheet, range))
            .unwrap_or_default();
        if input.read(cx).value().as_ref() != text {
            input.update(cx, |state, cx| state.set_value(text, window, cx));
        }
    }

    pub(crate) fn is_sheet_range_pick_target(&self, input: &Entity<InputState>) -> bool {
        self.sheet_state()
            .and_then(|s| s.range_pick.as_ref())
            .is_some_and(|pick| &pick.input == input)
    }

    pub(crate) fn sheet_range_pick_active(&self) -> bool {
        self.sheet_state().is_some_and(|s| s.range_pick.is_some())
    }

    pub(crate) fn start_sheet_range_pick(
        &mut self,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name=self.sheet_state().and_then(|s|s.chart_edit.as_ref()).and_then(|p|p.series_editor.as_ref()).is_some_and(|e|e.name==input);
        self.start_sheet_range_pick_with_options(input,name,name,true,window,cx);
    }
    fn start_sheet_range_pick_with_options(&mut self,input:Entity<InputState>,single_cell:bool,formula:bool,qualified:bool,window:&mut Window,cx:&mut Context<Self>) {
        if !self.is_workbook() {
            return;
        }
        if self.sheet_range_pick_active() {
            self.finish_sheet_range_pick(false, window, cx);
        }
        let original = input.read(cx).value().to_string();
        let range = super::sheet_ui::parse_reference(original.trim().trim_start_matches('='))
            .and_then(|reference| self.resolve_reference(&reference))
            .filter(|(slide, _)| *slide == self.current_slide)
            .map(|(_, range)| range);

        let state = RangePick {
            single_cell, formula, qualified,
            anchor: None,
            input,
            original,
            slide: self.current_slide,
            ranges: self.selected_ranges(),
            selection: self.selection,
            zoom: self.zoom,
            scroll: self.workspace_scroll.offset(),
            dialog: self.panel_dialog.bounds,
            dragging: false,
            resizing: None,
            handles: Default::default(),
            captures: Default::default(),
            cancel: Default::default(),
        };
        self.wb_mut().range_pick = Some(state);
        if let Some(range) = range {
            self.set_selected_ranges(&[range]);
        }
        if self.panel_dialog.active {
            if let Some(mut bounds) = self.panel_dialog.bounds {
                bounds.size.width = px(320.).min(window.viewport_size().width);
                bounds.origin.x = window.viewport_size().width - bounds.size.width;
                self.panel_dialog.bounds = Some(bounds);
            }
        }
        self.focus_handle.focus(window, cx);
        self.update_sheet_range_pick(window, cx);
        cx.notify();
    }

    pub(crate) fn update_sheet_range_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pick) = self.sheet_state().and_then(|s| s.range_pick.as_ref()) else {
            return;
        };
        let single_cell = pick.single_cell;
        let (qualified,formula)=(pick.qualified||self.current_slide!=pick.slide,pick.formula);
        let Some(mut range) = self.selected_range() else {
            return;
        };
        if single_cell { range.last_column=range.first_column; range.last_row=range.first_row; }
        let input = pick.input.clone();
        let name = self.slide().name.clone().unwrap_or_else(|| {
            let index = self
                .presentation()
                .slides
                .iter()
                .position(|s| s.id == self.current_slide)
                .unwrap_or(0);
            format!("Sheet{}", index + 1)
        });
        let text = if qualified {ibis_pptx::format_ref(&name,range)} else {
            let first=ibis_doc_model::cell_name(range.first_column,range.first_row);
            if range.first_column==range.last_column&&range.first_row==range.last_row {first} else {format!("{}:{}",first,ibis_doc_model::cell_name(range.last_column,range.last_row))}
        };
        let text=if formula {format!("={text}")} else {text};
        if input.read(cx).value().as_ref() != text {
            input.update(cx, |state, cx| state.set_value(text, window, cx));
        }
    }

    /// End picking without changing the document. Only the owning field's draft changes.
    pub(crate) fn finish_sheet_range_pick(
        &mut self,
        cancel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pick) = self.sheet_state_mut().and_then(|s| s.range_pick.take()) else {
            return;
        };
        if cancel {
            pick.input
                .update(cx, |state, cx| state.set_value(pick.original, window, cx));
        }
        if self.current_slide != pick.slide {
            self.zoom = pick.zoom;
            self.workspace_scroll.set_offset(pick.scroll);
        }
        self.current_slide = pick.slide;
        self.selection = pick.selection;
        self.set_selected_ranges(&pick.ranges);
        self.wb_mut().cell_drag = false;
        if pick.dialog.is_some() {self.panel_dialog.bounds=pick.dialog;}
        window.release_pointer();
        // A live chart-wide change belongs to document Undo; leave focus on the sheet.
        // Series fields remain draft, so their dialog resumes at the edited input.
        let global = self
            .wb()
            .chart_edit
            .as_ref()
            .is_some_and(|p| p.series_editor.is_none() && p.range == pick.input);
        if global {
            if !cancel {
                self.set_sheet_chart_data(false, window, cx);
            }
            self.focus_handle.focus(window, cx);
        } else {
            if !cancel {
                let series=self.wb().chart_edit.as_ref().and_then(|p|p.series_editor.as_ref()).is_some_and(|s|s.name==pick.input||s.categories==pick.input||s.values==pick.input);
                if series {self.commit_chart_series_editor(cx);} else {pick.input.update(cx,|_,cx|cx.emit(InputEvent::Change));}
            }
            pick.input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Runs before keymap dispatch; Enter and Escape must not edit a cell or close the owner dialog.
    pub(crate) fn sheet_range_pick_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.sheet_range_pick_active() || !self.interaction_layer().allows_range_pick() {
            return false;
        }
        if key.key == "escape" {
            self.finish_sheet_range_pick(true, window, cx);
            cx.stop_propagation();
            return true;
        }
        if key.key == "enter" && self.focus_handle.is_focused(window) {
            self.finish_sheet_range_pick(false, window, cx);
            cx.stop_propagation();
            return true;
        }
        false
    }

    /// Capture sheet presses so chart overlays cannot consume the selection gesture.
    pub(crate) fn sheet_range_pick_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.sheet_range_pick_active() || !self.interaction_layer().allows_range_pick() {
            return false;
        }
        if self
            .panel_dialog
            .bounds
            .is_some_and(|bounds| bounds.contains(&event.position))
        {
            let cancel = self
                .wb()
                .range_pick
                .as_ref()
                .unwrap()
                .cancel
                .get()
                .is_some_and(|bounds| bounds.contains(&event.position));
            self.finish_sheet_range_pick(cancel, window, cx);
            self.wb_mut().range_pick_click_release = true;
            return true;
        }
        if event.button != MouseButton::Left
            || !self
                .workspace_bounds
                .is_some_and(|bounds| bounds.contains(&event.position))
        {
            return false;
        }
        let handle = self
            .wb()
            .range_pick
            .as_ref()
            .unwrap()
            .handles
            .borrow()
            .iter()
            .find(|(_, bounds)| bounds.contains(&event.position))
            .map(|(&edge, _)| edge);
        if let Some(edge) = handle {
            if let Some(range) = self.selected_range() {
                let pick = self.wb_mut().range_pick.as_mut().unwrap();
                pick.resizing = Some((edge, range));
                pick.dragging = true;
                if let Some(&hitbox) = pick.captures.borrow().get(&edge) {
                    window.capture_pointer(hitbox);
                }
                self.focus_handle.focus(window, cx);
                cx.notify();
                return true;
            }
        }
        let Some((column, row)) = self.cell_under(event.position) else {
            return false;
        };
        self.set_cell(column, row);
        let pick = self.wb_mut().range_pick.as_mut().unwrap();
        pick.anchor = Some((column, row));
        pick.dragging = true;
        self.focus_handle.focus(window, cx);
        self.update_sheet_range_pick(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn sheet_range_pick_up(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.interaction_layer().is_confirmation() {return false;}
        if self
            .sheet_state()
            .is_some_and(|s| s.range_pick_click_release)
        {
            self.wb_mut().range_pick_click_release = false;
            return true;
        }
        let Some(pick) = self.sheet_state_mut().and_then(|s| s.range_pick.as_mut()) else {
            return false;
        };
        if !pick.dragging {
            return false;
        }
        let single_cell = pick.single_cell;
        let anchor = pick.anchor;
        pick.dragging = false;
        pick.resizing = None;
        window.release_pointer();
        if single_cell {
            if let Some((col,row)) = anchor { self.set_cell(col,row); }
        }
        self.update_sheet_range_pick(window, cx);
        if single_cell { self.finish_sheet_range_pick(false, window, cx); }
        cx.notify();
        true
    }

    pub(crate) fn sheet_range_pick_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.interaction_layer().allows_range_pick() {return false;}
        if !self
            .sheet_state()
            .and_then(|s| s.range_pick.as_ref())
            .is_some_and(|p| p.dragging)
        {
            return false;
        }
        if event.pressed_button == Some(MouseButton::Left) {
            let resizing = self.wb().range_pick.as_ref().unwrap().resizing;
            if let Some((edge, mut range)) = resizing {
                if let Some((column, row)) = self.cell_under(event.position) {
                    match edge {
                        RangeEdge::Top => range.first_row = row.min(range.last_row),
                        RangeEdge::Bottom => range.last_row = row.max(range.first_row),
                        RangeEdge::Left => range.first_column = column.min(range.last_column),
                        RangeEdge::Right => range.last_column = column.max(range.first_column),
                    }
                    self.set_selected_ranges(&[range]);
                }
            } else if !self.wb().range_pick.as_ref().unwrap().single_cell {
                self.extend_cells_to(event.position);
            }
            self.update_sheet_range_pick(window, cx);
        } else {
            self.wb_mut().range_pick.as_mut().unwrap().dragging = false;
            window.release_pointer();
        }
        cx.notify();
        true
    }
}

/// Register before bubbling even when a modeless dialog occludes the editor's own hitbox.
/// A root Div's capture handler is hover-filtered, so it cannot guard an occluding overlay.
pub(crate) fn event_surface(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.weak_entity();
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let down = editor.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let _ = down.update(cx, |this, cx| {
                    if !this.interaction_layer().allows_range_pick() { return; }
                    if this.sheet_range_pick_down(event, window, cx) {
                        cx.stop_propagation();
                    }
                });
            });
            let moved = editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let _ = moved.update(cx, |this, cx| {
                    if !this.interaction_layer().allows_range_pick() { return; }
                    if this.sheet_range_pick_move(event, window, cx) {
                        cx.stop_propagation();
                    }
                });
            });
            let up = editor.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let _ = up.update(cx, |this, cx| {
                    if !this.interaction_layer().allows_range_pick() { return; }
                    if this.sheet_range_pick_up(window, cx) {
                        cx.stop_propagation();
                    }
                });
            });
        },
    )
    .absolute()
    .inset_0()
}

/// Mark the actual Cancel control so the range session can discard its own draft before Button bubbling.
pub(crate) fn cancel_region(control: impl IntoElement, editor: &SlideEditor) -> Div {
    let cancel = editor
        .sheet_state()
        .and_then(|s| s.range_pick.as_ref())
        .map(|p| p.cancel.clone());
    div()
        .relative()
        .flex_none()
        .child(control)
        .when_some(cancel, |d, cancel| {
            d.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| cancel.set(Some(bounds)),
                )
                .absolute()
                .inset_0(),
            )
        })
}

/// The shared action renderer marks Cancel without allowing a custom button wrapper.
pub(crate) fn cancel_action(
    action: chrome::ActionDefinition,
    editor: &SlideEditor,
) -> chrome::ActionDefinition {
    let cancel = editor
        .sheet_state()
        .and_then(|s| s.range_pick.as_ref())
        .map(|p| p.cancel.clone());
    action.on_bounds(move |bounds, _, _| {
        if let Some(cancel) = &cancel {
            cancel.set(Some(bounds));
        }
    })
}
