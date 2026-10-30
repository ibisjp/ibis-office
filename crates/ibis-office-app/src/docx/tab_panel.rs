// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Tab stops and leader settings panel for docx paragraphs.

use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Emu, FlowDocument, ParaChange, TabAlign, TabChange, TabStop};

use crate::shell::{chrome::SidebarPanel, editor::SlideEditor};

pub(crate) struct TabEditState {
    pub(crate) editor: Option<Option<usize>>,
    pub(crate) pos_field: Entity<InputState>,
    pub(crate) align: TabAlign,
    pub(crate) leader: bool,
    pub(crate) stops: Vec<TabStop>,
    pub(crate) original_stops: Vec<TabStop>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) target: (u32, u32),
    pub(crate) document: Arc<FlowDocument>,
    pub(crate) error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

fn emu_to_pt(emu: Emu) -> f64 {
    emu as f64 / 12_700.0
}

fn pt_to_emu(pt: f64) -> Emu {
    (pt * 20.0).round() as i64 * 635
}

fn format_pt(pt: f64) -> String {
    format!("{:.1}", pt)
}

fn align_name(align: TabAlign) -> gpui::SharedString {
    match align {
        TabAlign::Left => crate::shell::i18n::text("docx.page-number.bottom-left"),
        TabAlign::Center => crate::shell::i18n::text("_shell.center-2"),
        TabAlign::Right => crate::shell::i18n::text("docx.page-number.bottom-right"),
        TabAlign::Decimal => crate::shell::i18n::text("_docx.decimal"),
    }
}

impl SlideEditor {
    pub(crate) fn open_tab_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.selected_paragraphs() else { return };
        let Some(document) = self.flow().cloned() else { return };
        let Some(p) = self.paragraph_of(target.0) else { return };
        let original_stops = p.tabs.clone();
        let stops = original_stops.clone();
        let is_empty = p.tabs.is_empty();
        let initial_val = stops.first().map_or("72".to_string(), |s| format_pt(emu_to_pt(s.position)));
        let initial_align = stops.first().map_or(TabAlign::Left, |s| s.align);
        let initial_leader = stops.first().map_or(false, |s| s.leader);

