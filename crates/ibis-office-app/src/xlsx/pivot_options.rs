// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ピボットテーブル オプション…: ピボットテーブルの名前・書式設定・総計・表示オプションの編集。
//! PivotTable Options panel (name, merge items, auto fit, preserve format, grand totals, captions, multiple filters).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::SlideId;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct PivotOptionsPanel {
    pub(crate) target: (SlideId, usize),
    pub(crate) name: Entity<InputState>,
    pub(crate) merge_item: bool,
    pub(crate) auto_fit_columns: bool,
    pub(crate) preserve_formatting: bool,
    pub(crate) multiple_field_filters: bool,
    pub(crate) row_grand_totals: bool,
    pub(crate) col_grand_totals: bool,
    pub(crate) grand_total_caption_enabled: bool,
    pub(crate) grand_total_caption: Entity<InputState>,
    pub(crate) row_header_caption_enabled: bool,
    pub(crate) row_header_caption: Entity<InputState>,
    pub(crate) col_header_caption_enabled: bool,
    pub(crate) col_header_caption: Entity<InputState>,
    pub(crate) refresh_on_load: bool,
    pub(crate) missing_caption_enabled: bool,
    pub(crate) missing_caption: Entity<InputState>,
    pub(crate) error_caption_enabled: bool,
    pub(crate) error_caption: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// データ > ピボットテーブル オプション…: open the pivot options panel.
    pub(crate) fn open_pivot_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let Some(pivot) = self.pivot_def(target) else { return };

        let name_val = pivot.name.clone();
        let name = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(name_val, window, cx);
            s
        });

        let grand_total_caption_enabled = pivot.grand_total_caption.is_some();
        let grand_total_val = pivot.grand_total_caption.clone().unwrap_or_else(|| text("_xlsx.grand-total").to_string());
        let grand_total_caption = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(grand_total_val, window, cx);
            s
        });

        let row_header_caption_enabled = pivot.row_header_caption.is_some();
        let row_header_val = pivot.row_header_caption.clone().unwrap_or_else(|| text("_xlsx.row-labels").to_string());
        let row_header_caption = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(row_header_val, window, cx);
            s
        });

        let col_header_caption_enabled = pivot.col_header_caption.is_some();
        let col_header_val = pivot.col_header_caption.clone().unwrap_or_else(|| text("_xlsx.column-labels").to_string());
        let col_header_caption = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(col_header_val, window, cx);
            s
        });

        let missing_caption_enabled = pivot.missing_caption.is_some();
        let missing_val = pivot.missing_caption.clone().unwrap_or_default();
        let missing_caption = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(missing_val, window, cx);
            s
        });

        let error_caption_enabled = pivot.error_caption.is_some();
        let error_val = pivot.error_caption.clone().unwrap_or_default();
        let error_caption = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(error_val, window, cx);
            s
        });

        let panel = PivotOptionsPanel {
            target,
            name,
            merge_item: pivot.merge_item,
            auto_fit_columns: pivot.auto_fit_columns,
            preserve_formatting: pivot.preserve_formatting,
            multiple_field_filters: pivot.multiple_field_filters,
            row_grand_totals: pivot.row_grand_totals,
            col_grand_totals: pivot.col_grand_totals,
            grand_total_caption_enabled,
            grand_total_caption,
            row_header_caption_enabled,
            row_header_caption,
            col_header_caption_enabled,
            col_header_caption,
            refresh_on_load: pivot.refresh_on_load,
            missing_caption_enabled,
            missing_caption,
            error_caption_enabled,
            error_caption,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };

        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            s.panels.pivot_options = Some(panel);
        }
        cx.notify();
    }

    /// Apply pivot table options to the active pivot table.
    pub(crate) fn apply_pivot_options(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.pivot_options.as_ref()) else { return };
        let target = p.target;
        let new_name = p.name.read(cx).value().to_string();
        let name_trimmed = new_name.trim().to_string();
        let merge_item = p.merge_item;
        let auto_fit_columns = p.auto_fit_columns;
        let preserve_formatting = p.preserve_formatting;
        let multiple_field_filters = p.multiple_field_filters;
        let row_grand_totals = p.row_grand_totals;
        let col_grand_totals = p.col_grand_totals;
        let missing_caption = if p.missing_caption_enabled {
            Some(p.missing_caption.read(cx).value().to_string())
        } else {
            None
        };
        let error_caption = if p.error_caption_enabled {
            Some(p.error_caption.read(cx).value().to_string())
        } else {
            None
        };

        let grand_total_caption = if p.grand_total_caption_enabled {
            let s = p.grand_total_caption.read(cx).value().trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };
        let row_header_caption = if p.row_header_caption_enabled {
            let s = p.row_header_caption.read(cx).value().trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };
        let col_header_caption = if p.col_header_caption_enabled {
            let s = p.col_header_caption.read(cx).value().trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };
        let refresh_on_load = p.refresh_on_load;

        self.change_pivot(
            target,
            move |pivot| {
                if !name_trimmed.is_empty() {
                    pivot.name = name_trimmed;
                }
                pivot.merge_item = merge_item;
                pivot.auto_fit_columns = auto_fit_columns;
                pivot.preserve_formatting = preserve_formatting;
                pivot.multiple_field_filters = multiple_field_filters;
                pivot.row_grand_totals = row_grand_totals;
                pivot.col_grand_totals = col_grand_totals;
                pivot.grand_total_caption = grand_total_caption;
                pivot.row_header_caption = row_header_caption;
                pivot.col_header_caption = col_header_caption;
                pivot.refresh_on_load = refresh_on_load;
                pivot.missing_caption = missing_caption;
                pivot.error_caption = error_caption;
            },
            cx,
        );

        if let Some(s) = self.sheet_state_mut() {
            s.panels.pivot_options = None;
        }
        cx.notify();
    }

    /// The PivotTable Options panel for the sidebar.
    pub(crate) fn pivot_options_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.pivot_options.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let merge_item = p.merge_item;
        let auto_fit_columns = p.auto_fit_columns;
        let preserve_formatting = p.preserve_formatting;
        let multiple_field_filters = p.multiple_field_filters;
        let row_grand_totals = p.row_grand_totals;
        let col_grand_totals = p.col_grand_totals;
        let grand_total_enabled = p.grand_total_caption_enabled;
        let row_header_enabled = p.row_header_caption_enabled;
        let col_header_enabled = p.col_header_caption_enabled;
        let refresh_on_load = p.refresh_on_load;
        let missing_enabled = p.missing_caption_enabled;
        let error_enabled = p.error_caption_enabled;

        let mut body = v_flex().gap_3().p_3().p_0();

        // 1. ピボットテーブル名
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.pivottable-name")))
                .child(mark("xlsx.pivot-options.name", crate::shell::chrome::text_field(&p.name).full_width().into_any_element())),
        );

        // 2. レイアウトと書式
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.layout-format")))
                .child(mark(
                    "xlsx.pivot-options.merge-item",
                    Checkbox::new("pivot-opt-merge")
                        .label(text("_xlsx.merge-and-center-cells"))
                        .checked(merge_item)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.merge_item = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.pivot-options.autofit",
                    Checkbox::new("pivot-opt-autofit")
                        .label(text("_xlsx.autofit-column-widths-on-update"))
                        .checked(auto_fit_columns)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.auto_fit_columns = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.pivot-options.preserve-format",
                    Checkbox::new("pivot-opt-preserve")
                        .label(text("_xlsx.preserve-cell-formatting-on-update"))
                        .checked(preserve_formatting)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.preserve_formatting = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.pivot-options.multiple-filters",
                    Checkbox::new("pivot-opt-filters")
                        .label(text("_xlsx.allow-multiple-filters-per-field"))
                        .checked(multiple_field_filters)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.multiple_field_filters = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                )),
        );

        // 3. 総計
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.grand-total")))
                .child(mark(
                    "xlsx.pivot-options.row-grand",
                    Checkbox::new("pivot-opt-row-grand")
                        .label(text("_xlsx.show-row-grand-totals"))
                        .checked(row_grand_totals)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.row_grand_totals = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.pivot-options.col-grand",
                    Checkbox::new("pivot-opt-col-grand")
                        .label(text("_xlsx.show-grand-totals-for-columns"))
                        .checked(col_grand_totals)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.col_grand_totals = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(mark(
                            "xlsx.pivot-options.grand-caption-enabled",
                            Checkbox::new("pivot-opt-grand-cb")
                                .label(text("_xlsx.grand-total-caption"))
                                .checked(grand_total_enabled)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                        p.grand_total_caption_enabled = *on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ))
                        .child(div().w(px(120.)).child(mark("xlsx.pivot-options.grand-caption", crate::shell::chrome::text_field(&p.grand_total_caption).full_width().into_any_element()))),
                ),
        );

        // 4. 表示（行/列ラベル、空白セル・エラー値）
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_shell.view")))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(mark(
                            "xlsx.pivot-options.row-caption-enabled",
                            Checkbox::new("pivot-opt-row-cb")
                                .label(text("_xlsx.row-label-display-name"))
                                .checked(row_header_enabled)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                        p.row_header_caption_enabled = *on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ))
                        .child(div().w(px(120.)).child(mark("xlsx.pivot-options.row-caption", crate::shell::chrome::text_field(&p.row_header_caption).full_width().into_any_element()))),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(mark(
                            "xlsx.pivot-options.col-caption-enabled",
                            Checkbox::new("pivot-opt-col-cb")
                                .label(text("_xlsx.column-labels-caption"))
                                .checked(col_header_enabled)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                        p.col_header_caption_enabled = *on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ))
                        .child(div().w(px(120.)).child(mark("xlsx.pivot-options.col-caption", crate::shell::chrome::text_field(&p.col_header_caption).full_width().into_any_element()))),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(mark(
                            "xlsx.pivot-options.missing-enabled",
                            Checkbox::new("pivot-opt-missing-cb")
                                .label(text("_xlsx.for-empty-cells-show"))
                                .checked(missing_enabled)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                        p.missing_caption_enabled = *on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ))
                        .child(div().w(px(100.)).child(mark("xlsx.pivot-options.missing-caption", crate::shell::chrome::text_field(&p.missing_caption).full_width().into_any_element()))),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(mark(
                            "xlsx.pivot-options.error-enabled",
                            Checkbox::new("pivot-opt-error-cb")
                                .label(text("_xlsx.for-error-values-show"))
                                .checked(error_enabled)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                        p.error_caption_enabled = *on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ))
                        .child(div().w(px(100.)).child(mark("xlsx.pivot-options.error-caption", crate::shell::chrome::text_field(&p.error_caption).full_width().into_any_element()))),
                ),
        );

        // 5. データ
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("fix.pivot-options.data")))
                .child(mark(
                    "xlsx.pivot-options.refresh-on-load",
                    Checkbox::new("pivot-opt-refresh")
                        .label(text("_xlsx.refresh-data-when-opening-the-file"))
                        .checked(refresh_on_load)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.pivot_options.as_mut()) {
                                p.refresh_on_load = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                )),
        );

        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(cx.theme().danger).child(p.message.clone()));
        }

        // 5. Actions (OK, キャンセル)
        let mut actions = crate::shell::chrome::action_bar();
        actions = actions.action(mark_action("xlsx.pivot-options.ok", crate::shell::chrome::ActionDefinition::new("pivot-opt-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_pivot_options(cx);
                }))));
        actions = actions.action(mark_action("xlsx.pivot-options.cancel", crate::shell::chrome::ActionDefinition::new("pivot-opt-cancel", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(text("_shell.cancel"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.panels.pivot_options = None;
                    }
                    cx.notify();
                }))));


        Some(SidebarPanel::with_footer(text("_xlsx.pivottable-options"), body.into_any_element(), actions))
    }
}
