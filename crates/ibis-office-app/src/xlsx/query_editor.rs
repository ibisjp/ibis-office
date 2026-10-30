// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > テーブルまたは範囲から: the クエリ エディター. It fills the
//! window over the sheet as Power Query's editor does: the operations over
//! the top (ホーム・変換・列の追加・結合), the 適用したステップ at the left, the
//! table after the chosen step in the middle, and the form of the operation
//! (or the M code, or the parameters) at the right. 閉じて読み込む writes the
//! result to a sheet and keeps the query in the workbook (データ >
//! クエリと接続 lists them to edit, 更新 or delete).
use std::collections::HashMap;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::xlsx::query_ops::{self, Kind, QOp, Values, TABS};
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::query::{self, BookResolver, Parameter, QTable, Query, Source, Step};
use ibis_doc_model::{CellValue, Command, SheetCommand, SlideId};

/// The form of an operation being asked.
pub(crate) struct Form {
    pub(crate) op: QOp,
    /// The step being edited (an index of the steps), or none for a new one.
    pub(crate) editing: Option<usize>,
    pub(crate) inputs: Vec<(&'static str, Entity<InputState>)>,
    /// The choice (an index) or flag (0 or 1) of each field.
    pub(crate) picks: HashMap<&'static str, usize>,
}

/// What the right side shows when no form is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Form,
    Code,
    Parameters,
}

pub(crate) struct QueryEditor {
    pub(crate) slide: SlideId,
    pub(crate) query: Query,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) source: QTable,
    /// The table after each step; the first is the source.
    pub(crate) results: Vec<QTable>,
    pub(crate) error: Option<(usize, String)>,
    /// Which table is shown: 0 the source, n the table after step n.
    pub(crate) selected: usize,
    pub(crate) columns: Vec<String>,
    pub(crate) tab: usize,
    pub(crate) side: Side,
    pub(crate) form: Option<Form>,
    pub(crate) name: Entity<InputState>,
    pub(crate) parameter_name: Entity<InputState>,
    pub(crate) parameter_value: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

pub(crate) struct QueriesPanel {
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl QueryEditor {
    /// The table shown.
    pub(crate) fn shown(&self) -> &QTable {
        self.results.get(self.selected).or_else(|| self.results.last()).unwrap_or(&self.source)
    }
}

fn input(text: &str, placeholder: &str, window: &mut Window, cx: &mut Context<SlideEditor>) -> Entity<InputState> {
    let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.to_string()));
    state.update(cx, |i, cx| i.set_value(text.to_string(), window, cx));
    state
}

impl SlideEditor {
    /// The sheet's name a slide has.
    fn query_sheet_name(&self, slide: SlideId) -> String {
        self.presentation().slide(slide).and_then(|s| s.name.clone()).unwrap_or_default()
    }

