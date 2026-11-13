// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Optional pointer-reorder behavior shared by editable data tables.
use super::*;
pub(crate) fn table_surface() -> Div {
    v_flex()
        .w_full()
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_md()
        .overflow_hidden()
}
pub const TABLE_ROW_HEIGHT: f32 = 36.;
/// How long a row takes to move aside for a dragged row (`live_reorder`).
pub(crate) const REORDER_MOVE_MS: u64 = 160;
/// The easing of that move (ease-out, cubic).
pub(crate) fn reorder_ease(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}
/// The shadow of a row lifted by the pointer (`live_reorder`): one above and
/// one below the row, so it stands out from the rows it floats over.
pub(crate) fn lifted_row_shadow() -> Vec<BoxShadow> {
    [-3., 3.]
        .into_iter()
        .map(|dy| BoxShadow { color: gpui_kit::black().opacity(0.18), offset: point(px(0.), px(dy)), blur_radius: px(6.), spread_radius: px(0.), inset: false })
        .collect()
}
/// Where a row carried by the pointer stands among the rows it may take the places of (`lo..=hi`): it passes a neighbour when
/// the centre of the carried row (`center`) passes the neighbour's centre. `at` is where it stood, `top_of(k)` the top of the
/// place of row `k`, `height` the one height of the rows. The live reorder of `RowDnd` (`live_reorder`).
pub(crate) fn reorder_step(mut at: usize, lo: usize, hi: usize, center: f32, height: f32, top_of: impl Fn(usize) -> f32) -> usize {
    while at < hi && center >= top_of(at + 1) + height / 2. {
        at += 1;
    }
    while at > lo && center <= top_of(at - 1) + height / 2. {
        at -= 1;
    }
    at
}
/// The heading row's background in every table (a light grey); `DataTable::header_bg` overrides it.
pub(crate) const TABLE_HEADER_BG: u32 = 0xf3f4f6;
/// What a screen keeps for one reorderable list (`DataTable::reorder_list`): the table's scroll and the drag of its rows. Cloning shares them.
#[derive(Clone, Default)]
pub struct ListState {
    /// The table's id (set when the table is made).
    id: Rc<std::cell::Cell<&'static str>>,
    pub table: super::virtual_table::TableState,
    pub dnd: super::row_dnd::RowDnd,
}
impl ListState {
    /// The bounds row `index` was last painted at.
    pub fn row_bounds(&self, index: usize) -> Option<Bounds<Pixels>> {
        self.dnd.row_bounds(self.id.get(), index)
    }
    /// Give up a drag in progress (Escape). Whether there was one.
    #[allow(dead_code)] // Part of the list's drag API; the window's Escape handling does not call it yet.
    pub(crate) fn cancel(&self) -> bool {
        self.dnd.cancel()
    }
    /// The id of the row that is dragged (or held).
    pub fn dragging(&self) -> Option<u64> {
        self.dnd.carried().map(|(_, id)| id)
    }
    /// How far below its own place the dragged row (row `index`) is carried.
    pub fn lift_offset(&self, index: usize) -> Option<f32> {
        self.dnd.lift_offset(self.id.get(), index)
    }
}
/// A control in a row that has its own click: pressing it does not begin to drag the row.
pub fn no_drag(child: impl IntoElement) -> Div {
    div().child(child).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}