        let pos_field = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial_val, window, cx);
            state
        });

        let subscription = cx.subscribe_in(&pos_field, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                if this.doc_state().and_then(|s|s.tab_edit.as_ref()).is_some_and(|p|p.editor.is_some()){this.finish_tab_row_editor(window,cx);}else{this.set_tab_stop(window,cx);}
            }
        });

        if let Some(state) = self.doc_state_mut() {
            state.tab_edit = Some(TabEditState {
                editor: None,
                pos_field,
                align: initial_align,
                leader: initial_leader,
                stops,
                original_stops,
                selected_index: if is_empty { None } else { Some(0) },
                target,
                document,
                error: None,
                _subscriptions: vec![subscription],
            });
            state.helper=Default::default();state.ruby=None;state.merge=None;state.text_box=None;
            state.font_edit = None;
            state.color_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
            state.paragraph_edit = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn choose_tab_align(&mut self, align: TabAlign, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.align = align;
        cx.notify();
    }

    pub(crate) fn choose_tab_leader(&mut self, leader: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.leader = leader;
        cx.notify();
    }

    pub(crate) fn select_tab_stop(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        if let Some(stop) = edit.stops.get(index).copied() {
            edit.selected_index = Some(index);
            edit.align = stop.align;
            edit.leader = stop.leader;
            let val = format_pt(emu_to_pt(stop.position));
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value(val, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn set_tab_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        let text = edit.pos_field.read(cx).value().trim().to_string();
        let Ok(pt) = text.parse::<f64>() else {
            edit.error = Some(crate::shell::i18n::text("_docx.enter-a-number-for-the-tab-position").into());
            cx.notify();
            return;
        };
        if pt <= 0.0 || !pt.is_finite() || pt > 1584.0 {
            edit.error = Some(crate::shell::i18n::text("_docx.enter-a-value-greater-than-0-and-up-to-1584").into());
            cx.notify();
            return;
        };
        let emu = pt_to_emu(pt);
        let align = edit.align;
        let leader = edit.leader;
        edit.stops.retain(|s| s.position != emu);
        edit.stops.push(TabStop { position: emu, align, leader });
        edit.stops.sort_by_key(|s| s.position);
        edit.selected_index = edit.stops.iter().position(|s| s.position == emu);
        edit.error = None;
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(format_pt(pt), window, cx));
        cx.notify();
    }

    pub(crate) fn clear_tab_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        if let Some(idx) = edit.selected_index && idx < edit.stops.len() {
            edit.stops.remove(idx);
            edit.selected_index = None;
            edit.error = None;
        } else {
            let text = edit.pos_field.read(cx).value().trim().to_string();
            if let Ok(pt) = text.parse::<f64>() {
                let emu = pt_to_emu(pt);
                edit.stops.retain(|s| s.position != emu);
                edit.selected_index = None;
                edit.error = None;
            }
        }
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        cx.notify();
    }

    pub(crate) fn clear_all_tab_stops(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.stops.clear();
        edit.selected_index = None;
        edit.error = None;
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        cx.notify();
    }

    pub(crate) fn apply_tab_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.tab_edit.as_ref()) else { return };
        let (first, last) = edit.target;
        let original = edit.original_stops.clone();
        let new_stops = edit.stops.clone();

        // Build list of TabChanges
        let mut changes = Vec::new();
        // Clear stops that were removed
        for old in &original {
            if !new_stops.iter().any(|s| s.position == old.position) {
                changes.push(TabChange { position: old.position, stop: None });
            }
        }
        // Set new or updated stops
        for new in &new_stops {
            changes.push(TabChange { position: new.position, stop: Some(*new) });
        }

        let commands = self.paragraph_format_commands(first, last, ParaChange::Tabs(changes));
        if commands.is_empty() || self.run_flow(commands, cx) {
            let document = self.flow().unwrap().clone();
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) {
                edit.original_stops = new_stops;
                edit.document = document;
                edit.error = None;
            }
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn open_tab_row_editor(&mut self, index:Option<usize>, w:&mut Window,cx:&mut Context<Self>) {
        if let Some(i)=index {self.select_tab_stop(i,w,cx);}
        if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()) {
            p.editor=Some(index);p.error=None;
            p.pos_field.update(cx,|i,cx|i.focus(w,cx));
            if index.is_none() {p.align=TabAlign::Left;p.leader=false;let next=p.stops.last().map_or(72.,|s|emu_to_pt(s.position)+72.);let value=if next<=1584. {format_pt(next)}else{String::new()};p.pos_field.update(cx,|i,cx|i.set_value(value,w,cx));}
        } cx.notify();
    }
    pub(crate) fn finish_tab_row_editor(&mut self,w:&mut Window,cx:&mut Context<Self>) {
        let mode=self.doc_state().and_then(|s|s.tab_edit.as_ref()).and_then(|p|p.editor);
        // Moving a stop replaces the edited stop instead of leaving its previous position behind.
        let valid=self.doc_state().and_then(|s|s.tab_edit.as_ref()).is_some_and(|p|p.pos_field.read(cx).value().trim().parse::<f64>().is_ok_and(|n|n>0.&&n<=1584.&&n.is_finite()));
        if !valid {self.set_tab_stop(w,cx);return;}
        let duplicate=self.doc_state().and_then(|s|s.tab_edit.as_ref()).is_some_and(|p| {
            let position=pt_to_emu(p.pos_field.read(cx).value().trim().parse::<f64>().unwrap());
            p.stops.iter().enumerate().any(|(i,stop)|Some(Some(i))!=mode && stop.position==position)
        });
        if duplicate {if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.error=Some(crate::shell::i18n::text("ui.tab-duplicate").to_string());}cx.notify();return;}
        if let Some(Some(i))=mode {if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()) {if i<p.stops.len(){p.stops.remove(i);}}}
        self.set_tab_stop(w,cx);self.apply_tab_edit(w,cx);
        if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.editor=None;}cx.notify();
    }
    pub(crate) fn delete_live_tab(&mut self,index:usize,w:&mut Window,cx:&mut Context<Self>) {
        self.select_tab_stop(index,w,cx);self.clear_tab_stop(w,cx);self.apply_tab_edit(w,cx);
        if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.editor=None;}cx.notify();
    }
    pub(crate) fn tab_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let target=self.selected_paragraphs();let doc=self.flow().cloned();
        if let (Some(target),Some(doc))=(target,doc) {
            let refresh=self.doc_state()?.tab_edit.as_ref().is_some_and(|p|p.editor.is_none() && (p.target!=target || p.document!=doc));
            if refresh {
                let stops=self.paragraph_of(target.0).map(|p|p.tabs.clone()).unwrap_or_default();
                if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.target=target;p.document=doc;p.stops=stops.clone();p.original_stops=stops;p.selected_index=None;}
            }
        }
        let p=self.doc_state()?.tab_edit.as_ref()?;
        let stops=p.stops.clone();let selected=p.selected_index;
        let header = h_flex().items_center().h_full().px_2().gap_1()
            .child(div().w(px(56.)).flex_none().text_sm().child(crate::shell::i18n::text("fix.help-panels.location")))
            .child(div().w(px(70.)).flex_none().text_sm().child(crate::shell::i18n::text("_shell.arrange")))
            .child(div().flex_1().min_w_0().text_sm().child(crate::shell::i18n::text("_docx.leader")));
        let rows = stops.into_iter().enumerate().map(|(i, stop)| {
            let on = selected == Some(i);
            let row = h_flex().id(("tab-row", i)).items_center().size_full().px_2().cursor_pointer()
                .when(on, |d| d.bg(rgb(crate::shell::chrome::SELECTED_ROW_BG)))
                .gap_1()
                .child(div().w(px(56.)).flex_none().text_sm().child(format!("{} pt", format_pt(emu_to_pt(stop.position)))))
                .child(div().w(px(70.)).flex_none().text_sm().child(align_name(stop.align)))
                .child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(crate::shell::i18n::text(if stop.leader { "_docx.dotted" } else { "_shell.none" })))
                .when(on, |d| d
                    .child(crate::shell::chrome::action_button(("tab-edit", i)).small().w(px(crate::shell::chrome::BUTTON_HEIGHT)).px_0()
                        .icon(crate::shell::icons::Material("edit"))
                        .on_click(cx.listener(move |e, _, w, cx| { cx.stop_propagation(); e.open_tab_row_editor(Some(i), w, cx); })))
                    .child(crate::shell::chrome::action_button(("tab-delete", i)).ghost().small().w(px(crate::shell::chrome::BUTTON_HEIGHT)).px_0()
                        .icon(crate::shell::icons::Material("delete"))
                        .on_click(cx.listener(move |e, _, w, cx| { cx.stop_propagation(); e.delete_live_tab(i, w, cx); }))))
                .on_click(cx.listener(move |e, event: &ClickEvent, w, cx| {
                    e.select_tab_stop(i, w, cx);
                    if event.click_count() == 2 { e.open_tab_row_editor(Some(i), w, cx); }
                }));
            (i, row.into_any_element(), on)
        }).collect();
        let table = crate::shell::chrome::data_table::DataTable::new("docx-tabs-table", Some(header.into_any_element()), rows, cx);
        let body=v_flex().px(px(12.)).gap_2()
            .p_0().child(h_flex().justify_end().child(crate::shell::chrome::primary_button("tab-add").label(crate::shell::i18n::text("ui.add")).on_click(cx.listener(|e,_,w,cx|e.open_tab_row_editor(None,w,cx)))))
            .child(table);
        Some(SidebarPanel{title:crate::shell::i18n::text("_docx.tabs-and-leaders"),body:body.into_any_element()})
    }
    pub(crate) fn tab_row_dialog(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.doc_state()?.tab_edit.as_ref()?;let mode=p.editor?;let align=p.align;let leader=p.leader;let input=p.pos_field.clone();
        let mut group=crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic);
        for (i,a,k) in [(0,TabAlign::Left,"docx.tab-panel.left"),(1,TabAlign::Center,"_shell.center"),(2,TabAlign::Right,"docx.tab-panel.right"),(3,TabAlign::Decimal,"_docx.decimal")] {
            let label=match i {0=>align_name(TabAlign::Left),2=>align_name(TabAlign::Right),_=>crate::shell::i18n::text(k)};
            group=group.child(crate::shell::chrome::segment_control::SegmentItem::label(("tab-align",i as usize),label,align==a).on_click(cx.listener(move |e,_,_,cx|e.choose_tab_align(a,cx))));
        }
        let body=v_flex().gap_2().p_3()
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("_docx.tab-stop-position-pt")))
            .child(crate::shell::chrome::text_field(&input).spinner("tab-position",crate::shell::chrome::SpinnerSpec::new(6.).range(0.05,1584.),cx,|_,_,_,_|{}).unit("pt",px(24.)))
            .child(div().text_sm().child(crate::shell::i18n::text("_shell.arrange"))).child(group)
            .child(div().text_sm().child(crate::shell::i18n::text("_docx.leader")))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).child(crate::shell::chrome::segment_control::SegmentItem::label("tab-none",crate::shell::i18n::text("_shell.none"),!leader).on_click(cx.listener(|e,_,_,cx|e.choose_tab_leader(false,cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("tab-dot",crate::shell::i18n::text("_docx.dotted"),leader).on_click(cx.listener(|e,_,_,cx|e.choose_tab_leader(true,cx)))))
            .children(p.error.clone().map(|s|div().text_sm().text_color(rgb(0xd93025)).child(s)));
        let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("tab-ok", "OK", crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.finish_tab_row_editor(w,cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
            .when_some(mode,|d,i|d.action(crate::shell::chrome::ActionDefinition::new("tab-remove", crate::shell::i18n::text("_shell.delete"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(move |e,_,w,cx|e.delete_live_tab(i,w,cx)))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("_docx.tabs-and-leaders"),body.into_any_element(),footer))
    }
}