    /// データ > テーブルまたは範囲から: the query of the selected range (the table around the cell when
    /// only one cell is selected).
    pub(crate) fn open_query_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range().filter(|r| r.first_row != r.last_row || r.first_column != r.last_column).or_else(|| self.current_region());
        let Some(range) = range else {
            self.set_status(crate::shell::i18n::text("query.no-data").to_string(), cx);
            return;
        };
        let sheet = self.query_sheet_name(self.current_slide);
        let table = self.current_sheet().and_then(|(_, s)| s.tables.iter().find(|t| t.range.first_row <= range.first_row && range.last_row <= t.range.last_row && t.range.first_column <= range.first_column && range.last_column <= t.range.last_column).map(|t| t.name.clone()));
        let first = ibis_doc_model::cell_name(range.first_column, range.first_row);
        let last = ibis_doc_model::cell_name(range.last_column, range.last_row);
        let taken: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.queries.iter().map(|q| q.name.clone()).collect()).unwrap_or_default();
        let base = crate::shell::i18n::text("query.default-name").to_string();
        let name = (1..).map(|n| format!("{base}{n}")).find(|n| !taken.contains(n)).unwrap_or(base);
        let mut query = match table {
            Some(t) => Query::new(&name, Source::Table(t)),
            None => {
                let mut q = Query::new(&name, Source::Range { sheet, range: format!("{first}:{last}") });
                // The first row holds the headings (the dialog's 先頭行を列ヘッダーとして使用 is on).
                q.steps.push(Step::PromoteHeaders);
                q
            }
        };
        query.output = None;
        self.open_query_editor(query, window, cx);
    }

    /// データ > 空のクエリ: a new blank query starting from an empty table.
    pub(crate) fn open_blank_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let taken: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.queries.iter().map(|q| q.name.clone()).collect()).unwrap_or_default();
        let base = crate::shell::i18n::text("query.default-name").to_string();
        let name = (1..).map(|n| format!("{base}{n}")).find(|n| !taken.contains(n)).unwrap_or(base);
        let mut query = Query::new(&name, Source::Blank);
        query.output = None;
        self.open_query_editor(query, window, cx);
    }

    /// The editor on a query (a new one, or one of the workbook's to edit).
    pub(crate) fn open_query_editor(&mut self, query: Query, window: &mut Window, cx: &mut Context<Self>) {
        let source = match query::read_source(self.presentation(), &query.source) {
            Ok(t) => t,
            Err(e) => {
                self.set_status(e, cx);
                return;
            }
        };
        let parameters = self.presentation().workbook.as_ref().map(|w| w.query_parameters.clone()).unwrap_or_default();
        let name = input(&query.name, "", window, cx);
        let editor = QueryEditor {
            slide: self.current_slide,
            selected: query.steps.len(),
            query,
            parameters,
            source,
            results: Vec::new(),
            error: None,
            columns: Vec::new(),
            tab: 0,
            side: Side::Form,
            form: None,
            name,
            parameter_name: input("", "", window, cx),
            parameter_value: input("", "", window, cx),
            message: String::new(),
            ui: Default::default(),
        };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.query_editor = Some(editor);
        self.query_recompute();
        let steps = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()).map_or(0, |q| q.query.steps.len());
        self.query_show(steps);
        cx.notify();
    }

    fn query_editor_mut(&mut self) -> Option<&mut QueryEditor> {
        self.sheet_state_mut()?.panels.query_editor.as_mut()
    }

    /// The tables after each step worked out again.
    pub(crate) fn query_recompute(&mut self) {
        let Some(ed) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()) else { return };
        let (query, source, parameters) = (ed.query.clone(), ed.source.clone(), ed.parameters.clone());
        let queries: Vec<Query> = self.presentation().workbook.as_ref().map(|w| w.queries.clone()).unwrap_or_default();
        let resolver = BookResolver { presentation: self.presentation(), queries: &queries, parameters: &parameters, current: &query.name, depth: 0 };
        let (tables, error) = query.run(source, &parameters, &resolver);
        if let Some(ed) = self.query_editor_mut() {
            ed.results = tables;
            ed.error = error;
        }
    }

    /// The table after step `index` shown (0: the source).
    pub(crate) fn query_show(&mut self, index: usize) {
        if let Some(ed) = self.query_editor_mut() {
            let last = ed.results.len().saturating_sub(1);
            ed.selected = index.min(last.max(0));
            ed.columns.retain(|c| ed.results.get(ed.selected).is_some_and(|t| t.column_index(c).is_some()));
        }
    }

    pub(crate) fn query_select_column(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(ed) = self.query_editor_mut() {
            match ed.columns.iter().position(|c| c == name) {
                Some(i) => {
                    ed.columns.remove(i);
                }
                None => ed.columns.push(name.to_string()),
            }
        }
        cx.notify();
    }

    /// A step put after the one shown (or in its place when `editing`).
    pub(crate) fn query_add_step(&mut self, step: Step, editing: Option<usize>, cx: &mut Context<Self>) {
        let Some(ed) = self.query_editor_mut() else { return };
        let at = match editing {
            Some(i) => {
                ed.query.steps[i] = step;
                i + 1
            }
            None => {
                let at = ed.selected.min(ed.query.steps.len());
                ed.query.steps.insert(at, step);
                at + 1
            }
        };
        ed.form = None;
        ed.message.clear();
        self.query_recompute();
        // Shown: the step itself, or the last that worked.
        self.query_show(at);
        if let Some(ed) = self.query_editor_mut()
            && let Some((bad, e)) = ed.error.clone()
        {
            ed.message = crate::shell::i18n::format("query.step-failed", &[("step", &(bad + 1)), ("error", &e)]);
        }
        cx.notify();
    }

    pub(crate) fn query_delete_step(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(ed) = self.query_editor_mut() else { return };
        if index >= ed.query.steps.len() {
            return;
        }
        ed.query.steps.remove(index);
        ed.form = None;
        ed.message.clear();
        self.query_recompute();
        self.query_show(index);
        cx.notify();
    }

    /// An operation chosen on the ribbon: made at once when it asks nothing (or only the columns selected),
    /// else its form opens.
    pub(crate) fn query_run_op(&mut self, op: QOp, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ed) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()) else { return };
        let selected = ed.columns.clone();
        let table = ed.shown().clone();
        if op.fields().is_empty() || (op.takes_columns_only() && !selected.is_empty()) {
            let values = query_ops::defaults(op, &selected, &table);
            if let Ok(step) = query_ops::build(op, &values) {
                self.query_add_step(step, None, cx);
                return;
            }
        }
        let values = query_ops::defaults(op, &selected, &table);
        self.query_open_form(op, None, values, window, cx);
    }

    fn query_open_form(&mut self, op: QOp, editing: Option<usize>, values: Values, window: &mut Window, cx: &mut Context<Self>) {
        let mut inputs = Vec::new();
        let mut picks = HashMap::new();
        for f in op.fields() {
            let value = values.get(f.id).cloned().unwrap_or_default();
            match f.kind {
                Kind::Text => inputs.push((f.id, input(&value, "", window, cx))),
                Kind::Choice(_) | Kind::Flag => {
                    picks.insert(f.id, value.parse::<usize>().unwrap_or(0));
                }
            }
        }
        if let Some(ed) = self.query_editor_mut() {
            ed.form = Some(Form { op, editing, inputs, picks });
            ed.side = Side::Form;
            ed.message.clear();
        }
        cx.notify();
    }

    /// The step at `index` of the steps edited in its form.
    pub(crate) fn query_edit_step(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()).and_then(|q| q.query.steps.get(index)).cloned() else { return };
        let (op, values) = query_ops::values_of(&step);
        if op.fields().is_empty() {
            return;
        }
        self.query_open_form(op, Some(index), values, window, cx);
    }

    /// The form's values made a step.
    pub(crate) fn query_apply_form(&mut self, cx: &mut Context<Self>) {
        let Some(ed) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()) else { return };
        let Some(form) = ed.form.as_ref() else { return };
        let mut values = Values::new();
        for (id, state) in &form.inputs {
            values.insert((*id).to_string(), state.read(cx).value().to_string());
        }
        for (id, n) in &form.picks {
            values.insert((*id).to_string(), n.to_string());
        }
        let (op, editing) = (form.op, form.editing);
        match query_ops::build(op, &values) {
            Ok(step) => self.query_add_step(step, editing, cx),
            Err(field) => {
                if let Some(ed) = self.query_editor_mut() {
                    ed.message = crate::shell::i18n::format("query.fill-in", &[("field", &crate::shell::i18n::text(&format!("query.field.{field}")))]);
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn query_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.panels.query_editor = None;
        }
        cx.notify();
    }

    /// 閉じて読み込む: the result written to its sheet and the query kept.
    pub(crate) fn query_close_and_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let Some(ed) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()) else { return };
        if let Some((bad, e)) = ed.error.clone() {
            let text = crate::shell::i18n::format("query.step-failed", &[("step", &(bad + 1)), ("error", &e)]);
            if let Some(ed) = self.query_editor_mut() {
                ed.message = text;
            }
            cx.notify();
            return;
        }
        let name = ed.name.read(cx).value().trim().to_string();
        let name = if name.is_empty() { ed.query.name.clone() } else { name };
        let mut query = ed.query.clone();
        let old_name = query.name.clone();
        query.name = name;
        let table = ed.results.last().cloned().unwrap_or_default();
        let (slide, parameters) = (ed.slide, ed.parameters.clone());
        // A query renamed keeps the sheet it loaded.
        let mut commands = Vec::new();
        let mut queries: Vec<Query> = self.presentation().workbook.as_ref().map(|w| w.queries.clone()).unwrap_or_default();
        if old_name != query.name {
            queries.retain(|q| q.name != old_name);
        }
        commands.push(Command::Sheet(SheetCommand::SetQueries { queries, parameters }));
        commands.push(Command::Sheet(SheetCommand::LoadQuery { slide, query, table }));
        if let Err(e) = self.document.execute(commands) {
            self.set_status(e.to_string(), cx);
            return;
        }
        if let Some(state) = self.sheet_state_mut() {
            state.panels.query_editor = None;
        }
        self.set_status(crate::shell::i18n::text("query.loaded").to_string(), cx);
        cx.notify();
    }

    // ------------------------------------------------------------ the list of queries

    /// データ > クエリと接続.
    pub(crate) fn open_queries_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let state = self.wb_mut();
        state.close_settings();
        state.panels.queries = Some(QueriesPanel { message: String::new(), ui: Default::default() });
        self.sidebar_open = true;
        cx.notify();
    }

    fn queries_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.queries.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    pub(crate) fn edit_query(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(query) = self.presentation().workbook.as_ref().and_then(|w| w.queries.get(index)).cloned() else { return };
        self.open_query_editor(query, window, cx);
    }

    pub(crate) fn delete_query(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(wb) = self.presentation().workbook.as_ref() else { return };
        let mut queries = wb.queries.clone();
        if index >= queries.len() {
            return;
        }
        let gone = queries.remove(index);
        let parameters = wb.query_parameters.clone();
        if let Err(e) = self.document.execute(vec![Command::Sheet(SheetCommand::SetQueries { queries, parameters })]) {
            return self.queries_message(e.to_string(), cx);
        }
        self.queries_message(crate::shell::i18n::format("query.deleted", &[("name", &gone.name)]), cx);
    }

    /// 更新: the query worked out again from its source and its sheet rewritten.
    pub(crate) fn refresh_query(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(wb) = self.presentation().workbook.as_ref() else { return };
        let Some(query) = wb.queries.get(index).cloned() else { return };
        let table = match query::result_of(self.presentation(), &query, &wb.queries, &wb.query_parameters, 0) {
            Ok(t) => t,
            Err(e) => return self.queries_message(crate::shell::i18n::format("query.refresh-failed", &[("name", &query.name), ("error", &e)]), cx),
        };
        let rows = table.rows.len();
        let slide = self.current_slide;
        if let Err(e) = self.document.execute(vec![Command::Sheet(SheetCommand::LoadQuery { slide, query: query.clone(), table })]) {
            return self.queries_message(e.to_string(), cx);
        }
        self.queries_message(crate::shell::i18n::format("query.refreshed", &[("name", &query.name), ("rows", &rows)]), cx);
    }

    pub(crate) fn queries_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let queries = self.presentation().workbook.as_ref().map(|w| w.queries.clone()).unwrap_or_default();
        let p = self.sheet_state()?.panels.queries.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = p.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: String, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        if queries.is_empty() {
            body = body.child(crate::shell::i18n::text("query.none"));
        }
        for (i, q) in queries.iter().enumerate() {
            let source = match &q.source {
                Source::Blank => crate::shell::i18n::text("query.source-blank").to_string(),
                Source::Table(t) => t.clone(),
                Source::Range { sheet, range } => format!("{sheet}!{range}"),
            };
            body = body
                .child(div().font_weight(FontWeight::BOLD).child(SharedString::from(q.name.clone())))
                .child(div().text_sm().child(SharedString::from(crate::shell::i18n::format("query.summary", &[("source", &source), ("steps", &q.steps.len())]))))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(mark(format!("edit-{i}"), crate::shell::chrome::action_button(SharedString::from(format!("query-edit-{i}"))).small().label(crate::shell::i18n::text("query.edit")).on_click(cx.listener(move |this, _, window, cx| this.edit_query(i, window, cx))).into_any_element()))
                        .child(mark(format!("refresh-{i}"), crate::shell::chrome::action_button(SharedString::from(format!("query-refresh-{i}"))).small().label(crate::shell::i18n::text("query.refresh")).on_click(cx.listener(move |this, _, _, cx| this.refresh_query(i, cx))).into_any_element()))
                        .child(mark(format!("delete-{i}"), crate::shell::chrome::destructive_button(SharedString::from(format!("query-delete-{i}")),cx).small().label(crate::shell::i18n::text("query.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_query(i, cx))).into_any_element())),
                );
        }
        body = body.children(crate::shell::chrome::panel_message(&p.message));
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("close".into(), crate::shell::chrome::ActionDefinition::new("queries-close", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(crate::shell::i18n::text("_shell.close"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.queries = None;
                    cx.notify();
                }))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("query.queries-title"), body.into_any_element(), panel_terminal_actions))
    }

    // ------------------------------------------------------------------- the editor's window

    /// The editor, which takes the place of the sheet while it is open.
    pub(crate) fn render_query_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let _ = window;
        let theme = cx.theme().clone();
        let ed = self.sheet_state()?.panels.query_editor.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = ed.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: String, action: crate::shell::chrome::ActionDefinition| {
            let ui = ed.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let text = |key: &str| crate::shell::i18n::text(key);

        // The title row.
        let title = h_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().font_weight(FontWeight::BOLD).child(text("query.editor-title")))
            .child(div().w(px(220.)).child(mark("name".into(), crate::shell::chrome::text_field(&ed.name).full_width().into_any_element())))
            .child(div().flex_1())
            .child(mark("load".into(), crate::shell::chrome::primary_button("query-load").small().label(text("query.close-and-load")).on_click(cx.listener(|this, _, window, cx| this.query_close_and_load(window, cx))).into_any_element()))
            .child(mark("cancel".into(), crate::shell::chrome::action_button("query-cancel").small().label(text("query.cancel")).on_click(cx.listener(|this, _, _, cx| this.query_cancel(cx))).into_any_element()));

        // The ribbon: tabs, then the operations of the tab.
        let mut tabs = h_flex().w_full().px_3().gap_1().pt_1();
        for (i, (key, _)) in TABS.iter().enumerate() {
            let label = text(key);
            let selected = ed.tab == i;
            tabs = tabs.child(mark(
                format!("tab-{i}"),
                div()
                    .id(SharedString::from(format!("query-tab-{i}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(theme.accent))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(ed) = this.query_editor_mut() {
                            ed.tab = i;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        tabs = tabs
            .child(div().flex_1())
            .child(mark(
                "code".into(),
                crate::shell::chrome::action_button("query-code").small().label(text("query.advanced-editor")).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(ed) = this.query_editor_mut() {
                        ed.side = if ed.side == Side::Code { Side::Form } else { Side::Code };
                    }
                    cx.notify();
                })).into_any_element(),
            ))
            .child(mark(
                "parameters".into(),
                crate::shell::chrome::action_button("query-parameters").small().label(text("query.manage-parameters")).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(ed) = this.query_editor_mut() {
                        ed.side = if ed.side == Side::Parameters { Side::Form } else { Side::Parameters };
                    }
                    cx.notify();
                })).into_any_element(),
            ));
        let mut ribbon = h_flex().w_full().px_3().py_1().gap_1().flex_wrap().border_b_1().border_color(theme.border);
        for &op in TABS[ed.tab.min(TABS.len() - 1)].1 {
            let id = op.id();
            ribbon = ribbon.child(mark(
                format!("op-{id}"),
                crate::shell::chrome::action_button(SharedString::from(format!("query-op-{id}"))).small().label(text(&format!("query.op.{id}"))).on_click(cx.listener(move |this, _, window, cx| this.query_run_op(op, window, cx))).into_any_element(),
            ));
        }

        // The steps.
        let mut steps = v_flex().w(px(210.)).h_full().p_2().gap_1().border_r_1().border_color(theme.border).child(div().font_weight(FontWeight::BOLD).child(text("query.applied-steps")));
        let step_row = |i: usize, label: SharedString, deletable: bool, cx: &mut Context<Self>| {
            let selected = ed.selected == i;
            let failed = ed.error.as_ref().is_some_and(|(bad, _)| *bad + 1 == i);
            let mut row = h_flex()
                .id(SharedString::from(format!("query-step-{i}")))
                .gap_1()
                .px_1()
                .rounded_md()
                .cursor_pointer()
                .when(selected, |d| d.bg(theme.accent))
                .child(div().flex_1().min_w_0().text_size(px(13.)).when(failed, |d| d.text_color(rgb(0xc62828))).child(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.query_show(i);
                    cx.notify();
                }));
            if deletable {
                row = row
                    .child(
                        div()
                            .id(SharedString::from(format!("query-step-edit-{i}")))
                            .px_1()
                            .cursor_pointer()
                            .child("✎")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.query_edit_step(i - 1, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("query-step-delete-{i}")))
                            .px_1()
                            .cursor_pointer()
                            .child("✕")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.query_delete_step(i - 1, cx);
                            })),
                    );
            }
            mark(format!("step-{i}"), row.into_any_element())
        };
        steps = steps.child(step_row(0, text("query.source"), false, cx));
        for (k, step) in ed.query.steps.iter().enumerate() {
            steps = steps.child(step_row(k + 1, text(&format!("query.op.{}", query_ops::step_id(step))), true, cx));
        }

        // The table.
        let shown = ed.shown();
        let mut header = h_flex();
        for (c, col) in shown.columns.iter().enumerate() {
            let selected = ed.columns.contains(&col.name);
            let name = col.name.clone();
            header = header.child(mark(
                format!("column-{c}"),
                div()
                    .id(SharedString::from(format!("query-column-{c}")))
                    .w(px(130.))
                    .flex_shrink_0()
                    .px_2()
                    .py_1()
                    .border_1()
                    .border_color(theme.border)
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .when(selected, |d| d.bg(theme.accent))
                    .child(div().truncate().child(SharedString::from(format!("{} {}", type_mark(col.ty), col.name))))
                    .on_click(cx.listener(move |this, _, _, cx| this.query_select_column(&name, cx)))
                    .into_any_element(),
            ));
        }
        let mut grid = v_flex().child(header);
        for row in shown.rows.iter().take(200) {
            let mut line = h_flex();
            for v in row {
                let (shown_text, colour) = match v {
                    CellValue::Error(e) => (e.as_str().to_string(), Some(rgb(0xc62828))),
                    other => (query::value_text(other), None),
                };
                let right = matches!(v, CellValue::Number(_));
                line = line.child(div().w(px(130.)).flex_shrink_0().px_2().py(px(2.)).border_1().border_color(theme.border).text_size(px(13.)).when(right, |d| d.text_right()).when_some(colour, |d, c| d.text_color(c)).child(div().truncate().child(SharedString::from(shown_text))));
            }
            grid = grid.child(line);
        }
        let table = div().id("query-table").flex_1().min_w_0().h_full().overflow_scroll().child(grid);

        // The right side.
        let mut side = v_flex().w(px(300.)).h_full().p_2().gap_2().border_l_1().border_color(theme.border).id("query-side").overflow_y_scroll();
        match (ed.side, ed.form.as_ref()) {
            (Side::Code, _) => {
                side = side.child(div().font_weight(FontWeight::BOLD).child(text("query.advanced-editor")));
                let mut q = ed.query.clone();
                q.name = ed.name.read(cx).value().to_string();
                for line in query::query_to_m(&q).lines() {
                    side = side.child(div().text_size(px(12.)).font_family("Menlo").child(SharedString::from(line.to_string())));
                }
            }
            (Side::Parameters, _) => {
                side = side.child(div().font_weight(FontWeight::BOLD).child(text("query.manage-parameters"))).child(div().text_size(px(12.)).child(text("query.parameters-note")));
                for (k, p) in ed.parameters.iter().enumerate() {
                    side = side.child(
                        h_flex()
                            .gap_1()
                            .child(div().flex_1().child(SharedString::from(format!("{} = {}", p.name, p.value))))
                            .child(mark(
                                format!("parameter-delete-{k}"),
                                div()
                                    .id(SharedString::from(format!("query-parameter-delete-{k}")))
                                    .px_1()
                                    .cursor_pointer()
                                    .child("✕")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(ed) = this.query_editor_mut() {
                                            if k < ed.parameters.len() {
                                                ed.parameters.remove(k);
                                            }
                                        }
                                        this.query_recompute();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            )),
                    );
                }
                side = side
                    .child(div().text_size(px(12.)).child(text("query.field.parameter-name")))
                    .child(mark("parameter-name".into(), crate::shell::chrome::text_field(&ed.parameter_name).full_width().into_any_element()))
                    .child(div().text_size(px(12.)).child(text("query.field.parameter-value")))
                    .child(mark("parameter-value".into(), crate::shell::chrome::text_field(&ed.parameter_value).full_width().into_any_element()))
                    .child(mark(
                        "parameter-set".into(),
                        crate::shell::chrome::action_button("query-parameter-set").small().label(text("query.set-parameter")).on_click(cx.listener(|this, _, _, cx| this.query_set_parameter(cx))).into_any_element(),
                    ));
            }
            (Side::Form, Some(form)) => {
                side = side.child(div().font_weight(FontWeight::BOLD).child(text(&format!("query.op.{}", form.op.id()))));
                for f in form.op.fields() {
                    side = side.child(div().text_size(px(12.)).child(text(&format!("query.field.{}", f.id))));
                    match f.kind {
                        Kind::Text => {
                            if let Some((_, state)) = form.inputs.iter().find(|(id, _)| *id == f.id) {
                                side = side.child(mark(format!("field-{}", f.id), crate::shell::chrome::text_field(state).full_width().into_any_element()));
                            }
                        }
                        Kind::Choice(options) => {
                            let current = form.picks.get(f.id).copied().unwrap_or(0).min(options.len() - 1);
                            let id = f.id;
                            let count = options.len();
                            side = side.child(mark(
                                format!("field-{id}"),
                                crate::shell::chrome::action_button(SharedString::from(format!("query-field-{id}")))
                                    .small()
                                    .label(text(options[current]))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(form) = this.query_editor_mut().and_then(|e| e.form.as_mut()) {
                                            let n = form.picks.get(id).copied().unwrap_or(0);
                                            form.picks.insert(id, (n + 1) % count);
                                        }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            ));
                        }
                        Kind::Flag => {
                            let on = form.picks.get(f.id).copied().unwrap_or(0) == 1;
                            let id = f.id;
                            side = side.child(mark(
                                format!("field-{id}"),
                                crate::xlsx::toggle::toggle_row(SharedString::from(format!("query-field-{id}")), on, text(&format!("query.field.{id}")), cx.listener(move |this, _, _, cx| {
                                    if let Some(form) = this.query_editor_mut().and_then(|e| e.form.as_mut()) {
                                        let n = form.picks.get(id).copied().unwrap_or(0);
                                        form.picks.insert(id, 1 - n.min(1));
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                            ));
                        }
                    }
                }
                side = side
                    .child(crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar().action(mark_action("apply".into(), crate::shell::chrome::ActionDefinition::new("query-apply", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("query.ok")).on_click(cx.listener(|this, _, _, cx| this.query_apply_form(cx))))).action(mark_action("form-cancel".into(), crate::shell::chrome::ActionDefinition::new("query-form-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("query.cancel")).on_click(cx.listener(|this, _, _, cx| {
                            if let Some(ed) = this.query_editor_mut() {
                                ed.form = None;
                                ed.message.clear();
                            }
                            cx.notify();
                        })))))));
            }
            (Side::Form, None) => {
                side = side.child(div().text_size(px(12.)).child(text("query.hint")));
            }
        }

        let status = div().w_full().px_3().py_1().border_t_1().border_color(theme.border).text_size(px(12.)).child(SharedString::from(if ed.message.is_empty() { crate::shell::i18n::format("query.status", &[("rows", &shown.rows.len()), ("columns", &shown.columns.len())]) } else { ed.message.clone() }));
        Some(
            v_flex()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .child(title)
                .child(tabs)
                .child(ribbon)
                .child(h_flex().flex_1().min_h_0().child(steps).child(table).child(side))
                .child(status)
                .into_any_element(),
        )
    }

    fn query_set_parameter(&mut self, cx: &mut Context<Self>) {
        let Some(ed) = self.sheet_state().and_then(|s| s.panels.query_editor.as_ref()) else { return };
        let name = ed.parameter_name.read(cx).value().trim().to_string();
        let value = ed.parameter_value.read(cx).value().to_string();
        if name.is_empty() {
            return;
        }
        if let Some(ed) = self.query_editor_mut() {
            match ed.parameters.iter_mut().find(|p| p.name == name) {
                Some(p) => p.value = value,
                None => ed.parameters.push(Parameter { name, value }),
            }
        }
        self.query_recompute();
        cx.notify();
    }
}

fn type_mark(ty: query::QType) -> &'static str {
    match ty {
        query::QType::Any => "ABC123",
        query::QType::Text => "ABC",
        query::QType::Whole => "123",
        query::QType::Decimal => "1.2",
        query::QType::Date => "📅",
        query::QType::DateTime => "📅",
        query::QType::Time => "⏱",
        query::QType::Logical => "T/F",
    }
}
