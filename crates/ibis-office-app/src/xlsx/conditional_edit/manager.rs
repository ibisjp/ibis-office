// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::*;
use crate::shell::{chrome, i18n::text,icons::Material};
impl SlideEditor {
 pub(crate) fn open_conditional_manager(&mut self,w:&mut Window,cx:&mut Context<Self>){self.open_conditional(None,w,cx);if let Some(p)=self.wb_mut().conditional_edit.as_mut(){p.manager=true;}self.focus_handle.focus(w,cx);cx.notify();}
 pub(super) fn conditional_manager_panel(&self,cx:&Context<Self>)->Option<SidebarPanel>{
  let p=self.wb().conditional_edit.as_ref()?;let(_,sheet)=self.current_sheet()?;
  let header=chrome::table_row_with_actions(vec![text("xlsx.cf.rule").into_any_element(),text("_shell.range").into_any_element()],div().into_any_element(),false,px(196.));
  let mark=|id:String,child:AnyElement|{let ui=p.ui.clone();div().relative().child(child).child(canvas(|_,_,_|(),move|b,_,_,_|{ui.borrow_mut().insert(id.clone(),b);}).absolute().inset_0()).into_any_element()};
        let mark_action = |id:String, action: crate::shell::chrome::ActionDefinition| {let ui=p.ui.clone();action.on_bounds(move |b, _, _| {ui.borrow_mut().insert(id.clone(),b);})
        };
  let mut rows=gpui_kit::component::v_flex();
  let selected=self.selected_range();
  let mut rules=sheet.conditional.iter().enumerate().filter(|(_,f)|!p.filter_selection_only||selected.is_some_and(|s|f.ranges.iter().any(|r|r.first_row<=s.last_row&&r.last_row>=s.first_row&&r.first_column<=s.last_column&&r.last_column>=s.first_column))).flat_map(|(i,f)|f.rules.iter().enumerate().map(move |(j,r)|(i,j,r,f))).collect::<Vec<_>>();rules.sort_by_key(|(_,_,r,_)|r.priority);
  for(i,j,r,f)in rules {
   let label=match &r.kind{CfKind::Expression(s)=>format!("={s}"),CfKind::CellIs{operator,formulas}=>format!("{} {}",OPERATORS.iter().find(|(op,_)|*op==operator).map(|(_,name)|text(name).to_string()).unwrap_or_else(||operator.clone()),formulas.join(", ")),CfKind::Top10{..}=>text("docx.conditional-quick.top-10-items").to_string(),_=>text("xlsx.cf.other-rule").to_string()};
   let ranges=f.ranges.iter().map(|r|format!("{}:{}",ibis_doc_model::cell_name(r.first_column,r.first_row),ibis_doc_model::cell_name(r.last_column,r.last_row))).collect::<Vec<_>>().join(", ");
   let editable=f.rules.len()==1&&matches!(r.kind,CfKind::Expression(_)|CfKind::CellIs{..}|CfKind::TimePeriod{..}|CfKind::Top10{..});
   let mut actions=h_flex().gap_1().child(mark(format!("edit-{i}"),chrome::action_button(("conditional-edit",i)).w(px(28.)).h(px(28.)).px_0().icon(Material("edit")).disabled(!editable).tooltip(text("_xlsx.edit")).on_click(cx.listener(move|e,_,w,cx|e.open_conditional(Some(i),w,cx))).into_any_element()));
   for (part,icon_name,label)in[(0,"content_copy","_shell.duplicate"),(1,"delete","_shell.delete"),(2,"arrow_upward","xlsx.cf.move-up"),(3,"arrow_downward","xlsx.cf.move-down")]{
    let key=match part{0=>format!("duplicate-{i}-{j}"),1=>format!("delete-{i}-{j}"),2=>format!("up-{i}-{j}"),_=>format!("down-{i}-{j}")};
    actions=actions.child(mark(key,chrome::action_button((SharedString::from(format!("conditional-manager-{i}-{j}")),part)).w(px(28.)).h(px(28.)).px_0().icon(Material(icon_name)).tooltip(text(label)).on_click(cx.listener(move|e,_,w,cx|{match part{0=>e.duplicate_conditional_rule(i,j,w,cx),1=>e.delete_conditional_rule(i,j,w,cx),2=>e.manage_conditional(i,j,Some(true),w,cx),_=>e.manage_conditional(i,j,Some(false),w,cx)}if let Some(p)=e.wb_mut().conditional_edit.as_mut(){p.manager=true;}e.focus_handle.focus(w,cx);cx.notify();})).into_any_element()));
   }
   actions=actions.child(mark(format!("rule-stop-{i}-{j}"),chrome::checkbox_button(("conditional-stop",i),"",r.stop,false,cx.listener(move|e,_,w,cx|{e.manage_conditional(i,j,None,w,cx);if let Some(p)=e.wb_mut().conditional_edit.as_mut(){p.manager=true;}e.focus_handle.focus(w,cx);cx.notify();})).into_any_element()));
   rows=rows.child(chrome::table_row_with_actions(vec![div().truncate().child(label).into_any_element(),div().truncate().child(ranges).into_any_element()],actions.into_any_element(),false,px(196.)));

  }
  let body=chrome::panel_form().p_0().p_0().child(h_flex().w_full().justify_end().child(chrome::action_button("conditional-add").label(text("_xlsx.add-rule")).on_click(cx.listener(|e,_,w,cx|e.open_conditional(None,w,cx)))))
    .child(h_flex().gap_2().child(mark("filter-sheet".into(),chrome::radio_button("conditional-filter-sheet",!p.filter_selection_only,false,text("xlsx.cf.filter-sheet"),cx.listener(|e,_,_,cx|{if let Some(p)=e.wb_mut().conditional_edit.as_mut(){p.filter_selection_only=false;}cx.notify();})).into_any_element())).child(mark("filter-selection".into(),chrome::radio_button("conditional-filter-selection",p.filter_selection_only,false,text("xlsx.cf.filter-selection"),cx.listener(|e,_,_,cx|{if let Some(p)=e.wb_mut().conditional_edit.as_mut(){p.filter_selection_only=true;}cx.notify();})).into_any_element())))
    .child(chrome::table("conditional-rules-table",header.into_any_element(),rows.into_any_element(),p.scroll.clone(),px(320.)));
  let footer=chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("conditional-manager-close", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.close")).on_click(cx.listener(|e,_,w,cx|e.close_panel_dialog(w,cx))))
    .action(mark_action("clear".into(), crate::shell::chrome::ActionDefinition::new("conditional-clear", "", crate::shell::chrome::ActionButtonKind::Destructive).label(text("_xlsx.delete-rules-in-selection")).on_click(cx.listener(|e,_,_,cx|e.clear_conditional_selection(cx)))));
  Some(SidebarPanel::with_footer(text("xlsx.cf.manage-title"),body.into_any_element(),footer))
 }
}
