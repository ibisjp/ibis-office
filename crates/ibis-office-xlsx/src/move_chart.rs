// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフの移動… (Move Chart to Chart Sheet or Existing Sheet):
//! Move chart as an object to another worksheet (`ChartMoveTarget::ExistingSheet`),
//! or to a newly created sheet (`ChartMoveTarget::NewSheet`).
//! Supports Undo/Redo and workbook structure protection checks (XLSX-CHEDIT-020).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::{ChartMoveTarget, Command, ShapeId, ShapeKind, SheetCommand, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::ext::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MoveChartMode {
    NewSheet,
    ExistingSheet,
}

pub(crate) struct MoveChartPanel {
    pub(crate) chart_shape: ShapeId,
    pub(crate) mode: MoveChartMode,
    pub(crate) new_sheet_name: Entity<InputState>,
    pub(crate) target_slide: Option<SlideId>,
    pub(crate) error: Option<SharedString>,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

#[ibis_office_macros::editor_ext(MoveChartExt)]
impl SlideEditor {
    /// Opens the "Move Chart" (グラフの移動) sidebar panel for the selected chart.
    pub fn open_move_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chart_shape = if let Some((id, _)) = self.sheet_chart() {
            id
        } else if let Some(id) = self.slide().shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Chart(_) => Some(s.id),
            _ => None,
        }) {
            self.selection = Some(id);
            id
        } else {
            self.status = Some(text("_xlsx.select-a-chart").into());
            cx.notify();
            return;
        };

        self.commit_cell_edit(cx);
        self.wb_mut().close_settings();

        // Default new sheet name (e.g. "Chart1", "Chart2", ...)
        let mut n = 1;
        let default_name = loop {
            let candidate = format!("Chart{n}");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &candidate, None) {
                break candidate;
            }
            n += 1;
        };

        let new_sheet_name = cx.new(|cx| InputState::new(window, cx).default_value(default_name));

        // Find default existing sheet target (first other slide, or current slide)
        let other_slide = self
            .presentation()
            .workbook
            .as_deref()
            .and_then(|wb| {
                wb.sheets.iter().find_map(|s| {
                    s.slide.filter(|&id| id != self.current_slide)
                })
            })
            .unwrap_or(self.current_slide);

        self.wb_mut().panels.move_chart = Some(MoveChartPanel {
            chart_shape,
            mode: MoveChartMode::NewSheet,
            new_sheet_name,
            target_slide: Some(other_slide),
            error: None,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });

        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_move_chart_mode(&mut self, mode: MoveChartMode, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
            p.mode = mode;
            p.error = None;
        }
        cx.notify();
    }

    pub(crate) fn set_move_chart_target_slide(&mut self, slide: SlideId, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
            p.target_slide = Some(slide);
            p.mode = MoveChartMode::ExistingSheet;
            p.error = None;
        }
        cx.notify();
    }

    pub(crate) fn apply_move_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.move_chart.as_ref() else { return };
        let mode = p.mode;
        let chart_shape = p.chart_shape;
        let current_slide = self.current_slide;

        match mode {
            MoveChartMode::NewSheet => {
                let name = p.new_sheet_name.read(cx).text().to_string().trim().to_string();
                if self.presentation().is_workbook_structure_locked() {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.cannot-create-a-new-sheet-because-the-workbook-s").into());
                    }
                    cx.notify();
                    return;
                }
                if !ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.invalid-sheet-name").into());
                    }
                    cx.notify();
                    return;
                }
                let new_slide_id = self.document.ids().new_slide_id();
                let new_shape_id = self.document.ids().new_shape_id();

                self.run_sheet(
                    vec![Command::Sheet(SheetCommand::MoveChart {
                        from_slide: current_slide,
                        shape: chart_shape,
                        target: ChartMoveTarget::NewSheet {
                            name,
                            slide: new_slide_id,
                            sheet_shape: new_shape_id,
                        },
                    })],
                    cx,
                );

                self.wb_mut().panels.move_chart = None;
                self.sidebar_open = true;
                if self.presentation().slide(new_slide_id).is_some() {
                    self.select_slide(new_slide_id, window, cx);
                    self.selection = Some(chart_shape);
                    self.open_sheet_chart_panel(window,cx);
                }else{self.wb_mut().chart_edit=None;}
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            MoveChartMode::ExistingSheet => {
                let Some(dest_slide) = p.target_slide else {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.please-select-a-destination-sheet").into());
                    }
                    cx.notify();
                    return;
                };

                if dest_slide == current_slide {
                    // Moving within the same sheet is a no-op
                    self.wb_mut().panels.move_chart = None;
                    self.sidebar_open = true;
                    cx.notify();
                    return;
                }

                self.run_sheet(
                    vec![Command::Sheet(SheetCommand::MoveChart {
                        from_slide: current_slide,
                        shape: chart_shape,
                        target: ChartMoveTarget::ExistingSheet { slide: dest_slide },
                    })],
                    cx,
                );

                self.wb_mut().panels.move_chart = None;
                self.sidebar_open = true;
                if self.presentation().slide(dest_slide).is_some() {
                    self.select_slide(dest_slide, window, cx);
                    self.selection = Some(chart_shape);
                    self.open_sheet_chart_panel(window,cx);
                }else{self.wb_mut().chart_edit=None;}
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
        }
    }

    pub fn move_chart_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.move_chart.as_ref()?;
        let mode = p.mode;
        let is_locked = self.presentation().is_workbook_structure_locked();
        let target_slide = p.target_slide;
        let error = p.error.clone();
        let ui = p.ui.clone();

        let mark = move |id: &str, el: AnyElement| {
            let id = id.to_string();
            let ui = ui.clone();
            div()
                .relative()
                .child(el)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let id = id.to_string();
            let ui = p.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
        };

        let mut body = v_flex().gap_3();


        if let Some(err) = error {
            body = body.child(
                div()
                    .p(px(6.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(err),
            );
        }

        // 一番上のセグメント：選択で下のコントロールを切り替える。
        let seg_item = |id: &'static str, label: &'static str, on: bool, mode: MoveChartMode, disabled: bool, cx: &mut Context<Self>| {
            let ui = p.ui.clone();
            crate::shell::chrome::segment_control::SegmentItem::label(id, text(label), on)
                .disabled(disabled)
                .on_bounds(move |b| {
                    ui.borrow_mut().insert(id.to_string(), b);
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_move_chart_mode(mode, cx)))
        };
        let new_sheet_active = mode == MoveChartMode::NewSheet;
        let existing_sheet_active = mode == MoveChartMode::ExistingSheet;
        body = body.child(
            crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(seg_item("move-chart-mode-new", "_xlsx.new-sheet", new_sheet_active, MoveChartMode::NewSheet, is_locked, cx))
                .child(seg_item("move-chart-mode-existing", "_xlsx.object-in", existing_sheet_active, MoveChartMode::ExistingSheet, false, cx)),
        );

        if new_sheet_active {
            let new_sheet_input = crate::shell::chrome::text_field(&p.new_sheet_name).full_width().disabled(is_locked);
            body = body.child(mark("move-chart-new-name-input", new_sheet_input.into_any_element()));
        } else {
            let choices=self.presentation().workbook.as_deref().map(|wb|wb.sheets.iter().filter_map(|sheet|sheet.slide.map(|slide|(slide,sheet.name.clone()))).collect::<Vec<_>>()).unwrap_or_default();
            let current=choices.iter().find(|(id,_)|Some(*id)==target_slide).map(|(_,name)|name.clone()).unwrap_or_default();
            let entries=choices.into_iter().map(|(id,name)|crate::shell::chrome::MenuItem::new("move-chart-sheet",name).checked(Some(id)==target_slide).on(move|e,_,cx|e.set_move_chart_target_slide(id,cx)).into()).collect();
            let sheets_list=crate::shell::chrome::Dropdown::with_button("move-chart-sheets",crate::shell::chrome::dropdown_button("move-chart-sheets-button",current).disabled(is_locked),entries,true,0,&cx.weak_entity());
            body = body.child(sheets_list);
        }

        // Section 3: Action Buttons
        let ok_btn = crate::shell::chrome::ActionDefinition::new("move-chart-ok-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
            .label(text("OK"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.apply_move_chart(window, cx);
            }));

        let cancel_btn = crate::shell::chrome::ActionDefinition::new("move-chart-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
            .label(text("_shell.cancel"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.wb_mut().panels.move_chart = None;
                this.sidebar_open = true;
                cx.notify();
            }));

        let buttons_row = crate::shell::chrome::action_bar()
            .action(mark_action("move-chart-ok", ok_btn))
            .action(mark_action("move-chart-cancel", cancel_btn));



        Some(SidebarPanel::with_footer(text("_xlsx.move-chart"), body.into_any_element(), buttons_row))
    }
}
