// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The scrolling mode of the shared table (`DataTable::virtualized`): a pane with one table in it. The header row
//! stays where it is, only the value rows scroll (and only they have a scrollbar), the rows are made for the
//! visible range alone (so tens of thousands of rows stay light), and the columns that say so are resized by
//! dragging the border of their heading. The pane around it uses `SidebarPanel::fill`, which leaves the pane without
//! a scrollbar of its own.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Range;
use super::data_table::table_surface;
use super::row_dnd::{DropHandler, DropKind, RowDnd};

/// The background of a group row (a heading row inside the rows).
pub(crate) const GROUP_ROW_BG: u32 = 0xf8f9fb;
/// The size of a row-end action button, and the gap between two.
const ACTION_SIZE: f32 = 28.;
/// The room of the grip at the end of a draggable row (`DataTable::drag_handles`).
const GRIP_WIDTH: f32 = 24.;
/// A table is "wide" (its `TableRow::action_when_wide` buttons show) when, with every button and the grip in place, the flex
/// column keeps at least this much for its text.
const WIDE_FLEX_MIN: f32 = 120.;

/// How wide a column is.
#[derive(Clone, Copy, Debug)]
pub enum ColumnWidth {
    Fixed(f32),
    /// Dragged by the border of its heading, between `min` and `max`.
    Resizable { min: f32, default: f32, max: f32 },
    /// Takes what the other columns leave. At most one.
    Flex,
}

#[derive(Clone, Copy, Debug)]
pub struct TableColumn {
    pub id: &'static str,
    pub width: ColumnWidth,
}

impl TableColumn {
    pub const fn fixed(id: &'static str, width: f32) -> Self {
        Self { id, width: ColumnWidth::Fixed(width) }
    }
    pub const fn resizable(id: &'static str, min: f32, default: f32, max: f32) -> Self {
        Self { id, width: ColumnWidth::Resizable { min, default, max } }
    }
    pub const fn flex(id: &'static str) -> Self {
        Self { id, width: ColumnWidth::Flex }
    }
}

pub struct Drag {
    id: &'static str,
    x0: f32,
    w0: f32,
    min: f32,
    max: f32,
}

#[derive(Default)]
struct ColumnState {
    widths: BTreeMap<&'static str, f32>,
    drag: Option<Drag>,
}

/// What the screen keeps for one table: the scroll position and the dragged column widths. Cloning shares them.
#[derive(Clone, Default)]
pub struct TableState {
    pub scroll: UniformListScrollHandle,
    columns: Rc<RefCell<ColumnState>>,
    built: Rc<RefCell<Range<usize>>>,
    largest_batch: Rc<Cell<usize>>,
    /// The width of the rows' area as of the last layout (0 before the first), for the actions that show only in a wide table.
    body_width: Rc<Cell<f32>>,
}

impl TableState {
    pub fn width(&self, column: &TableColumn) -> f32 {
        match column.width {
            ColumnWidth::Fixed(w) => w,
            ColumnWidth::Resizable { min, default, max } => self.columns.borrow().widths.get(column.id).copied().unwrap_or(default).clamp(min, max),
            ColumnWidth::Flex => 0.,
        }
    }

    /// Scroll so that the row is in view (nothing moves if it already is).
    pub fn scroll_to(&self, index: usize) {
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
    }

    /// The scroll offset of the rows, in px (0 at the top).
    pub fn scrolled(&self) -> f32 {
        -f32::from(self.scroll.0.borrow().base_handle.offset().y)
    }

    /// The height of the rows' viewport as of the last layout (0 before the first).
    pub(crate) fn viewport(&self) -> f32 {
        f32::from(self.scroll.0.borrow().base_handle.bounds().size.height)
    }

    /// The most the rows can scroll, in px.
    pub fn max_scroll(&self) -> f32 {
        f32::from(self.scroll.0.borrow().base_handle.max_offset().y)
    }

    /// The rows the screen should have ready: the visible ones and a screenful on each side.
    pub fn window(&self, row_height: f32, count: usize) -> Range<usize> {
        let viewport = if self.viewport() > 0. { self.viewport() } else { 800. };
        let visible = (viewport / row_height).ceil() as usize + 1;
        let first = (self.scrolled() / row_height).floor().max(0.) as usize;
        first.saturating_sub(visible).min(count)..(first + 2 * visible).min(count)
    }

