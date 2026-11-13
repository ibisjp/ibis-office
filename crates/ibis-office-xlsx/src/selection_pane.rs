// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 選択ウィンドウ (配置 > 選択ウィンドウ / 表示 > 選択ウィンドウ):
//! ワークシート上の図形・画像・グラフの一覧を表示し、選択、表示/非表示（目のアイコン）、
//! 名前の変更、順序の入れ替え（↑ / ↓）を行うサイドバーパネル。

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::rc::Rc;
use crate::shell::palette::rgb;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{Command, Shape, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::ext::*;

/// The pane, while open: the shape being renamed and its input field.
#[derive(Default)]
pub struct SelectionPanePanel {
    pub renaming: Option<(ShapeId, Entity<InputState>)>,
    _subscription: Option<Subscription>,
    reorder:crate::shell::chrome::data_table::ListState,
}

#[ibis_office_macros::editor_ext(SelectionPane)]
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
    pub fn open_xlsx_selection_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let state = self.wb_mut();
        state.close_settings();
        state.panels.selection_pane = Some(SelectionPanePanel::default());
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択ウィンドウを閉じる.
    pub fn close_xlsx_selection_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn toggle_sheet_shape_hidden(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let is_hidden = self.slide().hidden_shapes.contains(&shape_id);
        let hidden = !is_hidden;
        if hidden && self.selection == Some(shape_id) {
            self.selection = None;
        }
        self.run_sheet(vec![Command::SetShapeHidden { slide, shape: shape_id, hidden }], cx);
    }

    /// シート上の全図形をすべて表示 / すべて非表示.
    pub fn set_all_sheet_shapes_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
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
    pub fn start_sheet_shape_renaming(&mut self, shape_id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn finish_sheet_shape_renaming(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.wb_mut().panels.selection_pane.as_mut().and_then(|p| p.renaming.take()) else { return };
        let name = field.read(cx).value().trim().to_string();
        let slide = self.current_slide;
        if !name.is_empty() && self.slide().shape_names.get(&id) != Some(&name) && self.slide().shape(id).is_some() {
            self.run_sheet(vec![Command::SetShapeName { slide, shape: id, name }], cx);
        }
    }

    /// 選択ウィンドウのサイドバーパネルUIを生成.
    pub fn xlsx_selection_pane_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
        if !is_open {
            return None;
        }

        let slide = self.slide();
        // shapes[0] is sheet, get shapes from 1.. in reverse (frontmost at top)
        let floating_shapes: Vec<(usize, Shape)> = slide.shapes.iter().enumerate().skip(1).map(|(i, s)| (i, s.clone())).collect();
        let renaming_id = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(id, _)| *id);
        let renaming_field = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(_, f)| f.clone());

        use crate::shell::chrome::{self,data_table::{DataTable,no_drag},virtual_table::{TableColumn,TableRow}};
        let reorder=self.wb().panels.selection_pane.as_ref()?.reorder.clone();
        let editor=cx.entity().downgrade();
        let all_hidden=!floating_shapes.is_empty()&&floating_shapes.iter().all(|(_,s)|slide.hidden_shapes.contains(&s.id));
        let all_editor=editor.clone();
        let header=vec![
            chrome::action_button("xlsx-selpane-visibility-all").w(px(28.)).h(px(28.)).px_0().icon(Material(if all_hidden{"visibility_off"}else{"visibility"})).tooltip(crate::shell::i18n::text("pptx.selection-pane.show-all")).on_click(move |_,_,cx|{let _=all_editor.update(cx,|e,cx|e.set_all_sheet_shapes_hidden(!all_hidden,cx));}).into_any_element(),
            div().child(crate::shell::i18n::text("fix.goto-special.select")).into_any_element(),
        ];
        // What each row shows (front to back), worked out once: (id, index in the sheet, label, hidden, selected).
        let selected_ids=self.selected_ids();
        let items:Rc<Vec<(ShapeId,usize,String,bool,bool)>>=Rc::new(floating_shapes.iter().rev().map(|(index,shape)|(shape.id,*index,self.xlsx_shape_label(shape,*index),slide.hidden_shapes.contains(&shape.id),selected_ids.contains(&shape.id))).collect());
        let ids:Rc<Vec<ShapeId>>=Rc::new(items.iter().map(|i|i.0).collect());
        let count=items.len();
        let build={
            let (items,editor,renaming_id,renaming_field)=(items.clone(),editor.clone(),renaming_id,renaming_field.clone());
            move |range:std::ops::Range<usize>,_:&mut Window,_:&mut App|->Vec<TableRow>{
                range.filter_map(|row_index|items.get(row_index).map(|it|(row_index,it.clone()))).map(|(row_index,(id,index,label,hidden,selected))|{
                    let toggle=editor.clone();
                    let eye=no_drag(chrome::action_button(format!("xlsx-selpane-eye-{index}")).w(px(28.)).h(px(28.)).px_0().icon(Material(if hidden{"visibility_off"}else{"visibility"})).on_click(move |_,_,cx|{let _=toggle.update(cx,|e,cx|e.toggle_sheet_shape_hidden(id,cx));})).into_any_element();
                    let pick=editor.clone();
                    let name=if renaming_id==Some(id) {no_drag(div().w_full().min_w_0().child(chrome::text_field(renaming_field.as_ref().unwrap()).full_width())).w_full().into_any_element()} else {
                        div().w_full().min_w_0().truncate().text_sm().when(hidden,|d|d.text_color(rgb(0x888888))).child(SharedString::from(label.clone())).into_any_element()
                    };
                    let rename=editor.clone();
                    let _=&pick;
                    TableRow::new(vec![eye,name]).selected(selected)
                        .on_click(move |ev,_,cx|{let m=ev.modifiers();let _=pick.update(cx,|e,cx|{if m.shift||m.platform||m.control{e.toggle_select_sheet_shape(id);}else{e.select_sheet_shape(id);}cx.notify();});})
                        .draggable(row_index as u64,SharedString::from(label))
                        .drop_between(row_index as u64)
                        .action("xlsx-selpane-rename","edit",crate::shell::i18n::text("pptx.selection-pane.rename"),move |_,w,cx|{let _=rename.update(cx,|e,cx|e.start_sheet_shape_renaming(id,w,cx));})
                }).collect()
            }
        };
        let table=DataTable::reorder_list("sheet-selection-table",vec![TableColumn::fixed("eye",32.),TableColumn::flex("name")],header,&reorder,count,build,move |e,from,to,_,cx|{
            if let (Some(&shape),Some(&target))=(ids.get(from),ids.get(to)){
                if let Some(index)=e.slide().shapes.iter().position(|s|s.id==target){let slide=e.current_slide;e.run_sheet(vec![Command::MoveShape{slide,shape,index}],cx);}
            }
        },cx).row_actions(1);
        let body=chrome::panel_form().p_0().size_full().child(div().w_full().flex_1().min_h_0().child(table));

        Some(SidebarPanel {
            title: crate::shell::i18n::text("fix.goto-special.select"),
            body: body.into_any_element(),
        }.fill())
    }
}
