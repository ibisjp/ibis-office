// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pivot table's field pane (ピボットテーブルのフィールド) as two tables: above, the fields (search, a check box
//! that places the field, drag source, only the rows in view are made); below, the layout (フィルター / 列 / 行 / 値 as
//! group rows with their fields under them; fields are dragged in, between and out). A value field's settings are a
//! dialog of their own (集計方法 / 計算の種類) over the pane, and so are a row or column field's.
//! Both tables are the shared `DataTable::virtualized`; the drag and drop is `RowDnd`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{PivotAxis, PivotTable, ShapeKind, SlideId};

use crate::shell::chrome::data_table::DataTable;
use crate::shell::chrome::row_dnd::{DropPlace, RowDnd, RowDropEvent};
use crate::shell::chrome::virtual_table::{TableColumn, TableRow, TableState};
use crate::shell::chrome::{self, Dropdown, DropdownItem, SidebarPanel, TEXT_SECONDARY};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use crate::shell::palette::rgb;
use crate::ext::*;

const FIELDS_TABLE: &str = "pivot-fields-table";
const LAYOUT_TABLE: &str = "pivot-layout-table";
const ROW_HEIGHT: f32 = 30.;
/// The most layout rows shown before the layout table scrolls.
const LAYOUT_VISIBLE_ROWS: usize = 14;

/// The areas in the order the layout table lists them.
const AREAS: [(PivotAxis, &str, &str); 4] = [
    (PivotAxis::Page, "_xlsx.filters", "filter_alt"),
    (PivotAxis::Column, "_xlsx.columns", "view_column"),
    (PivotAxis::Row, "_xlsx.rows", "table_rows"),
    (PivotAxis::Values, "_xlsx.values", "functions"),
];

fn area_index(axis: PivotAxis) -> usize {
    AREAS.iter().position(|a| a.0 == axis).unwrap_or(0)
}

fn item_id(area: usize, pos: usize) -> u64 {
    ((area as u64) << 32) | pos as u64
}
fn group_id(area: usize) -> u64 {
    ((area as u64) << 32) | 0xFFFF_FFFF
}
/// The area and the place in it (`None`: the group row itself).
fn split_id(id: u64) -> (usize, Option<usize>) {
    let low = (id & 0xFFFF_FFFF) as usize;
    ((id >> 32) as usize, (low != 0xFFFF_FFFF).then_some(low))
}

/// The function codes and the ids of their names.
pub(crate) fn function_label_id(function: &str) -> &'static str {
    match function {
        "count" => "xlsx.pivot-value.fn.count",
        "average" => "xlsx.pivot-value.fn.average",
        "max" => "xlsx.pivot-value.fn.max",
        "min" => "xlsx.pivot-value.fn.min",
        "product" => "xlsx.pivot-value.fn.product",
        "countNums" => "xlsx.pivot-value.fn.count-numbers",
        "stdDev" => "xlsx.pivot-value.fn.stddev",
        "stdDevp" => "xlsx.pivot-value.fn.stddevp",
        "var" => "xlsx.pivot-value.fn.var",
        "varp" => "xlsx.pivot-value.fn.varp",
        _ => "xlsx.pivot-value.fn.sum",
    }
}

const NUMBER_FORMATS: &[(&str, Option<&str>)] = &[
    ("_xlsx.general", None),
    ("_xlsx.number", Some("#,##0")),
    ("_xlsx.number-2-decimals", Some("#,##0.00")),
    ("_xlsx.currency", Some("\"¥\"#,##0;\"¥\"\\-#,##0")),
    ("_xlsx.percentage", Some("0.0%")),
    ("_xlsx.percentage-2-decimals", Some("0.00%")),
];

const SHOW_AS: &[(&str, Option<&str>)] = &[
    ("_xlsx.no-calculation", None),
    ("_xlsx.of-grand-total", Some("percentOfTotal")),
    ("_xlsx.of-column-total", Some("percentOfCol")),
    ("_xlsx.of-row-total", Some("percentOfRow")),
];

/// What the value field dialog has been given but not yet applied (OK applies it, キャンセル drops it).
#[derive(Clone)]
pub(crate) struct ValueDraft {
    pub function: String,
    pub show_as: Option<String>,
    pub number_format: Option<String>,
    /// The name the dialog opened with (an unchanged name follows the function, as Excel's does).
    pub initial_name: String,
}

