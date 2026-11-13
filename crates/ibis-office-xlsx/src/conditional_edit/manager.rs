// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 条件付き書式ルールの管理 (the manager): the rules in evaluation order in the shared scrolling table. A
//! rule's 「条件を満たす場合は停止」 (stop if true) is shown at the end of the row as a mark (changed in the rule editor,
//! not by a click). A row is dragged by its grip at the end to change the priority (no up/down buttons); the
//! row's buttons edit, duplicate and delete. 編集 and ルールを追加 open the rule editor as a child dialog: this dialog
//! stays open behind it (`DialogState::parent`).
use super::*;
use crate::shell::chrome::data_table::DataTable;
use crate::shell::chrome::row_dnd::{DropPlace, RowDnd, RowDropEvent};
use crate::shell::chrome::virtual_table::{TableColumn, TableRow, TableState};
use crate::shell::chrome::{self, TEXT_SECONDARY};
use crate::shell::i18n::text;
use crate::shell::palette::rgb;
use gpui_kit::component::tooltip::Tooltip;
use std::rc::Rc;

/// The id of the table (the tests find its rows with it).
pub(crate) const RULES_TABLE: &str = "conditional-rules-table";
const ROW_HEIGHT: f32 = 32.;
/// The table's height in the dialog (the dialog scrolls, the rows scroll inside).
const TABLE_HEIGHT: f32 = 300.;

/// What the manager's state keeps for the table (scroll, column widths) and its drag and drop.
#[derive(Default)]
pub(crate) struct ManagerTable {
    pub(crate) table: TableState,
    pub(crate) dnd: RowDnd,
}

/// One rule as the table shows it.
struct RuleRow {
    index: usize,
    rule: usize,
    label: String,
    ranges: String,
    stop: bool,
    editable: bool,
}

/// The id a rule row has in the drag and drop: its format and its place in it.
fn row_id(index: usize, rule: usize) -> u64 {
    (index as u64) << 32 | rule as u64
}

fn row_key(id: u64) -> (usize, usize) {
    ((id >> 32) as usize, (id & 0xffff_ffff) as usize)
}

fn tip(id: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = text(id);
    move |window, cx| Tooltip::new(label.clone()).build(window, cx)
}

#[ibis_office_macros::editor_ext(Manager)]
impl SlideEditor {
    pub(crate) fn open_conditional_manager(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().conditional_edit = None;
        self.open_conditional(None, w, cx);
        if let Some(p) = self.wb_mut().conditional_edit.as_mut() {
            p.manager = true;
        }
        self.focus_handle.focus(w, cx);
        cx.notify();
    }

