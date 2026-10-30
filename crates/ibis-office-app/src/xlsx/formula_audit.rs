// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ワークシート分析: 数式の検証（Evaluate Formula）とウォッチ ウィンドウ（Watch Window）。
//!
//! Excel の「数式」>「ワークシート分析」にある:
//! 1. 「数式の検証」(Evaluate Formula): 選択セルの数式を最も内側の演算から順に
//!    1 ステップずつ評価（簡約）し、下線ハイライト・ステップイン・ステップアウト・やり直しを提供する。
//! 2. 「ウォッチ ウィンドウ」(Watch Window): ワークブック内の重要セル（シート、名前、セル、値、数式）を
//!    一覧登録・常時監視し、クリックでのセルジャンプ、再計算時の自動同期を提供する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_calc::eval::Book;
use ibis_calc::eval::step::{EvaluateSession, StepInfo};
use ibis_doc_model::{BookView, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 数式の検証パネル（サイドバー表示用）。
pub(crate) struct EvaluateFormulaPanel {
    pub(crate) session: Option<EvaluateSession>,
    pub(crate) info: StepInfo,
    pub(crate) target_cell_label: String,
    pub(crate) message: Option<String>,
}

/// ウォッチ ウィンドウの 1 登録項目。
#[derive(Clone, Debug, PartialEq)]
pub struct WatchEntry {
    pub id: u64,
    pub slide_id: SlideId,
    pub sheet_name: String,
    pub col: u32,
    pub row: u32,
    pub cell_ref: String,
    pub name: Option<String>,
    pub value_text: String,
    pub formula_text: Option<String>,
}

/// ウォッチ ウィンドウ パネル（サイドバー表示用）。
pub(crate) struct WatchWindowPanel {
    pub(crate) selected_id: Option<u64>,
    pub(crate) next_id: u64,
    pub(crate) editor: Option<Entity<InputState>>,
    scroll:ScrollHandle,
    message:String,
}

impl Default for WatchWindowPanel {
    fn default() -> Self {
        Self {
            selected_id: None,
            next_id: 1,
            editor:None,
            scroll:ScrollHandle::new(),
            message:String::new(),
        }
    }
}

impl SlideEditor {
    // =========================================================================
    // 数式の検証（Evaluate Formula）
    // =========================================================================

    /// 数式の検証パネルを開く。
    pub(crate) fn open_evaluate_formula(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);

        let Some((col, row)) = self.current_cell() else { return };
        let current_slide = self.current_slide;
        let book_view = BookView::new(self.presentation());
        let sheet_idx = book_view.index_of_slide(current_slide).unwrap_or(0);
        let sheet_name = book_view.sheet_name(sheet_idx).to_string();
        let cell_name_str = ibis_doc_model::cell_name(col, row);
        let cell_label = format!("{}!${}${}", sheet_name, cell_name_str.trim_end_matches(|c: char| c.is_ascii_digit()), row + 1);

        let formula_opt = book_view.formula(sheet_idx, row, col).map(|s| s.to_string());

        let panel = match formula_opt {
            Some(formula) => {
                match EvaluateSession::new(&book_view, sheet_idx, row, col) {
                    Some(session) => {
                        let info = session.info(&book_view);
                        EvaluateFormulaPanel {
                            session: Some(session),
                            info,
                            target_cell_label: cell_label,
                            message: None,
                        }
                    }
                    None => {
                        EvaluateFormulaPanel {
                            session: None,
                            info: StepInfo {
                                formula_text: format!("={formula}"),
                                underline_range: None,
                                can_step_in: false,
                                step_in_target: None,
                                is_finished: true,
                                result_value: None,
                                depth: 0,
                            },
                            target_cell_label: cell_label,
                            message: Some(text("_xlsx.failed-to-parse-the-formula").to_string()),
                        }
                    }
                }
            }
            None => {
                let cell_val = book_view.value(sheet_idx, row, col);
                let val_str = match &cell_val {
                    ibis_doc_model::CellValue::Empty => text("_xlsx.blank").to_string(),
                    ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                    ibis_doc_model::CellValue::Text(t) => t.clone(),
                    ibis_doc_model::CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                    ibis_doc_model::CellValue::Error(e) => e.to_string(),
                };
                EvaluateFormulaPanel {
                    session: None,
                    info: StepInfo {
                        formula_text: val_str,
                        underline_range: None,
                        can_step_in: false,
                        step_in_target: None,
                        is_finished: true,
                        result_value: Some(cell_val),
                        depth: 0,
                    },
                    target_cell_label: cell_label,
                    message: Some(text("_xlsx.the-cell-currently-being-evaluated-does-not-cont").to_string()),
                }
            }
        };

        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.evaluate_formula = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 数式の検証: 1 ステップ進める（検証）。
    pub(crate) fn evaluate_step(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        session.step(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: ステップ イン（参照先セルの数式へ潜る）。
    pub(crate) fn evaluate_step_in(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        let _ = session.step_in(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: ステップ アウト（親数式へ戻る）。
    pub(crate) fn evaluate_step_out(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        let _ = session.step_out(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: やり直し（最初に戻る）。
    pub(crate) fn evaluate_restart(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let _ = session.restart();
        let book_view = BookView::new(self.presentation());
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証パネル（サイドバー描画用）。
    pub(crate) fn evaluate_formula_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.evaluate_formula.as_ref()?;
        let cell_label = p.target_cell_label.clone();
        let info = p.info.clone();
        let message = p.message.clone();
        let has_session = p.session.is_some();

        let formula_str = &info.formula_text;
        let span = info.underline_range;

        // 式表示要素の構築（下線対象範囲のハイライト付き）
        let formula_display = if let Some((start, end)) = span {
            if start <= end && end <= formula_str.len() {
                let before = &formula_str[..start];
                let middle = &formula_str[start..end];
                let after = &formula_str[end..];
                h_flex()
                    .flex_wrap()
                    .font_family(".AppleSystemUIFontMonospaced")
                    .text_sm()
                    .child(SharedString::from(before.to_string()))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .border_b_2()
                            .border_color(rgb(0x1a73e8))
                            .bg(rgba(0x1a73e818))
                            .px(px(2.))
                            .child(SharedString::from(middle.to_string())),
                    )
                    .child(SharedString::from(after.to_string()))
            } else {
                h_flex().font_family(".AppleSystemUIFontMonospaced").text_sm().child(SharedString::from(formula_str.clone()))
            }
        } else {
            h_flex().font_family(".AppleSystemUIFontMonospaced").text_sm().child(SharedString::from(formula_str.clone()))
        };

        let mut body = v_flex()
            .gap_3()
            .px(px(16.))
            .py(px(12.))
            .p_0().child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().text_color(rgb(0x666666)).child(text("_xlsx.evaluation-cell")))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(cell_label))),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().text_color(rgb(0x666666)).child(text("_xlsx.formula-and-step-evaluation")))
                    .child(
                        div()
                            .p(px(8.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(rgb(0xd0d0d0))
                            .bg(rgb(0xf8f9fa))
                            .child(formula_display),
                    ),
            );

        if let Some(target) = &info.step_in_target {
            if info.can_step_in {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x1a73e8))
                        .child(format!("{} {} ({})", text("_xlsx.reference"), target, text("_xlsx.step-in-available"))),
                );
            }
        }

        if let Some(msg) = message {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xd93025))
                    .child(SharedString::from(msg)),
            );
        }

        if has_session {
            let is_finished = info.is_finished;
            let can_step_in = info.can_step_in;
            let depth = info.depth;

            let mut btn_row = crate::shell::chrome::action_bar();

            // 「検証」ボタン
            let mut eval_btn = crate::shell::chrome::ActionDefinition::new("eval-step", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_xlsx.evaluate"));
            if is_finished {
                eval_btn = eval_btn.disabled(true);
            }
            btn_row = btn_row.action(eval_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step();
                cx.notify();
            })));

            // 「ステップ イン」ボタン
            let mut step_in_btn = crate::shell::chrome::ActionDefinition::new("eval-step-in", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_xlsx.step-in"));
            if !can_step_in {
                step_in_btn = step_in_btn.disabled(true);
            }
            btn_row = btn_row.action(step_in_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step_in();
                cx.notify();
            })));

            // 「ステップ アウト」ボタン
            let mut step_out_btn = crate::shell::chrome::ActionDefinition::new("eval-step-out", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_xlsx.step-out"));
            if depth == 0 {
                step_out_btn = step_out_btn.disabled(true);
            }
            btn_row = btn_row.action(step_out_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step_out();
                cx.notify();
            })));

            // 「やり直し」ボタン
            btn_row = btn_row.action(
                crate::shell::chrome::ActionDefinition::new("eval-restart", "", crate::shell::chrome::ActionButtonKind::Normal)

                    .label(text("xlsx.eval.restart"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.evaluate_restart();
                        cx.notify();
                    })),
            );

            body = body.child(btn_row);
        }

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(
                crate::shell::chrome::ActionDefinition::new("eval-close", "", crate::shell::chrome::ActionButtonKind::Normal)
                    .label(text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.sheet_state_mut() {
                            s.panels.evaluate_formula = None;
                        }
                        this.sidebar_open = false;
                        cx.notify();
                    })),
            );
        Some(SidebarPanel::with_footer(text("_xlsx.evaluate-formula").to_string(), body.into_any_element(), panel_terminal_actions))
    }

    // =========================================================================
    // ウォッチ ウィンドウ（Watch Window）
    // =========================================================================

    /// ウォッチ ウィンドウ パネルを開く。
    pub(crate) fn open_watch_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.refresh_watches();

        let panel = WatchWindowPanel::default();
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.watch_window = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 現在選択中のセル（または複数セル選択範囲）をウォッチ式に追加する。
    pub(crate) fn add_watch_cell(&mut self) {
        let Some((cur_col, cur_row)) = self.current_cell() else { return };
        let current_slide = self.current_slide;

        let target_cells: Vec<(u32, u32)> = match (self.selected_range(), self.current_sheet()) {
            (Some(range), Some((_, sheet))) if range.first_row != range.last_row || range.first_column != range.last_column => {
                let total = ((range.last_column - range.first_column + 1) as u64) * ((range.last_row - range.first_row + 1) as u64);
                if total <= 64 {
                    let mut non_empty = Vec::new();
                    let mut all_cells = Vec::new();
                    for r in range.first_row..=range.last_row {
                        for c in range.first_column..=range.last_column {
                            all_cells.push((c, r));
                            if sheet.cell(r, c).is_some_and(|cell| !cell.value.is_empty() || cell.formula.is_some()) {
                                non_empty.push((c, r));
                            }
                        }
                    }
                    if non_empty.is_empty() { all_cells } else { non_empty }
                } else {
                    vec![(cur_col, cur_row)]
                }
            }
            _ => vec![(cur_col, cur_row)],
        };

        // 1. presentation から必要な情報を抽出
        let entries_data: Vec<(u32, u32, String, String, Option<String>, String, Option<String>)> = {
            let book_view = BookView::new(self.presentation());
            let sheet_idx = book_view.index_of_slide(current_slide).unwrap_or(0);
            let sheet_name = book_view.sheet_name(sheet_idx).to_string();

            target_cells
                .into_iter()
                .map(|(col, row)| {
                    let cell_name_str = ibis_doc_model::cell_name(col, row);
                    let col_letters = cell_name_str.trim_end_matches(|c: char| c.is_ascii_digit());
                    let cell_ref = format!("${}${}", col_letters, row + 1);

                    let name = self.presentation().workbook.as_ref().and_then(|wb| {
                        wb.names
                            .iter()
                            .find(|n| {
                                if n.hidden || n.name.starts_with("_xlnm.") {
                                    return false;
                                }
                                if n.sheet.is_some() && n.sheet != Some(sheet_idx) {
                                    return false;
                                }
                                let formula = n.formula.trim().trim_start_matches('=');
                                formula.contains(&format!("${}${}", col_letters, row + 1)) || formula.contains(&cell_name_str)
                            })
                            .map(|n| n.name.clone())
                    });

                    let cell_val = book_view.value(sheet_idx, row, col);
                    let value_text = match cell_val {
                        ibis_doc_model::CellValue::Empty => String::new(),
                        ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                        ibis_doc_model::CellValue::Text(t) => t,
                        ibis_doc_model::CellValue::Bool(b) => {
                            if b {
                                "TRUE".into()
                            } else {
                                "FALSE".into()
                            }
                        }
                        ibis_doc_model::CellValue::Error(e) => e.to_string(),
                    };
                    let formula_text = book_view.formula(sheet_idx, row, col).map(|f| format!("={f}"));
                    (col, row, sheet_name.clone(), cell_ref, name, value_text, formula_text)
                })
                .collect()
        };

        // 2. wb_mut にエントリを追加
        let s = self.wb_mut();
        for (col, row, sheet_name, cell_ref, name, value_text, formula_text) in entries_data {
            if s.panels.watches.iter().any(|w| w.slide_id == current_slide && w.col == col && w.row == row) {
                continue;
            }
            let max_existing = s.panels.watches.iter().map(|w| w.id).max().unwrap_or(0);
            let next_id = s.panels.watch_window.as_ref().map_or(max_existing + 1, |p| p.next_id.max(max_existing + 1));
            if let Some(p) = s.panels.watch_window.as_mut() {
                p.next_id = next_id + 1;
            }
            s.panels.watches.push(WatchEntry {
                id: next_id,
                slide_id: current_slide,
                sheet_name,
                col,
                row,
                cell_ref,
                name,
                value_text,
                formula_text,
            });
        }
    }

    /// ウォッチ項目を削除する。
    pub(crate) fn delete_watch(&mut self, id: u64) {
        let s = self.wb_mut();
        s.panels.watches.retain(|w| w.id != id);
        if let Some(p) = s.panels.watch_window.as_mut() {
            if p.selected_id == Some(id) {
                p.selected_id = None;
            }
        }
    }

    /// すべてのウォッチ項目を削除する。
    pub(crate) fn clear_all_watches(&mut self) {
        let s = self.wb_mut();
        s.panels.watches.clear();
        if let Some(p) = s.panels.watch_window.as_mut() {
            p.selected_id = None;
        }
    }

    /// ウォッチ項目のセルへジャンプ（該当シートを選択し、セルを選択）。
    pub(crate) fn jump_to_watch(&mut self, id: u64) {
        let entry_opt = self.sheet_state().and_then(|s| s.panels.watches.iter().find(|w| w.id == id).cloned());
        let Some(entry) = entry_opt else { return };

        // シート切り替え
        self.current_slide = entry.slide_id;
        // セル選択
        self.set_cell(entry.col, entry.row);
    }

    /// ウォッチ一覧の全セルの最新値・数式を更新する。
    pub(crate) fn refresh_watches(&mut self) {
        let updates: Vec<(u64, String, String, Option<String>)> = {
            let book_view = BookView::new(self.presentation());
            let Some(watches) = self.sheet_state().map(|s| &s.panels.watches) else { return };
            watches.iter().map(|entry| {
                let (sheet_name, value_text, formula_text) = if let Some(sheet_idx) = book_view.index_of_slide(entry.slide_id) {
                    let name = book_view.sheet_name(sheet_idx).to_string();
                    let cell_val = book_view.value(sheet_idx, entry.row, entry.col);
                    let val_str = match cell_val {
                        ibis_doc_model::CellValue::Empty => String::new(),
                        ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                        ibis_doc_model::CellValue::Text(t) => t,
                        ibis_doc_model::CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                        ibis_doc_model::CellValue::Error(e) => e.to_string(),
                    };
                    let formula_str = book_view.formula(sheet_idx, entry.row, entry.col).map(|f| format!("={f}"));
                    (name, val_str, formula_str)
                } else {
                    (entry.sheet_name.clone(), entry.value_text.clone(), entry.formula_text.clone())
                };
                (entry.id, sheet_name, value_text, formula_text)
            }).collect()
        };

        let s = self.wb_mut();
        for (id, sheet_name, value_text, formula_text) in updates {
            if let Some(entry) = s.panels.watches.iter_mut().find(|w| w.id == id) {
                entry.sheet_name = sheet_name;
                entry.value_text = value_text;
                entry.formula_text = formula_text;
            }
        }
    }

    pub(crate) fn edit_watch(&mut self,id:u64,w:&mut Window,cx:&mut Context<Self>){
        let Some(entry)=self.sheet_state().and_then(|s|s.panels.watches.iter().find(|e|e.id==id)).cloned()else{return;};
        let field=cx.new(|cx|InputState::new(w,cx));
        let reference=format!("{}!{}",ibis_calc::formula::quote_sheet(&entry.sheet_name),entry.cell_ref);
        field.update(cx,|i,cx|i.set_value(reference,w,cx));
        if let Some(p)=self.wb_mut().panels.watch_window.as_mut(){p.selected_id=Some(id);p.editor=Some(field);p.message.clear();}
        self.panel_dialog.bounds=None;cx.notify();
    }
    fn apply_watch_edit(&mut self,cx:&mut Context<Self>){
        let Some((id,value))=self.sheet_state().and_then(|s|s.panels.watch_window.as_ref()).and_then(|p|Some((p.selected_id?,p.editor.as_ref()?.read(cx).value().to_string())))else{return;};
        let target=super::sheet_ui::parse_reference(&value).and_then(|r|self.resolve_reference(&r));
        let Some((slide,r))=target else{if let Some(p)=self.wb_mut().panels.watch_window.as_mut(){p.message=text("xlsx.watch.invalid-cell").to_string();}return cx.notify();};
        if let Some(entry)=self.wb_mut().panels.watches.iter_mut().find(|e|e.id==id){entry.slide_id=slide;entry.col=r.first_column;entry.row=r.first_row;entry.cell_ref=ibis_doc_model::cell_name(entry.col,entry.row);}
        if let Some(p)=self.wb_mut().panels.watch_window.as_mut(){p.editor=None;p.message.clear();}
        self.refresh_watches();self.panel_dialog.bounds=None;cx.notify();
    }
    /// Watched cells use the shared table, with editing in a separate dialog.
    pub(crate) fn watch_window_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel>{
        let p=self.sheet_state()?.panels.watch_window.as_ref()?;
        if let Some(input)=&p.editor {
            let body=crate::shell::chrome::panel_form().p_0().child(crate::shell::chrome::text_field_row(text("fix.phonetic.cell"),super::range_field::local_range_field(input,"watch-cell-reference",true,cx))).children(crate::shell::chrome::panel_message(&p.message));
            let footer=crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("watch-edit-ok",text("OK"),crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,_,cx|e.apply_watch_edit(cx))))
                .action(crate::shell::chrome::ActionDefinition::new("watch-edit-cancel",text("_shell.cancel"),crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,_,cx|{if let Some(p)=e.wb_mut().panels.watch_window.as_mut(){p.editor=None;p.message.clear();}e.panel_dialog.bounds=None;cx.notify();})));
            return Some(SidebarPanel::with_footer(text("_xlsx.add-watch"),body.into_any_element(),footer).modal_named("watch_editor"));
        }
        let selected=p.selected_id;let scroll=p.scroll.clone();
        let header=crate::shell::chrome::table_row(["_xlsx.sheet","_shell.name","fix.phonetic.cell","_xlsx.values","_xlsx.equation"].into_iter().map(|key|text(key).into_any_element()).collect(),div().into_any_element(),false);
        let mut rows=v_flex();
        for item in &self.sheet_state()?.panels.watches {
            let id=item.id;
            let actions=h_flex().gap_1()
                .child(crate::shell::chrome::action_button(("watch-edit",id as usize)).w(px(28.)).h(px(28.)).px_0().icon(crate::shell::icons::Material("edit")).on_click(cx.listener(move|e,_,w,cx|e.edit_watch(id,w,cx))))
                .child(crate::shell::chrome::action_button(("watch-delete",id as usize)).w(px(28.)).h(px(28.)).px_0().icon(crate::shell::icons::Material("delete")).on_click(cx.listener(move|e,_,_,cx|{e.delete_watch(id);cx.notify();})));
            let cells=[item.sheet_name.clone(),item.name.clone().unwrap_or_default(),item.cell_ref.clone(),item.value_text.clone(),item.formula_text.clone().unwrap_or_default()].into_iter().map(|v|div().truncate().child(v).into_any_element()).collect();
            rows=rows.child(crate::shell::chrome::table_row(cells,actions.into_any_element(),selected==Some(id)));
        }
        if self.sheet_state()?.panels.watches.is_empty(){rows=rows.child(div().p_3().text_color(rgb(0x999999)).child(text("_xlsx.no-watch-cells-yet-click-add-watch-to-add")));}
        let body=crate::shell::chrome::panel_form().p_0()
            .child(h_flex().w_full().justify_end().child(crate::shell::chrome::action_button("watch-add").label(text("_xlsx.add-watch")).on_click(cx.listener(|e,_,_,cx|{e.add_watch_cell();cx.notify();}))))
            .child(crate::shell::chrome::table("watch-cells",header.into_any_element(),rows.into_any_element(),scroll,px(320.)));
        let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::dialog_close_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.watch-window"),body.into_any_element(),footer).modeless_named("watch_window_panel"))
    }
}