#[derive(IntoElement)]
pub struct DataTable {
    id: &'static str,
    header: Option<AnyElement>,
    rows: Vec<(usize, AnyElement, bool)>,
    height: f32,
    editor: WeakEntity<SlideEditor>,
    virt: Option<super::virtual_table::Virtual>,
    header_bg: u32,
}
impl DataTable {
    pub fn new(
        id: &'static str,
        header: Option<AnyElement>,
        rows: Vec<(usize, AnyElement, bool)>,
        cx: &Context<SlideEditor>,
    ) -> Self {
        Self {
            id,
            header,
            rows,
            height: TABLE_ROW_HEIGHT,
            editor: cx.weak_entity(),
            virt: None,
            header_bg: TABLE_HEADER_BG,
        }
    }
    /// The scrolling table of a pane with one table (see `virtual_table.rs`): the heading row stays, only the value
    /// rows scroll and have a scrollbar, and `build` is asked for the visible rows alone. `header` has one element
    /// per column; `build` returns one `TableRow` per row of the range, each with one element per column.
    pub fn virtualized(
        id: &'static str,
        columns: Vec<super::virtual_table::TableColumn>,
        header: Vec<AnyElement>,
        state: super::virtual_table::TableState,
        count: usize,
        build: impl Fn(std::ops::Range<usize>, &mut Window, &mut App) -> Vec<super::virtual_table::TableRow> + 'static,
        cx: &Context<SlideEditor>,
    ) -> Self {
        let mut table = Self::new(id, None, Vec::new(), cx);
        table.virt = Some(super::virtual_table::Virtual { columns, header, state, count, build: Rc::new(build), empty: None, dnd: None, row_actions: 0, wide_actions: 0, drag_handle: false, live_reorder: false, fit_rows: None, headerless: false });
        table
    }
    /// The heading row's background, when it must differ from the app-wide `TABLE_HEADER_BG`.
    #[allow(dead_code)] // Shared-control option; no table has another heading colour yet.
    pub(crate) fn header_bg(mut self, color: u32) -> Self {
        self.header_bg = color;
        self
    }
    /// What the scrolling table shows in place of the rows when there are none.
    pub fn empty(mut self, element: AnyElement) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.empty = Some(element);
        }
        self
    }
    /// Rows are dragged between this table and the others that share `dnd`, and `on_drop` is told what fell on a row
    /// of this table (the ids the rows were given with `TableRow::draggable`, `drop_between`, `drop_into`).
    pub fn row_dnd(
        mut self,
        dnd: super::row_dnd::RowDnd,
        on_drop: impl Fn(&mut SlideEditor, super::row_dnd::RowDropEvent, &mut Window, &mut Context<SlideEditor>) + 'static,
    ) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.dnd = Some((dnd, Rc::new(on_drop)));
        }
        self
    }

    /// The rows of this table dragged by `row_dnd` reorder live, as `reorderable` tables do: the rows between the dragged
    /// row's place and the place it would take slide aside (with the table's one move animation) and the dragged row goes
    /// to that place, instead of an insertion line. For a table whose rows only change places among themselves (every
    /// row `drop_between`, no group rows).
    pub fn live_reorder(mut self) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.live_reorder = true;
        }
        self
    }

    /// The most buttons (`TableRow::action`) a row has at its end; the room is kept in every row and the heading row.
    pub fn row_actions(mut self, most: usize) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.row_actions = most;
        }
        self
    }

    /// How many of the `row_actions` (the last ones) are `TableRow::action_when_wide`: they show only when the table is wide.
    pub fn wide_actions(mut self, count: usize) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.wide_actions = count;
        }
        self
    }

    /// A draggable row (`TableRow::draggable`) ends in the three-line grip that says it can be dragged (after the row's
    /// buttons). The room is kept in every row and the heading row, so the columns line up.
    pub fn drag_handles(mut self) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.drag_handle = true;
        }
        self
    }

    /// The table stands among other controls of a pane (it does not fill the pane): as high as its rows, up to `most` of them, then it scrolls.
    pub fn fit_rows(mut self, most: usize) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.fit_rows = Some(most);
        }
        self
    }

    /// No heading row.
    pub fn headerless(mut self) -> Self {
        if let Some(v) = self.virt.as_mut() {
            v.headerless = true;
        }
        self
    }

    /// A list the user reorders by dragging its rows (the series of a chart, the objects of a slide...): the shared scrolling table
    /// with its lifted row, shadow and live move (`live_reorder`). `build` makes the rows of the visible range (each `TableRow` with its
    /// own `draggable(id, label)` and `drop_between(id)`); `commit` is told the id of the row that was moved and the id of the row whose
    /// place it took (the row that stood where the moved row now is).
    pub fn reorder_list(
        id: &'static str,
        columns: Vec<super::virtual_table::TableColumn>,
        header: Vec<AnyElement>,
        list: &ListState,
        count: usize,
        build: impl Fn(std::ops::Range<usize>, &mut Window, &mut App) -> Vec<super::virtual_table::TableRow> + 'static,
        commit: impl Fn(&mut SlideEditor, usize, usize, &mut Window, &mut Context<SlideEditor>) + 'static,
        cx: &Context<SlideEditor>,
    ) -> Self {
        list.id.set(id);
        Self::virtualized(id, columns, header, list.table.clone(), count, build, cx)
            .drag_handles()
            .live_reorder()
            .row_dnd(list.dnd.clone(), move |e, ev, w, cx| commit(e, ev.from as usize, ev.target as usize, w, cx))
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}
impl RenderOnce for DataTable {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(v) = self.virt {
            if let Some((dnd, _)) = v.dnd.as_ref() {
                let (dnd, editor) = (dnd.clone(), self.editor.clone());
                cx.defer(move |cx| {
                    let _ = editor.update(cx, |e, _| e.active_row_dnd = Some(dnd));
                });
            }
            return super::virtual_table::render(self.id, self.height, v, self.editor, self.header_bg);
        }
        let mut surface = table_surface().id(self.id);
        if let Some(header) = self.header {
            surface = surface.child(
                div()
                    .h(px(self.height))
                    .min_h(px(self.height))
                    .flex_none()
                    .bg(rgb(self.header_bg))
                    .overflow_hidden()
                    .child(header),
            );
        }
        surface.children(self.rows.into_iter().map(|(_, row, _)| div().h(px(self.height)).child(row)))
    }
}
