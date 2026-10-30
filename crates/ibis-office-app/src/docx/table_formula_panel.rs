// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Table formula insertion and edit panel (Word's 表の計算式).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::{chrome::SidebarPanel, editor::SlideEditor};

pub(crate) const FORMULA_FORMATS: [(&str, &str); 6] = [
    ("", "_docx.none-2"),
    ("#,##0", "#,##0"),
    ("#,##0.00", "#,##0.00"),
    ("¥#,##0", "¥#,##0"),
    ("0%", "0%"),
    ("0.00%", "0.00%"),
];

pub(crate) const PASTE_FUNCTIONS: [&str; 8] = [
    ("SUM()"),
    ("AVERAGE()"),
    ("COUNT()"),
    ("MAX()"),
    ("MIN()"),
    ("PRODUCT()"),
    ("ROUND()"),
    ("ABS()"),
];

pub(crate) const DIRECTION_KEYWORDS: [&str; 4] = ["ABOVE", "LEFT", "BELOW", "RIGHT"];

pub(crate) struct TableFormulaState {
    pub(crate) formula: Entity<InputState>,
    pub(crate) format_index: usize,
    pub(crate) target: (u32, usize, usize),
    pub(crate) error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl SlideEditor {
    /// キャレット位置が表の中にあるとき、「表の計算式」サイドバーを開く。
    pub(crate) fn open_table_formula(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target) = self.table_at_caret() else { return };
        let default_formula = self.suggest_table_formula();
        let formula = cx.new(|cx| {
            let mut f = InputState::new(window, cx);
            f.set_value(&default_formula, window, cx);
            f.focus(window, cx);
            f
        });
        let subscriptions = vec![
            cx.subscribe_in(&formula, window, |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.apply_table_formula(window, cx);
                }
            }),
        ];
        if let Some(state) = self.doc_state_mut() {
            state.table_formula = Some(TableFormulaState {
                formula,
                format_index: 0,
                target,
                error: None,
                _subscriptions: subscriptions,
            });
            state.table_size = None;
            state.picture_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 表示形式（書式スイッチ）を選択する。
    pub(crate) fn set_table_formula_format(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut().and_then(|s| s.table_formula.as_mut()) {
            state.format_index = idx;
        }
        cx.notify();
    }

    /// 入力欄に関数（例: `SUM()`）を挿入・上書きする。
    pub(crate) fn paste_function_to_formula(&mut self, func: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.table_formula.as_ref()) else { return };
        let field = state.formula.clone();
        field.update(cx, |i, cx| {
            let cur_val = i.value();
            let cur = cur_val.trim();
            let new_val = if cur.is_empty() || cur == "=" {
                format!("={func}")
            } else if cur.starts_with('=') {
                format!("={func}")
            } else {
                format!("={func}")
            };
            i.set_value(&new_val, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// 入力欄に方向キーワード（`ABOVE`, `LEFT` 等）を付加する。
    pub(crate) fn paste_direction_to_formula(&mut self, dir: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.table_formula.as_ref()) else { return };
        let field = state.formula.clone();
        field.update(cx, |i, cx| {
            let cur_val = i.value();
            let cur = cur_val.trim();
            let new_val = if cur.ends_with('(') {
                format!("{cur}{dir})")
            } else if cur.is_empty() || cur == "=" {
                format!("=SUM({dir})")
            } else {
                format!("{cur} {dir}")
            };
            i.set_value(&new_val, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// 「表の計算式」サイドバーの入力内容を適用してフィールドを挿入する。
    pub(crate) fn apply_table_formula(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.table_formula.as_ref()) else { return };
        let formula_code = state.formula.read(cx).value().trim().to_string();
        let format_opt = FORMULA_FORMATS.get(state.format_index).map(|(fmt, _)| *fmt).filter(|f| !f.is_empty());
        let target = state.target;

        if formula_code.is_empty() || formula_code == "=" {
            if let Some(state) = self.doc_state_mut().and_then(|s| s.table_formula.as_mut()) {
                state.error = Some(crate::shell::i18n::text("docx.table-formula-panel.please-enter-formula").into());
            }
            cx.notify();
            return;
        }

        // キャレットが同じセル内にあることを確認
        if self.table_at_caret() != Some(target) {
            if let Some(state) = self.doc_state_mut().and_then(|s| s.table_formula.as_mut()) {
                state.error = Some(crate::shell::i18n::text("_docx.select-the-target-cell-in-the-table").into());
            }
            cx.notify();
            return;
        }

        self.insert_table_formula(&formula_code, format_opt, cx);
        if let Some(state) = self.doc_state_mut() {
            state.table_formula = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 「表の計算式」サイドバーパネルの描画要素。
    pub(crate) fn table_formula_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.table_formula.as_ref()?;
        let formula_input = state.formula.clone();
        let format_idx = state.format_index;
        let error = state.error.clone();

        let mut format_buttons = h_flex().gap_1().flex_wrap();
        for (i, (_, label)) in FORMULA_FORMATS.iter().enumerate() {
            let selected = i == format_idx;
            format_buttons = format_buttons.child(
                crate::shell::chrome::radio_button(format!("table-formula-fmt-{i}"),selected,false,crate::shell::i18n::text(label),cx.listener(move |this, _, _, cx| this.set_table_formula_format(i, cx))),
            );
        }

        let mut func_buttons = h_flex().gap_1().flex_wrap();
        for func in PASTE_FUNCTIONS {
            func_buttons = func_buttons.child(
                crate::shell::chrome::action_button(format!("table-formula-func-{func}"))
                    .small()
                    .label(crate::shell::i18n::text(func))
                    .on_click(cx.listener(move |this, _, window, cx| this.paste_function_to_formula(func, window, cx))),
            );
        }

        let mut dir_buttons = h_flex().gap_1().flex_wrap();
        for dir in DIRECTION_KEYWORDS {
            dir_buttons = dir_buttons.child(
                crate::shell::chrome::action_button(format!("table-formula-dir-{dir}"))
                    .small()
                    .label(crate::shell::i18n::text(dir))
                    .on_click(cx.listener(move |this, _, window, cx| this.paste_direction_to_formula(dir, window, cx))),
            );
        }

        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("docx.table-formula-panel.insert-formula-into-table-cell")))
            .child(div().text_sm().child(crate::shell::i18n::text("docx.table-formula-panel.formula-2")))
            .child(crate::shell::chrome::text_field(&formula_input).full_width())
            .child(div().text_sm().child(crate::shell::i18n::text("_shell.format")))
            .child(format_buttons)
            .child(div().text_sm().child(crate::shell::i18n::text("docx.table-formula-panel.paste-function")))
            .child(func_buttons)
            .child(div().text_sm().child(crate::shell::i18n::text("docx.table-formula-panel.direction-keywords")))
            .child(dir_buttons)
            .children(error.map(|e| div().text_sm().child(e)))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("table-formula-apply", crate::shell::i18n::text("OK"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, window, cx| this.apply_table_formula(window, cx))),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("table-formula-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(state) = this.doc_state_mut() {
                                    state.table_formula = None;
                                }
                                this.focus_handle.focus(window, cx);
                                cx.notify();
                            })),
                    );
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.table-formula-panel.formula"), body.into_any_element(), panel_actions))
    }
}