/// What the field pane keeps: the two tables, the search, the drag and drop, and the dialogs' state.
pub(crate) struct FieldsUi {
    pub search: Entity<InputState>,
    pub(crate) fields_table: TableState,
    pub(crate) layout_table: TableState,
    pub dnd: RowDnd,
    pub selected_field: Option<usize>,
    /// The selected layout row: the area (index into `AREAS`) and the place in it.
    pub(crate) selected_item: Option<(usize, usize)>,
    /// Whether a field holds only numbers (found for the fields in view only).
    kinds: Rc<RefCell<BTreeMap<usize, bool>>>,
    pub(crate) value_tab: usize,
    pub draft: Option<ValueDraft>,
    _search: Subscription,
}

impl FieldsUi {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let placeholder = text("xlsx.pivot-fields.search-placeholder");
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let fields_table = TableState::default();
        let reset = fields_table.clone();
        let subscription = cx.subscribe_in(&search, window, move |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                reset.scroll_to(0);
                cx.notify();
            }
        });
        Self {
            search,
            fields_table,
            layout_table: TableState::default(),
            dnd: RowDnd::default(),
            selected_field: None,
            selected_item: None,
            kinds: Rc::default(),
            value_tab: 0,
            draft: None,
            _search: subscription,
        }
    }
}

/// A row of the layout table.
enum Layout {
    Group { area: usize, count: usize },
    /// A field placed in an area (`field`: `None` for the place of the values among the columns).
    Item { area: usize, pos: usize, field: Option<usize>, name: String, function: Option<String> },
    Empty { area: usize },
}

fn layout_rows(p: &PivotTable) -> Vec<Layout> {
    let mut rows = Vec::new();
    for (area, (axis, _, _)) in AREAS.iter().enumerate() {
        let items: Vec<(Option<usize>, String, Option<String>)> = match axis {
            PivotAxis::Page => p.page_fields.iter().map(|f| (Some(f.field as usize), p.fields[f.field as usize].name.clone(), None)).collect(),
            PivotAxis::Column | PivotAxis::Row => {
                let list = if *axis == PivotAxis::Row { &p.row_fields } else { &p.col_fields };
                list.iter().map(|&f| if f < 0 { (None, text("_xlsx.values-2").to_string(), None) } else { (Some(f as usize), p.fields[f as usize].name.clone(), None) }).collect()
            }
            PivotAxis::Values => p.data_fields.iter().map(|d| (Some(d.field as usize), p.fields[d.field as usize].name.clone(), Some(d.function.clone()))).collect(),
        };
        rows.push(Layout::Group { area, count: items.len() });
        if items.is_empty() {
            rows.push(Layout::Empty { area });
        }
        for (pos, (field, name, function)) in items.into_iter().enumerate() {
            rows.push(Layout::Item { area, pos, field, name, function });
        }
    }
    rows
}

/// The field placed at `pos` of area `area`.
fn field_at(p: &PivotTable, area: usize, pos: usize) -> Option<usize> {
    match AREAS[area].0 {
        PivotAxis::Page => p.page_fields.get(pos).map(|f| f.field as usize),
        PivotAxis::Column => p.col_fields.get(pos).copied().filter(|f| *f >= 0).map(|f| f as usize),
        PivotAxis::Row => p.row_fields.get(pos).copied().filter(|f| *f >= 0).map(|f| f as usize),
        PivotAxis::Values => p.data_fields.get(pos).map(|d| d.field as usize),
    }
}

fn badge_colors(axis: PivotAxis) -> u32 {
    match axis {
        PivotAxis::Page => 0xfde7c8,
        PivotAxis::Column => 0xd9ecfb,
        PivotAxis::Row => 0xdff1e2,
        PivotAxis::Values => 0xece0f7,
    }
}

fn heading(label: SharedString) -> AnyElement {
    div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(0x3c4043)).child(label).into_any_element()
}

