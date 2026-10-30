// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ふりがな（ルビ）の編集と表示設定 (`XLSX-CELL-006`).
//!
//! セルのふりがな文字列を設定・編集するパネル (`PhoneticPanel`)、
//! およびふりがなの表示/非表示トグル (`toggle_show_phonetic`).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct PhoneticPanel {
    pub(crate) input: Entity<InputState>,
    pub(crate) target_cell: (u32, u32),
    pub(crate) cell_name: String,
}

impl SlideEditor {
    /// ふりがな編集パネルを開く.
    pub(crate) fn open_phonetic_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((col, row)) = self.current_cell() else { return };
        let slide_id = self.current_slide;
        let s = self.presentation().slide(slide_id);
        let sheet = s.and_then(ibis_doc_model::sheet_calc::sheet_of_slide);
        let existing = sheet
            .and_then(|sh| sh.cell(row, col))
            .and_then(|c| c.phonetic.as_deref())
            .unwrap_or("")
            .to_string();

        let state = cx.new(|cx| InputState::new(window, cx));
        state.update(cx, |i, cx| i.set_value(existing, window, cx));

        let cell_name = ibis_doc_model::cell_name(col, row);
        let panel = PhoneticPanel {
            input: state,
            target_cell: (row, col),
            cell_name,
        };

        self.begin_sheet_settings();
        let wb = self.wb_mut();
        wb.panels.phonetic = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// ふりがなを適用.
    pub(crate) fn apply_phonetic(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.phonetic.as_ref()) else { return };
        let (row, column) = p.target_cell;
        let value = p.input.read(cx).value().trim().to_string();
        let phonetic = if value.is_empty() { None } else { Some(value) };
        let slide = self.current_slide;

        let cmd = SheetCommand::SetCellPhonetic {
            slide,
            row,
            column,
            phonetic,
        };
        let _ = self.document.execute(vec![Command::Sheet(cmd)]);
        self.wb_mut().panels.phonetic = None;
        self.sidebar_open = false;
        cx.notify();
    }

    /// ふりがなを解除.
    pub(crate) fn clear_phonetic(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.phonetic.as_ref()) else { return };
        let (row, column) = p.target_cell;
        let slide = self.current_slide;

        let cmd = SheetCommand::SetCellPhonetic {
            slide,
            row,
            column,
            phonetic: None,
        };
        let _ = self.document.execute(vec![Command::Sheet(cmd)]);
        self.wb_mut().panels.phonetic = None;
        self.sidebar_open = false;
        cx.notify();
    }

    /// ふりがなの表示/非表示トグル.
    pub(crate) fn toggle_show_phonetic(&mut self, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let current_show = self
            .presentation()
            .slide(slide_id)
            .and_then(ibis_doc_model::sheet_calc::sheet_of_slide)
            .map(|s| s.show_phonetic)
            .unwrap_or(false);

        let cmd = SheetCommand::SetShowPhonetic {
            slide: slide_id,
            show: !current_show,
        };
        let _ = self.document.execute(vec![Command::Sheet(cmd)]);
        cx.notify();
    }

    /// ふりがな編集のサイドバーパネル描画.
    pub(crate) fn phonetic_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.phonetic.as_ref()?;
        let cell_label = p.cell_name.clone();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                div()
                    .text_sm()
                    .child(format!("{}: {}", text("fix.phonetic.cell"), cell_label)),
            )
            .child(div().text_sm().child(text("fix.phonetic.furigana")))
            .child(crate::shell::chrome::text_field(&p.input).full_width())
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("phonetic-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(text("_shell.apply"))
                            .on_click(cx.listener(|this, _, _, cx| this.apply_phonetic(cx))),
                    )

            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(
                        crate::shell::chrome::ActionDefinition::new("phonetic-clear", "", crate::shell::chrome::ActionButtonKind::Destructive)
                            .label(text("_shell.delete"))
                            .on_click(cx.listener(|this, _, _, cx| this.clear_phonetic(cx))),
                    );
        Some(SidebarPanel::with_footer(text("fix.phonetic.edit-furigana").to_string(), body.into_any_element(), panel_actions))
    }
}
