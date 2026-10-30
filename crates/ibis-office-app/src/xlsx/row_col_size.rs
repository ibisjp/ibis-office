// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 行の高さ・列の幅（Row Height & Column Width）の数値指定パネル・コマンド.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EMU_PER_POINT, Emu, SheetCommand, sheet_command::Axis};

use super::dedupe_edit::column_name;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 行の高さ・列の幅の設定パネル.
pub(crate) struct RowColSizePanel {
    pub(crate) axis: Axis,
    pub(crate) value: Entity<InputState>,
    pub(crate) first: u32,
    pub(crate) last: u32,
    pub(crate) message: String,
    _subscription: Subscription,
}

impl SlideEditor {
    /// 行の高さパネルを開く (`xlsx.row-height`)
    pub(crate) fn open_row_height_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_row_col_size_panel(Axis::Rows, window, cx);
    }

    /// 列の幅パネルを開く (`xlsx.column-width`)
    pub(crate) fn open_column_width_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_row_col_size_panel(Axis::Columns, window, cx);
    }

    fn open_row_col_size_panel(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let r = self.selected_range().unwrap_or(ibis_doc_model::CellRange {
            first_row: 0,
            last_row: 0,
            first_column: 0,
            last_column: 0,
        });
        let (first, last, current_val_text) = match axis {
            Axis::Rows => {
                let (first, last) = self.selected_lines(Axis::Rows).unwrap_or((r.first_row, r.last_row));
                let h = sheet.rows.get(first as usize).copied().filter(|&h| h > 0).unwrap_or(sheet.default_row);
                let pt = if h > 0 { h as f32 / EMU_PER_POINT as f32 } else { 18.75 };
                (first, last, format!("{:.2}", pt))
            }
            Axis::Columns => {
                let (first, last) = self.selected_lines(Axis::Columns).unwrap_or((r.first_column, r.last_column));
                let w = sheet.columns.get(first as usize).copied().filter(|&w| w > 0).unwrap_or(sheet.default_column);
                // 1文字あたり約 7 pt (約 54 pt = 8.43 文字)
                let pt = if w > 0 { w as f32 / EMU_PER_POINT as f32 } else { 54.0 };
                let chars = (pt / 6.4).max(0.1);
                (first, last, format!("{:.2}", chars))
            }
        };

        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| {
            i.set_value(current_val_text.trim_end_matches('0').trim_end_matches('.').to_string(), window, cx);
            i.focus(window, cx);
        });
        let subscription = cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.apply_row_col_size(cx);
            }
        });

        self.wb_mut().panels.row_col_size = Some(RowColSizePanel {
            axis,
            value: input,
            first,
            last,
            message: String::new(),
            _subscription: subscription,
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// 行の高さ（pt単位）を直接設定
    pub(crate) fn set_selected_row_height(&mut self, height_pt: f32, cx: &mut Context<Self>) {
        let (first, last) = self.selected_lines(Axis::Rows).or_else(|| {
            self.selected_range().map(|r| (r.first_row, r.last_row))
        }).unwrap_or((0, 0));
        let emu = (height_pt.clamp(0.0, 409.0) * EMU_PER_POINT as f32).round() as Emu;
        let slide = self.current_slide;
        let _ = self.document.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first, last, size: emu })]);
        cx.notify();
    }

    /// 列の幅（文字数単位）を直接設定
    pub(crate) fn set_selected_column_width(&mut self, width_chars: f32, cx: &mut Context<Self>) {
        let (first, last) = self.selected_lines(Axis::Columns).or_else(|| {
            self.selected_range().map(|r| (r.first_column, r.last_column))
        }).unwrap_or((0, 0));
        let pt = width_chars.clamp(0.0, 255.0) * 6.4;
        let emu = (pt * EMU_PER_POINT as f32).round() as Emu;
        let slide = self.current_slide;
        let _ = self.document.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Columns, first, last, size: emu })]);
        cx.notify();
    }

    /// パネルで入力された数値を適用
    pub(crate) fn apply_row_col_size(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.row_col_size.as_ref()) else { return };
        let (axis, first, last) = (p.axis, p.first, p.last);
        let val_str = p.value.read(cx).value().trim().to_string();
        let num: f32 = match val_str.parse() {
            Ok(v) if v >= 0.0 => v,
            _ => {
                if let Some(p) = self.wb_mut().panels.row_col_size.as_mut() {
                    p.message = text("_xlsx.enter-a-valid-number").to_string();
                }
                return cx.notify();
            }
        };

        let emu: Emu = match axis {
            Axis::Rows => (num.clamp(0.0, 409.0) * EMU_PER_POINT as f32).round() as Emu,
            Axis::Columns => (num.clamp(0.0, 255.0) * 6.4 * EMU_PER_POINT as f32).round() as Emu,
        };

        let slide = self.current_slide;
        let _ = self.document.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis, first, last, size: emu })]);
        if let Some(p) = self.wb_mut().panels.row_col_size.as_mut() {
            p.message = text("_xlsx.applied").to_string();
        }
        cx.notify();
    }

    pub(crate) fn close_row_col_size_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.row_col_size = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    pub(crate) fn row_col_size_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.row_col_size.as_ref()?;
        let axis = p.axis;
        let title = match axis {
            Axis::Rows => text("_xlsx.row-height"),
            Axis::Columns => text("_xlsx.column-width"),
        };
        let label = match axis {
            Axis::Rows => text("_xlsx.row-height-pt"),
            Axis::Columns => text("_xlsx.column-width-chars"),
        };
        let (first, last) = (p.first, p.last);
        let span_text = match axis {
            Axis::Rows => format!("{} {}-{}", text("_xlsx.row"), first + 1, last + 1),
            Axis::Columns => format!("{} {}-{}", text("_xlsx.column"), column_name(first), column_name(last)),
        };
        let message = p.message.clone();
        // Row height in points (0–409), column width in characters (0–255), as `apply_row_col_size` clamps them.
        let spec = match axis {
            Axis::Rows => crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 409.0),
            Axis::Columns => crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 255.0),
        };

        let mut body = crate::shell::chrome::panel_form().gap(px(12.))
            .p_0().child(div().text_sm().text_color(cx.theme().muted_foreground).child(span_text))
            .child(
                v_flex().gap(px(4.))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                    .child(crate::shell::chrome::text_field(&p.value).full_width().spinner("row-col-size-value", spec, cx, |this, _, _, cx| this.apply_row_col_size(cx))),
            );

        if !message.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x107c41)).child(message));
        }

        let presets = match axis {
            Axis::Rows => vec![("13.5 pt".to_string(), 13.5f32), (text("xlsx.size.preset-default-row").to_string(), 18.75), ("24 pt".to_string(), 24.0), ("30 pt".to_string(), 30.0)],
            Axis::Columns => vec![
                (crate::shell::i18n::format("xlsx.size.preset-chars", &[("n", &4)]), 4.0f32),
                (text("xlsx.size.preset-default-column").to_string(), 8.43),
                (crate::shell::i18n::format("xlsx.size.preset-chars", &[("n", &12)]), 12.0),
                (crate::shell::i18n::format("xlsx.size.preset-chars", &[("n", &20)]), 20.0),
            ],
        };

        let mut preset_row = h_flex().gap(px(6.)).flex_wrap();
        for (name, val) in presets {
            preset_row = preset_row.child(
                crate::shell::chrome::action_button(format!("preset-{val}"))
                    .small()
                    .label(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(p) = this.sheet_state().and_then(|s| s.panels.row_col_size.as_ref()) {
                            p.value.update(cx, |i, cx| i.set_value(format!("{val}"), window, cx));
                        }
                        this.apply_row_col_size(cx);
                    })),
            );
        }

        body = body.child(v_flex().gap(px(4.)).child(div().text_sm().text_color(cx.theme().muted_foreground).child(text("_xlsx.presets"))).child(preset_row));
        let panel_actions = crate::shell::chrome::action_bar()
                .action(
                    crate::shell::chrome::ActionDefinition::new("row-col-size-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("_xlsx.apply"))
                        .on_click(cx.listener(|this, _, _, cx| this.apply_row_col_size(cx))),
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
