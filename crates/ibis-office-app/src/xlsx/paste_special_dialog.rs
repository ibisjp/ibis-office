// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 形式を選択して貼り付け… (Paste Special) dialog panel.
//! Allows fine-grained selection of paste target (all, formulas, values, formats,
//! comments, validation, all except borders, column widths, number format combos),
//! arithmetic operations (none, add, subtract, multiply, divide), skip blanks,
//! transpose, and paste link.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use super::clipboard::{PasteOp, PasteSpecialOptions, PasteType};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::xlsx::toggle::toggle_row;
use crate::shell::chrome::{radio_button, radio_group};

pub(crate) struct PasteSpecialDialog {
    pub(crate) paste_type: PasteType,
    pub(crate) operation: PasteOp,
    pub(crate) skip_blanks: bool,
    pub(crate) transpose: bool,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

const PASTE_TYPE_OPTIONS: &[(PasteType, &str)] = &[
    (PasteType::All, "_xlsx.all"),
    (PasteType::Formulas, "_xlsx.equation"),
    (PasteType::Values, "_xlsx.values"),
    (PasteType::Formats, "_xlsx.formats"),
    (PasteType::Comments, "xlsx.paste-comments"),
    (PasteType::Validation, "xlsx.paste-validation"),
    (PasteType::AllExceptBorders, "xlsx.paste-all-except-borders"),
    (PasteType::ColumnWidths, "xlsx.paste-column-widths"),
    (PasteType::FormulasAndNumberFormats, "xlsx.paste-formulas-and-number-formats"),
    (PasteType::ValuesAndNumberFormats, "xlsx.paste-values-and-number-formats"),
];

const PASTE_OP_OPTIONS: &[(PasteOp, &str)] = &[
    (PasteOp::None, "xlsx.paste-none"),
    (PasteOp::Add, "_xlsx.add"),
    (PasteOp::Subtract, "_xlsx.subtract"),
    (PasteOp::Multiply, "_xlsx.multiply"),
    (PasteOp::Divide, "_xlsx.divide"),
];

impl SlideEditor {
    /// Open the 形式を選択して貼り付け… dialog panel (keyboard shortcut ⌘⌥V).
    pub(crate) fn open_paste_special_dialog(&mut self, cx: &mut Context<Self>) {
        if self.sheet_state().and_then(|s| s.clip.as_ref()).is_none() {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        }
        self.open_paste_special_dialog_panel(cx);
    }

    /// Always open the 形式を選択して貼り付け… dialog panel from the menu.
    pub(crate) fn open_paste_special_dialog_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let skip_blanks = self.sheet_state().is_some_and(|s| s.panels.skip_blanks);
        let state = self.wb_mut();
        state.close_settings();
        state.panels.paste_special_dialog = Some(PasteSpecialDialog {
            paste_type: PasteType::All,
            operation: PasteOp::None,
            skip_blanks,
            transpose: false,
            ui: Default::default(),
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// Apply the settings selected in the Paste Special dialog and close the panel.
    pub(crate) fn apply_paste_special_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.sheet_state().and_then(|s| s.panels.paste_special_dialog.as_ref()) else { return };
        let options = PasteSpecialOptions {
            paste_type: d.paste_type,
            operation: d.operation,
            skip_blanks: d.skip_blanks,
            transpose: d.transpose,
        };
        if let Some(state) = self.sheet_state_mut() {
            state.panels.paste_special_dialog = None;
        }
        self.apply_paste_special_options(options, cx);
    }

    /// Render the Paste Special sidebar panel.
    pub(crate) fn paste_special_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let d = self.sheet_state()?.panels.paste_special_dialog.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = d.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: String, action: crate::shell::chrome::ActionDefinition| {
            let ui = d.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let current_pt = d.paste_type;
        let current_op = d.operation;
        let skip_blanks = d.skip_blanks;
        let transpose = d.transpose;

        let mut body = v_flex().gap_2().px(px(14.)).py(px(8.)).p_0();

        // Group 1: 貼り付け (Paste Type)
        let mut pt_group = radio_group();
        for (i, &(pt, key)) in PASTE_TYPE_OPTIONS.iter().enumerate() {
            // Only use bundled symbols whose meaning matches the paste mode. Values have no
            // suitable existing symbol; reserve the same slot instead of implying plain text.
            let icon_name = match pt {
                PasteType::All => Some("content_paste"),
                PasteType::Formulas | PasteType::FormulasAndNumberFormats => Some("functions"),
                PasteType::Formats => Some("format_paint"),
                PasteType::Comments => Some("comment"),
                PasteType::Validation => Some("rule"),
                PasteType::AllExceptBorders => Some("border_clear"),
                PasteType::ColumnWidths => Some("view_column"),
                PasteType::Values | PasteType::ValuesAndNumberFormats => None,
            };
            let icon_slot = div().w(px(16.)).h(px(16.)).flex_none()
                .children(icon_name.map(|name| crate::shell::chrome::icon(name, 16., rgb(0x5f6368).into())));
            let label = crate::shell::i18n::text(key);
            pt_group = pt_group.child(mark(
                format!("paste-type-{i}"),
                radio_button(
                    SharedString::from(format!("pt-row-{i}")),
                    current_pt == pt,
                    false,
                    "",
                    cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.sheet_state_mut().and_then(|s| s.panels.paste_special_dialog.as_mut()) {
                            d.paste_type = pt;
                        }
                        cx.notify();
                    }),
                )
                .aria_label(label.clone())
                .gap_0()
                .child(div().w(px(8.)).flex_none())
                .child(icon_slot)
                .child(div().pl(px(8.)).text_sm().child(label))
                .into_any_element(),
            ));
        }


