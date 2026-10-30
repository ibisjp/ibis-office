// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 選択ウィンドウ (配置 > 選択ウィンドウ / 表示 > 選択ウィンドウ):
//! ワークシート上の図形・画像・グラフの一覧を表示し、選択、表示/非表示（目のアイコン）、
//! 名前の変更、順序の入れ替え（↑ / ↓）を行うサイドバーパネル。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use std::{rc::Rc,cell::RefCell};
use crate::shell::palette::rgb;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{Command, Shape, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// The pane, while open: the shape being renamed and its input field.
#[derive(Default)]
pub(crate) struct SelectionPanePanel {
    pub(crate) renaming: Option<(ShapeId, Entity<InputState>)>,
    _subscription: Option<Subscription>,
    reorder:Rc<RefCell<crate::shell::chrome::data_table::RowReorder>>,
}

impl SlideEditor {
    /// 選択ウィンドウの表示 / 非表示トグル.
    pub(crate) fn toggle_xlsx_selection_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_open = self.wb().panels.selection_pane.is_some();
        if is_open {
            self.close_xlsx_selection_pane(window, cx);
        } else {
            self.open_xlsx_selection_pane(window, cx);
        }
    }

    /// 選択ウィンドウを開く.
    pub(crate) fn open_xlsx_selection_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let state = self.wb_mut();
        state.close_settings();
        state.panels.selection_pane = Some(SelectionPanePanel::default());
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択ウィンドウを閉じる.
    pub(crate) fn close_xlsx_selection_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().panels.selection_pane = None;
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 図形の表示名を取得（設定された名前、または種類に応じた連番名）.
    pub(crate) fn xlsx_shape_label(&self, shape: &Shape, index: usize) -> String {
        let slide = self.slide();
        if let Some(name) = slide.shape_names.get(&shape.id) {
            return name.clone();
        }
        match &shape.kind {
            ShapeKind::TextBox(_) => crate::shell::i18n::format("_xlsx.text-box-n", &[("n", &index)]),
            ShapeKind::Picture(_) => crate::shell::i18n::format("_xlsx.picture-n", &[("n", &index)]),
            ShapeKind::Chart(_) => crate::shell::i18n::format("_xlsx.chart-n", &[("n", &index)]),
            ShapeKind::Line(_) => crate::shell::i18n::format("_xlsx.line-n", &[("n", &index)]),
            ShapeKind::Auto(_) => crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &index)]),
            _ => crate::shell::i18n::format("_xlsx.object-n", &[("n", &index)]),
        }
    }

    /// 図形の表示 / 非表示切り替え（目のアイコン）.
    pub(crate) fn toggle_sheet_shape_hidden(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let is_hidden = self.slide().hidden_shapes.contains(&shape_id);
        let hidden = !is_hidden;
        if hidden && self.selection == Some(shape_id) {
            self.selection = None;
        }
        self.run_sheet(vec![Command::SetShapeHidden { slide, shape: shape_id, hidden }], cx);
    }

    /// シート上の全図形をすべて表示 / すべて非表示.
    pub(crate) fn set_all_sheet_shapes_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let s = self.slide();
        // shapes[0] is the worksheet itself, skip index 0
        let commands: Vec<Command> = s
            .shapes
            .iter()
            .skip(1)
            .filter(|sh| !s.is_attached(sh.id) && s.hidden_shapes.contains(&sh.id) != hidden)
            .map(|sh| Command::SetShapeHidden { slide, shape: sh.id, hidden })
            .collect();
        if hidden {
            self.selection = None;
        }
        self.run_sheet(commands, cx);
    }

    /// 名前のインライン変更を開始.
    pub(crate) fn start_sheet_shape_renaming(&mut self, shape_id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pos) = self.slide().shapes.iter().skip(1).position(|s| s.id == shape_id) else { return };
        let shape = &self.slide().shapes[pos + 1];
        let name = self.xlsx_shape_label(shape, pos + 1);
        let field = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(&field, window, move |e: &mut Self, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                e.finish_sheet_shape_renaming(window, cx);
            }
        });
        field.update(cx, |f, cx| f.focus(window, cx));
        if let Some(p) = self.wb_mut().panels.selection_pane.as_mut() {
            p.renaming = Some((shape_id, field));
            p._subscription = Some(subscription);
        }
        cx.notify();
    }

    /// 名前のインライン変更を確定.
    pub(crate) fn finish_sheet_shape_renaming(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.wb_mut().panels.selection_pane.as_mut().and_then(|p| p.renaming.take()) else { return };
        let name = field.read(cx).value().trim().to_string();
        let slide = self.current_slide;
        if !name.is_empty() && self.slide().shape_names.get(&id) != Some(&name) && self.slide().shape(id).is_some() {
            self.run_sheet(vec![Command::SetShapeName { slide, shape: id, name }], cx);
        }
    }

    /// 選択ウィンドウのサイドバーパネルUIを生成.
    pub(crate) fn xlsx_selection_pane_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
        if !is_open {
            return None;
        }

        let slide = self.slide();
        // shapes[0] is sheet, get shapes from 1.. in reverse (frontmost at top)
        let floating_shapes: Vec<(usize, Shape)> = slide.shapes.iter().enumerate().skip(1).map(|(i, s)| (i, s.clone())).collect();
        let renaming_id = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(id, _)| *id);
        let renaming_field = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(_, f)| f.clone());

        use crate::shell::chrome::{self,data_table::{DataTable,row_controls}};
        let reorder=self.wb().panels.selection_pane.as_ref()?.reorder.clone();
        let all_hidden=!floating_shapes.is_empty()&&floating_shapes.iter().all(|(_,s)|slide.hidden_shapes.contains(&s.id));
        let header=h_flex().h_full().px_2().gap_2().items_center()
            .child(chrome::action_button("xlsx-selpane-visibility-all").w(px(28.)).h(px(28.)).px_0().icon(Material(if all_hidden{"visibility_off"}else{"visibility"})).tooltip(crate::shell::i18n::text("pptx.selection-pane.show-all")).on_click(cx.listener(move |e,_,_,cx|e.set_all_sheet_shapes_hidden(!all_hidden,cx))))
            .child(crate::shell::i18n::text("fix.goto-special.select"));
        let mut rows=Vec::new();
        let ids=floating_shapes.iter().rev().map(|(_,s)|s.id).collect::<Vec<_>>();
        for (row_index,(index,shape)) in floating_shapes.iter().rev().enumerate(){
            let id=shape.id;let hidden=slide.hidden_shapes.contains(&id);let selected=self.selected_ids().contains(&id);
            let eye=chrome::action_button(format!("xlsx-selpane-eye-{index}")).w(px(28.)).h(px(28.)).px_0().icon(Material(if hidden{"visibility_off"}else{"visibility"})).on_click(cx.listener(move |e,_,_,cx|e.toggle_sheet_shape_hidden(id,cx)));
            let name=if renaming_id==Some(id) {div().flex_1().min_w_0().child(chrome::text_field(renaming_field.as_ref().unwrap()).full_width()).into_any_element()} else {
                div().flex_1().min_w_0().truncate().child(self.xlsx_shape_label(shape,*index)).on_mouse_down(MouseButton::Left,cx.listener(move |e,event:&MouseDownEvent,_,cx|{if event.modifiers.shift||event.modifiers.platform||event.modifiers.control{e.toggle_select_sheet_shape(id);}else{e.select_sheet_shape(id);}cx.notify();})).into_any_element()
            };
            let edit=chrome::action_button(format!("xlsx-selpane-rename-{index}")).w(px(28.)).h(px(28.)).px_0().icon(Material("edit")).on_click(cx.listener(move |e,_,w,cx|e.start_sheet_shape_renaming(id,w,cx)));
            let row=h_flex().h_full().px_2().gap_2().items_center().text_sm().when(hidden,|d|d.text_color(rgb(0x888888)))
                .child(row_controls(reorder.clone(),row_index,0,eye)).child(name).child(row_controls(reorder.clone(),row_index,1,edit)).child(Icon::new(Material("drag_handle")).size(px(18.)));
            rows.push((row_index,row.into_any_element(),selected));
        }
        let table=DataTable::new("sheet-selection-table",Some(header.into_any_element()),rows,cx).reorderable(reorder,move |e,from,to,_,cx|{
            if let (Some(&shape),Some(&target))=(ids.get(from),ids.get(to)){
                if let Some(index)=e.slide().shapes.iter().position(|s|s.id==target){let slide=e.current_slide;e.run_sheet(vec![Command::MoveShape{slide,shape,index}],cx);}
            }
        });
        let body=chrome::panel_form().p_0().child(table);

        Some(SidebarPanel {
            title: crate::shell::i18n::text("fix.goto-special.select"),
            body: body.into_any_element(),
        })
    }
}
