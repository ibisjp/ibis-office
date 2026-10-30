// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Numeric column width and row height controls for the caret's table cell.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent,InputState};
use gpui_kit::component::{Selectable as _,Sizable as _,h_flex,v_flex};
use gpui_kit::*;
use ibis_doc_model::{Block,FlowTable,FlowCommand,RowHeight};
use ibis_doc_model::flow_command;
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};

#[derive(Clone,Copy,PartialEq,Eq)]
pub(crate) enum HeightRule {Auto,AtLeast,Exact}
pub(crate) struct TableSizeState {
    pub(crate) fields:[Entity<InputState>;2],
    pub(crate) rule:HeightRule,
    target:(u32,usize,usize),
    column:usize,
    table:FlowTable,
    initial:([String;2],HeightRule),
    error:Option<String>,
    _subscriptions:Vec<Subscription>,
}
impl SlideEditor {
    pub(crate) fn caret_table(&self)->Option<&FlowTable> {
        let (id,_,_)=self.table_at_caret()?;
        let d=self.flow()?;
        let (container,index)=flow_command::locate(d,id)?;
        match flow_command::blocks(d,container)?.get(index)? {Block::Table(t)=>Some(t),_=>None}
    }
    pub(crate) fn open_table_size(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target)=self.table_at_caret() else{return};
        let Some(table)=self.caret_table().cloned() else{return};
        let Some(row)=table.rows.get(target.1) else{return};
        let column=row.cells[..target.2].iter().map(|c|c.span.max(1) as usize).sum::<usize>();
        let Some(width)=table.columns.get(column) else{return};
        let (rule,height)=match row.height {RowHeight::Auto=>(HeightRule::Auto,0),RowHeight::AtLeast(h)=>(HeightRule::AtLeast,h),RowHeight::Exact(h)=>(HeightRule::Exact,h)};
        let initial=[format!("{:.2}",*width as f64/12700.),format!("{:.2}",height as f64/12700.)];
        let fields=initial.clone().map(|value|cx.new(|cx|{let mut f=InputState::new(window,cx);f.set_value(value,window,cx);f}));
        let subscriptions=fields.iter().map(|f|cx.subscribe_in(f,window,|this,_,event:&InputEvent,window,cx|{
            if let InputEvent::PressEnter {..}=event {this.apply_table_size(window,cx);}
        })).collect();
        if let Some(state)=self.doc_state_mut() {
            state.table_size=Some(TableSizeState {fields,rule,target,column,table,initial:(initial,rule),error:None,_subscriptions:subscriptions});
            state.picture_edit=None;state.font_edit=None; state.color_edit=None; state.paragraph_edit=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    pub(crate) fn choose_table_height(&mut self,rule:HeightRule,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.table_size.as_mut()) {
            if edit.rule!=rule {
                if edit.rule==HeightRule::Auto || rule==HeightRule::Auto {
                    let field=edit.fields[1].clone();
                    field.update(cx,|i,cx|i.set_value(if rule==HeightRule::Auto {"0"}else{"12"},window,cx));
                }
                edit.rule=rule;
            }
        }
        cx.notify();
    }
    pub(crate) fn apply_table_size(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.table_size.as_ref()) else{return};
        let values=edit.fields.clone().map(|f|f.read(cx).value().trim().to_string());
        let target=edit.target;let column=edit.column;let rule=edit.rule;
        let changed=[values[0]!=edit.initial.0[0],(rule!=HeightRule::Auto && values[1]!=edit.initial.0[1]) || rule!=edit.initial.1];
        let numbers=values.clone().map(|v|v.parse::<f64>().ok().filter(|v|v.is_finite() && *v<=1584.));
        let valid=match numbers {[Some(w),Some(h)]=>w>=0.05 && if rule==HeightRule::Auto {h==0.}else{h>=0.05},_=>false};
        if !valid || self.table_at_caret()!=Some(target) || self.caret_table()!=Some(&edit.table) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.table_size.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.table-size-panel.check-table-selection-values-width").into());}
            cx.notify();return;
        }
        if !changed.iter().any(|v|*v) {self.focus_handle.focus(window,cx);return;}
        let mut table=edit.table.clone();
        let emu=|v:f64|(v*20.).round() as i64*635;
        if changed[0] {table.columns[column]=emu(numbers[0].unwrap());}
        if changed[1] {table.rows[target.1].height=match rule {HeightRule::Auto=>RowHeight::Auto,HeightRule::AtLeast=>RowHeight::AtLeast(emu(numbers[1].unwrap())),HeightRule::Exact=>RowHeight::Exact(emu(numbers[1].unwrap()))};}
        if table==edit.table {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.table_size.as_mut()) {edit.initial=(values,rule);edit.error=None;}
            self.focus_handle.focus(window,cx);return;
        }
        let Some((container,index))=self.flow().and_then(|d|flow_command::locate(d,target.0)) else{return};
        if self.run_flow(vec![FlowCommand::ReplaceBlocks {container,index,remove:1,blocks:vec![Block::Table(table.clone())]}],cx) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.table_size.as_mut()) {edit.table=table;edit.initial=(values,rule);edit.error=None;}
            self.focus_handle.focus(window,cx);
        }
        cx.notify();
    }
    pub(crate) fn table_size_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let edit=self.doc_state()?.table_size.as_ref()?;
        let fields=edit.fields.clone();let rule=edit.rule;let error=edit.error.clone();
        let field_cx=&*cx;
        // Column width and row height (pt): ▲▼ by 1 pt, applied as Enter applies them.
        let row=|label:&'static str,i:usize|h_flex().gap_2().child(div().w(px(80.)).text_sm().child(crate::shell::i18n::text(label)))
            .child(crate::shell::chrome::text_field(&fields[i]).full_width().unit("pt",px(22.)).spinner(format!("table-size-{i}"),crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0,1584.0),field_cx,|this,_,window,cx|this.apply_table_size(window,cx)));
        let choice=|id:&'static str,label:&'static str,r:HeightRule|crate::shell::chrome::radio_button(id, rule==r, false, crate::shell::i18n::text(label), cx.listener(move|this,_,window,cx|this.choose_table_height(r,window,cx)));
        let body=v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("docx.table-size-panel.changes-width-column-where")))
            .child(row("docx.table-size-panel.column-width-pt",0))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow).child(choice("row-auto","_shell.auto",HeightRule::Auto)).child(choice("row-minimum","_shell.at-least",HeightRule::AtLeast)).child(choice("row-exact","docx.common.exactly",HeightRule::Exact)))
            .child(row("docx.table-size-panel.row-height-pt",1))
            .children(error.map(|e|div().text_sm().child(e)))
            ;
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("table-size-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this,_,window,cx|this.apply_table_size(window,cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.table-size-panel.table-column-width-row-height"), body.into_any_element(), panel_actions))
    }
}