        // Group 2: 演算 (Operation)
        let mut op_group = radio_group();
        for (i, &(op, key)) in PASTE_OP_OPTIONS.iter().enumerate() {
            op_group = op_group.child(mark(
                format!("paste-op-{i}"),
                radio_button(
                    SharedString::from(format!("op-row-{i}")),
                    current_op == op,
                    false,
                    crate::shell::i18n::text(key),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.sheet_state_mut().and_then(|s| s.panels.paste_special_dialog.as_mut()) {
                            d.operation = op;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
        }
        body=body.child(h_flex().w_full().gap_3().items_start()
            .child(v_flex().flex_1().min_w_0().gap_2().child(crate::shell::i18n::text("_xlsx.paste")).child(pt_group))
            .child(v_flex().flex_1().min_w_0().gap_2().child(crate::shell::i18n::text("xlsx.paste-operation")).child(op_group)));

        // Group 3: オプション (Options)
        body = body.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x1f1f1f))
                .pt_1()
                .child(crate::shell::i18n::text("xlsx.paste-options")),
        );
        let mut opt_group = v_flex().gap_0p5();
        opt_group = opt_group.child(mark(
            "skip-blanks".into(),
            toggle_row(
                SharedString::from("skip-blanks-toggle"),
                skip_blanks,
                crate::shell::i18n::text("xlsx.paste-skip-blanks"),
                cx.listener(move |this, _, _, cx| {
                    if let Some(d) = this.sheet_state_mut().and_then(|s| s.panels.paste_special_dialog.as_mut()) {
                        d.skip_blanks = !d.skip_blanks;
                    }
                    cx.notify();
                }),
            )
            .into_any_element(),
        ));
        opt_group = opt_group.child(mark(
            "transpose".into(),
            toggle_row(
                SharedString::from("transpose-toggle"),
                transpose,
                crate::shell::i18n::text("_xlsx.transpose"),
                cx.listener(move |this, _, _, cx| {
                    if let Some(d) = this.sheet_state_mut().and_then(|s| s.panels.paste_special_dialog.as_mut()) {
                        d.transpose = !d.transpose;
                    }
                    cx.notify();
                }),
            )
            .into_any_element(),
        ));
        body = body.child(opt_group);

        // Group 4: Buttons (OK, Paste Link)

        let panel_terminal_actions = crate::shell::chrome::action_bar()
                .action(mark_action("ok".into(), crate::shell::chrome::ActionDefinition::new("paste-special-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(crate::shell::i18n::text("query.ok"))
                        .on_click(cx.listener(|this, _, _, cx| this.apply_paste_special_dialog(cx)))))
                .action(crate::shell::chrome::dialog_cancel_definition(cx))
                .action(mark_action("paste-link".into(), crate::shell::chrome::ActionDefinition::new("paste-special-link", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(crate::shell::i18n::text("_xlsx.paste-link"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.sheet_state_mut() {
                                state.panels.paste_special_dialog = None;
                            }
                            this.paste_link_special(cx);
                        }))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.paste-special"), body.into_any_element(), panel_terminal_actions))
    }
}