/// `name` with the part the search matched marked.
fn highlighted(name: &str, needle: &str) -> AnyElement {
    let plain = || div().whitespace_nowrap().truncate().child(SharedString::from(name.to_string())).into_any_element();
    if needle.is_empty() {
        return plain();
    }
    let (lower, n) = (name.to_lowercase(), needle.to_lowercase());
    match lower.find(&n) {
        Some(at) if lower.len() == name.len() && name.is_char_boundary(at) && name.is_char_boundary(at + n.len()) => h_flex()
            .whitespace_nowrap()
            .child(SharedString::from(name[..at].to_string()))
            .child(div().bg(rgb(0xfff0a0)).child(SharedString::from(name[at..at + n.len()].to_string())))
            .child(SharedString::from(name[at + n.len()..].to_string()))
            .into_any_element(),
        _ => plain(),
    }
}

#[ibis_office_macros::editor_ext(PivotFields)]
impl SlideEditor {
    fn pivot_ref(&self, target: (SlideId, usize)) -> Option<&PivotTable> {
        self.presentation().slide(target.0)?.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1) } else { None })
    }

    fn pivot_ui_mut(&mut self) -> Option<&mut FieldsUi> {
        self.wb_mut().panels.pivot.as_mut()?.ui.as_mut()
    }

    /// What the pane shows: the two tables, or the dialog of a value (or a row/column) field over them.
    pub(crate) fn pivot_fields_dispatch(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.pivot.as_ref()?;
        p.ui.as_ref()?;
        let (value, field) = (p.value, p.field_selected);
        if value.is_some() {
            return self.pivot_value_dialog_panel(cx).map(|d| d.modal_named("pivot_value_dialog"));
        }
        if field.is_some() {
            return self.pivot_field_dialog_panel(cx).map(|d| d.modal_named("pivot_field_dialog"));
        }
        self.pivot_fields_list_panel(cx)
    }

    /// The pane under a dialog (the dialog keeps it on screen).
    pub(crate) fn pivot_fields_background(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.pivot_fields_list_panel(cx)
    }

    /// The dialog is gone: its state with it, the pane stays.
    pub(crate) fn pivot_dialog_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
            p.value = None;
            p.value_name = None;
            p.field_selected = None;
            if let Some(ui) = p.ui.as_mut() {
                ui.draft = None;
                ui.value_tab = 0;
            }
        }
        self.dialog.active = false;
        self.dialog.overlay = None;
        self.dialog.key = None;
        self.dialog.bounds = None;
        window.release_pointer();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// A value field's dialog (the pencil of its row, a double click, データ > 値フィールドの設定).
    pub(crate) fn open_pivot_value_dialog(&mut self, pos: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) else { return };
        let Some(df) = self.pivot_ref(target).and_then(|p| p.data_fields.get(pos).cloned()) else { return };
        let name = cx.new(|cx| InputState::new(window, cx));
        name.update(cx, |i, cx| i.set_value(df.name.clone(), window, cx));
        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
            p.value = Some(pos);
            p.value_name = Some(name);
            p.field_selected = None;
            if let Some(ui) = p.ui.as_mut() {
                ui.value_tab = 0;
                ui.draft = Some(ValueDraft { function: df.function.clone(), show_as: df.show_as.clone(), number_format: df.number_format.clone(), initial_name: df.name });
            }
        }
        self.dialog.bounds = None;
        cx.notify();
    }

    /// A row or column field's dialog (小計・レイアウト).
    pub fn open_pivot_field_dialog(&mut self, area: usize, pos: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
            p.value = None;
            p.value_name = None;
            p.field_selected = Some((AREAS[area].0, pos));
        }
        self.dialog.bounds = None;
        cx.notify();
    }

    pub(crate) fn pivot_value_dialog_ok(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, pos, draft, name)) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| {
            Some((p.target?, p.value?, p.ui.as_ref()?.draft.clone()?, p.value_name.as_ref().map(|n| n.read(cx).value().to_string()).unwrap_or_default()))
        }) else { return };
        self.change_pivot(target, |p| {
            p.set_function(pos, &draft.function);
            if name != draft.initial_name {
                p.set_data_field_name(pos, &name);
            }
            p.set_number_format(pos, draft.number_format.clone());
            p.set_show_as(pos, draft.show_as.clone());
        }, cx);
        self.pivot_dialog_closed(window, cx);
    }

    pub(crate) fn set_draft(&mut self, f: impl FnOnce(&mut ValueDraft)) {
        if let Some(d) = self.pivot_ui_mut().and_then(|u| u.draft.as_mut()) {
            f(d);
        }
    }

    // ----- The pane -------------------------------------------------------------------------------------------

    /// What a drop on the layout table (or on the field table: out of the layout) does.
    fn pivot_row_dropped(&mut self, event: RowDropEvent, cx: &mut Context<Self>) {
        let Some(target) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) else { return };
        let Some(p) = self.pivot_ref(target) else { return };
        let from_field = if event.from_table == FIELDS_TABLE {
            Some((event.from as usize, None))
        } else {
            let (sa, sp) = split_id(event.from);
            sp.and_then(|sp| field_at(p, sa, sp).map(|f| (f, Some((sa, sp)))))
        };
        let Some((field, source)) = from_field else { return };
        if event.target_table == FIELDS_TABLE {
            // Dropped on the list of fields: out of the table.
            if source.is_some() {
                self.change_pivot(target, |p| p.place(field, None, 0), cx);
            }
            return;
        }
        let (ta, tpos) = split_id(event.target);
        let mut at = match (event.place, tpos) {
            (DropPlace::Before, Some(i)) => i,
            (DropPlace::After, Some(i)) => i + 1,
            _ => usize::MAX,
        };
        if let Some((sa, sp)) = source {
            if sa == ta && at != usize::MAX && sp < at {
                at -= 1;
            }
        }
        let axis = AREAS[ta].0;
        self.change_pivot(target, move |p| p.place(field, Some(axis), at), cx);
        if let Some(ui) = self.pivot_ui_mut() {
            ui.selected_item = None;
        }
    }

    fn pivot_fields_list_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (target, ui_parts) = {
            let p = self.sheet_state()?.panels.pivot.as_ref()?;
            let ui = p.ui.as_ref()?;
            (p.target?, (ui.search.clone(), ui.fields_table.clone(), ui.layout_table.clone(), ui.dnd.clone(), ui.selected_field, ui.selected_item, ui.kinds.clone()))
        };
        let (search, fields_table, layout_table, dnd, selected_field, selected_item, kinds) = ui_parts;
        let needle = search.read(cx).value().trim().to_string();
        let lower = needle.to_lowercase();
        let (names, areas, layout, total): (Vec<String>, Vec<Option<PivotAxis>>, Vec<Layout>, usize) = {
            let p = self.pivot_ref(target)?;
            (p.fields.iter().map(|f| f.name.clone()).collect(), (0..p.fields.len()).map(|i| p.place_of(i).map(|(a, _)| a)).collect(), layout_rows(p), p.fields.len())
        };
        let shown: Vec<usize> = (0..total).filter(|&i| lower.is_empty() || names[i].to_lowercase().contains(&lower)).collect();
        // Whether a field holds only numbers is worked out for the rows in view, once.
        {
            let window = fields_table.window(ROW_HEIGHT, shown.len());
            let missing: Vec<usize> = shown[window].iter().copied().filter(|i| !kinds.borrow().contains_key(i)).collect();
            if !missing.is_empty() {
                let found: Vec<(usize, bool)> = {
                    let p = self.pivot_ref(target)?;
                    missing.iter().map(|&i| (i, p.fields[i].shared.iter().filter(|v| !matches!(v, ibis_doc_model::CellValue::Empty)).all(|v| matches!(v, ibis_doc_model::CellValue::Number(_))) && !p.fields[i].shared.is_empty())).collect()
                };
                kinds.borrow_mut().extend(found);
            }
        }
        let editor = cx.entity().downgrade();
        let shown = Rc::new(shown);
        let names = Rc::new(names);
        let areas = Rc::new(areas);

        // The count under the search.
        let count_text = if lower.is_empty() {
            format("xlsx.pivot-fields.count-all", &[("n", &total.to_string())])
        } else {
            format("xlsx.pivot-fields.count-found", &[("n", &shown.len().to_string()), ("total", &total.to_string())])
        };
        let top = v_flex()
            .gap_1()
            .child(chrome::text_field(&search).full_width())
            .child(h_flex().justify_between().text_size(px(12.)).text_color(rgb(TEXT_SECONDARY)).child(count_text).child(text("xlsx.pivot-fields.hint")));

        // The fields.
        let fields_columns = vec![TableColumn::fixed("check", 28.), TableColumn::fixed("type", 26.), TableColumn::flex("name"), TableColumn::fixed("area", 72.)];
        let fields_header = vec![div().into_any_element(), div().into_any_element(), heading(text("xlsx.pivot-fields.column-field")), heading(text("xlsx.pivot-fields.column-area"))];
        let fields_build = {
            let (shown, names, areas, editor, kinds, needle) = (shown.clone(), names.clone(), areas.clone(), editor.clone(), kinds.clone(), needle.clone());
            move |range: Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|n| shown.get(n).map(|&f| (n, f)))
                    .map(|(n, f)| {
                        let area = areas[f];
                        let numbers = kinds.borrow().get(&f).copied().unwrap_or(false);
                        let (toggle_editor, click_editor) = (editor.clone(), editor.clone());
                        let check = div()
                            .id(("pivot-field-check", n))
                            .w_full()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(chrome::checkbox_box(area.is_some()))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                let _ = toggle_editor.update(cx, |e, cx| {
                                    if let Some(t) = e.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) {
                                        e.toggle_pivot_field(t, f, cx);
                                    }
                                });
                                cx.stop_propagation();
                            });
                        let badge = area.map(|a| {
                            let label = text(AREAS[area_index(a)].1);
                            div().px(px(7.)).rounded_full().bg(rgb(badge_colors(a))).text_size(px(11.)).text_color(rgb(0x3c4043)).child(label)
                        });
                        let cells = vec![
                            check.into_any_element(),
                            div().w_full().flex().justify_center().child(chrome::icon(if numbers { "numbers" } else { "abc" }, 16., rgb(0x80868b).into())).into_any_element(),
                            highlighted(&names[f], &needle),
                            h_flex().children(badge).into_any_element(),
                        ];
                        TableRow::new(cells)
                            .selected(selected_field == Some(f))
                            .on_click(move |_, _, cx| {
                                let _ = click_editor.update(cx, |e, cx| {
                                    if let Some(u) = e.pivot_ui_mut() {
                                        u.selected_field = Some(f);
                                    }
                                    cx.notify();
                                });
                            })
                            .draggable(f as u64, SharedString::from(names[f].clone()))
                            .drop_into(f as u64)
                    })
                    .collect()
            }
        };
        let fields_empty = div().w_full().py_4().flex().justify_center().text_sm().text_color(rgb(TEXT_SECONDARY)).child(text("xlsx.pivot-fields.no-match"));
        let fields_element = DataTable::virtualized(FIELDS_TABLE, fields_columns, fields_header, fields_table, shown.len(), fields_build, cx)
            .row_height(ROW_HEIGHT)
            .empty(fields_empty.into_any_element())
            .row_dnd(dnd.clone(), |e, ev, _, cx| e.pivot_row_dropped(ev, cx));

        // The layout.
        let layout = Rc::new(layout);
        let layout_columns = vec![TableColumn::fixed("grip", 24.), TableColumn::flex("name"), TableColumn::fixed("summary", 76.)];
        let layout_header = vec![
            div().into_any_element(),
            h_flex().gap_2().items_center().child(heading(text("xlsx.pivot-fields.layout-title"))).child(div().text_size(px(11.)).text_color(rgb(TEXT_SECONDARY)).child(text("xlsx.pivot-fields.layout-note"))).into_any_element(),
            div().into_any_element(),
        ];
        let layout_build = {
            let (layout, editor) = (layout.clone(), editor.clone());
            move |range: Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| layout.get(i))
                    .map(|row| match row {
                        Layout::Group { area, count } => TableRow::group(Some(AREAS[*area].2), text(AREAS[*area].1)).count(*count).drop_into(group_id(*area)),
                        Layout::Empty { area } => TableRow::new(vec![
                            div().into_any_element(),
                            div().w_full().h(px(22.)).flex().items_center().px_2().border_1().border_color(rgb(0xb9bec7)).rounded(px(4.)).text_size(px(12.)).text_color(rgb(0x80868b)).child(text("xlsx.pivot-fields.drop-here")).into_any_element(),
                            div().into_any_element(),
                        ])
                        .drop_into(group_id(*area)),
                        Layout::Item { area, pos, field, name, function } => {
                            let (area, pos, field) = (*area, *pos, *field);
                            let axis = AREAS[area].0;
                            let chip = function.as_ref().map(|f| div().px(px(5.)).rounded(px(3.)).bg(rgb(0xece0f7)).text_size(px(11.)).text_color(rgb(0x5b3d85)).child(text(function_label_id(f))));
                            let cells = vec![
                                div().w_full().flex().justify_center().child(chrome::icon("drag_indicator", 16., rgb(0x9aa0a6).into())).into_any_element(),
                                div().w_full().min_w_0().truncate().child(SharedString::from(name.clone())).into_any_element(),
                                h_flex().children(chip).into_any_element(),
                            ];
                            let mut r = TableRow::new(cells).selected(false).drop_between(item_id(area, pos));
                            if field.is_some() {
                                r = r.draggable(item_id(area, pos), SharedString::from(name.clone()));
                                let click_editor = editor.clone();
                                r = r.on_click(move |ev, w, cx| {
                                    let clicks = ev.click_count();
                                    let _ = click_editor.update(cx, |e, cx| {
                                        if let Some(u) = e.pivot_ui_mut() {
                                            u.selected_item = Some((area, pos));
                                        }
                                        if clicks >= 2 && axis == PivotAxis::Values {
                                            e.open_pivot_value_dialog(pos, w, cx);
                                        } else if clicks >= 2 && matches!(axis, PivotAxis::Row | PivotAxis::Column) {
                                            e.open_pivot_field_dialog(area, pos, cx);
                                        }
                                        cx.notify();
                                    });
                                });
                                if matches!(axis, PivotAxis::Values | PivotAxis::Row | PivotAxis::Column) {
                                    let tune_editor = editor.clone();
                                    let tip = text(if axis == PivotAxis::Values { "xlsx.pivot-fields.action.value-settings" } else { "xlsx.pivot-fields.action.field-settings" });
                                    r = r.action("pivot-layout-tune", "tune", tip, move |_, w, cx| {
                                        let _ = tune_editor.update(cx, |e, cx| {
                                            if axis == PivotAxis::Values { e.open_pivot_value_dialog(pos, w, cx) } else { e.open_pivot_field_dialog(area, pos, cx) }
                                        });
                                    });
                                }
                                let remove_editor = editor.clone();
                                let tip = format("xlsx.pivot-fields.action.remove", &[("name", name)]);
                                r = r.action("pivot-layout-remove", "close", tip, move |_, _, cx| {
                                    let _ = remove_editor.update(cx, |e, cx| {
                                        let Some(t) = e.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) else { return };
                                        let Some(f) = e.pivot_ref(t).and_then(|p| field_at(p, area, pos)) else { return };
                                        e.change_pivot(t, |p| p.place(f, None, 0), cx);
                                    });
                                });
                            }
                            r
                        }
                    })
                    .collect()
            }
        };
        let layout_height = ROW_HEIGHT * (layout.len().min(LAYOUT_VISIBLE_ROWS) as f32 + 1.) + 2.;
        let layout_element = DataTable::virtualized(LAYOUT_TABLE, layout_columns, layout_header, layout_table, layout.len(), layout_build, cx)
            .row_height(ROW_HEIGHT)
            .row_actions(2)
            .row_dnd(dnd, |e, ev, _, cx| e.pivot_row_dropped(ev, cx));
        let _ = selected_item;

        let message = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).map(|p| p.message.clone()).unwrap_or_default();
        let body = chrome::panel_form()
            .p_0()
            .size_full()
            .child(top)
            .child(div().w_full().flex_1().min_h(px(ROW_HEIGHT * 4.)).child(fields_element))
            .child(div().w_full().flex_shrink_0().h(px(layout_height)).min_h(px(ROW_HEIGHT * 4.)).child(layout_element))
            .children(chrome::panel_message(&message));
        Some(SidebarPanel { title: text("xlsx.pivot-fields"), body: body.into_any_element() }.fill())
    }

    // ----- The value field's dialog ---------------------------------------------------------------------------

    fn pivot_value_dialog_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (target, pos, name_input, ui_tab, draft) = {
            let p = self.sheet_state()?.panels.pivot.as_ref()?;
            let ui = p.ui.as_ref()?;
            (p.target?, p.value?, p.value_name.clone()?, ui.value_tab, ui.draft.clone()?)
        };
        let source = {
            let p = self.pivot_ref(target)?;
            let df = p.data_fields.get(pos)?;
            p.fields.get(df.field as usize)?.name.clone()
        };
        let owner = cx.weak_entity();
        let muted = |s: SharedString| div().text_size(px(12.)).text_color(rgb(TEXT_SECONDARY)).child(s);
        let mut body = chrome::panel_form().p_0();
        body = body
            .child(chrome::control_row(text("xlsx.pivot-value.source-name"), div().w_full().min_w_0().truncate().text_color(rgb(0x1f1f1f)).child(SharedString::from(source))))
            .child(chrome::text_field_row(text("xlsx.pivot-value.custom-name"), chrome::text_field(&name_input).full_width()));
        if ui_tab == 0 {
            let items: Vec<DropdownItem> = ibis_doc_model::pivot::FUNCTIONS
                .iter()
                .map(|(code, _)| {
                    let code: &'static str = code;
                    DropdownItem::new(text(function_label_id(code))).checked(draft.function == code).on_select(Rc::new(move |e: &mut SlideEditor, _, cx| {
                        e.set_draft(|d| d.function = code.to_string());
                        cx.notify();
                    }))
                })
                .collect();
            body = body
                .child(chrome::dropdown_row(text("xlsx.pivot-value.summarize-by"), Dropdown::new("pivot-value-function", text(function_label_id(&draft.function)), items, &owner).fill_width()))
                .child(muted(text("xlsx.pivot-value.summarize-help")));
            let current = NUMBER_FORMATS.iter().find(|(_, c)| c.map(str::to_string) == draft.number_format).map(|(l, _)| *l).unwrap_or("_xlsx.number-format-custom");
            let items: Vec<DropdownItem> = NUMBER_FORMATS
                .iter()
                .map(|(label, code)| {
                    let code = code.map(str::to_string);
                    DropdownItem::new(text(label)).checked(draft.number_format == code).on_select(Rc::new(move |e: &mut SlideEditor, _, cx| {
                        let code = code.clone();
                        e.set_draft(|d| d.number_format = code);
                        cx.notify();
                    }))
                })
                .collect();
            let sample = ibis_doc_model::chart::format_number(12480000.0, draft.number_format.as_deref().unwrap_or("General"));
            body = body
                .child(chrome::dropdown_row(text("xlsx.pivot-value.number-format"), Dropdown::new("pivot-value-format", text(current), items, &owner).fill_width()))
                .child(muted(format("xlsx.pivot-value.sample", &[("sample", &sample)]).into()));
        } else {
            let current = SHOW_AS.iter().find(|(_, m)| m.map(str::to_string) == draft.show_as).map(|(l, _)| *l).unwrap_or("_xlsx.no-calculation");
            let items: Vec<DropdownItem> = SHOW_AS
                .iter()
                .map(|(label, mode)| {
                    let mode = mode.map(str::to_string);
                    DropdownItem::new(text(label)).checked(draft.show_as == mode).on_select(Rc::new(move |e: &mut SlideEditor, _, cx| {
                        let mode = mode.clone();
                        e.set_draft(|d| d.show_as = mode);
                        cx.notify();
                    }))
                })
                .collect();
            // The types that compare with another item need a base field and item; the ones the table has do not.
            let off = |id: &'static str| Dropdown::with_button(id, chrome::dropdown_button(id, text("xlsx.pivot-value.not-needed")).disabled(true), Vec::new(), true, 0, &owner).fill_width();
            body = body
                .child(chrome::dropdown_row(text("xlsx.pivot-value.show-as"), Dropdown::new("pivot-value-show-as", text(current), items, &owner).fill_width()))
                .child(chrome::dropdown_row(text("xlsx.pivot-value.base-field"), off("pivot-value-base-field")))
                .child(chrome::dropdown_row(text("xlsx.pivot-value.base-item"), off("pivot-value-base-item")));
        }
        let footer = chrome::action_bar()
            .action(chrome::ActionDefinition::new("pivot-value-ok", text("OK"), chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.pivot_value_dialog_ok(w, cx))))
            .action(chrome::dialog_cancel_definition(cx));
        let tabs = [text("xlsx.pivot-value.tab-summarize"), text("xlsx.pivot-value.tab-show-as")];
        Some(
            SidebarPanel::with_footer(text("xlsx.pivot-value.title"), body.into_any_element(), footer).header_tabs(
                "pivot-value-tabs",
                tabs,
                ui_tab,
                |e, i, cx| {
                    if let Some(u) = e.pivot_ui_mut() {
                        u.value_tab = i;
                    }
                    cx.notify();
                },
                cx,
            ),
        )
    }

    // ----- The row or column field's dialog -------------------------------------------------------------------

    fn pivot_field_dialog_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (target, axis, pos) = {
            let p = self.sheet_state()?.panels.pivot.as_ref()?;
            let (axis, pos) = p.field_selected?;
            (p.target?, axis, pos)
        };
        let area = area_index(axis);
        let (f, fld, table_repeat) = {
            let p = self.pivot_ref(target)?;
            let f = field_at(p, area, pos)?;
            (f, p.fields.get(f)?.clone_light(), p.fill_down_labels)
        };
        let owner_id = |s: &str| SharedString::from(format!("pivot-field-dialog-{s}"));
        let toggle = |id: &'static str, label: &'static str, on: bool, change: fn(&mut ibis_doc_model::PivotField)| {
            chrome::checkbox_button(owner_id(id), text(label), on, false, cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    if let Some(fld) = p.fields.get_mut(f) {
                        change(fld);
                    }
                }, cx)
            }))
        };
        let mut body = chrome::panel_form().p_0();
        body = body.child(chrome::control_row(text("xlsx.pivot-value.source-name"), div().w_full().min_w_0().truncate().child(SharedString::from(fld.name.clone()))));
        body = body.child(div().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.subtotals")));
        body = body.child(toggle("subtotal", "_xlsx.show-subtotals", fld.default_subtotal, |x| x.default_subtotal = !x.default_subtotal));
        if fld.default_subtotal {
            body = body.child(toggle("subtotal-top", "_xlsx.show-subtotals-at-top-of-group", fld.subtotal_top, |x| x.subtotal_top = !x.subtotal_top));
        }
        body = body.child(div().pt(px(2.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.layout")));
        let current = if fld.compact { 0 } else if fld.outline { 1 } else { 2 };
        let mut layouts = chrome::radio_group().layout(chrome::RadioGroupLayout::Flow);
        for (i, (id, label)) in [("compact", "_xlsx.compact-form"), ("outline", "_xlsx.outline-form"), ("tabular", "_xlsx.tabular-form")].into_iter().enumerate() {
            layouts = layouts.child(chrome::radio_button(owner_id(id), current == i, false, text(label), cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    if let Some(fld) = p.fields.get_mut(f) {
                        fld.compact = i == 0;
                        fld.outline = i != 2;
                    }
                }, cx)
            })));
        }
        body = body.child(layouts);
        let repeat = fld.fill_down_labels.unwrap_or(table_repeat);
        body = body.child(chrome::checkbox_button(owner_id("repeat"), text("_xlsx.repeat-item-labels"), repeat, false, cx.listener(move |this, _, _, cx| {
            this.change_pivot(target, |p| {
                let table_default = p.fill_down_labels;
                if let Some(fld) = p.fields.get_mut(f) {
                    let next = !fld.fill_down_labels.unwrap_or(table_default);
                    fld.fill_down_labels = Some(next);
                }
            }, cx)
        })));
        body = body.child(toggle("blank", "xlsx.pivot-blank-rows", fld.insert_blank_row, |x| x.insert_blank_row = !x.insert_blank_row));
        let footer = chrome::action_bar().action(chrome::dialog_close_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.pivot-field.title"), body.into_any_element(), footer))
    }
}

/// The few parts of a field the field dialog reads (the field's items and values stay where they are).
struct LightField {
    name: String,
    default_subtotal: bool,
    subtotal_top: bool,
    compact: bool,
    outline: bool,
    fill_down_labels: Option<bool>,
    insert_blank_row: bool,
}

trait CloneLight {
    fn clone_light(&self) -> LightField;
}
impl CloneLight for ibis_doc_model::PivotField {
    fn clone_light(&self) -> LightField {
        LightField {
            name: self.name.clone(),
            default_subtotal: self.default_subtotal,
            subtotal_top: self.subtotal_top,
            compact: self.compact,
            outline: self.outline,
            fill_down_labels: self.fill_down_labels,
            insert_blank_row: self.insert_blank_row,
        }
    }
}