    /// The range of rows the last frame made (the rows the list asked for), and (`largest_batch`) the most it asked for at once.
    pub fn last_built(&self) -> Range<usize> {
        self.built.borrow().clone()
    }

    /// The most rows the list has asked for at once, since the table was made.
    pub fn largest_batch(&self) -> usize {
        self.largest_batch.get()
    }

    pub fn dragging(&self) -> bool {
        self.columns.borrow().drag.is_some()
    }
}

/// A heading row of the table: one cell over the whole row, in the same rows (the same scrolling and the same
/// making of the visible rows only) as the rows below it. `collapsed` is `None` for a heading that does not fold;
/// the screen keeps whether it is folded and makes the rows below accordingly (`TableRow::on_click` toggles).
pub struct GroupHeader {
    pub icon: Option<&'static str>,
    pub label: SharedString,
    pub count: Option<usize>,
    pub collapsed: Option<bool>,
}

/// What a row is dragged by: its id (the screen's own number) and the label that follows the pointer.
pub struct RowDrag {
    pub id: u64,
    pub label: SharedString,
}

/// A button at the end of a row (delete, settings…).
pub struct RowAction {
    /// Shown only when the table is wide (see `WIDE_FLEX_MIN`).
    pub(crate) wide_only: bool,
    pub id: &'static str,
    pub icon: &'static str,
    pub tooltip: SharedString,
    pub on_click: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

/// A row of the table.
#[derive(Default)]
pub struct TableRow {
    /// One element per column, in the order of the columns (unused by a group row).
    pub cells: Vec<AnyElement>,
    pub selected: bool,
    pub on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    /// Makes the row a heading row.
    pub group: Option<GroupHeader>,
    /// The row can be dragged (needs `DataTable::row_dnd`).
    pub drag: Option<RowDrag>,
    /// The row takes drops (its id, and whether as a whole or above/below).
    pub drop: Option<(u64, DropKind)>,
    /// Called when the button goes down on the (draggable) row, with the press's window.
    pub on_press: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    /// Which rows this one may change places with (`drag_scope`); 0 for the rows that are free.
    pub scope: u64,
    /// Buttons at the end of the row (the table reserves `DataTable::row_actions` of them).
    pub actions: Vec<RowAction>,
}

impl TableRow {
    pub fn new(cells: Vec<AnyElement>) -> Self {
        Self { cells, ..Self::default() }
    }

    /// A heading row.
    pub fn group(icon: Option<&'static str>, label: impl Into<SharedString>) -> Self {
        Self { group: Some(GroupHeader { icon, label: label.into(), count: None, collapsed: None }), ..Self::default() }
    }

    pub fn count(mut self, count: usize) -> Self {
        if let Some(g) = self.group.as_mut() {
            g.count = Some(count);
        }
        self
    }

    /// The heading folds: `collapsed` is its state now (the screen toggles it in `on_click`).
    pub fn collapsible(mut self, collapsed: bool) -> Self {
        if let Some(g) = self.group.as_mut() {
            g.collapsed = Some(collapsed);
        }
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Draggable by `id` (the label follows the pointer).
    pub fn draggable(mut self, id: u64, label: impl Into<SharedString>) -> Self {
        self.drag = Some(RowDrag { id, label: label.into() });
        self
    }

    /// The row stays among the rows of the same scope (a number the screen gives; the rows of one text box's paragraphs, say) when
    /// it is dragged in a `live_reorder` table, and a row without a scope does not stop among them: it goes past them.
    /// Something to do when the button goes down on the row (a screen that changes the rows when one is picked up).
    pub fn on_press(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(f));
        self
    }

    pub fn drag_scope(mut self, scope: u64) -> Self {
        self.scope = scope;
        self
    }

    /// Takes a dropped row above or below itself.
    pub fn drop_between(mut self, id: u64) -> Self {
        self.drop = Some((id, DropKind::Between));
        self
    }

    /// Takes a dropped row as a whole (a group row, or the placeholder of an empty group).
    pub fn drop_into(mut self, id: u64) -> Self {
        self.drop = Some((id, DropKind::Into));
        self
    }

    pub fn action(mut self, id: &'static str, icon: &'static str, tooltip: impl Into<SharedString>, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.actions.push(RowAction { wide_only: false, id, icon, tooltip: tooltip.into(), on_click: Rc::new(f) });
        self
    }