    /// The rules of the sheet in priority order, narrowed to the selection when the manager says so.
    fn conditional_rule_rows(&self, p: &ConditionalPanel) -> Vec<RuleRow> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let selected = self.selected_range();
        let mut rules = sheet
            .conditional
            .iter()
            .enumerate()
            .filter(|(_, f)| !p.filter_selection_only || selected.is_some_and(|s| f.ranges.iter().any(|r| r.first_row <= s.last_row && r.last_row >= s.first_row && r.first_column <= s.last_column && r.last_column >= s.first_column)))
            .flat_map(|(i, f)| f.rules.iter().enumerate().map(move |(j, r)| (i, j, r, f)))
            .collect::<Vec<_>>();
        rules.sort_by_key(|(_, _, r, _)| r.priority);
        rules
            .into_iter()
            .map(|(i, j, r, f)| {
                let label = match &r.kind {
                    CfKind::Expression(s) => format!("={s}"),
                    CfKind::CellIs { operator, formulas } => format!("{} {}", OPERATORS.iter().find(|(op, _)| *op == operator).map(|(_, name)| text(name).to_string()).unwrap_or_else(|| operator.clone()), formulas.join(", ")),
                    CfKind::Top10 { .. } => text("docx.conditional-quick.top-10-items").to_string(),
                    _ => text("xlsx.cf.other-rule").to_string(),
                };
                let ranges = f.ranges.iter().map(|r| format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))).collect::<Vec<_>>().join(", ");
                let editable = f.rules.len() == 1 && matches!(r.kind, CfKind::Expression(_) | CfKind::CellIs { .. } | CfKind::TimePeriod { .. } | CfKind::Top10 { .. });
                RuleRow { index: i, rule: j, label, ranges, stop: r.stop, editable }
            })
            .collect()
    }

    pub(super) fn conditional_manager_panel(&self, cx: &Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().conditional_edit.as_ref().filter(|p| p.manager)?;
        self.conditional_manager_view(p, cx)
    }

    /// The manager behind the rule editor (drawn by `shell/chrome.rs` as the parent dialog).
    pub(crate) fn conditional_parent_panel(&self, cx: &Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().conditional_edit.as_ref()?.parent.as_deref()?;
        self.conditional_manager_view(p, cx)
    }

    fn conditional_manager_view(&self, p: &ConditionalPanel, cx: &Context<Self>) -> Option<SidebarPanel> {
        let rules = Rc::new(self.conditional_rule_rows(p));
        let editor = cx.entity().downgrade();
        let mark = |id: &'static str, child: AnyElement| {
            let ui = p.ui.clone();
            div().relative().child(child).child(canvas(|_, _, _| (), move |b, _, _, _| { ui.borrow_mut().insert(id.to_string(), b); }).absolute().inset_0()).into_any_element()
        };
        let mark_action = |id: &'static str, action: chrome::ActionDefinition| {
            let ui = p.ui.clone();
            action.on_bounds(move |b, _, _| { ui.borrow_mut().insert(id.to_string(), b); })
        };

        // 条件を満たす場合は停止 is shown at the end of the row, as a mark only: it is changed in the rule editor.
        let columns = vec![TableColumn::flex("rule"), TableColumn::resizable("range", 70., 120., 260.), TableColumn::fixed("stop", 32.)];
        let heading = |id: &'static str| div().text_xs().font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT_SECONDARY)).child(text(id)).into_any_element();
        let stop_heading = div().id("conditional-stop-heading").size_full().tooltip(tip("xlsx.cf.stop-if-true")).into_any_element();
        let header = vec![heading("xlsx.cf.rule"), heading("_shell.range"), stop_heading];

        let build = {
            let rules = rules.clone();
            let editor = editor.clone();
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|n| rules.get(n).map(|r| (n, r)))
                    .map(|(n, r)| {
                        let (i, j) = (r.index, r.rule);
                        // Not a button: no pointer, no click, and not drawn disabled.
                        let stop = div()
                            .id(("conditional-rule-stop", n))
                            .cursor_default()
                            .tooltip(tip("xlsx.cf.stop-if-true"))
                            .child(chrome::checkbox_box(r.stop))
                            .into_any_element();
                        let mut row = TableRow::new(vec![
                            div().w_full().truncate().text_sm().child(SharedString::from(r.label.clone())).into_any_element(),
                            div().w_full().truncate().text_xs().text_color(rgb(TEXT_SECONDARY)).child(SharedString::from(r.ranges.clone())).into_any_element(),
                            stop,
                        ])
                        .draggable(row_id(i, j), SharedString::from(r.label.clone()))
                        .drop_between(row_id(i, j));
                        if r.editable {
                            let editor = editor.clone();
                            row = row.action("conditional-edit", "edit", text("_xlsx.edit"), move |_, w, cx| {
                                let _ = editor.update(cx, |e, cx| e.open_conditional(Some(i), w, cx));
                            });
                        }
                        let (duplicate, delete) = (editor.clone(), editor.clone());
                        row.action("conditional-duplicate", "content_copy", text("_shell.duplicate"), move |_, w, cx| {
                            let _ = duplicate.update(cx, |e, cx| e.duplicate_conditional_rule(i, j, w, cx));
                        })
                        .action("conditional-delete", "delete", text("_shell.delete"), move |_, w, cx| {
                            let _ = delete.update(cx, |e, cx| e.delete_conditional_rule(i, j, w, cx));
                        })
                    })
                    .collect()
            }
        };
        let empty = div().w_full().p_3().text_sm().text_color(rgb(TEXT_SECONDARY)).child(text("xlsx.cf.no-rules")).into_any_element();
        let table = DataTable::virtualized(RULES_TABLE, columns, header, p.manager_table.table.clone(), rules.len(), build, cx)
            .row_height(ROW_HEIGHT)
            .row_actions(3)
            .drag_handles()
            .empty(empty)
            .row_dnd(p.manager_table.dnd.clone(), |e, ev, w, cx| e.conditional_rule_dropped(ev, w, cx));

        let radios = h_flex()
            .gap_2()
            .child(mark("filter-sheet", chrome::radio_button("conditional-filter-sheet", !p.filter_selection_only, false, text("xlsx.cf.filter-sheet"), cx.listener(|e, _, _, cx| {
                if let Some(p) = e.wb_mut().conditional_edit.as_mut() {
                    p.filter_selection_only = false;
                }
                cx.notify();
            })).into_any_element()))
            .child(mark("filter-selection", chrome::radio_button("conditional-filter-selection", p.filter_selection_only, false, text("xlsx.cf.filter-selection"), cx.listener(|e, _, _, cx| {
                if let Some(p) = e.wb_mut().conditional_edit.as_mut() {
                    p.filter_selection_only = true;
                }
                cx.notify();
            })).into_any_element()));
        let add = chrome::action_button("conditional-add").label(text("_xlsx.add-rule")).on_click(cx.listener(|e, _, w, cx| e.open_conditional(None, w, cx)));
        let body = chrome::panel_form()
            .p_0()
            .child(h_flex().w_full().items_center().justify_between().gap_2().child(radios).child(add))
            .child(div().w_full().h(px(TABLE_HEIGHT)).child(table));
        let footer = chrome::action_bar()
            .action(chrome::ActionDefinition::new("conditional-manager-close", "", chrome::ActionButtonKind::Normal).label(text("_shell.close")).on_click(cx.listener(|e, _, w, cx| e.close_dialog(w, cx))))
            .action(mark_action("clear", chrome::ActionDefinition::new("conditional-clear", "", chrome::ActionButtonKind::Destructive).label(text("_xlsx.delete-rules-in-selection")).on_click(cx.listener(|e, _, _, cx| e.clear_conditional_selection(cx)))));
        Some(SidebarPanel::with_footer(text("xlsx.cf.manage-title"), body.into_any_element(), footer))
    }

    /// The manager of the open sheet, if its rules are still the ones of the sheet shown.
    fn conditional_manager_slide(&self) -> Option<SlideId> {
        let p = self.wb().conditional_edit.as_ref().filter(|p| p.manager)?;
        (p.slide == self.current_slide).then_some(p.slide)
    }

    pub(super) fn duplicate_conditional_rule(&mut self, index: usize, rule: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(slide) = self.conditional_manager_slide() else { return };
        self.run_conditional_manager(vec![Command::Sheet(SheetCommand::DuplicateConditionalRule { slide, index, rule })], cx);
    }

    pub(super) fn delete_conditional_rule(&mut self, index: usize, rule: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(slide) = self.conditional_manager_slide() else { return };
        self.run_conditional_manager(vec![Command::Sheet(SheetCommand::DeleteConditionalRule { slide, index, rule })], cx);
    }

    /// A rule row dropped on another: the rule moves to that place in the evaluation order (one undo step).
    pub(crate) fn conditional_rule_dropped(&mut self, ev: RowDropEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(slide) = self.conditional_manager_slide() else { return };
        let (from, target) = (row_key(ev.from), row_key(ev.target));
        if from == target {
            return;
        }
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut order: Vec<(u32, usize, usize)> = sheet.conditional.iter().enumerate().flat_map(|(i, f)| f.rules.iter().enumerate().map(move |(j, r)| (r.priority, i, j))).collect();
        order.sort_by_key(|&(priority, _, _)| priority);
        let (Some(at), Some(anchor)) = (order.iter().position(|&(_, i, j)| (i, j) == from), order.iter().position(|&(_, i, j)| (i, j) == target)) else { return };
        let to = match ev.place {
            DropPlace::Before => if at < anchor { anchor - 1 } else { anchor },
            DropPlace::After => if at < anchor { anchor } else { anchor + 1 },
            DropPlace::Into => return,
        };
        if to == at {
            return;
        }
        // The rule moves one place at a time, so the whole move is one command list (one undo step).
        let commands = (0..at.abs_diff(to)).map(|_| Command::Sheet(SheetCommand::MoveConditionalRule { slide, index: from.0, rule: from.1, up: to < at })).collect();
        self.run_conditional_manager(commands, cx);
    }

    fn run_conditional_manager(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) && let Some(p) = self.wb_mut().conditional_edit.as_mut() {
            p.error = err.to_string();
        }
        cx.notify();
    }
}