    /// A button that shows only when the table is wide enough (the room is not kept when it is narrow). Add it after the
    /// buttons that always show, and count it in `DataTable::wide_actions`.
    pub fn action_when_wide(mut self, id: &'static str, icon: &'static str, tooltip: impl Into<SharedString>, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.actions.push(RowAction { wide_only: true, id, icon, tooltip: tooltip.into(), on_click: Rc::new(f) });
        self
    }
}

pub(crate) type RowBuilder = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<TableRow>>;

pub(crate) struct Virtual {
    pub columns: Vec<TableColumn>,
    pub header: Vec<AnyElement>,
    pub state: TableState,
    pub count: usize,
    pub build: RowBuilder,
    pub empty: Option<AnyElement>,
    /// The drag and drop the table takes part in, and what it does with a drop on one of its rows.
    pub dnd: Option<(RowDnd, DropHandler)>,
    /// The most buttons a row has at its end (their room is kept in the heading row too, so the columns line up).
    pub row_actions: usize,
    /// How many of `row_actions` (the last ones) are `action_when_wide`.
    pub wide_actions: usize,
    /// A draggable row ends in the three-line grip (`drag_handle`) that says it can be dragged; the room is kept in every row and the heading row.
    pub drag_handle: bool,
    /// The dragged row's table reorders live (`DataTable::live_reorder`).
    pub live_reorder: bool,
    /// A table in a pane that is not filled by it (a list among other controls): as high as its rows, up to this many (`DataTable::fit_rows`).
    pub fit_rows: Option<usize>,
    /// No heading row (`DataTable::headerless`).
    pub headerless: bool,
}

fn eid(id: &str, part: &str) -> ElementId {
    SharedString::from(format!("{id}-{part}")).into()
}

fn cell_box(column: &TableColumn, width: f32) -> Div {
    let d = div().h_full().flex().items_center().overflow_hidden();
    match column.width {
        ColumnWidth::Flex => d.flex_1().min_w_0(),
        _ => d.w(px(width)).flex_none(),
    }
}

pub(crate) fn render(id: &'static str, height: f32, v: Virtual, editor: WeakEntity<SlideEditor>, header_bg: u32) -> Stateful<Div> {
    let Virtual { columns, header, state, count, build, empty, dnd, row_actions: all_actions, wide_actions, drag_handle, live_reorder, fit_rows, headerless } = v;
    let grip_width = if drag_handle { GRIP_WIDTH } else { 0. };
    let widths: Vec<f32> = columns.iter().map(|c| state.width(c)).collect();
    if let (Some((d, _)), true) = (dnd.as_ref(), live_reorder) {
        d.set_live(id, count, height);
    }
    // The buttons that show only in a wide table: wide when the flex column keeps `WIDE_FLEX_MIN` with everything in place.
    let fixed: f32 = widths.iter().sum::<f32>() + grip_width + 2. * 4.; // `px_1` of a row, both sides
    let wide = wide_actions > 0 && state.body_width.get() - fixed - all_actions as f32 * ACTION_SIZE >= WIDE_FLEX_MIN;
    let row_actions = if wide { all_actions } else { all_actions - wide_actions.min(all_actions) };
    let actions_width = row_actions as f32 * ACTION_SIZE;

    // The heading row: fixed. Each resizable column ends in a handle.
    let head_height = if headerless { 0. } else { height };
    let mut head = h_flex().id(eid(id, "head")).relative().w_full().h(px(head_height)).flex_none().overflow_hidden().px_1().items_center().bg(rgb(header_bg)).when(!headerless, |d| d.border_b_1().border_color(rgb(BORDER)));
    for ((column, width), cell) in columns.iter().zip(widths.iter()).zip(header) {
        let mut b = cell_box(column, *width);
        b = b.child(div().flex_1().min_w_0().child(cell));
        if let ColumnWidth::Resizable { min, max, .. } = column.width {
            let (cid, w0) = (column.id, *width);
            let st = state.clone();
            b = b.child(
                div()
                    .id(eid(id, cid))
                    .w(px(6.))
                    .h_full()
                    .flex_none()
                    .cursor_col_resize()
                    .child(div().mx_auto().w(px(1.)).h_full().bg(rgb(BORDER)))
                    .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, _, cx| {
                        st.columns.borrow_mut().drag = Some(Drag { id: cid, x0: f32::from(ev.position.x), w0, min, max });
                        cx.stop_propagation();
                    }),
            );
        }
        head = head.child(b);
    }
    if row_actions > 0 {
        head = head.child(div().w(px(actions_width)).h_full().flex_none());
    }
    if drag_handle {
        head = head.child(div().w(px(grip_width)).h_full().flex_none());
    }
    // The drag follows the pointer anywhere in the window.
    let (moving_state, up_state) = (state.clone(), state.clone());
    let (moving_editor, up_editor) = (editor.clone(), editor.clone());
    head = head.child(
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let (st, ed) = (moving_state.clone(), moving_editor.clone());
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let moved = {
                        let mut c = st.columns.borrow_mut();
                        let Some(d) = c.drag.as_ref() else { return };
                        let w = (d.w0 + f32::from(ev.position.x) - d.x0).clamp(d.min, d.max);
                        let id = d.id;
                        c.widths.insert(id, w);
                        true
                    };
                    if moved {
                        let _ = ed.update(cx, |_, cx| cx.notify());
                    }
                });
                let (st, ed) = (up_state.clone(), up_editor.clone());
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && st.columns.borrow_mut().drag.take().is_some() {
                        let _ = ed.update(cx, |_, cx| cx.notify());
                    }
                });
            },
        )
        .absolute()
        .size_0(),
    );

    // The drag of a row follows the pointer anywhere in the window, and ends where the button goes up.
    if let Some((dnd, handler)) = dnd.clone() {
        dnd.handle(id, handler);
        let (moving, up) = (dnd.clone(), dnd.clone());
        let (moving_editor, up_editor) = (editor.clone(), editor.clone());
        head = head.child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let (dnd, ed) = (moving.clone(), moving_editor.clone());
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture || !dnd.pressed() {
                            return;
                        }
                        if dnd.moved(ev.position) {
                            let _ = ed.update(cx, |_, cx| cx.notify());
                            cx.stop_propagation();
                        }
                    });
                    let (dnd, ed) = (up.clone(), up_editor.clone());
                    window.on_mouse_event(move |ev: &MouseUpEvent, phase, w, cx| {
                        if phase != DispatchPhase::Capture || ev.button != MouseButton::Left || !dnd.pressed() {
                            return;
                        }
                        let was_drag = dnd.dragging().is_some();
                        let drop = dnd.released();
                        let _ = ed.update(cx, |e, cx| {
                            if let Some((handler, event)) = drop {
                                handler(e, event, w, cx);
                            }
                            cx.notify();
                        });
                        if was_drag {
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        );
    }

    // The value rows: only the ones in view are made.
    let body = if let Some(empty) = empty.filter(|_| count == 0) {
        div().id(eid(id, "empty")).flex_1().min_h_0().w_full().child(empty)
    } else {
        let (built, total) = (state.built.clone(), state.largest_batch.clone());
        let row_columns = columns.clone();
        let row_widths = widths.clone();
        let row_dnd = dnd.as_ref().map(|(d, _)| d.clone());
        let list = uniform_list(eid(id, "rows"), count, move |range, window, cx| {
            *built.borrow_mut() = range.clone();
            if let Some(d) = row_dnd.as_ref() {
                d.forget(id);
            }
            let rows = build(range.clone(), window, cx);
            total.set(total.get().max(rows.len()));
            rows.into_iter()
                .enumerate()
                .map(|(n, row)| {
                    let i = range.start + n;
                    // A live reorder shows the move itself, no insertion line.
                    let hover = row_dnd.as_ref().filter(|_| !live_reorder).and_then(|d| d.hover()).filter(|(t, at, _)| *t == id && *at == i).map(|(_, _, place)| place);
                    let slide = row_dnd.as_ref().filter(|_| live_reorder).and_then(|d| d.live_shift(id, i));
                    let held = row_dnd.as_ref().zip(row.drag.as_ref()).is_some_and(|(d, drag)| d.carried() == Some((id, drag.id)));
                    // The rows may have changed since the press (a screen that folds rows when one is picked up): the held row's place is where it is now.
                    if let (true, Some(d)) = (held, row_dnd.as_ref()) {
                        d.rebase(id, i);
                    }
                    let dragged = row_dnd.as_ref().zip(row.drag.as_ref()).is_some_and(|(d, drag)| d.dragging() == Some((id, drag.id)));
                    let grouped = row.group.is_some();
                    let row_scope = row.scope;
                    let mut line = h_flex()
                        .id((id, i))
                        .relative()
                        .w_full()
                        .h(px(height))
                        .flex_none()
                        .px_1()
                        .items_center()
                        .cursor_pointer()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(if hover == Some(super::row_dnd::DropPlace::Into) {
                            DROP_TINT
                        } else if row.selected {
                            SELECTED_ROW_BG
                        } else if grouped {
                            GROUP_ROW_BG
                        } else {
                            0xffffff
                        }))
                        .when(dragged && !live_reorder, |d| d.opacity(0.45));
                    if !row.selected && hover != Some(super::row_dnd::DropPlace::Into) {
                        line = line.hover(|s| s.bg(rgb(0xf1f3f4)));
                    }
                    if let Some(g) = row.group {
                        line = line.child(group_cell(g));
                    } else {
                        for ((column, width), cell) in row_columns.iter().zip(row_widths.iter()).zip(row.cells) {
                            line = line.child(cell_box(column, *width).child(div().w_full().min_w_0().child(cell)));
                        }
                    }
                    if row.actions.iter().any(|a| wide || !a.wide_only) {
                        let mut actions = h_flex().h_full().flex_none().justify_end().items_center().w(px(actions_width)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
                        for a in row.actions.into_iter().filter(|a| wide || !a.wide_only) {
                            let click = a.on_click;
                            let tip = a.tooltip;
                            actions = actions.child(
                                div()
                                    .id((a.id, i))
                                    .size(px(ACTION_SIZE))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(crate::palette::shade(0.27, 0.08)))
                                    .relative()
                                    .child(icon(a.icon, 18., rgb(0x1f1f1f).into()))
                                    .children({
                                        // Scenario tests click the row's buttons where they were painted.
                                        #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
                                        {
                                            let key = format!("{id}-{}-{i}", a.id);
                                            Some(canvas(|_, _, _| (), move |b, _, _, _| crate::scenario::button_probe::record_bounds(&key, b)).absolute().inset_0())
                                        }
                                        #[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
                                        {
                                            None::<Div>
                                        }
                                    })
                                    .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                                    .on_click(move |ev, w, cx| {
                                        click(ev, w, cx);
                                        cx.stop_propagation();
                                    }),
                            );
                        }
                        line = line.child(actions);
                    } else if row_actions > 0 && !grouped {
                        line = line.child(div().w(px(actions_width)).h_full().flex_none());
                    }
                    if drag_handle && !grouped {
                        line = line.child(
                            div().w(px(grip_width)).h_full().flex_none().flex().items_center().justify_center().children(row.drag.is_some().then(|| icon("drag_handle", 18., rgb(TEXT_SECONDARY).into()))),
                        );
                    }
                    if let (Some(d), Some(drag)) = (row_dnd.clone(), row.drag) {
                        let label = drag.label;
                        let did = drag.id;
                        let scope = row.scope;
                        let on_press = row.on_press.clone();
                        line = line.on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                            d.press(id, did, i, scope, label.clone(), ev.position);
                            if let Some(f) = on_press.as_ref() {
                                f(window, cx);
                            }
                            // A live table lifts the row at once.
                            window.refresh();
                        });
                    }
                    // A live reorder registers the row where the list puts it (the slot), not where the move has taken it: the
                    // row that slides is inside the slot.
                    let mut slot_register = None;
                    if let (Some(d), Some((drop_id, kind))) = (row_dnd.clone(), row.drop) {
                        if live_reorder {
                            slot_register = Some((d, drop_id, kind));
                        } else {
                            line = line.child(
                                canvas(|_, _, _| (), move |bounds, _, _, _| d.register(id, i, drop_id, kind, row_scope, bounds)).absolute().inset_0(),
                            );
                        }
                    }
                    match hover {
                        Some(super::row_dnd::DropPlace::Before) => {
                            line = line.child(div().absolute().left_0().right_0().top(px(-1.)).h(px(2.)).bg(rgb(PRIMARY_BUTTON_BG)));
                        }
                        Some(super::row_dnd::DropPlace::After) => {
                            line = line.child(div().absolute().left_0().right_0().bottom(px(-1.)).h(px(2.)).bg(rgb(PRIMARY_BUTTON_BG)));
                        }
                        _ => {}
                    }
                    if let Some(click) = row.on_click {
                        line = line.on_click(move |ev, w, cx| click(ev, w, cx));
                    }
                    if !live_reorder {
                        return line.into_any_element();
                    }
                    let mut slot = div().relative().w_full().h(px(height)).flex_none();
                    if let Some((d, drop_id, kind)) = slot_register {
                        slot = slot.child(canvas(|_, _, _| (), move |bounds, _, _, _| d.register(id, i, drop_id, kind, row_scope, bounds)).absolute().inset_0());
                    }
                    // The dragged row is carried by the pointer, in front of the rows (deferred), with the lifted row's shadow
                    // (what `reorderable` tables draw); the others slide aside.
                    let lift = if held { row_dnd.as_ref().and_then(|d| d.lift_offset(id, i)) } else { None };
                    let moving = match slide {
                        _ if lift.is_some() => deferred(line.top(px(lift.unwrap_or(0.))).shadow(super::data_table::lifted_row_shadow())).with_priority(1).into_any_element(),
                        Some((from, to, moves)) if from != to => line
                            .with_animation(
                                SharedString::from(format!("{id}-slide-{i}-{moves}")),
                                Animation::new(std::time::Duration::from_millis(super::data_table::REORDER_MOVE_MS)).with_easing(super::data_table::reorder_ease),
                                move |row, t| row.top(px((from as f32 + (to - from) as f32 * t) * height)),
                            )
                            .into_any_element(),
                        Some((_, to, _)) => line.top(px(to as f32 * height)).into_any_element(),
                        None => line.into_any_element(),
                    };
                    slot.child(moving).into_any_element()
                })
                .collect()
        })
        .track_scroll(&state.scroll)
        .size_full();
        let (measured, seen, ed) = (state.body_width.clone(), wide, editor.clone());
        let measure = canvas(|_, _, _| (), move |b, _, _, cx| {
            let w = f32::from(b.size.width);
            if (measured.get() - w).abs() > 0.5 {
                measured.set(w);
                // Whether the wide buttons show may have changed with the width: lay out again.
                let fixed_now = fixed + all_actions as f32 * ACTION_SIZE;
                if wide_actions > 0 && ((w - fixed_now >= WIDE_FLEX_MIN) != seen) {
                    let ed = ed.clone();
                    cx.defer(move |cx| { let _ = ed.update(cx, |_, cx| cx.notify()); });
                }
            }
        }).absolute().size_full();
        div().id(eid(id, "body")).relative().flex_1().min_h_0().w_full().child(list).child(measure).child(always_visible_scrollbar_overlay(eid(id, "scrollbar"), state.scroll.0.borrow().base_handle.clone(), None))
    };

    // A live reorder carries the row itself: no label under the pointer.
    let ghost = dnd.as_ref().filter(|_| !live_reorder).and_then(|(d, _)| d.ghost(id)).map(|(label, at)| super::row_dnd::ghost_element(label, at));
    // A list among other controls is as high as its rows (the heading row, the rows up to `fit_rows`, the frame's two lines).
    let surface = table_surface().id(id);
    let surface = match fit_rows {
        Some(most) => surface.h(px(head_height + height * count.clamp(1, most.max(1)) as f32 + 2.)).flex_none(),
        None => surface.h_full().min_h_0(),
    };
    surface.child(head).child(body).children(ghost)
}

/// The tint of a row that takes the drop being dragged over it.
const DROP_TINT: u32 = 0xe6f0fb;

/// A group row's one cell: the fold arrow (when it folds), the icon, the label in bold, the count.
fn group_cell(g: GroupHeader) -> AnyElement {
    let mut cell = h_flex().w_full().h_full().min_w_0().gap(px(6.)).items_center().px_1();
    if let Some(collapsed) = g.collapsed {
        cell = cell.child(icon(if collapsed { "chevron_right" } else { "expand_more" }, 18., rgb(TEXT_SECONDARY).into()));
    }
    if let Some(name) = g.icon {
        cell = cell.child(icon(name, 16., rgb(TEXT_SECONDARY).into()));
    }
    cell = cell.child(div().min_w_0().truncate().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(TEXT)).child(g.label));
    if let Some(n) = g.count {
        cell = cell.child(div().flex_none().text_size(px(12.)).text_color(rgb(TEXT_SECONDARY)).child(SharedString::from(n.to_string())));
    }
    cell.into_any_element()
}
